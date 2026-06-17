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
    /// Cycle 43 importance-sampled Lloyd weight α. 0.0 = standard Lloyd
    /// (uniform pixel weights, default). > 0 = perceptual-loss-aware
    /// weighted Lloyd: per-pixel weight `1 / (1 + α · |luma diff|)`,
    /// downweighting texture/edge pixels so palette centroids gravitate
    /// to smooth-gradient regions (where SSIMULACRA2 banding penalties
    /// dominate).
    ///
    /// Pareto bench on 05 mountain (stochastic photo, var=320):
    /// - α=0, n=192: 341 KB / SSIM 65.33
    /// - α=0.5, n=144: 324 KB / SSIM 60.04 — **-17 KB at iso-gate**.
    ///
    /// See `docs/research/png/04i-cycle43-importance-sampled-lloyd.md`.
    pub importance_alpha: f32,
}

impl Default for QuantizeOpts {
    fn default() -> Self {
        Self {
            n_colors: 256,
            oxipng_preset: 5,
            strip_metadata: true,
            dither_strength: 0.0,
            importance_alpha: 0.0,
        }
    }
}

/// SoA palette padded to multiple of 4 for f32x4 lane consumption.
/// Pad slots hold +1e9 so they never win argmin.
struct IcmSoAPalette {
    l: Vec<f32>,
    a: Vec<f32>,
    b: Vec<f32>,
    k_pad: usize,
}
impl IcmSoAPalette {
    fn from_oklab(pal: &[Oklab]) -> Self {
        let k_real = pal.len();
        let k_pad = (k_real + 3) & !3usize;
        let mut l = Vec::with_capacity(k_pad);
        let mut a = Vec::with_capacity(k_pad);
        let mut b = Vec::with_capacity(k_pad);
        for c in pal {
            l.push(c.l);
            a.push(c.a);
            b.push(c.b);
        }
        for _ in k_real..k_pad {
            l.push(1.0e9);
            a.push(1.0e9);
            b.push(1.0e9);
        }
        Self { l, a, b, k_pad }
    }
}

