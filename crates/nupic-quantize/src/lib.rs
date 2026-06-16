//! `nupic-quantize` — perceptual palette quantization for indexed PNG.
//!
//! Stone-layer crate. The 03c-bis essay's reversal landed here:
//! Stone C reduces to two changes vs cement imagequant:
//!
//! 1. **OKLab argmin assignment** instead of cement's Lab L2 metric
//!    (Stone A dependency: `nupic-color`)
//! 2. **No Floyd-Steinberg dither** — hard nearest-palette per pixel
//!
//! That's the whole algorithm. No differentiable training, no STE,
//! no Adam. Across the seven `assets/png-bench/inputs/` fixtures it
//! ties or beats cement SSIMULACRA2 on every image (02-pluto jumps
//! +137 points from -65 to +72), while output size drops to ~25 % of
//! cement because index streams without dither compress dramatically
//! better in deflate.
//!
//! Public API (one-shot pipeline + lower-level pieces):
//!
//! ```no_run
//! # use nupic_quantize::{quantize_indexed_png, QuantizeOpts};
//! let src_rgba: Vec<u8> = vec![0u8; 32 * 32 * 4];
//! let png_bytes = quantize_indexed_png(&src_rgba, 32, 32, QuantizeOpts::default()).unwrap();
//! ```

#![allow(clippy::excessive_precision)]
#![allow(clippy::inline_always)]

use nupic_color::{Oklab, oklab_to_srgb_u8, srgb_u8_to_oklab};
use rgb::Rgb;

/// Quantization options. Reasonable defaults match the 03c-ter
/// graduation spec.
#[derive(Clone, Copy, Debug)]
pub struct QuantizeOpts {
    /// Target palette size (1..=256). Default 256 (max for 8-bit indexed PNG).
    pub n_colors: usize,
    /// oxipng preset (0..=6). Default 5 (matches `nupic compress` default
    /// effort=5).
    pub oxipng_preset: u8,
    /// Drop sRGB / iCCP / pHYs etc. chunks. Default `true` (matches the
    /// `nupic compress --strip-metadata` behaviour on PNG path).
    pub strip_metadata: bool,
    /// Stone E Floyd-Steinberg light dither strength (0.0 = no dither,
    /// 0.5 = light dither sweet spot for photo content, 1.0 = full FS).
    /// Default 0.0 — opt-in for photo-heavy workloads via
    /// `--dither <strength>` CLI flag. Trade-off:strength 0.5 adds
    /// ~7% size for +1~5 SSIMULACRA2 pts on photo fixtures; logos /
    /// transparent photos see no benefit or slight regression. See
    /// `docs/research/png/03e-stone-e-fs-dither.md`.
    pub dither_strength: f32,
}

impl Default for QuantizeOpts {
    fn default() -> Self {
        Self {
            n_colors: 256,
            oxipng_preset: 5,
            strip_metadata: true,
            dither_strength: 0.0,
        }
    }
}

/// One-shot pipeline: produce an indexed PNG byte stream from an RGBA8
/// source via the Stone C algorithm.
///
/// Panics if `src_rgba.len() != width * height * 4`.
///
/// # Errors
///
/// Returns `Err` if imagequant's median-cut fails (extremely rare;
/// typically only on degenerate inputs that already fail at q_min=0).
pub fn quantize_indexed_png(
    src_rgba: &[u8],
    width: u32,
    height: u32,
    opts: QuantizeOpts,
) -> Result<Vec<u8>, QuantizeError> {
    let (mut palette_oklab, mut palette_alpha) =
        train_palette_rgba(src_rgba, width, height, opts.n_colors)?;
    // Stone D: palette refinement via Lloyd's k-means.
    (palette_oklab, palette_alpha) = refine_palette_kmeans(
        src_rgba,
        width,
        height,
        &palette_oklab,
        &palette_alpha,
        DEFAULT_REFINE_ITERS,
    );
    // Resolve dither strength: NaN means "auto-classify"; finite > 0
    // means explicit; else no dither.
    let resolved_strength = if opts.dither_strength.is_nan() {
        classify_for_auto_dither(src_rgba, width)
    } else {
        opts.dither_strength
    };
    let (indices, palette_srgb) = if resolved_strength > 0.0 {
        apply_palette_rgba_fs_dither(
            src_rgba,
            width,
            height,
            &palette_oklab,
            &palette_alpha,
            resolved_strength,
        )
    } else {
        apply_palette_rgba(src_rgba, width, height, &palette_oklab, &palette_alpha)
    };
    let (indices, palette_srgb, palette_alpha) =
        compact_palette(indices, palette_srgb, palette_alpha);
    let trns_opt = if palette_alpha.iter().all(|&a| a == 255) {
        None
    } else {
        Some(palette_alpha.as_slice())
    };
    let raw = encode_indexed_png_with_alpha(width, height, &indices, &palette_srgb, trns_opt)?;
    let preset = opts.oxipng_preset.min(6);
    let mut oxipng_opts = oxipng::Options::from_preset(preset);
    if opts.strip_metadata {
        oxipng_opts.strip = oxipng::StripChunks::Safe;
    }
    // Phase 3.5 (Cycle 21): effort ≥ 7 unlocks Zopfli deflater for
    // -0.3% corpus size at 2.7× wall time. iterations = (effort-6) × 5,
    // capped at 30: effort=7 → 5, effort=8 → 10, effort=9 → 15,
    // effort=10 → 20. Pre-Phase-3.5 effort > 6 had no effect (preset
    // capped at 6, libdeflate). Cycle 21 essay documents the corpus
    // sweep showing zero SSIM regression on all 7 fixtures.
    if opts.oxipng_preset >= 7 {
        let iters = ((opts.oxipng_preset - 6) as u8 * 5).min(30).max(1);
        oxipng_opts.deflate = oxipng::Deflaters::Zopfli {
            iterations: std::num::NonZeroU8::new(iters).unwrap(),
        };
    }
    oxipng::optimize_from_memory(&raw, &oxipng_opts)
        .map_err(|e| QuantizeError::Oxipng(format!("{e:?}")))
}

/// Stone C's quantizer output: per-pixel palette index buffer + the
/// final sRGB palette + per-entry alpha (for tRNS chunk emission).
/// Use this if you want to feed indices into a custom PNG encoder
/// (e.g. animated PNG, JPEG XL) instead of the canned
/// [`quantize_indexed_png`] pipeline.
///
/// `palette_alpha` is always populated;callers that want to skip the
/// `tRNS` chunk should check `palette_alpha.iter().all(|&a| a == 255)`
/// — when true, no transparency information is present and `tRNS` can
/// be omitted.
pub struct QuantizedImage {
    pub indices: Vec<u8>,
    pub palette_srgb: Vec<Rgb<u8>>,
    pub palette_alpha: Vec<u8>,
}

