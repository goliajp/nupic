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
    let palette = train_palette(src_rgba, width, height, opts.n_colors)?;
    let (indices, palette_srgb) = apply_palette(src_rgba, width, height, &palette);
    let raw = encode_indexed_png(width, height, &indices, &palette_srgb)?;
    let preset = opts.oxipng_preset.min(6);
    let mut oxipng_opts = oxipng::Options::from_preset(preset);
    if opts.strip_metadata {
        oxipng_opts.strip = oxipng::StripChunks::Safe;
    }
    oxipng::optimize_from_memory(&raw, &oxipng_opts)
        .map_err(|e| QuantizeError::Oxipng(format!("{e:?}")))
}

/// Stone C's quantizer output: per-pixel palette index buffer + the
/// final sRGB palette. Use this if you want to feed indices into a
/// custom PNG encoder (e.g. animated PNG, JPEG XL) instead of the
/// canned [`quantize_indexed_png`] pipeline.
pub struct QuantizedImage {
    pub indices: Vec<u8>,
    pub palette_srgb: Vec<Rgb<u8>>,
}

/// Full quantization: train palette via imagequant median-cut, then
/// apply it via OKLab argmin (no dither).
pub fn quantize(
    src_rgba: &[u8],
    width: u32,
    height: u32,
    n_colors: usize,
) -> Result<QuantizedImage, QuantizeError> {
    let palette = train_palette(src_rgba, width, height, n_colors)?;
    let (indices, palette_srgb) = apply_palette(src_rgba, width, height, &palette);
    Ok(QuantizedImage { indices, palette_srgb })
}

/// Train palette: imagequant median-cut → convert to OKLab. The
/// median-cut step uses `quality (70, 95)` first, falling back to
/// `(0, 95)` on QualityTooLow.
pub fn train_palette(
    src_rgba: &[u8],
    width: u32,
    height: u32,
    n_colors: usize,
) -> Result<Vec<Oklab>, QuantizeError> {
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
    let mut out: Vec<Oklab> = palette_rgba.iter()
        .map(|c| srgb_u8_to_oklab(Rgb { r: c.r, g: c.g, b: c.b }))
        .collect();
    if out.len() > n { out.truncate(n); }
    Ok(out)
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
    use rayon::iter::{IndexedParallelIterator, ParallelIterator};
    use rayon::slice::{ParallelSlice, ParallelSliceMut};

    let n_pixels = (width as usize) * (height as usize);
    assert_eq!(src_rgba.len(), n_pixels * 4);
    let k = palette.len();
    let mut indices = vec![0u8; n_pixels];
    src_rgba
        .par_chunks_exact(4)
        .zip(indices.par_chunks_exact_mut(1))
        .for_each(|(px, idx)| {
            let p = srgb_u8_to_oklab(Rgb { r: px[0], g: px[1], b: px[2] });
            let mut best_j = 0usize;
            let mut best_d2 = f32::INFINITY;
            for j in 0..k {
                let pj = palette[j];
                let dl = p.l - pj.l;
                let da = p.a - pj.a;
                let db = p.b - pj.b;
                let d2 = dl.mul_add(dl, da.mul_add(da, db * db));
                if d2 < best_d2 {
                    best_d2 = d2;
                    best_j = j;
                }
            }
            idx[0] = best_j as u8;
        });
    let palette_srgb: Vec<Rgb<u8>> = palette.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
    (indices, palette_srgb)
}

/// Encode an indexed PNG byte stream (palette + index data). Alpha
/// channel is not encoded in this minimal pipeline — Stone C's
/// alpha-via-tRNS handling is a Stone D follow-up.
pub fn encode_indexed_png(
    width: u32,
    height: u32,
    indices: &[u8],
    palette_srgb: &[Rgb<u8>],
) -> Result<Vec<u8>, QuantizeError> {
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
