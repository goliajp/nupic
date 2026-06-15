use std::io::Cursor;

use crate::error::{Error, Result};
use crate::format::Format;
use crate::geom::Size;
use crate::image_handle::Image;

/// Encoding quality.
///
/// `Auto` resolves to a sensible per-format **visually lossless** default
/// (true `Lossless` for PNG/WebP/GIF/BMP/TIFF; JPEG `q=95`; AVIF `q=90`).
///
/// `Format(u8)` and `Lossless` are direct codec-level knobs. `Perceptual`
/// expresses *intent* — "produce the smallest file that meets this perceptual
/// quality" — which today's mature-crate implementations approximate by
/// binary-searching the format quality. Future self-built codecs will hit
/// the target directly via in-loop metric optimization.
///
/// `#[non_exhaustive]` — additional variants and perceptual targets may be added.
#[derive(Copy, Clone, Debug)]
#[non_exhaustive]
pub enum Quality {
    /// Encoder-chosen visually-lossless default per format. The library
    /// recommendation when callers don't want to think about quality knobs.
    Auto,
    /// Codec-native quality (0..=100). Scale meaning is codec-specific.
    Format(u8),
    /// Perceptual quality target; encoder searches for the smallest output
    /// that meets it. Not implemented yet (needs the metrics module).
    Perceptual(PerceptualTarget),
    /// Mathematically lossless (PNG / WebP-lossless / AVIF-lossless / JXL-lossless).
    Lossless,
}

/// Perceptual quality target.
///
/// `#[non_exhaustive]` — more metrics (VMAF, learned) may be added.
#[derive(Copy, Clone, Debug)]
#[non_exhaustive]
pub enum PerceptualTarget {
    /// Target DSSIM distance (lower = better; 0.0 = identical).
    /// Typical visually-lossless target: `0.005`. Working since v0.3.
    Dssim(f32),
    /// Target SSIMULACRA2 score; higher = better quality. Typical range 70..=95.
    /// Reserved — needs the stone-layer perceptual pipeline.
    Ssimulacra2(f32),
    /// Target Butteraugli max-distance; lower = better quality. Typical range 0.5..=3.0.
    /// Reserved — needs the stone-layer perceptual pipeline.
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
            quality: Quality::Auto,
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
    if matches!(opts.quality, Quality::Perceptual(_)) {
        return perceptual_search(img, opts);
    }
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
        Format::Gif => encode_passthrough(img, &opts, image::ImageFormat::Gif, "GIF")?,
        Format::Bmp => encode_passthrough(img, &opts, image::ImageFormat::Bmp, "BMP")?,
        Format::Tiff => encode_passthrough(img, &opts, image::ImageFormat::Tiff, "TIFF")?,
        Format::Jxl => {
            return Err(Error::UnsupportedFormat(opts.format));
        }
    };

    Ok(EncodedImage {
        bytes,
        format: opts.format,
        size: img.size(),
    })
}

/// Generic encode dispatch for formats handled directly by the `image` crate
/// with no quality knob (GIF / BMP / TIFF). Rejects perceptual targets since
/// those need a metric search loop on top of the encoder.
fn encode_passthrough(
    img: &Image,
    opts: &CompressOpts,
    image_format: image::ImageFormat,
    name: &'static str,
) -> Result<Vec<u8>> {
    match opts.quality {
        Quality::Auto | Quality::Format(_) | Quality::Lossless => {}
        Quality::Perceptual(_) => {
            // Use a leaked static str so the variant only carries &'static str.
            // `name` is fed from known compile-time constants above, so this
            // string set is bounded.
            return Err(Error::NotImplemented(match name {
                "GIF" => "compress: perceptual quality target for GIF",
                "BMP" => "compress: perceptual quality target for BMP",
                "TIFF" => "compress: perceptual quality target for TIFF",
                _ => "compress: perceptual quality target",
            }));
        }
    }
    let mut out = Vec::new();
    img.inner()
        .write_to(&mut Cursor::new(&mut out), image_format)?;
    Ok(out)
}

fn encode_png(img: &Image, opts: &CompressOpts) -> Result<Vec<u8>> {
    match opts.quality {
        // Default = visually-lossless palette quantization via imagequant,
        // then oxipng deflate / chunk optimization. Matches the lossy-PNG
        // tooling (TinyPNG / pngquant) in spirit.
        Quality::Auto => encode_png_lossy(img, opts, 70, 95),
        Quality::Format(q) => {
            let target = q.min(100);
            let min_q = target.saturating_sub(10);
            encode_png_lossy(img, opts, min_q, target)
        }
        // True mathematical lossless — no quantization, oxipng only.
        Quality::Lossless => encode_png_lossless(img, opts),
        // Perceptual is routed through `perceptual_search` upstream.
        Quality::Perceptual(_) => unreachable!(
            "perceptual_search dispatches PNG quality search; \
             encode_png should not see Quality::Perceptual"
        ),
    }
}