/// Full quantization: train palette via imagequant median-cut(RGBA-
/// aware),Stone D Lloyd's k-means refinement (5 iterations default),
/// then apply via OKLab+alpha argmin (no dither). Stone D bench shows
/// strict size + SSIMULACRA2 win on all 7 corpus fixtures vs no-
/// refinement (avg +24.68 SSIM, -0.6% size at 5 iters).
pub fn quantize(
    src_rgba: &[u8],
    width: u32,
    height: u32,
    n_colors: usize,
) -> Result<QuantizedImage, QuantizeError> {
    quantize_with(
        src_rgba,
        width,
        height,
        n_colors,
        DEFAULT_REFINE_ITERS,
    )
}

/// Default Lloyd's k-means refinement iteration **cap** for Stone D.
/// EPS-based early-exit handles fast-convergence inputs (03-wikipedia
/// exits at iter 3); cap of 100 lets slow-convergence inputs (e.g.
/// 02-pluto reaches its SSIM=79 plateau at iter 46) finish naturally
/// instead of being cut short. Wall-clock scales with actual iters,
/// not with the cap.
///
/// Per-fixture convergence iter count (EPS 0.0005,
/// `docs/research/png/03g-adaptive-iter.md` Pass 3 bench):
///
/// | fixture | converged_iter |
/// |---|---|
/// | 01-transparency | 48 |
/// | 02-pluto | 46 |
/// | 03-wikipedia | 3 |
/// | 04-portrait | 34 |
/// | 05-mountain | 67 |
/// | 06-landscape | 48 |
/// | 07-product | 21 |
///
/// Choosing 100 as cap (was 20) gives every fixture room to converge;
/// 02-pluto SSIM improves +7.4 vs iter=20. Callers wanting fixed iter
/// count for benchmark reproducibility can call `quantize_with(...)`
/// with explicit value.
pub const DEFAULT_REFINE_ITERS: usize = 100;

/// Full quantization with explicit Stone D refinement iteration count.
/// `refine_iters = 0` reproduces phase 2.1 behaviour (no refinement).
/// Calls [`quantize_with_dither`] internally with `dither_strength = 0.0`.
pub fn quantize_with(
    src_rgba: &[u8],
    width: u32,
    height: u32,
    n_colors: usize,
    refine_iters: usize,
) -> Result<QuantizedImage, QuantizeError> {
    quantize_with_dither(src_rgba, width, height, n_colors, refine_iters, 0.0)
}

/// Full quantization with all knobs:Stone D refine iterations + Stone E
/// FS dither strength (NaN = auto-classify via `classify_for_auto_dither`).
/// This is what `nupic-core` / `nupic-cli` route through to expose the
/// `--dither auto` flag on `--use-nupic-png` path uniformly with the
/// default Path A oxipng pipeline。
pub fn quantize_with_dither(
    src_rgba: &[u8],
    width: u32,
    height: u32,
    n_colors: usize,
    refine_iters: usize,
    dither_strength: f32,
) -> Result<QuantizedImage, QuantizeError> {
    let (mut palette_oklab, mut palette_alpha) =
        train_palette_rgba(src_rgba, width, height, n_colors)?;
    if refine_iters > 0 {
        (palette_oklab, palette_alpha) = refine_palette_kmeans(
            src_rgba,
            width,
            height,
            &palette_oklab,
            &palette_alpha,
            refine_iters,
        );
    }
    let resolved_strength = if dither_strength.is_nan() {
        classify_for_auto_dither(src_rgba, width)
    } else {
        dither_strength
    };
    let (indices, palette_srgb) = if resolved_strength > 0.0 {
        apply_palette_rgba_fs_dither(
            src_rgba,
            width,
            height,
            &palette_oklab,
            &palette_alpha,
            resolved_strength,
        )
    } else {
        apply_palette_rgba(src_rgba, width, height, &palette_oklab, &palette_alpha)
    };
    let (indices, palette_srgb, palette_alpha) =
        compact_palette(indices, palette_srgb, palette_alpha);
    Ok(QuantizedImage { indices, palette_srgb, palette_alpha })
}

/// (Original `quantize_with` body — kept inline for legacy callers that
/// still construct via the explicit non-dither helper below. Removed in
/// favour of `quantize_with_dither`.)
#[doc(hidden)]
fn _quantize_with_legacy_inline(
    src_rgba: &[u8],
    width: u32,
    height: u32,
    n_colors: usize,
    refine_iters: usize,
) -> Result<QuantizedImage, QuantizeError> {
    let (mut palette_oklab, mut palette_alpha) =
        train_palette_rgba(src_rgba, width, height, n_colors)?;
    if refine_iters > 0 {
        (palette_oklab, palette_alpha) = refine_palette_kmeans(
            src_rgba,
            width,
            height,
            &palette_oklab,
            &palette_alpha,
            refine_iters,
        );
    }
    let (indices, palette_srgb) =
        apply_palette_rgba(src_rgba, width, height, &palette_oklab, &palette_alpha);
    let (indices, palette_srgb, palette_alpha) =
        compact_palette(indices, palette_srgb, palette_alpha);
    Ok(QuantizedImage { indices, palette_srgb, palette_alpha })
}

