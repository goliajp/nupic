//! Pixel-space filters: tonal adjustments, color conversions, blur/sharpen.
//!
//! Every variant is a thin wrap over `image::imageops` today; the public
//! surface is `#[non_exhaustive]` so future research-grade filters
//! (perceptual-aware sharpen, learned denoise, etc.) slot in without
//! breaking callers.

use image::{DynamicImage, imageops};

use crate::error::{Error, Result};
use crate::image_handle::Image;

/// Built-in filter selector.
///
/// `#[non_exhaustive]` — new filters arrive without SemVer breakage.
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FilterKind {
    Grayscale,
    Invert,
    /// Gaussian blur with `amount` as sigma in pixels.
    Blur,
    /// Unsharp-mask sharpening. `amount` = sigma; threshold defaults to 1.
    Sharpen,
    /// Brightness shift: `amount` ∈ [-255, 255] (added to each channel).
    Brightness,
    /// Contrast scale: `amount` ∈ [-100, 100] (% adjustment, applied perceptually).
    Contrast,
    /// Hue rotation in degrees.
    Hue,
}

#[derive(Copy, Clone, Debug)]
pub struct FilterOpts {
    pub kind: FilterKind,
    pub amount: f32,
}

impl FilterOpts {
    pub fn new(kind: FilterKind) -> Self {
        Self { kind, amount: default_amount(kind) }
    }

    pub fn with_amount(mut self, amount: f32) -> Self {
        self.amount = amount;
        self
    }
}

fn default_amount(kind: FilterKind) -> f32 {
    match kind {
        FilterKind::Grayscale | FilterKind::Invert => 0.0,
        FilterKind::Blur => 1.5,
        FilterKind::Sharpen => 1.5,
        FilterKind::Brightness => 20.0,
        FilterKind::Contrast => 20.0,
        FilterKind::Hue => 90.0,
    }
}

pub fn filter(img: Image, opts: FilterOpts) -> Result<Image> {
    let inner = img.into_inner();
    let out = match opts.kind {
        FilterKind::Grayscale => DynamicImage::ImageRgba8(
            imageops::grayscale_alpha(&inner).convert(),
        ),
        FilterKind::Invert => {
            let mut rgba = inner.into_rgba8();
            imageops::invert(&mut rgba);
            DynamicImage::ImageRgba8(rgba)
        }
        FilterKind::Blur => {
            if !(opts.amount.is_finite() && opts.amount >= 0.0) {
                return Err(Error::Invalid(format!(
                    "blur sigma must be a non-negative finite number, got {}",
                    opts.amount
                )));
            }
            DynamicImage::ImageRgba8(imageops::blur(&inner.into_rgba8(), opts.amount))
        }
        FilterKind::Sharpen => {
            if !(opts.amount.is_finite() && opts.amount > 0.0) {
                return Err(Error::Invalid(format!(
                    "sharpen sigma must be > 0, got {}",
                    opts.amount
                )));
            }
            DynamicImage::ImageRgba8(imageops::unsharpen(&inner.into_rgba8(), opts.amount, 1))
        }
        FilterKind::Brightness => DynamicImage::ImageRgba8(imageops::brighten(
            &inner.into_rgba8(),
            opts.amount.round() as i32,
        )),
        FilterKind::Contrast => {
            DynamicImage::ImageRgba8(imageops::contrast(&inner.into_rgba8(), opts.amount))
        }
        FilterKind::Hue => DynamicImage::ImageRgba8(imageops::huerotate(
            &inner.into_rgba8(),
            opts.amount.round() as i32,
        )),
    };
    Ok(Image::from_inner(out))
}

/// Trait helper so `grayscale_alpha`'s `LumaA` output can convert into RGBA
/// in one call. (image crate's GrayAlphaImage → DynamicImage path.)
trait Convertible {
    fn convert(self) -> image::RgbaImage;
}

impl Convertible for image::GrayAlphaImage {
    fn convert(self) -> image::RgbaImage {
        let (w, h) = (self.width(), self.height());
        let mut out = image::RgbaImage::new(w, h);
        for (x, y, p) in self.enumerate_pixels() {
            let l = p.0[0];
            let a = p.0[1];
            out.put_pixel(x, y, image::Rgba([l, l, l, a]));
        }
        out
    }
}
