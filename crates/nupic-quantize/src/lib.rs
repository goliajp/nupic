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
}

impl Default for QuantizeOpts {
    fn default() -> Self {
        Self {
            n_colors: 256,
            oxipng_preset: 5,
            strip_metadata: true,
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
    let (indices, palette_srgb) =
        apply_palette_rgba(src_rgba, width, height, &palette_oklab, &palette_alpha);
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

/// Default Lloyd's k-means refinement iteration count for Stone D
/// palette polish. Empirical sweet spot from
/// `docs/research/png/03d-stone-d-design.md` §5 / §5.2 bench:
///
/// | iters | corpus size | corpus SSIM avg | 04-portrait Δ |
/// |---|---|---|---|
/// | 5 | -14.0 KB | +24.68 | +0.49 |
/// | 10 | -20.9 KB | +24.63 | +0.54 |
/// | **20** | **-19.8 KB** | **+24.82** | **+1.15** |
/// | 50 | -22.5 KB | +25.89 | +1.26 |
///
/// At 20 iterations every fixture strictly improves over baseline +
/// over iter=5; 04-portrait gets +1.15 pt and 02-pluto starts catching
/// up (full convergence needs 50 iters for 02 specifically). Choosing
/// 20 as default trades 4× wall-clock for measurable quality on
/// every fixture; callers who want full convergence on the harder
/// fixtures (02-pluto +7 pt at iter=50) can call
/// `quantize_with(..., refine_iters=50)` explicitly.
pub const DEFAULT_REFINE_ITERS: usize = 20;

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