/// **Stone D**: Lloyd's k-means refinement of the OKLab+alpha palette,
/// starting from imagequant's median-cut centroids.
///
/// Each iteration: (1) assign every pixel to its closest palette entry
/// via the 4-D OKLab+alpha argmin (same metric as `apply_palette_rgba`);
/// (2) recompute every cluster's mean OKLab and mean alpha;
/// (3) replace each palette entry with its cluster mean. Empty
/// clusters keep their previous centroid. Loop exits early if no
/// centroid moves more than `EPS` (4-D OKLab+alpha L2 distance).
///
/// Bench on 7-fixture corpus(see `docs/research/png/03d-stone-d-design.md`):
/// avg +24.68 SSIMULACRA2 at 5 iterations, -0.6% size — strict win.
#[must_use]
pub fn refine_palette_kmeans(
    src_rgba: &[u8],
    width: u32,
    height: u32,
    palette_oklab: &[Oklab],
    palette_alpha: &[u8],
    n_iters: usize,
) -> (Vec<Oklab>, Vec<u8>) {
    use rayon::iter::ParallelIterator;
    use rayon::slice::ParallelSlice;

    let n_pixels = (width as usize) * (height as usize);
    assert_eq!(src_rgba.len(), n_pixels * 4);
    let k = palette_oklab.len();
    const ALPHA_WEIGHT: f32 = 2.0;
    const ALPHA_SCALE: f32 = ALPHA_WEIGHT / 255.0;
    const EPS_SQ: f32 = 0.0005 * 0.0005;

    let mut palette = palette_oklab.to_vec();
    let mut alpha = palette_alpha.to_vec();

    // Phase 3.0: precompute OKLab + alpha for each pixel ONCE upfront.
    // Pre-Phase-3.0 each iter ran srgb_u8_to_oklab 3 times per pixel
    // (assign / sum-accumulate / SSE). For 05-photo-mountain that's
    // 960K × 100 iter × 3 = 288 million sRGB → OKLab conversions, which
    // dominated Lloyd's runtime (2.27 s out of 2.75 s total encode).
    // Memory cost: 16 bytes per pixel (4 × f32 = L, a, b, alpha-scaled)
    // = ~ 15 MB for a 1200 × 800 image; acceptable for the runtime win.
    let pixels_oklab_alpha: Vec<(f32, f32, f32, u8)> = src_rgba
        .par_chunks_exact(4)
        .map(|px| {
            let p = srgb_u8_to_oklab(Rgb { r: px[0], g: px[1], b: px[2] });
            (p.l, p.a, p.b, px[3])
        })
        .collect();

    // Pre-allocate assigned buffer; reused each iter.
    let mut assigned: Vec<u8> = vec![0u8; pixels_oklab_alpha.len()];
    // Phase 3.1: cap split-on-empty force-iter contribution to early
    // iters. On simple inputs (logos, < 50 unique colors) split-on-empty
    // perpetually finds empty slots and force-iters Lloyd's to the
    // n_iters cap even though genuine centroid movement converged
    // after 1-2 iters. Limit force-iter to first SPLIT_FORCE_ITERS;
    // after that, EPS_SQ governs convergence regardless of split.
    const SPLIT_FORCE_ITERS: usize = 30;
    for iter_idx in 0..n_iters {
        use rayon::iter::IndexedParallelIterator;
        use rayon::slice::ParallelSliceMut;
        // Parallel per-pixel assign over precomputed OKLab pixels.
        const CHUNK: usize = 8192;
        let palette_ref: &[Oklab] = &palette;
        let alpha_ref: &[u8] = &alpha;
        pixels_oklab_alpha
            .par_chunks(CHUNK)
            .zip(assigned.par_chunks_mut(CHUNK))
            .for_each(|(pixels, out)| {
                for (pi, &(pl, pa_l, pb, pa_alpha)) in pixels.iter().enumerate() {
                    let mut best_j = 0usize;
                    let mut best_d2 = f32::INFINITY;
                    for j in 0..k {
                        let pj = palette_ref[j];
                        let dl = pl - pj.l;
                        let da = pa_l - pj.a;
                        let db = pb - pj.b;
                        let d_alpha = (pa_alpha as i32 - alpha_ref[j] as i32) as f32 * ALPHA_SCALE;
                        let d2 = dl.mul_add(
                            dl,
                            da.mul_add(da, db.mul_add(db, d_alpha * d_alpha)),
                        );
                        if d2 < best_d2 {
                            best_d2 = d2;
                            best_j = j;
                        }
                    }
                    out[pi] = best_j as u8;
                }
            });
        let _ = (palette_ref.len(), alpha_ref.len()); // silence unused

        // Sequential accumulation over precomputed OKLab pixels.
        // Phase 3.0: also accumulate Σx² in the SAME pass so SSE_j =
        // Σx² − (Σx)² / count is computable without a second loop.
        //
        // Cycle 16 NOTE: tried par_chunks + per-thread Acc + reduce,
        // got 3× SLOWER (Acc{9 Vec × 256 × 8B} alloc + reduce-tree
        // overhead dwarfed the actual accumulate work). Sequential
        // single-thread is memory-bandwidth-bound, not CPU-bound; loop
        // body is < 50 ns/pixel with sum buffers fitting L1.
        let mut sum_l = vec![0.0f64; k];
        let mut sum_a = vec![0.0f64; k];
        let mut sum_b = vec![0.0f64; k];
        let mut sum_alpha = vec![0u64; k];
        let mut sum_l2 = vec![0.0f64; k];
        let mut sum_a2 = vec![0.0f64; k];
        let mut sum_b2 = vec![0.0f64; k];
        let mut sum_alpha2 = vec![0.0f64; k]; // already-scaled (alpha*SCALE)²
        let mut count = vec![0u64; k];
        for (pi, &(pl, pa, pb, pa_alpha)) in pixels_oklab_alpha.iter().enumerate() {
            let j = assigned[pi] as usize;
            let pl_f64 = pl as f64;
            let pa_f64 = pa as f64;
            let pb_f64 = pb as f64;
            let palpha_scaled = pa_alpha as f64 * ALPHA_SCALE as f64;
            sum_l[j] += pl_f64;
            sum_a[j] += pa_f64;
            sum_b[j] += pb_f64;
            sum_alpha[j] += pa_alpha as u64;
            sum_l2[j] += pl_f64 * pl_f64;
            sum_a2[j] += pa_f64 * pa_f64;
            sum_b2[j] += pb_f64 * pb_f64;
            sum_alpha2[j] += palpha_scaled * palpha_scaled;
            count[j] += 1;
        }
        let mut max_move = 0.0f32;
        // SSE_j = (sum_l2 − sum_l²/count) + (sum_a2 − …) + …
        // (each term is the per-axis variance times count)
        let mut sse = vec![0.0f64; k];
        for j in 0..k {
            if count[j] == 0 { continue; }
            let nc = count[j] as f64;
            let mean_alpha_scaled = (sum_alpha[j] as f64 / nc) * ALPHA_SCALE as f64;
            sse[j] = (sum_l2[j] - sum_l[j] * sum_l[j] / nc)
                + (sum_a2[j] - sum_a[j] * sum_a[j] / nc)
                + (sum_b2[j] - sum_b[j] * sum_b[j] / nc)
                + (sum_alpha2[j] - nc * mean_alpha_scaled * mean_alpha_scaled);
        }

        let mut empty_slots: Vec<usize> = (0..k).filter(|&j| count[j] == 0).collect();

        for j in 0..k {
            if count[j] == 0 {
                continue;
            }
            let nc = count[j] as f64;
            let new_l = (sum_l[j] / nc) as f32;
            let new_a = (sum_a[j] / nc) as f32;
            let new_b = (sum_b[j] / nc) as f32;
            let new_alpha = (sum_alpha[j] as f64 / nc).round() as u8;
            let old = palette[j];
            let dl = new_l - old.l;
            let da = new_a - old.a;
            let db = new_b - old.b;
            let d_alpha =
                (new_alpha as i32 - alpha[j] as i32) as f32 * ALPHA_SCALE;
            let move_sq = dl.mul_add(
                dl,
                da.mul_add(da, db.mul_add(db, d_alpha * d_alpha)),
            );
            if move_sq > max_move {
                max_move = move_sq;
            }
            palette[j] = Oklab { l: new_l, a: new_a, b: new_b };
            alpha[j] = new_alpha;
        }

        // Phase 2.7 split-on-empty: for each empty slot, find highest-
        // SSE cluster and split its centroid via slight perturbation.
        // Next iteration's assign will distribute pixels to the new
        // centroid based on argmin proximity.
        //
        // Phase 3.1: skip split entirely after SPLIT_FORCE_ITERS. On
        // simple inputs (logos, < 50 unique colors) split-on-empty
        // perpetually finds empty slots and keeps moving centroids
        // (via the perturbation itself), preventing EPS_SQ convergence
        // even though no genuine improvement happens. Capping the
        // split window at SPLIT_FORCE_ITERS lets natural convergence
        // end Lloyd's; the `compact_palette` step strips unused entries
        // afterward so output PNG isn't bloated.
        if !empty_slots.is_empty() && iter_idx < SPLIT_FORCE_ITERS {
            let mut sse_ordered: Vec<(usize, f64)> =
                (0..k).filter(|&j| count[j] > 0).map(|j| (j, sse[j])).collect();
            sse_ordered.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            for empty_j in empty_slots.drain(..) {
                if let Some(&(donor, donor_sse)) = sse_ordered.first() {
                    if donor_sse <= 0.0 { break; }
                    // Perturb donor centroid +ε in OKLab; place empty
                    // at donor−ε. Next iter argmin will partition pixels
                    // around the original centroid into the two halves.
                    let donor_c = palette[donor];
                    let donor_a = alpha[donor];
                    // Use sqrt(sse/count) as perturbation magnitude scale.
                    let sigma = (donor_sse / count[donor] as f64).sqrt().max(0.001) as f32;
                    palette[empty_j] = Oklab {
                        l: donor_c.l - sigma * 0.5,
                        a: donor_c.a,
                        b: donor_c.b,
                    };
                    alpha[empty_j] = donor_a;
                    palette[donor] = Oklab {
                        l: donor_c.l + sigma * 0.5,
                        a: donor_c.a,
                        b: donor_c.b,
                    };
                    // Force a non-trivial max_move so loop doesn't exit
                    // early after a split.
                    max_move = max_move.max(EPS_SQ * 4.0);
                    // Remove donor from candidate list (so next empty
                    // slot picks the next-highest-SSE cluster).
                    sse_ordered.remove(0);
                }
            }
        }

        if max_move < EPS_SQ {
            break;
        }
    }
    (palette, alpha)
}

