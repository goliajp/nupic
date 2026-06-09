use std::io::Cursor;

use crate::error::{Error, Result};
use crate::format::Format;
use crate::geom::Size;
use crate::image_handle::Image;

/// Encoding quality.
///
/// `Format(u8)` and `Lossless` are direct codec-level knobs. `Perceptual`
/// expresses *intent* — "produce the smallest file that meets this perceptual
/// quality" — which today's mature-crate implementations approximate by
/// binary-searching the format quality. Future self-built codecs will hit
/// the target directly via in-loop metric optimization.
///
/// `#[non_exhaustive]` — additional perceptual targets may be added.
#[derive(Copy, Clone, Debug)]
#[non_exhaustive]
pub enum Quality {
    /// Codec-native quality (0..=100). Scale meaning is codec-specific.
    Format(u8),
    /// Perceptual quality target; encoder searches for the smallest output
    /// that meets it. Not implemented in v0.1 (needs the metrics module).
    Perceptual(PerceptualTarget),
    /// Mathematically lossless (PNG / WebP-lossless / AVIF-lossless / JXL-lossless).
    Lossless,
}

/// Perceptual quality target.
///
/// `#[non_exhaustive]` — more metrics (DSSIM, VMAF, learned) may be added.
#[derive(Copy, Clone, Debug)]
#[non_exhaustive]
pub enum PerceptualTarget {
    /// Target SSIMULACRA2 score; higher = better quality. Typical range 70..=95.
    Ssimulacra2(f32),
    /// Target Butteraugli max-distance; lower = better quality. Typical range 0.5..=3.0.
    Butteraugli(f32),
}

#[derive(Clone, Debug)]
pub struct CompressOpts {
    pub format: Format,
    pub quality: Quality,
    pub strip_metadata: bool,
    /// Encoder effort, 0 (fastest) ..= 10 (slowest, best compression).
    pub effort: u8,
}

impl Default for CompressOpts {
    fn default() -> Self {
        Self {
            format: Format::Auto,
            quality: Quality::Format(80),
            strip_metadata: false,
            effort: 5,
        }
    }
}

impl CompressOpts {
    pub fn new(format: Format, quality: Quality) -> Self {
        Self {
            format,
            quality,
            ..Self::default()
        }
    }
}

/// The result of [`encode`]: bytes + metadata about what was produced.
#[derive(Clone, Debug)]
pub struct EncodedImage {
    pub bytes: Vec<u8>,
    pub format: Format,
    pub size: Size,
}

/// Encode an image with format-aware compression.
///
/// The caller is expected to resolve [`Format::Auto`] before calling. Returns
/// [`Error::Invalid`] if `opts.format == Format::Auto`.
pub fn encode(img: &Image, opts: CompressOpts) -> Result<EncodedImage> {
    let bytes = match opts.format {
        Format::Auto => {
            return Err(Error::Invalid(
                "Format::Auto must be resolved by the caller before encode()".into(),
            ));
        }
        Format::Png => encode_png(img, &opts)?,
        Format::Jpeg => encode_jpeg(img, &opts)?,
        Format::Webp => encode_webp(img, &opts)?,
        Format::Avif => encode_avif(img, &opts)?,
        Format::Gif | Format::Bmp | Format::Tiff | Format::Jxl => {
            return Err(Error::UnsupportedFormat(opts.format));
        }
    };

    Ok(EncodedImage {
        bytes,
        format: opts.format,
        size: img.size(),
    })
}

fn encode_png(img: &Image, opts: &CompressOpts) -> Result<Vec<u8>> {
    // PNG is always lossless on the wire; `Quality::Format` and
    // `Quality::Lossless` both map to "compress this PNG as well as you can",
    // controlled by `effort`. `Quality::Perceptual` would require palette
    // quantization driven by the metric — not implemented yet.
    match opts.quality {
        Quality::Lossless | Quality::Format(_) => {}
        Quality::Perceptual(_) => {
            return Err(Error::NotImplemented(
                "compress: perceptual quality target for PNG (palette quantization)",
            ));
        }
    }

    let mut raw = Vec::new();
    img.inner()
        .write_to(&mut Cursor::new(&mut raw), image::ImageFormat::Png)?;

    let preset = u8::min(opts.effort, 6);
    let mut oxipng_opts = oxipng::Options::from_preset(preset);
    if opts.strip_metadata {
        oxipng_opts.strip = oxipng::StripChunks::Safe;
    }

    let optimized = oxipng::optimize_from_memory(&raw, &oxipng_opts)
        .map_err(|e| Error::Codec(Box::new(e)))?;
    Ok(optimized)
}