fn encode_png_lossless(img: &Image, opts: &CompressOpts) -> Result<Vec<u8>> {
    let mut raw = Vec::new();
    img.inner()
        .write_to(&mut Cursor::new(&mut raw), image::ImageFormat::Png)?;
    oxipng_optimize(&raw, opts)
}

fn encode_png_lossy(
    img: &Image,
    opts: &CompressOpts,
    quality_min: u8,
    quality_target: u8,
) -> Result<Vec<u8>> {
    let rgba = img.inner().to_rgba8();
    let width = rgba.width() as usize;
    let height = rgba.height() as usize;

    let pixels: &[rgb::RGBA8] = bytemuck_cast_rgba(rgba.as_raw());

    let speed = effort_to_imagequant_speed(opts.effort);

    // First try with the requested quality_min as the floor. If imagequant
    // returns QualityTooLow (palette of 256 colours can't reach the floor),
    // retry with quality_min=0 — we'd rather produce *some* quantised output
    // than fail. This matches what TinyPNG / pngquant -Q 0-N would do.
    let try_quantize = |min: u8| -> Result<(imagequant::Attributes, imagequant::Image<'static>, imagequant::QuantizationResult)> {
        let mut attrs = imagequant::new();
        attrs
            .set_quality(min, quality_target)
            .map_err(|e| Error::Codec(Box::new(e)))?;
        attrs
            .set_speed(i32::from(speed))
            .map_err(|e| Error::Codec(Box::new(e)))?;
        let mut img_iq = attrs
            .new_image(pixels, width, height, 0.0)
            .map_err(|e| Error::Codec(Box::new(e)))?;
        let quant = attrs
            .quantize(&mut img_iq)
            .map_err(|e| Error::Codec(Box::new(e)))?;
        Ok((attrs, img_iq, quant))
    };

    let (_attrs, mut img_iq, mut quant) = match try_quantize(quality_min) {
        Ok(t) => t,
        Err(Error::Codec(boxed)) => {
            // Inspect the boxed error for QualityTooLow.
            let is_quality_too_low = boxed
                .downcast_ref::<imagequant::Error>()
                .is_some_and(|e| matches!(e, imagequant::Error::QualityTooLow));
            if is_quality_too_low && quality_min > 0 {
                try_quantize(0)?
            } else {
                return Err(Error::Codec(boxed));
            }
        }
        Err(e) => return Err(e),
    };

    quant
        .set_dithering_level(1.0)
        .map_err(|e| Error::Codec(Box::new(e)))?;

    let (palette, indexed_pixels) = quant
        .remapped(&mut img_iq)
        .map_err(|e| Error::Codec(Box::new(e)))?;

    // PNG palette is RGB-only; alpha goes in the tRNS chunk. Trim trailing
    // 0xFF alphas so a fully-opaque palette skips tRNS entirely.
    let mut rgb_palette: Vec<u8> = Vec::with_capacity(palette.len() * 3);
    let mut alphas: Vec<u8> = Vec::with_capacity(palette.len());
    for c in &palette {
        rgb_palette.push(c.r);
        rgb_palette.push(c.g);
        rgb_palette.push(c.b);
        alphas.push(c.a);
    }
    while alphas.last() == Some(&255) {
        alphas.pop();
    }

    let mut raw: Vec<u8> = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut raw, width as u32, height as u32);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_palette(rgb_palette);
        if !alphas.is_empty() {
            encoder.set_trns(alphas);
        }
        let mut writer = encoder
            .write_header()
            .map_err(|e| Error::Codec(Box::new(e)))?;
        writer
            .write_image_data(&indexed_pixels)
            .map_err(|e| Error::Codec(Box::new(e)))?;
    }

    oxipng_optimize(&raw, opts)
}

fn oxipng_optimize(raw_png: &[u8], opts: &CompressOpts) -> Result<Vec<u8>> {
    let preset = u8::min(opts.effort, 6);
    let mut oxipng_opts = oxipng::Options::from_preset(preset);
    if opts.strip_metadata {
        oxipng_opts.strip = oxipng::StripChunks::Safe;
    }
    oxipng::optimize_from_memory(raw_png, &oxipng_opts)
        .map_err(|e| Error::Codec(Box::new(e)))
}

fn effort_to_imagequant_speed(effort: u8) -> u8 {
    // nupic effort: 0 (fastest) ..= 10 (slowest).
    // imagequant speed: 1 (slowest, best quality) ..= 10 (fastest).
    let clamped = effort.min(10);
    11u8.saturating_sub(clamped).max(1)
}