/// Cycle 91c (R9 production wiring): SoA + `f32x4` ICM step.
/// Bit-exact replacement for the Cycle 71 scalar inner loop — same
/// data term (OKLab L²), same Potts smoothness (4-neighbor mismatch
/// count × λ²), same argmin tiebreak (first-min by index). Validated
/// in `docs/research/png/04ss-cycle89-icm-simd.md` (1.67× clean
/// speedup, byte-identical PNG output on baseline-7).
fn icm_step_simd(
    src_oklab: &[Oklab],
    w: usize,
    h: usize,
    pal: &IcmSoAPalette,
    indices: &mut [u8],
    lambda_sq: f32,
) {
    use wide::{f32x4, CmpLt, CmpNe};
    let one_f4 = f32x4::splat(1.0);
    let zero_f4 = f32x4::splat(0.0);
    let four_f4 = f32x4::splat(4.0);
    let lam_f4 = f32x4::splat(lambda_sq);
    let inf_f4 = f32x4::splat(f32::INFINITY);

    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let px = src_oklab[i];
            let n_up_u = if y > 0 { indices[i - w] } else { 255u8 };
            let n_dn_u = if y + 1 < h { indices[i + w] } else { 255u8 };
            let n_lf_u = if x > 0 { indices[i - 1] } else { 255u8 };
            let n_rt_u = if x + 1 < w { indices[i + 1] } else { 255u8 };

            let pl_f4 = f32x4::splat(px.l);
            let pa_f4 = f32x4::splat(px.a);
            let pb_f4 = f32x4::splat(px.b);

            let mut min_d2 = inf_f4;
            let mut min_idx = f32x4::from([0.0, 1.0, 2.0, 3.0]);
            let mut idx_iter = f32x4::from([0.0, 1.0, 2.0, 3.0]);

            let n_up_active = n_up_u != 255;
            let n_dn_active = n_dn_u != 255;
            let n_lf_active = n_lf_u != 255;
            let n_rt_active = n_rt_u != 255;
            let nup_v = if n_up_active { f32x4::splat(n_up_u as f32) } else { inf_f4 };
            let ndn_v = if n_dn_active { f32x4::splat(n_dn_u as f32) } else { inf_f4 };
            let nlf_v = if n_lf_active { f32x4::splat(n_lf_u as f32) } else { inf_f4 };
            let nrt_v = if n_rt_active { f32x4::splat(n_rt_u as f32) } else { inf_f4 };

            let mut j = 0usize;
            while j < pal.k_pad {
                let cl = f32x4::new([pal.l[j], pal.l[j+1], pal.l[j+2], pal.l[j+3]]);
                let ca = f32x4::new([pal.a[j], pal.a[j+1], pal.a[j+2], pal.a[j+3]]);
                let cb = f32x4::new([pal.b[j], pal.b[j+1], pal.b[j+2], pal.b[j+3]]);
                let dl = pl_f4 - cl;
                let da = pa_f4 - ca;
                let db = pb_f4 - cb;
                let data = dl * dl + da * da + db * db;

                let mut smooth_count = zero_f4;
                if n_up_active {
                    let neq = idx_iter.cmp_ne(nup_v);
                    smooth_count += neq.blend(one_f4, zero_f4);
                }
                if n_dn_active {
                    let neq = idx_iter.cmp_ne(ndn_v);
                    smooth_count += neq.blend(one_f4, zero_f4);
                }
                if n_lf_active {
                    let neq = idx_iter.cmp_ne(nlf_v);
                    smooth_count += neq.blend(one_f4, zero_f4);
                }
                if n_rt_active {
                    let neq = idx_iter.cmp_ne(nrt_v);
                    smooth_count += neq.blend(one_f4, zero_f4);
                }
                let cost = data + lam_f4 * smooth_count;

                let mask = cost.cmp_lt(min_d2);
                min_d2 = mask.blend(cost, min_d2);
                min_idx = mask.blend(idx_iter, min_idx);

                idx_iter += four_f4;
                j += 4;
            }
            let arr_d = min_d2.to_array();
            let arr_i = min_idx.to_array();
            let mut best_d = arr_d[0];
            let mut best_j = arr_i[0] as u8;
            for k in 1..4 {
                if arr_d[k] < best_d {
                    best_d = arr_d[k];
                    best_j = arr_i[k] as u8;
                }
            }
            indices[i] = best_j;
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
    // Cycle 43: use importance-sampled Lloyd when α > 0.
    // Cycle 55: adaptive iter cap. For 5 MP+ images use 20 iters
    // (captures ~95 % of Lloyd's SSIM gain at 80 % less compute).
    // Sweep showed 5MP fixtures lose 0.2-1.3 SSIM at cap=20 vs cap=100,
    // well within TinyPNG-gate buffer on 25/27. Smaller images keep
    // cap=100 to preserve baseline-7 marketing accuracy.
    let n_pixels = (width as usize) * (height as usize);
    // Cycle 79: 3-tier Lloyd iter cap to hit NAS/CDN < 250ms KPI:
    //   ≥ 5MP → cap=10  (small SSIM cost ≤ 1.8 per c78 sweep,
    //                    saves ~100ms vs cap=20 on 5-15 MP)
    //   2-5MP → cap=30  (Cycle 78 sweep showed plateau ~30)
    //   < 2MP → cap=100 (Cycle 17, baseline-7 fixtures, perf budget OK)
    let refine_cap = if n_pixels >= 5_000_000 { 10 }
        else if n_pixels >= 2_000_000 { 30 }
        else { DEFAULT_REFINE_ITERS };
    (palette_oklab, palette_alpha) = if opts.importance_alpha > 0.0 {
        refine_palette_kmeans_importance(
            src_rgba, width, height,
            &palette_oklab, &palette_alpha,
            refine_cap, opts.importance_alpha,
        )
    } else {
        refine_palette_kmeans(
            src_rgba, width, height,
            &palette_oklab, &palette_alpha,
            refine_cap,
        )
    };
    // Resolve dither strength: NaN means "auto-classify"; finite > 0
    // means explicit; else no dither.
    //
    // Cycle 73: tier-trans smooth-gradient REQUIRES dither for visual
    // correctness — Cycle 71 shipped opt-in via `--dither auto` only,
    // and the bench / user default of dither_strength=0.0 produced
    // VISUALLY BROKEN output on 01/02 (Read-tool inspection: posterized
    // dice, harsh alpha-edge ring). Force-override explicit 0.0 to the
    // auto classifier result. Since classify_for_auto_dither returns
    // 0.0 for everything EXCEPT tier-trans smooth-gradient (opq<0.95
    // + adj_mn≤5), opaque content is unaffected. Callers that want
    // dither off for tier-trans must now pass an explicit positive
    // override (any value works — they have read the API).
    let auto_d = classify_for_auto_dither(src_rgba, width);
    let resolved_strength = if opts.dither_strength.is_nan() || opts.dither_strength == 0.0 {
        auto_d
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
    // Cycle 73: visual regression fix. v1.2.0 Cycle 71 unconditionally
    // annealed tier-trans content (opq<0.95) and produced visually
    // broken output on 01 transparency-demo (posterized dice) and
    // 02 pluto (harsh black ring at alpha boundary) — despite
    // SSIMULACRA2 numbers improving. The ICM step OVERWRITES any
    // FS-dithered indices with smooth piecewise-constant assignment,
    // which destroys fine alpha gradient. Joint anneal now restricted
    // to OPAQUE content with low variance.
    //   - n_pixels < 2.5M (keeps 5MP perf untouched)
    //   - opq ≥ 0.95 (tier-trans skips — pipeline relies on dither)
    //   - var < 200 (stochastic content skipped — joint hurts noise)
    let n_total = src_rgba.len() / 4;
    let small_enough = n_total < 2_500_000;
    let (indices, palette_oklab, palette_alpha, palette_srgb) = if small_enough {
        let n_opaque = src_rgba.chunks_exact(4).filter(|p| p[3] == 255).count();
        let opq = n_opaque as f64 / n_total as f64;
        let should_anneal = if opq < 0.95 {
            false
        } else {
            // var check using cycle-44's stats
            let (_adj_mn, var) = compute_adj_lum_diff_stats(src_rgba, width as usize);
            var < 200.0
        };
        if should_anneal {
            let src_oklab: Vec<Oklab> = src_rgba
                .chunks_exact(4)
                .map(|px| srgb_u8_to_oklab(Rgb { r: px[0], g: px[1], b: px[2] }))
                .collect();
            let mut idx = indices;
            let mut pal_ok = palette_oklab.clone();
            let alpha_vec = palette_alpha.clone();
            const LAMBDAS: [f32; 3] = [0.0001, 0.00005, 0.00002];
            let w = width as usize;
            let h = height as usize;
            let k = pal_ok.len();
            for &lambda_sq in &LAMBDAS {
                // Cycle 91c R9 SIMD (production wiring): bit-exact f32x4
                // replacement for the Cycle 71 scalar ICM. 1.67× speedup
                // on baseline-7 (see 04ss / 04uu). Algorithm unchanged.
                let soa = IcmSoAPalette::from_oklab(&pal_ok);
                icm_step_simd(&src_oklab, w, h, &soa, &mut idx, lambda_sq);
                // Palette retrain: centroid = mean of assigned pixels
                let mut sum_l = vec![0.0f64; k];
                let mut sum_a = vec![0.0f64; k];
                let mut sum_b = vec![0.0f64; k];
                let mut count = vec![0u32; k];
                for (px, &j) in src_oklab.iter().zip(idx.iter()) {
                    let ji = j as usize;
                    sum_l[ji] += px.l as f64;
                    sum_a[ji] += px.a as f64;
                    sum_b[ji] += px.b as f64;
                    count[ji] += 1;
                }
                for j in 0..k {
                    if count[j] > 0 {
                        let c = count[j] as f64;
                        pal_ok[j] = Oklab {
                            l: (sum_l[j] / c) as f32,
                            a: (sum_a[j] / c) as f32,
                            b: (sum_b[j] / c) as f32,
                        };
                    }
                }
            }
            let new_pal_srgb: Vec<Rgb<u8>> = pal_ok.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
            (idx, pal_ok, alpha_vec, new_pal_srgb)
        } else {
            (indices, palette_oklab, palette_alpha, palette_srgb)
        }
    } else {
        (indices, palette_oklab, palette_alpha, palette_srgb)
    };
    let _ = palette_oklab; // silence unused
    let (indices, palette_srgb, palette_alpha) =
        compact_palette(indices, palette_srgb, palette_alpha);
    // Cycle 59: luma-sort palette. Marginal +0-1% size win on
    // luma-gradient content (27 whale -0.85 %) via deflate LZ77
    // locality — adjacent pixels with similar luma now have nearby
    // palette indices. No effect on most fixtures, no regression
    // observed. Free post-process.
    let (indices, palette_srgb, palette_alpha) = {
        let n = palette_srgb.len();
        let lumas: Vec<i32> = palette_srgb.iter().map(|c| (c.r as i32 + c.g as i32 + c.b as i32) / 3).collect();
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&i| lumas[i]);
        let mut inv_map = vec![0u8; n];
        for (new_i, &old_i) in order.iter().enumerate() { inv_map[old_i] = new_i as u8; }
        let new_indices: Vec<u8> = indices.iter().map(|&i| inv_map[i as usize]).collect();
        let new_palette: Vec<Rgb<u8>> = order.iter().map(|&old_i| palette_srgb[old_i]).collect();
        let new_alpha: Vec<u8> = order.iter().map(|&old_i| palette_alpha[old_i]).collect();
        (new_indices, new_palette, new_alpha)
    };
    let trns_opt = if palette_alpha.iter().all(|&a| a == 255) {
        None
    } else {
        Some(palette_alpha.as_slice())
    };
    let raw = encode_indexed_png_with_alpha(width, height, &indices, &palette_srgb, trns_opt)?;
    // Cycle 47: adaptive oxipng preset for large images. preset=5
    // on 5MP+ images costs 1.5-2 s for only +0.1-0.9 % size benefit
    // vs preset=1; on small fixtures (baseline-7) preset=5 gives
    // 4-7 % smaller. Switch to preset=1 when image is ≥ 5 MP.
    // Cycle 63: for < 5 MP, drop preset 5 → 3 — bench shows
    // preset=3/4/5 produce IDENTICAL size while preset=3 is 4-12 %
    // faster. preset=2 starts to hurt size on landscape fixtures.
    let n_pixels = (width as usize) * (height as usize);
    // Cycle 79: 3-tier preset for NAS/CDN < 250ms KPI:
    //   ≥ 5MP → preset=0 (~150-350 ms oxipng, +5-15 % size vs preset=1)
    //   2-5MP → preset=1 (~200-700 ms, balanced)
    //   < 2MP → preset=3 (baseline-7 fixtures, perf budget OK)
    // Pre-Cycle 79: 5MP+ preset=1 = 500-1400 ms (oxipng dominated 55-76%).
    let preset_default = if n_pixels >= 5_000_000 { 0 }
        else if n_pixels >= 2_000_000 { 1 }
        else { 3 };
    let preset = if opts.oxipng_preset != QuantizeOpts::default().oxipng_preset {
        opts.oxipng_preset.min(6) // user explicit override
    } else {
        preset_default
    };
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
    // Cycle 37: sub-sample at stride=8 by default. Sweep on 7-fixture
    // corpus showed stride=8 is net SSIM-positive (+0.24 avg) AND
    // -84 % refine time (~10s → ~1.5s on 5MP). Full-pixel Lloyd was
    // over-fitting to noise; subsample acts as regulariser.
    //
    // Cycle 46: for 5MP+ images, bump stride to 16 — refine time cut
    // ~50 % at small SSIM cost (well within gate buffers).
    let n_pixels = (width as usize) * (height as usize);
    let stride = if n_pixels >= 5_000_000 { 16 } else { 8 };
    let (pal, alpha, _iters_run) = refine_palette_kmeans_instrumented_strided(
        src_rgba, width, height, palette_oklab, palette_alpha, n_iters, 0.0005, stride,
    );
    (pal, alpha)
}