/// Train palette: imagequant median-cut → convert to OKLab. The
/// median-cut step uses `quality (70, 95)` first, falling back to
/// `(0, 95)` on QualityTooLow.
///
/// **RGB-only** variant — alpha is discarded. Phase 1.x callers stay
/// on this; new callers should prefer [`train_palette_rgba`].
pub fn train_palette(
    src_rgba: &[u8],
    width: u32,
    height: u32,
    n_colors: usize,
) -> Result<Vec<Oklab>, QuantizeError> {
    train_palette_rgba(src_rgba, width, height, n_colors).map(|(oklab, _)| oklab)
}

/// Train palette and **preserve per-entry alpha** alongside OKLab. The
/// returned `Vec<u8>` is parallel to the `Vec<Oklab>` — `alpha[i]` is
/// the alpha of `palette_oklab[i]` as quantized by imagequant.
///
/// Phase 2.1 entry point — enables tRNS chunk emission downstream.
pub fn train_palette_rgba(
    src_rgba: &[u8],
    width: u32,
    height: u32,
    n_colors: usize,
) -> Result<(Vec<Oklab>, Vec<u8>), QuantizeError> {
    fn try_iq(src_rgba: &[u8], w: u32, h: u32, q_min: u8) -> Result<Vec<rgb::RGBA8>, ()> {
        let pixels: Vec<rgb::RGBA8> = src_rgba.chunks_exact(4)
            .map(|c| rgb::RGBA8 { r: c[0], g: c[1], b: c[2], a: c[3] })
            .collect();
        let mut attrs = imagequant::new();
        attrs.set_quality(q_min, 95).map_err(|_| ())?;
        attrs.set_speed(4).map_err(|_| ())?;
        let mut img = attrs.new_image(pixels.as_slice(), w as usize, h as usize, 0.0).map_err(|_| ())?;
        let mut quant = attrs.quantize(&mut img).map_err(|_| ())?;
        let _ = quant.set_dithering_level(0.0);
        let (palette, _idx) = quant.remapped(&mut img).map_err(|_| ())?;
        Ok(palette)
    }
    let n = n_colors.min(256);
    let palette_rgba = try_iq(src_rgba, width, height, 70)
        .or_else(|_| try_iq(src_rgba, width, height, 0))
        .map_err(|_| QuantizeError::ImagequantFailed)?;
    let mut oklab: Vec<Oklab> = palette_rgba.iter()
        .map(|c| srgb_u8_to_oklab(Rgb { r: c.r, g: c.g, b: c.b }))
        .collect();
    let mut alpha: Vec<u8> = palette_rgba.iter().map(|c| c.a).collect();
    if oklab.len() > n {
        oklab.truncate(n);
        alpha.truncate(n);
    }
    // Phase 2.7: pad palette to `n` entries via duplication so Stone D
    // split-on-empty has the full slot budget to work with. imagequant
    // returns fewer than `n` when its quality threshold (95) is hit
    // early on easy inputs (e.g. 04-portrait returns ~ 100-200 entries
    // out of 256). Without padding, Lloyd refinement only operates on
    // imagequant's output count and palette stays small. Pad entries
    // are duplicates of existing entries; Lloyd will immediately split
    // them via the split-on-empty heuristic in `refine_palette_kmeans`.
    if let (Some(&first_ok), Some(&first_a)) = (oklab.first(), alpha.first()) {
        while oklab.len() < n {
            oklab.push(first_ok);
            alpha.push(first_a);
        }
    }
    Ok((oklab, alpha))
}

