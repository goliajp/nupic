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
        classify_for_auto_dither(src_rgba)
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
pub fn quantize_with(
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

    for _ in 0..n_iters {
        // Parallel per-pixel assign + per-thread partial reductions.
        let assigned: Vec<u8> = src_rgba
            .par_chunks_exact(4)
            .map(|px| {
                let p = srgb_u8_to_oklab(Rgb { r: px[0], g: px[1], b: px[2] });
                let pa = px[3];
                let mut best_j = 0usize;
                let mut best_d2 = f32::INFINITY;
                for j in 0..k {
                    let pj = palette[j];
                    let dl = p.l - pj.l;
                    let da = p.a - pj.a;
                    let db = p.b - pj.b;
                    let d_alpha = (pa as i32 - alpha[j] as i32) as f32 * ALPHA_SCALE;
                    let d2 = dl.mul_add(
                        dl,
                        da.mul_add(da, db.mul_add(db, d_alpha * d_alpha)),
                    );
                    if d2 < best_d2 {
                        best_d2 = d2;
                        best_j = j;
                    }
                }
                best_j as u8
            })
            .collect();

        // Sequential accumulation (small enough to not need parallel reduce).
        let mut sum_l = vec![0.0f64; k];
        let mut sum_a = vec![0.0f64; k];
        let mut sum_b = vec![0.0f64; k];
        let mut sum_alpha = vec![0u64; k];
        let mut count = vec![0u64; k];
        for (pi, px) in src_rgba.chunks_exact(4).enumerate() {
            let p = srgb_u8_to_oklab(Rgb { r: px[0], g: px[1], b: px[2] });
            let j = assigned[pi] as usize;
            sum_l[j] += p.l as f64;
            sum_a[j] += p.a as f64;
            sum_b[j] += p.b as f64;
            sum_alpha[j] += px[3] as u64;
            count[j] += 1;
        }
        let mut max_move = 0.0f32;
        // Phase 2.7: track per-cluster mean squared OKLab+alpha error
        // (within-cluster variance) so we can split high-error clusters
        // into empty slots. Without this, Stone D Lloyd lets clusters
        // go empty (114/256 effective palette on 04-portrait) which
        // loses color resolution vs TinyPNG's full 256.
        let mut sse = vec![0.0f64; k]; // sum of squared errors per cluster
        for (pi, px) in src_rgba.chunks_exact(4).enumerate() {
            let p = srgb_u8_to_oklab(Rgb { r: px[0], g: px[1], b: px[2] });
            let pa = px[3];
            let j = assigned[pi] as usize;
            if count[j] == 0 { continue; }
            let nc = count[j] as f64;
            let mean_l = (sum_l[j] / nc) as f32;
            let mean_a = (sum_a[j] / nc) as f32;
            let mean_b = (sum_b[j] / nc) as f32;
            let mean_alpha = (sum_alpha[j] as f64 / nc).round() as u8;
            let dl = (p.l - mean_l) as f64;
            let da = (p.a - mean_a) as f64;
            let db = (p.b - mean_b) as f64;
            let dpa = (pa as i32 - mean_alpha as i32) as f64 * ALPHA_SCALE as f64;
            sse[j] += dl * dl + da * da + db * db + dpa * dpa;
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
        if !empty_slots.is_empty() {
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
/// **3-tier decision tree**:
///
/// 1. `opaque_ratio < 0.95 || n_pixels < 200_000`:return `0.0`
///    (small / transparent — Stone D no-dither path already wins,see
///    02-pluto / 03-wikipedia-logo Pareto frontier in 03f essay).
/// 2. `mean_run > 2.0`:return `0.25`(UI screenshot / logo class
///    — strength 0.5 over-dithers,see testflight regression on
///    `03e-stone-e-fs-dither.md` §3).
/// 3. otherwise:return `0.5`(photo class — Pareto-optimal point in
///    03f sweep on 04/05/06/07 photo fixtures).
///
/// `mean_run` = mean length of consecutive RGB-identical pixel runs
/// in row-major order. Photo content rarely has 2 adjacent identical
/// pixels (skin / sky / landscape gradients);UI screenshots have
/// long flat-color runs (text backgrounds, solid panels).
///
/// 03f Pareto sweep showed perfect separation on the 7-fixture + 2
/// dogfood corpus:
/// - Photos: mean_run ∈ [1.10, 1.36] → tier-3 (0.5)
/// - UI: mean_run ∈ [7.89, 94.53] → tier-2 (0.25)
/// - Logos / transparent: tier-1 (0.0)
#[must_use]
pub fn classify_for_auto_dither(src_rgba: &[u8]) -> f32 {
    let mut n_opaque = 0usize;
    let mut n_total = 0usize;
    for px in src_rgba.chunks_exact(4) {
        n_total += 1;
        if px[3] == 255 {
            n_opaque += 1;
        }
    }
    if n_total == 0 {
        return 0.0;
    }
    let opaque_ratio = n_opaque as f64 / n_total as f64;
    if opaque_ratio < 0.95 || n_total < 200_000 {
        return 0.0;
    }
    // tier-2 vs tier-3: mean-run-length signal.
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
    if mean_run > 2.0 {
        0.25
    } else {
        0.5
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
    let mut pixels: Vec<(f32, f32, f32, f32)> = src_rgba
        .chunks_exact(4)
        .map(|px| {
            let p = srgb_u8_to_oklab(Rgb { r: px[0], g: px[1], b: px[2] });
            (p.l, p.a, p.b, px[3] as f32 * ALPHA_SCALE)
        })
        .collect();

    let mut indices = vec![0u8; n_pixels];
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let (l, a, b, pa) = pixels[idx];

            let mut best_j = 0usize;
            let mut best_d2 = f32::INFINITY;
            for j in 0..k {
                let pj = palette_oklab[j];
                let pa_j = palette_alpha[j] as f32 * ALPHA_SCALE;
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
                let pa_j = palette_alpha[best_j] as f32 * ALPHA_SCALE;
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
    let mut indices = vec![0u8; n_pixels];
    src_rgba
        .par_chunks_exact(4)
        .zip(indices.par_chunks_exact_mut(1))
        .for_each(|(px, idx)| {
            let p = srgb_u8_to_oklab(Rgb { r: px[0], g: px[1], b: px[2] });
            let pa = px[3];
            let mut best_j = 0usize;
            let mut best_d2 = f32::INFINITY;
            for j in 0..k {
                let pj = palette_oklab[j];
                let dl = p.l - pj.l;
                let da = p.a - pj.a;
                let db = p.b - pj.b;
                let d_alpha = (pa as i32 - palette_alpha[j] as i32) as f32 * ALPHA_SCALE;
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