/// Cycle 43 — Importance-Sampled Lloyd k-means.
///
/// Per-pixel weight `w_i = 1 / (1 + α · |luma(p_i) − luma(neighbor_i)|)`.
/// Smooth-gradient pixels (small luma diff) get higher weight; the
/// weighted centroid update biases palette entries toward smooth
/// regions where SSIMULACRA2 penalises palette banding most heavily.
///
/// Centroid update changes from arithmetic mean to weighted mean:
///   `c_j = Σ_{i: cluster=j} w_i · pixel_i / Σ_{i: cluster=j} w_i`.
///
/// Pixel-to-centroid assignment is unchanged (standard L2 in OKLab+α).
///
/// Pareto bench: on 05-mountain (var=320 stochastic photo), α=0.5
/// enables palette reduction from n=192 → n=144 while remaining above
/// the SSIM gate (60.04 vs 59.41 TinyPNG) — net -17 KB at iso-gate.
/// See `docs/research/png/04i-cycle43-importance-sampled-lloyd.md`.
#[must_use]
pub fn refine_palette_kmeans_importance(
    src_rgba: &[u8],
    width: u32,
    height: u32,
    palette_oklab: &[Oklab],
    palette_alpha: &[u8],
    n_iters: usize,
    importance_alpha: f32,
) -> (Vec<Oklab>, Vec<u8>) {
    use rayon::iter::{ParallelIterator, IndexedParallelIterator};
    use rayon::slice::{ParallelSlice, ParallelSliceMut};

    let n_pixels = (width as usize) * (height as usize);
    assert_eq!(src_rgba.len(), n_pixels * 4);
    let k = palette_oklab.len();
    const ALPHA_WEIGHT_C: f32 = 2.0;
    const ALPHA_SCALE_C: f32 = ALPHA_WEIGHT_C / 255.0;
    const EPS_SQ: f32 = 0.0005 * 0.0005;
    // Cycle 46: adaptive stride for large images. Stride=8 default
    // (Cycle 37); for 5MP+ images bump to stride=16. Per Cycle 37
    // sweep, stride=16 costs ~0.2-1.1 SSIM but cuts refine time ~50 %.
    // Large fixtures all have huge SSIM buffer vs TinyPNG (5+ pts),
    // so the SSIM loss is well within gate.
    let stride: usize = if n_pixels >= 5_000_000 { 16 } else { 8 };

    // Precompute weights for every pixel from row+col luma diff at
    // scales {1, 2}.
    // Cycle 44: multi-scale gradient — empirical sweep showed s ∈ {1, 2}
    // gives +0.17 SSIM AND -8 KB vs single-scale (Cycle 43) on 05 mountain.
    // Mathematically approximates SSIMULACRA2's multi-resolution spatial
    // filter (which uses pyramids at 5 scales). Two-scale is the cheap
    // approximation that captures most of the per-pixel perceptual
    // structure relevant to palette quantization.
    let w_usize = width as usize;
    let h_usize = height as usize;
    let mut weights = vec![1.0f32; n_pixels];
    if importance_alpha > 0.0 {
        // Precompute luma per pixel once (saves repeated /3 per scale).
        let luma: Vec<u8> = src_rgba.chunks_exact(4).map(|p| ((p[0] as u32 + p[1] as u32 + p[2] as u32) / 3) as u8).collect();
        const SCALES: [usize; 2] = [1, 2];
        for (i, w) in weights.iter_mut().enumerate() {
            let y = i / w_usize;
            let x = i % w_usize;
            let l0 = luma[i] as i32;
            let mut grad_sum = 0i32;
            let mut cnt = 0;
            for &s in &SCALES {
                if x + s < w_usize { grad_sum += (l0 - luma[i + s] as i32).abs(); cnt += 1; }
                if y + s < h_usize { grad_sum += (l0 - luma[(y + s) * w_usize + x] as i32).abs(); cnt += 1; }
            }
            let mg = if cnt > 0 { grad_sum as f32 / cnt as f32 } else { 0.0 };
            *w = 1.0 / (1.0 + importance_alpha * mg);
        }
    }

    // Pre-build strided pixel vector + weight vector
    let pixels: Vec<(f32, f32, f32, u8, f32)> = src_rgba
        .par_chunks_exact(4)
        .enumerate()
        .filter_map(|(i, px)| {
            if i % stride != 0 { return None; }
            let p = srgb_u8_to_oklab(Rgb { r: px[0], g: px[1], b: px[2] });
            Some((p.l, p.a, p.b, px[3], weights[i]))
        })
        .collect();

    let mut palette = palette_oklab.to_vec();
    let mut alpha = palette_alpha.to_vec();
    let mut assigned = vec![0u8; pixels.len()];
    // Cycle 45: SoA palette for SIMD-friendly access. Padded to k_pad
    // (multiple of 4) so f32x4 batched loads don't need bounds checks.
    let k_pad = (k + 3) & !3;
    let mut pal_l: Vec<f32> = Vec::with_capacity(k_pad);
    let mut pal_a: Vec<f32> = Vec::with_capacity(k_pad);
    let mut pal_b: Vec<f32> = Vec::with_capacity(k_pad);
    let mut pal_as: Vec<f32> = Vec::with_capacity(k_pad);
    pal_l.resize(k_pad, f32::INFINITY); // dummy entries → d2 = ∞
    pal_a.resize(k_pad, f32::INFINITY);
    pal_b.resize(k_pad, f32::INFINITY);
    pal_as.resize(k_pad, f32::INFINITY);
    for _ in 0..n_iters {
        // Refresh SoA palette from AoS palette each iter
        for j in 0..k {
            pal_l[j] = palette[j].l;
            pal_a[j] = palette[j].a;
            pal_b[j] = palette[j].b;
            pal_as[j] = alpha[j] as f32 * ALPHA_SCALE_C;
        }
        const CHUNK: usize = 8192;
        let pl_ref: &[f32] = &pal_l;
        let pa_ref: &[f32] = &pal_a;
        let pb_ref: &[f32] = &pal_b;
        let pas_ref: &[f32] = &pal_as;
        pixels.par_chunks(CHUNK).zip(assigned.par_chunks_mut(CHUNK))
            .for_each(|(chunk, out)| {
                use wide::{f32x4, CmpLt};
                for (pi, &(pl, pa_l, pb, pa_alpha, _w)) in chunk.iter().enumerate() {
                    let px_l = f32x4::splat(pl);
                    let px_a = f32x4::splat(pa_l);
                    let px_b = f32x4::splat(pb);
                    let px_as = f32x4::splat(pa_alpha as f32 * ALPHA_SCALE_C);
                    let mut min_d2 = f32x4::splat(f32::INFINITY);
                    let mut min_idx = f32x4::from([0.0, 1.0, 2.0, 3.0]);
                    let four = f32x4::splat(4.0);
                    let mut idx_iter = f32x4::from([0.0, 1.0, 2.0, 3.0]);
                    let mut j = 0;
                    while j < k_pad {
                        let pj_l: f32x4 = f32x4::new([pl_ref[j], pl_ref[j+1], pl_ref[j+2], pl_ref[j+3]]);
                        let pj_a: f32x4 = f32x4::new([pa_ref[j], pa_ref[j+1], pa_ref[j+2], pa_ref[j+3]]);
                        let pj_b: f32x4 = f32x4::new([pb_ref[j], pb_ref[j+1], pb_ref[j+2], pb_ref[j+3]]);
                        let pj_as: f32x4 = f32x4::new([pas_ref[j], pas_ref[j+1], pas_ref[j+2], pas_ref[j+3]]);
                        let dl = px_l - pj_l;
                        let da = px_a - pj_a;
                        let db = px_b - pj_b;
                        let das = px_as - pj_as;
                        let d2 = dl*dl + da*da + db*db + das*das;
                        // mask: lanes where d2 < min_d2
                        let mask = d2.cmp_lt(min_d2);
                        min_d2 = mask.blend(d2, min_d2);
                        min_idx = mask.blend(idx_iter, min_idx);
                        idx_iter += four;
                        j += 4;
                    }
                    // Horizontal min across 4 lanes
                    let d2_arr: [f32; 4] = min_d2.to_array();
                    let idx_arr: [f32; 4] = min_idx.to_array();
                    let mut bj = 0usize; let mut bd2 = f32::INFINITY;
                    for lane in 0..4 {
                        if d2_arr[lane] < bd2 {
                            bd2 = d2_arr[lane];
                            bj = idx_arr[lane] as usize;
                        }
                    }
                    out[pi] = bj as u8;
                }
            });
        // Weighted centroid update
        let mut sum_l = vec![0.0f64; k];
        let mut sum_a = vec![0.0f64; k];
        let mut sum_b = vec![0.0f64; k];
        let mut sum_alpha = vec![0.0f64; k];
        let mut sum_w = vec![0.0f64; k];
        for (pi, &(pl, pa_l, pb, pa_alpha, w)) in pixels.iter().enumerate() {
            let j = assigned[pi] as usize;
            let wf = w as f64;
            sum_l[j] += wf * pl as f64;
            sum_a[j] += wf * pa_l as f64;
            sum_b[j] += wf * pb as f64;
            sum_alpha[j] += wf * pa_alpha as f64;
            sum_w[j] += wf;
        }
        let mut max_move = 0.0f32;
        for j in 0..k {
            if sum_w[j] < 1e-9 { continue; }
            let inv = 1.0 / sum_w[j];
            let new_l = (sum_l[j] * inv) as f32;
            let new_a = (sum_a[j] * inv) as f32;
            let new_b = (sum_b[j] * inv) as f32;
            let new_alpha = (sum_alpha[j] * inv).round() as u8;
            let old = palette[j];
            let dl = new_l - old.l;
            let da = new_a - old.a;
            let db = new_b - old.b;
            let d_alpha = (new_alpha as i32 - alpha[j] as i32) as f32 * ALPHA_SCALE_C;
            let m = dl*dl + da*da + db*db + d_alpha*d_alpha;
            if m > max_move { max_move = m; }
            palette[j] = Oklab { l: new_l, a: new_a, b: new_b };
            alpha[j] = new_alpha;
        }
        if max_move < EPS_SQ { break; }
    }
    (palette, alpha)
}