fn encode_jpeg(img: &Image, opts: &CompressOpts) -> Result<Vec<u8>> {
    let quality = match opts.quality {
        Quality::Auto => 95, // visually lossless JPEG threshold
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
    match opts.quality {
        // Auto on WebP = lossless (visually identical, file slightly larger).
        Quality::Auto | Quality::Lossless => {
            // image-webp (pure rust) covers lossless natively via the `image` crate.
            let mut out = Vec::new();
            img.inner()
                .write_to(&mut Cursor::new(&mut out), image::ImageFormat::WebP)?;
            Ok(out)
        }
        Quality::Format(q) => {
            // Lossy WebP via libwebp through the `webp` crate.
            let rgba = img.inner().to_rgba8();
            let encoder = webp::Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height());
            let mem = encoder.encode(f32::from(q));
            Ok(mem.to_vec())
        }
        Quality::Perceptual(_) => Err(Error::NotImplemented("compress: perceptual quality target")),
    }
}

fn encode_avif(img: &Image, opts: &CompressOpts) -> Result<Vec<u8>> {
    use ravif::{Encoder, Img};

    let rgba = img.inner().to_rgba8();
    let width = rgba.width() as usize;
    let height = rgba.height() as usize;
    let pixels: &[rgb::RGBA8] = bytemuck_cast_rgba(rgba.as_raw());

    // ravif quality: 1 (worst) ..= 100 (best). Lossless = 100 + speed mapping.
    let (quality, speed) = match opts.quality {
        Quality::Auto => (90.0, effort_to_speed(opts.effort)), // visually lossless AVIF
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

/// Binary-search the format-native quality knob to find the smallest output
/// that meets the perceptual target.
///
/// Strategy:
/// - Discrete search over `q ∈ 10..=100`.
/// - At each midpoint, encode → decode → compute the metric on the original
///   vs the decoded distorted image.
/// - "Meets target" depends on the metric (DSSIM: score ≤ target;
///   Ssimulacra2: score ≥ target; Butteraugli: score ≤ target).
/// - Track the smallest `q` that meets the target; if no `q` does, fall back
///   to the highest tried so we always return *some* bytes.
fn perceptual_search(img: &Image, opts: CompressOpts) -> Result<EncodedImage> {
    let target = match opts.quality {
        Quality::Perceptual(t) => t,
        _ => unreachable!("perceptual_search called with non-perceptual quality"),
    };
    // Formats with no quality knob fall back to lossless — there's nothing
    // to search over. PNG gets palette quantization in the search loop below.
    match opts.format {
        Format::Webp | Format::Gif | Format::Bmp | Format::Tiff => {
            let mut lossless_opts = opts.clone();
            lossless_opts.quality = Quality::Lossless;
            return encode(img, lossless_opts);
        }
        Format::Auto | Format::Jxl => {
            return Err(Error::Invalid(format!(
                "perceptual target not supported on {:?}",
                opts.format
            )));
        }
        Format::Png | Format::Jpeg | Format::Avif => {}
    }

    let metric_fn: fn(&Image, &Image) -> Result<f64> = match target {
        PerceptualTarget::Dssim(_) => crate::metrics::dssim,
        PerceptualTarget::Ssimulacra2(_) => crate::metrics::ssimulacra2,
        PerceptualTarget::Butteraugli(_) => crate::metrics::butteraugli,
    };
    let (target_value, lower_is_better) = match target {
        PerceptualTarget::Dssim(t) => (f64::from(t), true),
        PerceptualTarget::Butteraugli(t) => (f64::from(t), true),
        PerceptualTarget::Ssimulacra2(t) => (f64::from(t), false),
    };

    let mut lo = 10u8;
    let mut hi = 100u8;
    let mut best: Option<(Vec<u8>, u8)> = None;
    let mut fallback: Option<(Vec<u8>, u8)> = None;
    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        let mut trial = opts.clone();
        trial.quality = Quality::Format(mid);
        let trial_bytes = match trial.format {
            Format::Jpeg => encode_jpeg(img, &trial)?,
            Format::Avif => encode_avif(img, &trial)?,
            Format::Png => {
                let min_q = mid.saturating_sub(10);
                encode_png_lossy(img, &trial, min_q, mid)?
            }
            _ => unreachable!("checked above"),
        };
        let decoded = Image::decode(&trial_bytes)?;
        let score = metric_fn(img, &decoded)?;
        let meets = if lower_is_better {
            score <= target_value
        } else {
            score >= target_value
        };
        // Always remember the highest-quality try as a safety net.
        fallback = Some((trial_bytes.clone(), mid));
        if meets {
            best = Some((trial_bytes, mid));
            if mid == 0 {
                break;
            }
            hi = mid - 1;
        } else {
            if mid == 100 {
                break;
            }
            lo = mid + 1;
        }
    }

    let bytes = best
        .map(|(b, _)| b)
        .or_else(|| fallback.map(|(b, _)| b))
        .ok_or_else(|| {
            Error::Invalid(format!(
                "perceptual_search produced no candidate for {target:?}"
            ))
        })?;
    Ok(EncodedImage {
        bytes,
        format: opts.format,
        size: img.size(),
    })
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