/// Hard-quantise an RGBA8 source against a pre-trained OKLab palette.
/// For each pixel: convert to OKLab, take argmin L2 over palette.
/// **No dither** — that's the Stone C insight.
///
/// rayon-parallel across pixels (work-stealing thread pool). Each
/// pixel is independent so this scales close to N-cores. The branchy
/// `if d2 < best_d2` is kept scalar inside the per-pixel loop — LLVM
/// has shown (Stone A) that portable SIMD wrappers don't beat the
/// auto-vectorised straight-line tightly-bounded inner loop on M2.
pub fn apply_palette(
    src_rgba: &[u8],
    width: u32,
    height: u32,
    palette: &[Oklab],
) -> (Vec<u8>, Vec<Rgb<u8>>) {
    // RGB-only legacy path — treat all palette entries as fully opaque
    // and ignore source alpha. Preserves the bit-exact 0.4-0.5 Stone C
    // behaviour for callers that don't need tRNS.
    let alpha = vec![255u8; palette.len()];
    let (indices, palette_srgb) = apply_palette_rgba(src_rgba, width, height, palette, &alpha);
    (indices, palette_srgb)
}

/// Auto-pick Stone E dither strength from a tiered content classifier.
///
/// **4-tier decision tree**(updated Cycle 8 Pass 1 with transparent-
/// photo tier discovered via 02-pluto fine-grain dither sweep):
///
/// 1. `opaque_ratio < 0.50 || n_pixels < 200_000`:return `0.0`
///    (small / transparency-dominant — Stone D no-dither path already
///    wins,see 01-transparency-demo / 03-wikipedia-logo Pareto
///    frontier in 03f essay).
/// 2. `0.50 ≤ opaque_ratio < 0.95`(partially-transparent photo,
///    e.g. 02-pluto):return `0.25`。Cycle 8 fine sweep showed
///    monotonic SSIM gain 0.05-0.25 on 02-pluto(79.66 → 80.44),
///    +2.5% size。Stronger dither over-mixes alpha boundaries。
/// 3. fully opaque + `mean_run > 2.0`:return `0.25`(UI screenshot
///    class — strength 0.5 over-dithers,see testflight regression
///    on `03e-stone-e-fs-dither.md` §3).
/// 4. fully opaque + low mean_run:return `0.5`(photo class — Pareto-
///    optimal point in 03f sweep on 04/05/06/07 photo fixtures).
///
/// `mean_run` = mean length of consecutive RGB-identical pixel runs
/// in row-major order. Photo content rarely has 2 adjacent identical
/// pixels (skin / sky / landscape gradients);UI screenshots have
/// long flat-color runs (text backgrounds, solid panels).
///
/// Phase 3.8 (Cycle 25): detect gradient-class content where lossless
/// PNG via oxipng beats palette-quantize + dither on BOTH dimensions.
///
/// 08-gradient-large evidence: lossless = 53 KB / SSIM 100 vs auto-dither
/// at d=0.7 = 497 KB / SSIM 68. The smooth gradient (low local diff)
/// + many distinct colors (uniq ≥ 1000) signature compresses brilliantly
/// in raw RGBA deflate but loses heavily through 256-palette quantize.
///
/// Returns `true` when `encode_png_stone_c` callers should prefer the
/// lossless RGBA path (`encode_png_lossless`) over palette-quantize.
///
/// Heuristic computed cheaply: O(N) for variance + O(min N, 1000) for
/// unique color count with early-exit. Same signals as
/// [`classify_for_auto_dither`].
#[must_use]
pub fn is_gradient_candidate(src_rgba: &[u8], width: u32) -> bool {
    let n_total = src_rgba.len() / 4;
    if n_total < 200_000 || width < 2 { return false; }
    // Must be fully opaque (mixed-alpha gradients aren't this pattern).
    let mut n_opaque = 0usize;
    for px in src_rgba.chunks_exact(4) {
        if px[3] == 255 { n_opaque += 1; }
    }
    if (n_opaque as f64 / n_total as f64) < 0.95 { return false; }

    let w = width as usize;
    let h = n_total / w;
    const TARGET: usize = 500_000;
    let samples_per_row = (w - 1).max(1);
    let target_rows = TARGET.div_ceil(samples_per_row);
    let step = (h / target_rows.max(1)).max(1);
    let mut sum_diff: u64 = 0;
    let mut count: u64 = 0;
    for y in (0..h).step_by(step) {
        for x in 0..w - 1 {
            let i = (y * w + x) * 4;
            let l0 = (src_rgba[i] as u32 + src_rgba[i + 1] as u32
                    + src_rgba[i + 2] as u32) / 3;
            let l1 = (src_rgba[i + 4] as u32 + src_rgba[i + 5] as u32
                    + src_rgba[i + 6] as u32) / 3;
            let d = (l0 as i32 - l1 as i32).unsigned_abs() as u64;
            sum_diff += d;
            count += 1;
        }
    }
    if count == 0 { return false; }
    let mean = sum_diff as f64 / count as f64;
    if mean >= 1.0 { return false; }  // not extreme-smooth

    // Confirm: lots of unique colors (gradient, not flat block)
    let step_u = if n_total > 1_000_000 { 4 } else { 1 };
    let mut uniq = std::collections::HashSet::with_capacity(1024);
    for p in src_rgba.chunks_exact(4).step_by(step_u) {
        if p[3] != 255 { continue; }
        let key = (p[0] as u32) | ((p[1] as u32) << 8) | ((p[2] as u32) << 16);
        uniq.insert(key);
        if uniq.len() >= 1000 { return true; }
    }
    false
}