/// As [`refine_palette_kmeans`] but with explicit EPS (early-exit
/// threshold on max-centroid-move 4D L2 in OKLab+alpha space) and
/// returns the actual iter count run (≤ `n_iters`). Used by the
/// Cycle 37 perf sweep to characterise convergence vs SSIM tradeoff.
#[must_use]
pub fn refine_palette_kmeans_instrumented(
    src_rgba: &[u8],
    width: u32,
    height: u32,
    palette_oklab: &[Oklab],
    palette_alpha: &[u8],
    n_iters: usize,
    eps: f32,
) -> (Vec<Oklab>, Vec<u8>, usize) {
    refine_palette_kmeans_instrumented_strided(
        src_rgba, width, height, palette_oklab, palette_alpha, n_iters, eps, 1,
    )
}

/// As [`refine_palette_kmeans_instrumented`] but with explicit pixel
/// stride. `stride = 1` = full pixels (no subsample); `stride = 4` =
/// every 4th pixel. Subsample reduces per-iter work by `stride×` at
/// the cost of noisier centroid updates. Used by Cycle 37 perf sweep.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn refine_palette_kmeans_instrumented_strided(
    src_rgba: &[u8],
    width: u32,
    height: u32,
    palette_oklab: &[Oklab],
    palette_alpha: &[u8],
    n_iters: usize,
    eps: f32,
    stride: usize,
) -> (Vec<Oklab>, Vec<u8>, usize) {
    use rayon::iter::ParallelIterator;
    use rayon::slice::ParallelSlice;

    let n_pixels = (width as usize) * (height as usize);
    assert_eq!(src_rgba.len(), n_pixels * 4);
    let k = palette_oklab.len();
    const ALPHA_WEIGHT: f32 = 2.0;
    const ALPHA_SCALE: f32 = ALPHA_WEIGHT / 255.0;
    let eps_sq: f32 = eps * eps;
    // Backwards-compat alias for the remainder of the function body.
    #[allow(non_snake_case)]
    let EPS_SQ: f32 = eps_sq;
    let stride = stride.max(1);

    let mut palette = palette_oklab.to_vec();
    let mut alpha = palette_alpha.to_vec();

    // Phase 3.0: precompute OKLab + alpha for each pixel ONCE upfront.
    // Pre-Phase-3.0 each iter ran srgb_u8_to_oklab 3 times per pixel
    // (assign / sum-accumulate / SSE). For 05-photo-mountain that's
    // 960K × 100 iter × 3 = 288 million sRGB → OKLab conversions, which
    // dominated Lloyd's runtime (2.27 s out of 2.75 s total encode).
    // Memory cost: 16 bytes per pixel (4 × f32 = L, a, b, alpha-scaled)
    // = ~ 15 MB for a 1200 × 800 image; acceptable for the runtime win.
    let pixels_oklab_alpha: Vec<(f32, f32, f32, u8)> = if stride == 1 {
        src_rgba
            .par_chunks_exact(4)
            .map(|px| {
                let p = srgb_u8_to_oklab(Rgb { r: px[0], g: px[1], b: px[2] });
                (p.l, p.a, p.b, px[3])
            })
            .collect()
    } else {
        // Cycle 37: sub-sample at given stride for sub-linear Lloyd cost.
        {
            use rayon::iter::IndexedParallelIterator;
            src_rgba
                .par_chunks_exact(4)
                .enumerate()
                .filter_map(|(i, px)| {
                    if i % stride != 0 { return None; }
                    let p = srgb_u8_to_oklab(Rgb { r: px[0], g: px[1], b: px[2] });
                    Some((p.l, p.a, p.b, px[3]))
                })
                .collect()
        }
    };

    // Pre-allocate assigned buffer; reused each iter.
    let mut assigned: Vec<u8> = vec![0u8; pixels_oklab_alpha.len()];
    // Phase 3.1: cap split-on-empty force-iter contribution to early
    // iters. On simple inputs (logos, < 50 unique colors) split-on-empty
    // perpetually finds empty slots and force-iters Lloyd's to the
    // n_iters cap even though genuine centroid movement converged
    // after 1-2 iters. Limit force-iter to first SPLIT_FORCE_ITERS;
    // after that, EPS_SQ governs convergence regardless of split.
    const SPLIT_FORCE_ITERS: usize = 30;
    let mut iters_run = 0usize;
    for iter_idx in 0..n_iters {
        iters_run = iter_idx + 1;
        use rayon::iter::IndexedParallelIterator;
        use rayon::slice::ParallelSliceMut;
        // Cycle 83: SoA + f32x4 SIMD K-best for the Lloyd assignment
        // step, matching Cycle 82 apply_palette_rgba pattern. Build
        // SoA palette once per Lloyd iter; pad to k_pad (mod 4) with
        // INFINITY dummies so vector loads don't need bounds checks.
        // ~10-20 % Lloyd wall-time reduction on 5MP+ (n=256 → 64 SIMD
        // steps per pixel vs 256 scalar).
        let k_pad = (k + 3) & !3;
        let mut pal_l: Vec<f32> = vec![f32::INFINITY; k_pad];
        let mut pal_a: Vec<f32> = vec![f32::INFINITY; k_pad];
        let mut pal_b: Vec<f32> = vec![f32::INFINITY; k_pad];
        let mut pal_as: Vec<f32> = vec![f32::INFINITY; k_pad];
        for j in 0..k {
            pal_l[j] = palette[j].l;
            pal_a[j] = palette[j].a;
            pal_b[j] = palette[j].b;
            pal_as[j] = alpha[j] as f32 * ALPHA_SCALE;
        }
        let pl_ref: &[f32] = &pal_l;
        let pa_ref: &[f32] = &pal_a;
        let pb_ref: &[f32] = &pal_b;
        let pas_ref: &[f32] = &pal_as;
        const CHUNK: usize = 8192;
        pixels_oklab_alpha
            .par_chunks(CHUNK)
            .zip(assigned.par_chunks_mut(CHUNK))
            .for_each(|(pixels, out)| {
                use wide::{f32x4, CmpLt};
                for (pi, &(pl, pa_l, pb, pa_alpha)) in pixels.iter().enumerate() {
                    let px_l = f32x4::splat(pl);
                    let px_a = f32x4::splat(pa_l);
                    let px_b = f32x4::splat(pb);
                    let px_as = f32x4::splat(pa_alpha as f32 * ALPHA_SCALE);
                    let mut min_d2 = f32x4::splat(f32::INFINITY);
                    let mut min_idx = f32x4::from([0.0, 1.0, 2.0, 3.0]);
                    let four = f32x4::splat(4.0);
                    let mut idx_iter = f32x4::from([0.0, 1.0, 2.0, 3.0]);
                    let mut j = 0;
                    while j < k_pad {
                        let pj_l = f32x4::new([pl_ref[j], pl_ref[j+1], pl_ref[j+2], pl_ref[j+3]]);
                        let pj_a = f32x4::new([pa_ref[j], pa_ref[j+1], pa_ref[j+2], pa_ref[j+3]]);
                        let pj_b = f32x4::new([pb_ref[j], pb_ref[j+1], pb_ref[j+2], pb_ref[j+3]]);
                        let pj_as = f32x4::new([pas_ref[j], pas_ref[j+1], pas_ref[j+2], pas_ref[j+3]]);
                        let dl = px_l - pj_l;
                        let da = px_a - pj_a;
                        let db = px_b - pj_b;
                        let das = px_as - pj_as;
                        let d2 = dl*dl + da*da + db*db + das*das;
                        let mask = d2.cmp_lt(min_d2);
                        min_d2 = mask.blend(d2, min_d2);
                        min_idx = mask.blend(idx_iter, min_idx);
                        idx_iter += four;
                        j += 4;
                    }
                    let d2_arr: [f32; 4] = min_d2.to_array();
                    let idx_arr: [f32; 4] = min_idx.to_array();
                    let mut bj = 0usize; let mut bd2 = f32::INFINITY;
                    for lane in 0..4 {
                        if d2_arr[lane] < bd2 {
                            bd2 = d2_arr[lane];
                            bj = idx_arr[lane] as usize;
                        }
                    }
                    out[pi] = bj as u8;
                }
            });

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
    (palette, alpha, iters_run)
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
        // Cycle 51: zero-copy slice cast. rgb::RGBA8 is repr(C) { r,g,b,a:u8 }
        // matching src_rgba's byte layout exactly. Saves a 20 MB Vec<RGBA8>
        // allocation on 5MP fixtures (~30% of imagequant RSS overhead).
        assert!(src_rgba.len() % 4 == 0);
        let pixels: &[rgb::RGBA8] = unsafe {
            std::slice::from_raw_parts(
                src_rgba.as_ptr() as *const rgb::RGBA8,
                src_rgba.len() / 4,
            )
        };
        // Cycle 52: adaptive imagequant speed. speed=4 has a +130 MB
        // allocation spike on ≥5 MP inputs (the best-quality codepath
        // builds a heavy k-d-tree structure). speed=8 uses ~2 MB more
        // than baseline AND runs 4× faster, with EQUIVALENT or BETTER
        // SSIM (+0.46 on 25-sofia). On < 5 MP, speed=4 keeps the small
        // size advantage for baseline-7 marketing accuracy.
        let n_pixels = (w as usize) * (h as usize);
        let speed = if n_pixels >= 5_000_000 { 8 } else { 4 };
        let mut attrs = imagequant::new();
        attrs.set_quality(q_min, 95).map_err(|_| ())?;
        attrs.set_speed(speed).map_err(|_| ())?;
        let mut img = attrs.new_image(pixels, w as usize, h as usize, 0.0).map_err(|_| ())?;
        let mut quant = attrs.quantize(&mut img).map_err(|_| ())?;
        // Cycle 36: skip the per-pixel remap. `remapped()` returns
        // (palette, indices), but we discard indices — `apply_palette_rgba`
        // re-does the per-pixel assignment in OKLab space (Stone C
        // insight). The remap step is O(N · K) and dominates 5MP
        // encoding time. `palette()` returns just the palette in O(K).
        Ok(quant.palette().to_vec())
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
/// Cycle 39 + 40: pick `n_colors` for size-priority routing.
/// Returns the smallest palette that keeps SSIMULACRA2 ≥ TinyPNG's
/// reference SSIM on the marketing baseline, with an outlier branch
/// for smooth-gradient-rich content discovered in the 500-corpus
/// (Cycle 40).
///
/// Rule (in order):
/// - `opq < 0.95` → 64 (transparency tier; huge SSIM buffer)
/// - `adj_mn < 1.5` + `var < 20` + opaque-region-uniq > 75 K → 256
///   (Cycle 40: smooth-gradient-rich photo. Identified via corpus-500
///   outliers: p244/p154/p184/p220/p123 all sit in adj_mn∈[1.07,1.29],
///   var∈[4.2,18.1], uniq∈[75K,136K] and crash to SSIM ~52-58 at
///   default n=192/208. Forcing n=256 recovers +6-12 SSIM at only
///   +0.5-2.5 % size on those fixtures. Baseline-7 unchanged
///   (04 adj_mn=3.81, 06 adj_mn=21.68 — all above the 1.5 cutoff.)
/// - opaque uniq > 100 K → 192 (high-uniq stochastic photo)
/// - else → 208 (smooth photo, gate-critical)
pub fn classify_for_palette_size(src_rgba: &[u8], width: usize) -> usize {
    let n_total = src_rgba.len() / 4;
    let mut n_opaque = 0usize;
    for px in src_rgba.chunks_exact(4) {
        if px[3] == 255 { n_opaque += 1; }
    }
    let opq = n_opaque as f64 / n_total as f64;
    if opq < 0.95 {
        // Cycle 75: tier-trans 3-way split by adj_mn + uniq_opq.
        //
        // adj_mn > 5         → sharp-mask logo (03 wiki adj_mn=8.20,
        //                       14 soft-trans adj_mn=5.10): n=256,
        //                       no dither (dither on AA edges noises)
        // adj_mn ≤ 5
        //   + uniq_opq < 5000 → translucent overlay (01 dice
        //                       uniq_opq=4348, 14 puppy uniq_opq=2529):
        //                       n=64 + d=0.7 — many alpha-blended
        //                       smooth tones need palette anchors
        //   + uniq_opq ≥ 5000 → photo + alpha edge (02 pluto
        //                       uniq_opq=19444, 21 earth 142K,
        //                       22 tree 55K, 23 statue 72K):
        //                       n=32 + d=0.7 — single-texture-photo,
        //                       dither carries tonal continuity at
        //                       smaller palette, big size win
        //
        // Visual verification 2026-06-17:
        //   01 dice  @ n=64+d=0.7 → 45 KB (visually pristine)
        //   02 pluto @ n=32+d=0.7 → 59 KB (visually pristine, -38 KB)
        //   21 earth @ n=32+d=0.7 → 530 KB (-360 KB visually unchanged)
        //   22 tree  @ n=32+d=0.7 → 710 KB (-150 KB)
        //   23 statue @ n=32+d=0.7 → 157 KB (-60 KB)
        let (adj_mn, _var) = compute_adj_lum_diff_stats(src_rgba, width);
        if adj_mn > 5.0 {
            return 256;
        }
        let step_u = if n_total > 1_000_000 { 4 } else { 1 };
        let mut uniq = std::collections::HashSet::with_capacity(5_500);
        for p in src_rgba.chunks_exact(4).step_by(step_u) {
            if p[3] != 255 { continue; }
            let key = (p[0] as u32) | ((p[1] as u32) << 8) | ((p[2] as u32) << 16);
            uniq.insert(key);
            if uniq.len() >= 5_000 { return 32; } // photo+edge
        }
        // Cycle 104 P-01: tier-trans low-uniq translucent content.
        // Cycle 102 spike + Cycle 103 30-fixture validation found:
        //   chroma_entropy < 5 (2D OKLab a/b 16×16 histogram Shannon ent)
        //   on this branch → K=96 d=0.2 beats K=64 d=0.7 by 9-22% size,
        //   SSIM stays well above TinyPNG floor. Triggering cohort
        //   (01 dice, 03 wiki when adj_mn-misclassed here, mi0): 3/3 wins.
        if chroma_entropy_oklab(src_rgba) < 5.0 {
            return 96;
        }
        return 64; // translucent overlay (entropy ≥ 5 fallback to Cycle 73)
    }
    // Cycle 40: smooth-gradient detection (cheap: one O(N/step) pass
    // computing adj_mn + var on a sub-sampled row grid).
    let (adj_mn, var) = compute_adj_lum_diff_stats(src_rgba, width);
    let step_u = if n_total > 1_000_000 { 4 } else { 1 };
    let mut uniq = std::collections::HashSet::with_capacity(100_500);
    for p in src_rgba.chunks_exact(4).step_by(step_u) {
        if p[3] != 255 { continue; }
        let key = (p[0] as u32) | ((p[1] as u32) << 8) | ((p[2] as u32) << 16);
        uniq.insert(key);
        if uniq.len() > 100_000 { break; }
    }
    let uniq_count = uniq.len();
    if adj_mn < 1.5 && var < 20.0 && uniq_count > 75_000 {
        return 256; // tier-4d-rich: smooth-gradient photo with many colors
    }
    // Cycle 64 + Cycle 77: widened detector for "smooth-detail uniq-rich"
    // outliers. Cycle 61 surfaced 4-5 fixtures at SSIM 54-58 at n=208;
    // Cycle 64 routed (adj_mn<5 + var<150 + uniq>75K) → 256 catching
    // 04 portrait class. Cycle 77 real-Auto corpus probe found more
    // outliers (n30 astronaut uniq=68K, n01 mars 72K, p120 family
    // 67K, p244 4K) all in uniq 50-75K range — just BELOW the Cycle 64
    // threshold. Sweep showed n=256 lifts SSIM +3-8 at +5-7% size on
    // these. Drop the uniq threshold from 75K to 50K.
    //
    // Baseline-7 unaffected: 04 (uniq=25K<50K), 06 (var=663>150),
    // 07 (uniq=25K<50K).
    if adj_mn < 5.0 && var < 150.0 && uniq_count > 50_000 {
        return 256;
    }
    if uniq_count > 100_000 {
        // Cycle 41 + Cycle 77: split high-uniq by variance.
        // - var > 200 ⇒ stochastic content (05 mountain var=320),
        //   palette quantisation noise hidden by image noise → n=192
        // - var ≤ 200 ⇒ photo class. Cycle 77 corpus-real probe
        //   (NASA n29/n30 + p120-class picsum HD + Wikimedia 5K)
        //   showed n=208 plateaus at SSIM 56-62 on these. Sweep
        //   to n=256 lifts SSIM +3-8 at +0-5 % size. The n=256 cap
        //   IS the PNG palette ceiling — pushing routing TO it
        //   recovers a quality stratum the previous n=208 wasted.
        //   Baseline-7 (04 uniq=25K, 06 var=663, 07 uniq=25K)
        //   doesn't hit this branch.
        if var > 200.0 { 192 } else { 256 }
    } else {
        208
    }
}

/// Cycle 43 — extended classifier returning both palette size AND
/// importance-sampled Lloyd α. For stochastic content (var > 200)
/// returns reduced palette (n=144) + α=0.5, enabling Pareto win:
/// 05 mountain saves -17 KB at iso-SSIM-gate. All other routes
/// match `classify_for_palette_size` with α=0 (standard Lloyd).
#[must_use]
pub fn classify_for_palette_size_with_importance(
    src_rgba: &[u8],
    width: usize,
) -> (usize, f32) {
    let n = classify_for_palette_size(src_rgba, width);
    // Re-derive var so we don't repeat the full scan — but compute_adj_lum_diff_stats
    // is cheap so call again rather than threading state.
    let (_adj_mn, var) = compute_adj_lum_diff_stats(src_rgba, width);
    if n == 192 && var > 200.0 {
        // Stochastic high-uniq photo: importance-sampled Lloyd at
        // α=0.5 enables palette drop n=192 → n=144 (-17 KB on
        // 05 mountain) while staying ≥ SSIM gate.
        (144, 0.5)
    } else {
        (n, 0.0)
    }
}

/// Compute mean and variance of adjacent-pixel luma absolute difference.
/// Sub-samples rows proportionally to target ~500 K samples.
/// Returns (mean, variance). Same algorithm as Cycle 17's classifier.
fn compute_adj_lum_diff_stats(src_rgba: &[u8], width: usize) -> (f64, f64) {
    let n_total = src_rgba.len() / 4;
    let w = width.max(2);
    let h = n_total / w;
    let target = 500_000;
    let target_rows = target / (w - 1).max(1);
    let step = (h / target_rows.max(1)).max(1);
    let mut sum_diff: u64 = 0;
    let mut sum_sq: u64 = 0;
    let mut count: u64 = 0;
    for y in (0..h).step_by(step) {
        for x in 0..w.saturating_sub(1) {
            let i = (y * w + x) * 4;
            if i + 7 >= src_rgba.len() { break; }
            let l0 = (src_rgba[i] as u32 + src_rgba[i+1] as u32 + src_rgba[i+2] as u32) / 3;
            let l1 = (src_rgba[i+4] as u32 + src_rgba[i+5] as u32 + src_rgba[i+6] as u32) / 3;
            let d = (l0 as i32 - l1 as i32).unsigned_abs() as u64;
            sum_diff += d;
            sum_sq += d * d;
            count += 1;
        }
    }
    if count == 0 { return (0.0, 0.0); }
    let mean = sum_diff as f64 / count as f64;
    let var = (sum_sq as f64 / count as f64) - mean * mean;
    (mean, var)
}

pub fn classify_for_auto_dither(src_rgba: &[u8], width: u32) -> f32 {
    // Cycle 38: classifier flattened to d=0.0 for opaque content,
    // since TinyPNG's SSIMULACRA2 is the industry-accepted quality
    // threshold and chasing peak SSIM above it costs +5-13% size.
    //
    // Cycle 73 patch: tier-trans smooth-gradient (opq<0.95 + adj_mn≤5)
    // gets d=0.7 back. v1.2.0 shipped d=0.0 + small palette + joint
    // anneal which posterized translucent regions on 01/02 (visual
    // regression discovered by user via Read-tool inspection). With
    // joint anneal disabled and n=256 (see classify_for_palette_size),
    // FS-dither at 0.7 is needed to preserve smooth alpha gradient.
    //
    // Sharp-mask transparency (adj_mn > 5: 03 wiki logo, 14 soft-trans)
    // keeps d=0.0 — dither on antialiased edges adds visible noise.
    let n_total = src_rgba.len() / 4;
    if n_total == 0 { return 0.0; }
    let n_opaque = src_rgba.chunks_exact(4).filter(|p| p[3] == 255).count();
    let opq = n_opaque as f64 / n_total as f64;
    if opq < 0.95 {
        let (adj_mn, _var) = compute_adj_lum_diff_stats(src_rgba, width as usize);
        if adj_mn <= 5.0 {
            // Cycle 104 P-01: keep in lock-step with classify_for_palette_size n=96
            // branch — fires only when (uniq_opq < 5000) AND (entropy < 5). Without
            // the uniq gate, this would over-trigger on 02 pluto (uniq=19K) which
            // is on the K=32 photo+edge path and needs d=0.7 dither (Cycle 73).
            let n_total_lut = src_rgba.len() / 4;
            let step_u = if n_total_lut > 1_000_000 { 4 } else { 1 };
            let mut uniq = std::collections::HashSet::with_capacity(5_500);
            let mut hit_cap = false;
            for p in src_rgba.chunks_exact(4).step_by(step_u) {
                if p[3] != 255 { continue; }
                let key = (p[0] as u32) | ((p[1] as u32) << 8) | ((p[2] as u32) << 16);
                uniq.insert(key);
                if uniq.len() >= 5_000 { hit_cap = true; break; }
            }
            if !hit_cap && chroma_entropy_oklab(src_rgba) < 5.0 {
                return 0.2;
            }
            return 0.7;
        }
    }
    0.0
}

/// Cycle 104 P-01 helper — Shannon entropy of (OKLab a, b) 2D histogram
/// (16 × 16 bins, range auto-fitted to data).  Measures how broadly chroma
/// is distributed; low entropy ⇒ narrow chroma palette ⇒ smaller indexed
/// palette can carry the content.  Used by classify_for_palette_size and
/// classify_for_auto_dither to route low-uniq translucent content
/// (01 dice, mi0 corpus) to K=96 d=0.2 instead of K=64 d=0.7 — Cycle 102
/// spike + Cycle 103 30-fixture validation, GREEN on baseline-7+5MP+
/// corpus-500 sample.
#[doc(hidden)]
fn chroma_entropy_oklab(src_rgba: &[u8]) -> f32 {
    let n = src_rgba.len() / 4;
    if n == 0 { return 0.0; }
    let step = if n > 1_000_000 { 4 } else { 1 };
    let mut oklab_ab: Vec<(f32, f32)> = Vec::with_capacity(n / step + 1);
    let mut a_min = f32::INFINITY; let mut a_max = f32::NEG_INFINITY;
    let mut b_min = f32::INFINITY; let mut b_max = f32::NEG_INFINITY;
    for p in src_rgba.chunks_exact(4).step_by(step) {
        let o = srgb_u8_to_oklab(Rgb { r: p[0], g: p[1], b: p[2] });
        if o.a < a_min { a_min = o.a; } if o.a > a_max { a_max = o.a; }
        if o.b < b_min { b_min = o.b; } if o.b > b_max { b_max = o.b; }
        oklab_ab.push((o.a, o.b));
    }
    let bins = 16usize;
    let mut hist = vec![0u32; bins * bins];
    let a_span = (a_max - a_min).max(1e-6);
    let b_span = (b_max - b_min).max(1e-6);
    for &(a, b) in &oklab_ab {
        let ai = (((a - a_min) / a_span) * bins as f32)
            .floor().clamp(0.0, bins as f32 - 1.0) as usize;
        let bi = (((b - b_min) / b_span) * bins as f32)
            .floor().clamp(0.0, bins as f32 - 1.0) as usize;
        hist[ai * bins + bi] += 1;
    }
    let total = oklab_ab.len() as f64;
    if total == 0.0 { return 0.0; }
    let mut entropy = 0.0f64;
    for &c in hist.iter() {
        if c > 0 {
            let p = c as f64 / total;
            entropy -= p * p.log2();
        }
    }
    entropy as f32
}

#[doc(hidden)]
#[allow(dead_code, clippy::too_many_lines)]
fn classify_for_auto_dither_legacy(src_rgba: &[u8], width: u32) -> f32 {
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
    // Cycle 34: tier-1c/2c sharp-mask peak-d scales with opaque-region
    // uniq color count. Sweep on 4 corpus fixtures:
    //
    //   fixture        opq    a_part  uniq   peak d  peak SSIM
    //   02 pluto       0.78   0.008   19 K   0.50    80.87
    //   22 tree-trans  0.30   0.052   26 K   0.70    66.99 (+0.25 vs 0.5)
    //   23 statue      0.16   0.001   43 K   0.80    80.73 (+0.10 vs 0.5)
    //   21 earth-hemi  0.60   0.046   86 K   0.85    67.43 (+1.01 vs 0.5)
    //
    // peak-d monotonic in uniq. Three-bucket split {<20K → 0.5,
    // <60K → 0.7, ≥60K → 0.85} hits or near-hits peak on all four:
    // 02 / 22 / 21 exact peak, 23 routes to 0.7 (gap 0.02 vs 0.8 peak).
    // Same uniq logic for tier-1c (opq < 0.5) and tier-2c (0.5–0.95);
    // helper consolidates the count.
    if opaque_ratio < 0.95 {
        let n_partial = n_total - n_opaque - n_zero_alpha;
        let a_partial_ratio = n_partial as f64 / n_total as f64;
        if a_partial_ratio >= 0.10 {
            // Cycle 35: smooth-gradient transparency benefits from dither.
            // Peak-d sweep on 01-trans-demo and 14-soft-trans (both tier-1
            // smooth, opq < 0.5 + a_partial ≥ 0.1):
            //   01 trans-demo  d=0.0 SSIM -46.43  d=0.7 SSIM -32.75  +13.68
            //   14 soft-trans  d=0.0 SSIM  66.90  d=0.7 SSIM  70.44   +3.55
            // Both peak at d=0.7 (then crash at 1.0). Size cost ~40 % but
            // metric gain is decisive. tier-2 smooth (opq ∈ [0.5, 0.95))
            // branch unchanged at 0.35 — no corpus evidence to retune.
            return if opaque_ratio < 0.50 { 0.7 } else { 0.35 };
        }
        // Sharp-mask: route by opaque-region uniq color count.
        let step_u = if n_total > 1_000_000 { 4 } else { 1 };
        let mut uniq = std::collections::HashSet::with_capacity(60_500);
        for p in src_rgba.chunks_exact(4).step_by(step_u) {
            if p[3] != 255 { continue; }
            let key = (p[0] as u32) | ((p[1] as u32) << 8) | ((p[2] as u32) << 16);
            uniq.insert(key);
            if uniq.len() > 60_000 { break; }
        }
        if uniq.len() > 60_000 {
            return 0.85; // tier-1c/2c-h: high-uniq sharp-mask (Cycle 34)
        }
        if uniq.len() > 20_000 {
            return 0.7; // tier-1c/2c-m: mid-uniq sharp-mask (Cycle 34)
        }
        return 0.5; // tier-1c/2c-l: low-uniq sharp-mask (was Cycle 28)
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
        // Phase 3.13 (Cycle 33): tier-4f chunky-run escape. When a high-
        // variance fixture also has mean_run > 2 (long same-color runs),
        // it's photographically tier-3-like (chunky patches) despite the
        // tier-4 uniq escape. 18-snowflake (var=123, mr=2.57, uniq=114K,
        // mean=2.66) peaks at d=0.25 (82.81) vs current d=0.7 (82.65) —
        // +0.16 SSIM and −115 KB. All 11 other tier-4b/4e fixtures have
        // mr < 1.6, so this branch fires uniquely on 18. N=1 evidence;
        // documented in essay 03z.
        if mean_run > 2.0 {
            return 0.25; // tier-4f (Cycle 33): chunky-run tier-3-like texture
        }
        // Phase 3.12 (Cycle 31 → Cycle 32): tier-4b/4e split by adj_mn.
        // Cycle 31 used `mean > 5.0 → 0.5` based on 5-fixture probe, missing
        // the baseline-7 + 11-photo-noisy tier-4 fixtures. Cycle 32 full-
        // corpus peak-d sweep showed the relationship is NON-monotonic —
        // peak-d shifts back to 0.7 at very high adj_mn:
        //
        //   fixture          var    adj_mn   peak d   note
        //   19 iceberg        52    3.80     0.7      tier-4b
        //   26 angkor         58    3.71     0.7      tier-4b
        //   24 melk           63    4.41     0.7      tier-4b
        //   07 product        85    4.13     0.7      tier-4b
        //   28 orca           68    6.78     0.5      tier-4e (band)
        //   25 sofia         209    6.86     0.5      tier-4e (band)
        //   05 mountain      320    9.44     0.7      Cycle 31 misroute
        //   11 noisy         297   12.68     0.7      Cycle 31 misroute
        //   06 landscape     663   21.68     0.7      Cycle 31 misroute
        //
        // Narrow band adj_mn ∈ (5, 7.5] catches the 25/28 sweet spot
        // without regressing 05/06/11 (peak 0.7). Cycle 31's open-band
        // rule cost −1.97 SSIM on 05/06/11 for +0.40 on 25/28 (net
        // −1.57); Cycle 32 narrow-band keeps the +0.40 and recovers all.
        //
        // 18 snowflake (adj_mn=2.66, want 0.25-0.5) still misroutes to 0.7
        // but gap only 0.16 SSIM — within noise band. Deferred.
        if mean > 5.0 && mean <= 7.5 {
            return 0.5; // tier-4e: coarse-texture band (peak shifts to 0.5)
        }
        return 0.7; // tier-4b: textured photo (peak 0.7 below/above the band)
    }
    // Phase 3.11 (Cycle 30): tier-4d high-uniq smooth photos. Cycle 27
    // shipped with threshold 50K based on N=3 evidence (13/17/20).
    // Round-2 corpus (24-melk, 25-sofia, 26-angkor, 27-whale, 28-orca,
    // 29-sundew) added N=6 more tier-4a fixtures. Re-tune analysis:
    //
    //   fixture     uniq    peak d
    //   04 portrait  25K    0.5  ← keep at 0.5
    //   16 earthrise 43K    0.5  ← keep at 0.5
    //   27 whale    118K    0.5  ← Cycle 27 wrongly bumped to 0.7
    //   29 sundew   131K    0.7
    //   17 aurora   159K    0.7
    //   20 rainbow  164K    0.7
    //   13 very-lg  1.2M    0.7
    //
    // Clear gap between 118K (want 0.5) and 131K (want 0.7). Threshold
    // 120K cleanly separates with no false-positive in current corpus.
    //
    // Note: 18 snowflake / 25 sofia / 28 orca have var ≥ 50 (tier-4b,
    // gets 0.7) but actually want 0.5. Independent misclass NOT fixed
    // by this cycle. Documented in essay 03w for future research.
    let step_u = if n_total > 1_000_000 { 4 } else { 1 };
    let mut uniq = std::collections::HashSet::with_capacity(120_500);
    for p in src_rgba.chunks_exact(4).step_by(step_u) {
        if p[3] != 255 { continue; }
        let key = (p[0] as u32) | ((p[1] as u32) << 8) | ((p[2] as u32) << 16);
        uniq.insert(key);
        if uniq.len() > 120_000 { break; }
    }
    if uniq.len() > 120_000 {
        0.7 // tier-4d: high-uniq smooth photo
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
    // Cycle 82: SoA palette + f32x4 SIMD K-best search. Pattern
    // mirrors refine_palette_kmeans_importance: pad k → k_pad (mod 4)
    // with INFINITY dummies, broadcast pixel into 4 lanes, vector-load
    // 4 palette entries per iter, mask-blend min. ~2x speedup on apply
    // for 5MP+ inputs (n=256 → 64 SIMD steps per pixel vs 256 scalar).
    let k_pad = (k + 3) & !3;
    let mut pal_l: Vec<f32> = vec![f32::INFINITY; k_pad];
    let mut pal_a: Vec<f32> = vec![f32::INFINITY; k_pad];
    let mut pal_b: Vec<f32> = vec![f32::INFINITY; k_pad];
    let mut pal_as: Vec<f32> = vec![f32::INFINITY; k_pad];
    for j in 0..k {
        pal_l[j] = palette_oklab[j].l;
        pal_a[j] = palette_oklab[j].a;
        pal_b[j] = palette_oklab[j].b;
        pal_as[j] = palette_alpha[j] as f32 * ALPHA_SCALE;
    }
    let pl_ref: &[f32] = &pal_l;
    let pa_ref: &[f32] = &pal_a;
    let pb_ref: &[f32] = &pal_b;
    let pas_ref: &[f32] = &pal_as;
    let mut indices = vec![0u8; n_pixels];
    src_rgba
        .par_chunks_exact(4)
        .zip(indices.par_chunks_exact_mut(1))
        .for_each(|(px, idx)| {
            use wide::{f32x4, CmpLt};
            let p = srgb_u8_to_oklab(Rgb { r: px[0], g: px[1], b: px[2] });
            let px_l = f32x4::splat(p.l);
            let px_a = f32x4::splat(p.a);
            let px_b = f32x4::splat(p.b);
            let px_as = f32x4::splat(px[3] as f32 * ALPHA_SCALE);
            let mut min_d2 = f32x4::splat(f32::INFINITY);
            let mut min_idx = f32x4::from([0.0, 1.0, 2.0, 3.0]);
            let four = f32x4::splat(4.0);
            let mut idx_iter = f32x4::from([0.0, 1.0, 2.0, 3.0]);
            let mut j = 0;
            while j < k_pad {
                let pj_l: f32x4 = f32x4::new([pl_ref[j], pl_ref[j+1], pl_ref[j+2], pl_ref[j+3]]);
                let pj_a: f32x4 = f32x4::new([pa_ref[j], pa_ref[j+1], pa_ref[j+2], pa_ref[j+3]]);
                let pj_b: f32x4 = f32x4::new([pb_ref[j], pb_ref[j+1], pb_ref[j+2], pb_ref[j+3]]);
                let pj_as: f32x4 = f32x4::new([pas_ref[j], pas_ref[j+1], pas_ref[j+2], pas_ref[j+3]]);
                let dl = px_l - pj_l;
                let da = px_a - pj_a;
                let db = px_b - pj_b;
                let das = px_as - pj_as;
                let d2 = dl*dl + da*da + db*db + das*das;
                let mask = d2.cmp_lt(min_d2);
                min_d2 = mask.blend(d2, min_d2);
                min_idx = mask.blend(idx_iter, min_idx);
                idx_iter += four;
                j += 4;
            }
            // Horizontal min over 4 lanes
            let d2_arr: [f32; 4] = min_d2.to_array();
            let idx_arr: [f32; 4] = min_idx.to_array();
            let mut bj = 0usize; let mut bd2 = f32::INFINITY;
            for lane in 0..4 {
                if d2_arr[lane] < bd2 {
                    bd2 = d2_arr[lane];
                    bj = idx_arr[lane] as usize;
                }
            }
            idx[0] = bj as u8;
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
        // Cycle 54: raw encode uses Fast compression — oxipng will
        // re-deflate the IDAT anyway, so the intermediate compression
        // level only affects raw-encode time. Fast saves ~60 ms on 5 MP
        // with zero final-size impact.
        enc.set_compression(png::Compression::Fast);
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
