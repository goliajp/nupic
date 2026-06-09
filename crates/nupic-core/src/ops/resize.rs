use fast_image_resize as fr;
use image::DynamicImage;

use crate::error::{Error, Result};
use crate::format::Filter;
use crate::image_handle::Image;

/// How a resize specifies the target dimensions.
///
/// `#[non_exhaustive]` — new modes (content-aware, perceptual) may slot in.
#[derive(Copy, Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ResizeMode {
    /// Set exact width; height auto-derives from the input aspect ratio.
    Width(u32),
    /// Set exact height; width auto-derives from the input aspect ratio.
    Height(u32),
    /// Set both dimensions exactly. Stretches if the target aspect differs.
    Exact { width: u32, height: u32 },
    /// Scale both dimensions by a factor (preserves aspect ratio).
    Scale(f32),
}

#[derive(Copy, Clone, Debug)]
pub struct ResizeOpts {
    pub mode: ResizeMode,
    pub filter: Filter,
}

impl ResizeOpts {
    pub fn new(mode: ResizeMode) -> Self {
        Self {
            mode,
            filter: Filter::Lanczos3,
        }
    }

    pub fn with_filter(mut self, filter: Filter) -> Self {
        self.filter = filter;
        self
    }
}

pub fn resize(img: Image, opts: ResizeOpts) -> Result<Image> {
    let (in_w, in_h) = (img.width(), img.height());
    let (out_w, out_h) = resolve_target(in_w, in_h, opts.mode)?;
    if out_w == 0 || out_h == 0 {
        return Err(Error::Invalid(format!(
            "resize target dimension cannot be 0, computed {out_w}×{out_h}"
        )));
    }
    let inner = img.into_inner();
    let resized = resize_rgba(inner, out_w, out_h, opts.filter)?;
    Ok(Image::from_inner(DynamicImage::ImageRgba8(resized)))
}

/// Compute target (width, height) for a `ResizeMode` given the input dimensions.
pub(crate) fn resolve_target(in_w: u32, in_h: u32, mode: ResizeMode) -> Result<(u32, u32)> {
    let (w, h) = match mode {
        ResizeMode::Width(w) => {
            let h = ((u64::from(w) * u64::from(in_h)) / u64::from(in_w.max(1))).max(1) as u32;
            (w, h)
        }
        ResizeMode::Height(h) => {
            let w = ((u64::from(h) * u64::from(in_w)) / u64::from(in_h.max(1))).max(1) as u32;
            (w, h)
        }
        ResizeMode::Exact { width, height } => (width, height),
        ResizeMode::Scale(s) => {
            if !(s > 0.0 && s.is_finite()) {
                return Err(Error::Invalid(format!(
                    "resize scale must be a positive finite number, got {s}"
                )));
            }
            (
                (f64::from(in_w) * f64::from(s)).round().max(1.0) as u32,
                (f64::from(in_h) * f64::from(s)).round().max(1.0) as u32,
            )
        }
    };
    Ok((w, h))
}

/// Resize a [`DynamicImage`] to RGBA8 of the requested size using fast_image_resize.
pub(crate) fn resize_rgba(
    input: DynamicImage,
    out_w: u32,
    out_h: u32,
    filter: Filter,
) -> Result<image::RgbaImage> {
    let rgba = input.into_rgba8();
    let (in_w, in_h) = (rgba.width(), rgba.height());
    let raw = rgba.into_raw();

    let src = fr::images::Image::from_vec_u8(in_w, in_h, raw, fr::PixelType::U8x4)
        .map_err(|e| Error::Codec(Box::new(e)))?;
    let mut dst = fr::images::Image::new(out_w, out_h, fr::PixelType::U8x4);
    let alg = filter_to_alg(filter);
    let resize_opts = fr::ResizeOptions::new().resize_alg(alg);
    let mut resizer = fr::Resizer::new();
    resizer
        .resize(&src, &mut dst, &resize_opts)
        .map_err(|e| Error::Codec(Box::new(e)))?;

    image::RgbaImage::from_raw(out_w, out_h, dst.into_vec()).ok_or_else(|| {
        Error::Invalid(format!(
            "resize output buffer size mismatch for {out_w}×{out_h}"
        ))
    })
}

fn filter_to_alg(f: Filter) -> fr::ResizeAlg {
    match f {
        Filter::Nearest => fr::ResizeAlg::Nearest,
        Filter::Triangle => fr::ResizeAlg::Convolution(fr::FilterType::Bilinear),
        Filter::CatmullRom => fr::ResizeAlg::Convolution(fr::FilterType::CatmullRom),
        Filter::Gaussian => fr::ResizeAlg::Convolution(fr::FilterType::Gaussian),
        Filter::Lanczos3 => fr::ResizeAlg::Convolution(fr::FilterType::Lanczos3),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_from_width_preserves_aspect() {
        let (w, h) = resolve_target(800, 600, ResizeMode::Width(400)).unwrap();
        assert_eq!((w, h), (400, 300));
    }

    #[test]
    fn target_from_height_preserves_aspect() {
        let (w, h) = resolve_target(800, 600, ResizeMode::Height(300)).unwrap();
        assert_eq!((w, h), (400, 300));
    }

    #[test]
    fn target_exact_stretches() {
        let (w, h) = resolve_target(800, 600, ResizeMode::Exact { width: 100, height: 100 })
            .unwrap();
        assert_eq!((w, h), (100, 100));
    }

    #[test]
    fn target_scale_halves() {
        let (w, h) = resolve_target(800, 600, ResizeMode::Scale(0.5)).unwrap();
        assert_eq!((w, h), (400, 300));
    }

    #[test]
    fn target_rejects_nonfinite_scale() {
        assert!(resolve_target(800, 600, ResizeMode::Scale(0.0)).is_err());
        assert!(resolve_target(800, 600, ResizeMode::Scale(-1.0)).is_err());
        assert!(resolve_target(800, 600, ResizeMode::Scale(f32::NAN)).is_err());
    }
}