#[must_use]
pub fn classify_for_auto_dither(src_rgba: &[u8], width: u32) -> f32 {
    let mut n_opaque = 0usize;
    let mut n_zero_alpha = 0usize;
    let mut n_total = 0usize;
    for px in src_rgba.chunks_exact(4) {
        n_total += 1;
        match px[3] {
            255 => n_opaque += 1,
            0 => n_zero_alpha += 1,
            _ => {}
        }
    }
    if n_total < 200_000 {
        return 0.0; // tier-1: small
    }
    let opaque_ratio = n_opaque as f64 / n_total as f64;
    if opaque_ratio < 0.50 {
        // Phase 3.10 (Cycle 28): tier-1c sharp-mask transparency.
        // When most pixels are alpha=0 (transparent BG) or alpha=255
        // (opaque foreground), partial-alpha pixels are rare (< 10%
        // of total). This is the "object on transparent" pattern,
        // where palette quantize covers FG colors well and the few
        // edge pixels benefit from light dither cheaply.
        //
        // 22-tree-trans (a_partial=0.052) → d=0.5: +1.8 SSIM / +4% size
        // 23-statue    (a_partial=0.001) → d=0.5: +0.4 SSIM / +4% size
        // 01-trans-demo(a_partial=0.291) → stay 0  (smooth-gradient)
        // 14-soft-trans(a_partial=0.991) → stay 0  (smooth-gradient)
        let n_partial = n_total - n_opaque - n_zero_alpha;
        let a_partial_ratio = n_partial as f64 / n_total as f64;
        if a_partial_ratio < 0.10 {
            return 0.5; // tier-1c: sharp-mask object on transparent
        }
        return 0.0; // tier-1: transparency-dominant (smooth-gradient or mixed)
    }
    if opaque_ratio < 0.95 {
        // tier-2: partially-transparent photo. Mirror the tier-1c sharp-
        // mask split — when partial-alpha pixels are < 10% of total
        // (object on transparent BG), dither helps cheaply.
        //
        // Cycle 28 evidence (a_partial across tier-2 fixtures):
        //   02 pluto       a_partial=0.008  d=0.50 SSIM 80.87 (peak)
        //   21 earth-hemi  a_partial=0.046  d=0.50 SSIM 66.42 (+0.50 SSIM
        //                                                       at +2.6% size)
        //   (no smooth-gradient tier-2 fixture in corpus yet)
        let n_partial = n_total - n_opaque - n_zero_alpha;
        let a_partial_ratio = n_partial as f64 / n_total as f64;
        if a_partial_ratio < 0.10 {
            return 0.5; // tier-2c: sharp-mask partial-transparent photo
        }
        // Cycle 20 default for smooth-gradient tier-2 fixtures (none in
        // current corpus but safe fallback).
        return 0.35;
    }
    // tier-3 vs tier-4: mean-run-length signal.
    let mut runs: u64 = 0;
    let mut total_runs: u64 = 0;
    let mut prev: [u8; 3] = [0, 0, 0];
    let mut cur_run: u64 = 0;
    for (i, p) in src_rgba.chunks_exact(4).enumerate() {
        let rgb = [p[0], p[1], p[2]];
        if i > 0 && rgb == prev {
            cur_run += 1;
        } else {
            if cur_run > 0 {
                runs += cur_run;
                total_runs += 1;
            }
            cur_run = 1;
        }
        prev = rgb;
    }
    if cur_run > 0 {
        runs += cur_run;
        total_runs += 1;
    }
    let mean_run = if total_runs == 0 {
        1.0
    } else {
        runs as f64 / total_runs as f64
    };
    // Phase 3.6 (Cycle 23): tier-3 needs uniq-color guard. Synthetic
    // smooth gradients (integer-quantized adjacent colors) trigger
    // mean_run > 2 with very HIGH uniq count (e.g. 08-gradient-large:
    // mean_run=6.13, uniq=117K), but they're photo-class content that
    // needs tier-4 dither, not tier-3. Real UI/logo/text has uniq < 200
    // (09=5, 10=5, 15=3, 03=129). Threshold uniq ≥ 1000 escapes to
    // tier-4. Costs one O(N) pass with early-exit at 1000.
    if mean_run > 2.0 {
        let step_u = if n_total > 1_000_000 { 4 } else { 1 };
        let mut uniq = std::collections::HashSet::with_capacity(1024);
        for p in src_rgba.chunks_exact(4).step_by(step_u) {
            if p[3] != 255 { continue; }
            let key = (p[0] as u32) | ((p[1] as u32) << 8) | ((p[2] as u32) << 16);
            uniq.insert(key);
            if uniq.len() >= 1000 { break; }
        }
        if uniq.len() < 1000 {
            return 0.25; // genuine tier-3: UI / logo / text
        }
        // else fall through to tier-4 (high uniq → photo-class content)
    }
    // tier-4 content split: variance of adjacent-pixel luminance diff
    // distinguishes textured photos (high var, want d=0.7) from smooth
    // portrait-class photos (low var, d=0.5 sweet spot). Cycle 11 sweep
    // on 4 photo fixtures:
    //   04-portrait var=34  → d=0.5  (face features, smooth skin tones)
    //   07-product var=85   → d=0.7  (product texture + soft background)
    //   05-mountain var=320 → d=0.75 (rocks, water, sky texture)
    //   06-landscape var=665→ d=0.7
    // Threshold var > 50 cleanly separates 04 from {05,06,07}.
    let w = width.max(1) as usize;
    if w < 2 {
        return 0.5;
    }
    let h = n_total / w;
    // Phase 3.3 (Cycle 17): proportional step so sampled rows span the
    // FULL image height regardless of size. Pre-Phase-3.3 used a fixed
    // step=4 with count-cap-break; for > 4 MP images (e.g. 1200×6400)
    // the break truncated sampling at top ~50% rows, biasing var-diff
    // to top-half content — adversarial test (cycle17_var_diff_sampling
    // part 3) confirmed smooth-top + textured-bot returned d=0.5 while
    // textured-top + smooth-bot returned d=0.7 on identical pixel pool.
    // New: target ~500 K samples by tuning step ∝ n_total / target;
    // every row reached, no early break.
    const TARGET_SAMPLES: usize = 500_000;
    let samples_per_row = (w - 1).max(1);
    let target_rows = TARGET_SAMPLES.div_ceil(samples_per_row);
    let step = (h / target_rows.max(1)).max(1);
    let mut sum_diff: u64 = 0;
    let mut sum_sq: u64 = 0;
    let mut count: u64 = 0;
    for y in (0..h).step_by(step) {
        for x in 0..w - 1 {
            let i = (y * w + x) * 4;
            let l0 = (src_rgba[i] as u32 + src_rgba[i + 1] as u32
                    + src_rgba[i + 2] as u32) / 3;
            let l1 = (src_rgba[i + 4] as u32 + src_rgba[i + 5] as u32
                    + src_rgba[i + 6] as u32) / 3;
            let d = (l0 as i32 - l1 as i32).unsigned_abs() as u64;
            sum_diff += d;
            sum_sq += d * d;
            count += 1;
        }
    }
    if count == 0 {
        return 0.5;
    }
    let mean = sum_diff as f64 / count as f64;
    let var = (sum_sq as f64 / count as f64) - mean * mean;
    // Phase 3.7 (Cycle 24): extreme-smooth gradient detector. When the
    // mean adjacent-pixel luminance diff is < 1.0, the image is a smooth
    // gradient that suffers heavy palette banding without strong dither.
    // 08-gradient-large (adj_mn=0.06) sweeps from SSIM 58.98 (d=0.5,
    // tier-4a) to SSIM 68.08 (d=0.7) — +9.1 SSIM. Real photos have
    // adj_mn ≥ 2.8 (13 very-large=2.84, 04 portrait=3.81), so the
    // 1.0 threshold catches gradient-class without affecting photos.
    if mean < 1.0 {
        return 0.7; // tier-4c: gradient (banding-prone, needs strong dither)
    }
    if var > 50.0 {
        return 0.7; // tier-4b: textured photo
    }
    // Phase 3.9 (Cycle 27): tier-4d high-uniq smooth photos. Real-photo
    // extended corpus (17-aurora uniq=159K, 20-rainbow uniq=164K, 13
    // very-large uniq=1.2M) all classified tier-4a (var ≤ 50) but want
    // d=0.7 (peak SSIM gain +0.7 to +2.3 vs d=0.5). They have smooth
    // local content (low var) but huge global palette demand (many
    // distinct colors). 04-portrait (uniq=25K) and 16-earthrise
    // (uniq=43K) sit below the threshold and prefer d=0.5 (peak).
    // Threshold 50K cleanly separates the two regimes.
    let step_u = if n_total > 1_000_000 { 4 } else { 1 };
    let mut uniq = std::collections::HashSet::with_capacity(50_500);
    for p in src_rgba.chunks_exact(4).step_by(step_u) {
        if p[3] != 255 { continue; }
        let key = (p[0] as u32) | ((p[1] as u32) << 8) | ((p[2] as u32) << 16);
        uniq.insert(key);
        if uniq.len() > 50_000 { break; }
    }
    if uniq.len() > 50_000 {
        0.7 // tier-4d: high-uniq smooth photo (aurora / rainbow / big-photo class)
    } else {
        0.5 // tier-4a: portrait / smooth photo with limited palette
    }
}

