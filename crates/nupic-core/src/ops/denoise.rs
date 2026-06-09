//! Denoising filters.
//!
//! v0.3 ships **Gaussian** (low-sigma blur) and **Median** (canonical edge-
//! preserving classical denoise). Future research-grade variants
//! (bilateral, non-local means, learned) arrive without breaking the API
//! because [`DenoiseKind`] is `#[non_exhaustive]`.

use image::{DynamicImage, Rgba, RgbaImage, imageops};

use crate::error::{Error, Result};
use crate::image_handle::Image;

#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DenoiseKind {
    /// Gaussian smoothing. `strength` = sigma in pixels.
    Gaussian,
    /// Median window. `strength` = window radius in pixels (1 → 3×3, 2 → 5×5, …).
    Median,
}

#[derive(Copy, Clone, Debug)]
pub struct DenoiseOpts {
    pub kind: DenoiseKind,
    pub strength: f32,
}

impl DenoiseOpts {
    pub fn new(kind: DenoiseKind) -> Self {
        let strength = match kind {
            DenoiseKind::Gaussian => 1.0,
            DenoiseKind::Median => 1.0,
        };
        Self { kind, strength }
    }

    pub fn with_strength(mut self, strength: f32) -> Self {
        self.strength = strength;
        self
    }
}

pub fn denoise(img: Image, opts: DenoiseOpts) -> Result<Image> {
    let inner = img.into_inner();
    let out = match opts.kind {
        DenoiseKind::Gaussian => {
            if !(opts.strength.is_finite() && opts.strength >= 0.0) {
                return Err(Error::Invalid(format!(
                    "denoise gaussian sigma must be non-negative, got {}",
                    opts.strength
                )));
            }
            DynamicImage::ImageRgba8(imageops::blur(&inner.into_rgba8(), opts.strength))
        }
        DenoiseKind::Median => {
            let radius = opts.strength.round() as i32;
            if !(0..=10).contains(&radius) {
                return Err(Error::Invalid(format!(
                    "denoise median radius must be 0..=10, got {radius}"
                )));
            }
            DynamicImage::ImageRgba8(median_filter(&inner.into_rgba8(), radius as u32))
        }
    };
    Ok(Image::from_inner(out))
}

/// Per-channel median filter over a `(2r+1) × (2r+1)` window. Hand-rolled,
/// no extra dep. Alpha channel is passed through (median of alpha makes
/// edges of transparent shapes look better than averaging).
fn median_filter(src: &RgbaImage, radius: u32) -> RgbaImage {
    if radius == 0 {
        return src.clone();
    }
    let (w, h) = (src.width(), src.height());
    let mut out = RgbaImage::new(w, h);
    let r = radius as i32;
    let window = (2 * radius + 1) * (2 * radius + 1);
    let half = (window / 2) as usize;
    let mut buf_r = Vec::with_capacity(window as usize);
    let mut buf_g = Vec::with_capacity(window as usize);
    let mut buf_b = Vec::with_capacity(window as usize);
    let mut buf_a = Vec::with_capacity(window as usize);
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            buf_r.clear();
            buf_g.clear();
            buf_b.clear();
            buf_a.clear();
            for dy in -r..=r {
                let sy = (y + dy).clamp(0, h as i32 - 1) as u32;
                for dx in -r..=r {
                    let sx = (x + dx).clamp(0, w as i32 - 1) as u32;
                    let p = src.get_pixel(sx, sy);
                    buf_r.push(p.0[0]);
                    buf_g.push(p.0[1]);
                    buf_b.push(p.0[2]);
                    buf_a.push(p.0[3]);
                }
            }
            // partial sort: select the median via slice::select_nth_unstable
            let (_, med_r, _) = buf_r.select_nth_unstable(half);
            let (_, med_g, _) = buf_g.select_nth_unstable(half);
            let (_, med_b, _) = buf_b.select_nth_unstable(half);
            let (_, med_a, _) = buf_a.select_nth_unstable(half);
            out.put_pixel(x as u32, y as u32, Rgba([*med_r, *med_g, *med_b, *med_a]));
        }
    }
    out
}