fn encode_jpeg(img: &Image, opts: &CompressOpts) -> Result<Vec<u8>> {
    let quality = match opts.quality {
        Quality::Format(q) => q,
        Quality::Lossless => {
            return Err(Error::Invalid(
                "JPEG baseline does not support lossless encoding".into(),
            ));
        }
        Quality::Perceptual(_) => {
            return Err(Error::NotImplemented("compress: perceptual quality target"));
        }
    };

    let mut out = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
    let rgb = img.inner().to_rgb8();
    image::ImageEncoder::write_image(
        encoder,
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        image::ExtendedColorType::Rgb8,
    )?;
    Ok(out)
}

fn encode_webp(img: &Image, opts: &CompressOpts) -> Result<Vec<u8>> {
    // image-webp (pure rust) supports lossless only as of mid-2026.
    // Lossy WebP requires a future libwebp-binding path or self-built encoder.
    match opts.quality {
        Quality::Lossless => {}
        Quality::Format(_) | Quality::Perceptual(_) => {
            return Err(Error::NotImplemented(
                "compress: lossy WebP (pure-rust encoder unavailable in v0.1; use --lossless)",
            ));
        }
    }

    let mut out = Vec::new();
    img.inner()
        .write_to(&mut Cursor::new(&mut out), image::ImageFormat::WebP)?;
    Ok(out)
}

fn encode_avif(img: &Image, opts: &CompressOpts) -> Result<Vec<u8>> {
    use ravif::{Encoder, Img};

    let rgba = img.inner().to_rgba8();
    let width = rgba.width() as usize;
    let height = rgba.height() as usize;
    let pixels: &[rgb::RGBA8] = bytemuck_cast_rgba(rgba.as_raw());

    // ravif quality: 1 (worst) ..= 100 (best). Lossless = 100 + speed mapping.
    let (quality, speed) = match opts.quality {
        Quality::Format(q) => (q.min(100) as f32, effort_to_speed(opts.effort)),
        Quality::Lossless => (100.0, effort_to_speed(opts.effort)),
        Quality::Perceptual(_) => {
            return Err(Error::NotImplemented("compress: perceptual quality target"));
        }
    };

    let encoder = Encoder::new()
        .with_quality(quality)
        .with_speed(speed)
        .with_alpha_quality(quality);

    let img_view = Img::new(pixels, width, height);
    let res = encoder
        .encode_rgba(img_view)
        .map_err(|e| Error::Codec(Box::new(e)))?;
    Ok(res.avif_file)
}

fn effort_to_speed(effort: u8) -> u8 {
    // nupic effort: 0 (fastest) ..= 10 (slowest).
    // ravif speed:  10 (fastest) ..= 1 (slowest).
    let clamped = effort.min(10);
    11u8.saturating_sub(clamped).max(1)
}

/// Safe-by-construction cast from a `&[u8]` row buffer to `&[rgb::RGBA8]`.
///
/// We require `bytes.len() % 4 == 0` and rely on `rgb::RGBA8` being a
/// `#[repr(C)]` 4-byte struct (matches the `image` crate's RGBA8 layout).
fn bytemuck_cast_rgba(bytes: &[u8]) -> &[rgb::RGBA8] {
    assert!(bytes.len() % 4 == 0, "RGBA buffer length must be a multiple of 4");
    // SAFETY: `rgb::RGBA8` is `#[repr(C)]` with four `u8` fields, identical
    // memory layout to `[u8; 4]`. Buffer length is divisible by 4. Lifetime
    // is preserved via the return signature.
    unsafe {
        std::slice::from_raw_parts(
            bytes.as_ptr() as *const rgb::RGBA8,
            bytes.len() / 4,
        )
    }
}