/// Floyd-Steinberg light dither in OKLab+alpha space. `strength`
/// scales the diffused residual; 0 = no dither (call
/// [`apply_palette_rgba`] instead), 1 = canonical FS.
///
/// Stone E research(`docs/research/png/03e-stone-e-fs-dither.md`)
/// shows strength 0.5 gives +1~+5 SSIMULACRA2 on photo fixtures at
/// +2-17% size cost; strength 0.75 still helps photos but at +10%
/// corpus size; strength 1.0 overshoots (full FS collapses SSIM on
/// 02-pluto and several others). Photo / non-transparent inputs
/// benefit most; 02-pluto-class transparent photos and pure-flat
/// logos see no benefit or slight regression.
///
/// Opt-in via `QuantizeOpts::dither_strength > 0.0` / CLI
/// `--dither <strength>`.
#[must_use]
pub fn apply_palette_rgba_fs_dither(
    src_rgba: &[u8],
    width: u32,
    height: u32,
    palette_oklab: &[Oklab],
    palette_alpha: &[u8],
    strength: f32,
) -> (Vec<u8>, Vec<Rgb<u8>>) {
    let w = width as usize;
    let h = height as usize;
    let n_pixels = w * h;
    let k = palette_oklab.len();
    assert_eq!(src_rgba.len(), n_pixels * 4);
    assert_eq!(palette_alpha.len(), k);
    const ALPHA_WEIGHT: f32 = 2.0;
    const ALPHA_SCALE: f32 = ALPHA_WEIGHT / 255.0;

    // Pre-convert all pixels to OKLab + scaled alpha (so diffusion is
    // dimensionally consistent with the distance metric).
    let pixels: Vec<(f32, f32, f32, f32)> = src_rgba
        .chunks_exact(4)
        .map(|px| {
            let p = srgb_u8_to_oklab(Rgb { r: px[0], g: px[1], b: px[2] });
            (p.l, p.a, p.b, px[3] as f32 * ALPHA_SCALE)
        })
        .collect();

    // Phase 3.1: precompute palette_alpha_scaled once (was recomputed
    // per pixel × per centroid: 245M wasted muls for 05-mountain).
    let palette_alpha_scaled: Vec<f32> = palette_alpha
        .iter().map(|&a| a as f32 * ALPHA_SCALE).collect();

    // Cycle 19 NOTE: serpentine-scan variant tried (alternate row L→R
    // / R→L scan to reduce directional smear). On 5 dithered fixtures,
    // net Δ SSIM = -0.045 (02 lost 0.2, others ≈ 0). OKLab+alpha
    // high-dim diffusion + Lloyd's-refined palette already symmetric
    // enough; serpentine adds no signal. Standard FS retained.
    let mut indices = vec![0u8; n_pixels];
    let mut pixels = pixels;
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let (l, a, b, pa) = pixels[idx];
            let mut best_j = 0usize;
            let mut best_d2 = f32::INFINITY;
            for j in 0..k {
                let pj = palette_oklab[j];
                let pa_j = palette_alpha_scaled[j];
                let dl = l - pj.l;
                let da = a - pj.a;
                let db = b - pj.b;
                let dpa = pa - pa_j;
                let d2 = dl.mul_add(
                    dl,
                    da.mul_add(da, db.mul_add(db, dpa * dpa)),
                );
                if d2 < best_d2 {
                    best_d2 = d2;
                    best_j = j;
                }
            }
            indices[idx] = best_j as u8;

            if strength > 0.0 {
                let pj = palette_oklab[best_j];
                let pa_j = palette_alpha_scaled[best_j];
                let err_l = (l - pj.l) * strength;
                let err_a = (a - pj.a) * strength;
                let err_b = (b - pj.b) * strength;
                let err_pa = (pa - pa_j) * strength;
                let mut diffuse = |target_idx: usize, weight: f32| {
                    pixels[target_idx].0 += err_l * weight;
                    pixels[target_idx].1 += err_a * weight;
                    pixels[target_idx].2 += err_b * weight;
                    pixels[target_idx].3 += err_pa * weight;
                };
                if x + 1 < w {
                    diffuse(idx + 1, 7.0 / 16.0);
                }
                if y + 1 < h {
                    if x > 0 {
                        diffuse((y + 1) * w + x - 1, 3.0 / 16.0);
                    }
                    diffuse((y + 1) * w + x, 5.0 / 16.0);
                    if x + 1 < w {
                        diffuse((y + 1) * w + x + 1, 1.0 / 16.0);
                    }
                }
            }
        }
    }
    let palette_srgb: Vec<Rgb<u8>> =
        palette_oklab.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
    (indices, palette_srgb)
}

/// Compact (indices, palette_srgb, palette_alpha) by removing unused
/// palette entries and remapping indices. Necessary after
/// `refine_palette_kmeans` because padded dupes / failed splits leave
/// 0-pixel entries that bloat the PLTE chunk without helping quality.
/// Phase 2.7 ship — restores small-image Auto < Lossless contract.
#[must_use]
pub fn compact_palette(
    indices: Vec<u8>,
    palette_srgb: Vec<Rgb<u8>>,
    palette_alpha: Vec<u8>,
) -> (Vec<u8>, Vec<Rgb<u8>>, Vec<u8>) {
    debug_assert_eq!(palette_srgb.len(), palette_alpha.len());
    let mut used = [false; 256];
    for &i in &indices {
        used[i as usize] = true;
    }
    let mut remap = [0u8; 256];
    let mut new_srgb: Vec<Rgb<u8>> = Vec::with_capacity(palette_srgb.len());
    let mut new_alpha: Vec<u8> = Vec::with_capacity(palette_alpha.len());
    for j in 0..palette_srgb.len() {
        if used[j] {
            remap[j] = new_srgb.len() as u8;
            new_srgb.push(palette_srgb[j]);
            new_alpha.push(palette_alpha[j]);
        }
    }
    let new_indices: Vec<u8> = indices.iter().map(|&i| remap[i as usize]).collect();
    (new_indices, new_srgb, new_alpha)
}

/// Alpha-aware variant of [`apply_palette`]. Each pixel is matched
/// against the palette using a 4-D distance metric:
///
/// `d² = (ΔL)² + (Δa)² + (Δb)² + ALPHA_WEIGHT² · (Δα/255)²`
///
/// where `ALPHA_WEIGHT = 2.0` — large enough that opaque pixels prefer
/// opaque palette entries even when the closest OKLab match is on a
/// transparent entry. Stone C's "OKLab argmin, no dither" insight is
/// preserved; alpha just becomes a fourth comparison axis.
///
/// Phase 2.1 entry point.
pub fn apply_palette_rgba(
    src_rgba: &[u8],
    width: u32,
    height: u32,
    palette_oklab: &[Oklab],
    palette_alpha: &[u8],
) -> (Vec<u8>, Vec<Rgb<u8>>) {
    use rayon::iter::{IndexedParallelIterator, ParallelIterator};
    use rayon::slice::{ParallelSlice, ParallelSliceMut};

    let n_pixels = (width as usize) * (height as usize);
    assert_eq!(src_rgba.len(), n_pixels * 4);
    assert_eq!(palette_oklab.len(), palette_alpha.len());
    let k = palette_oklab.len();
    const ALPHA_WEIGHT: f32 = 2.0;
    const ALPHA_SCALE: f32 = ALPHA_WEIGHT / 255.0;
    // Phase 3.1: precompute palette_alpha_scaled once.
    let palette_alpha_scaled: Vec<f32> = palette_alpha
        .iter().map(|&a| a as f32 * ALPHA_SCALE).collect();
    let mut indices = vec![0u8; n_pixels];
    src_rgba
        .par_chunks_exact(4)
        .zip(indices.par_chunks_exact_mut(1))
        .for_each(|(px, idx)| {
            let p = srgb_u8_to_oklab(Rgb { r: px[0], g: px[1], b: px[2] });
            let pa_scaled = px[3] as f32 * ALPHA_SCALE;
            let mut best_j = 0usize;
            let mut best_d2 = f32::INFINITY;
            for j in 0..k {
                let pj = palette_oklab[j];
                let dl = p.l - pj.l;
                let da = p.a - pj.a;
                let db = p.b - pj.b;
                let d_alpha = pa_scaled - palette_alpha_scaled[j];
                let d2 = dl.mul_add(dl,
                    da.mul_add(da,
                        db.mul_add(db, d_alpha * d_alpha)));
                if d2 < best_d2 {
                    best_d2 = d2;
                    best_j = j;
                }
            }
            idx[0] = best_j as u8;
        });
    let palette_srgb: Vec<Rgb<u8>> = palette_oklab.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
    (indices, palette_srgb)
}

/// Encode an indexed PNG byte stream (palette + index data, no tRNS).
/// Convenience wrapper around [`encode_indexed_png_with_alpha`] for
/// callers that don't need transparency.
pub fn encode_indexed_png(
    width: u32,
    height: u32,
    indices: &[u8],
    palette_srgb: &[Rgb<u8>],
) -> Result<Vec<u8>, QuantizeError> {
    encode_indexed_png_with_alpha(width, height, indices, palette_srgb, None)
}

/// Encode an indexed PNG byte stream with optional `tRNS` chunk for
/// per-palette-entry alpha. `palette_alpha`, when `Some`, must have
/// the same length as `palette_srgb`. Phase 2.1 entry point.
pub fn encode_indexed_png_with_alpha(
    width: u32,
    height: u32,
    indices: &[u8],
    palette_srgb: &[Rgb<u8>],
    palette_alpha: Option<&[u8]>,
) -> Result<Vec<u8>, QuantizeError> {
    if let Some(a) = palette_alpha {
        debug_assert_eq!(a.len(), palette_srgb.len(), "tRNS / palette length mismatch");
    }
    let mut rgb_palette: Vec<u8> = Vec::with_capacity(palette_srgb.len() * 3);
    for c in palette_srgb {
        rgb_palette.push(c.r);
        rgb_palette.push(c.g);
        rgb_palette.push(c.b);
    }
    let mut raw = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut raw, width, height);
        enc.set_color(png::ColorType::Indexed);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_palette(rgb_palette);
        if let Some(a) = palette_alpha {
            // Trim trailing 255s — PNG spec allows tRNS shorter than the
            // palette, with un-listed entries implicitly opaque.
            let last_nonopaque = a.iter().rposition(|&v| v != 255);
            let trimmed: Vec<u8> = match last_nonopaque {
                Some(i) => a[..=i].to_vec(),
                None => Vec::new(),
            };
            if !trimmed.is_empty() {
                enc.set_trns(trimmed);
            }
        }
        let mut writer = enc.write_header().map_err(|e| QuantizeError::PngEncode(format!("{e}")))?;
        writer.write_image_data(indices).map_err(|e| QuantizeError::PngEncode(format!("{e}")))?;
    }
    Ok(raw)
}

#[derive(Debug)]
pub enum QuantizeError {
    ImagequantFailed,
    PngEncode(String),
    Oxipng(String),
}

impl std::fmt::Display for QuantizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ImagequantFailed => write!(f, "imagequant median-cut failed"),
            Self::PngEncode(s) => write!(f, "png encode error: {s}"),
            Self::Oxipng(s) => write!(f, "oxipng error: {s}"),
        }
    }
}

impl std::error::Error for QuantizeError {}
