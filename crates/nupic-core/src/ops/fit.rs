use image::{DynamicImage, GenericImage, GenericImageView, Rgba, RgbaImage};

use crate::color::Color;
use crate::error::{Error, Result};
use crate::format::{Filter, FitMode};
use crate::geom::Size;
use crate::image_handle::Image;
use crate::ops::resize::resize_rgba;

#[derive(Copy, Clone, Debug)]
pub struct FitOpts {
    pub size: Size,
    pub mode: FitMode,
    pub filter: Filter,
    /// Background color used to fill padding when `mode = Contain`.
    pub background: Color,
}

impl FitOpts {
    pub fn new(size: Size, mode: FitMode) -> Self {
        Self {
            size,
            mode,
            filter: Filter::Lanczos3,
            background: Color::TRANSPARENT,
        }
    }

    pub fn with_filter(mut self, filter: Filter) -> Self {
        self.filter = filter;
        self
    }

    pub fn with_background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }
}

pub fn fit(img: Image, opts: FitOpts) -> Result<Image> {
    let (in_w, in_h) = (img.width(), img.height());
    let box_w = opts.size.width;
    let box_h = opts.size.height;
    if box_w == 0 || box_h == 0 {
        return Err(Error::Invalid(format!(
            "fit target size must be non-zero, got {box_w}×{box_h}"
        )));
    }
    if in_w == 0 || in_h == 0 {
        return Err(Error::Invalid("fit input image has zero area".into()));
    }
    let inner = img.into_inner();

    let result = match opts.mode {
        FitMode::Fill => {
            let scaled = resize_rgba(inner, box_w, box_h, opts.filter)?;
            DynamicImage::ImageRgba8(scaled)
        }
        FitMode::Inside => {
            if in_w <= box_w && in_h <= box_h {
                // sharp.Inside: do not upscale.
                inner
            } else {
                let (nw, nh) = scale_contain(in_w, in_h, box_w, box_h);
                let scaled = resize_rgba(inner, nw, nh, opts.filter)?;
                DynamicImage::ImageRgba8(scaled)
            }
        }
        FitMode::Outside => {
            if in_w >= box_w && in_h >= box_h {
                // sharp.Outside: do not downscale.
                inner
            } else {
                let (nw, nh) = scale_cover(in_w, in_h, box_w, box_h);
                let scaled = resize_rgba(inner, nw, nh, opts.filter)?;
                DynamicImage::ImageRgba8(scaled)
            }
        }
        FitMode::Contain => {
            let (nw, nh) = scale_contain(in_w, in_h, box_w, box_h);
            let scaled = resize_rgba(inner, nw, nh, opts.filter)?;
            DynamicImage::ImageRgba8(pad_to(scaled, box_w, box_h, opts.background))
        }
        FitMode::Cover => {
            let (nw, nh) = scale_cover(in_w, in_h, box_w, box_h);
            let scaled = resize_rgba(inner, nw, nh, opts.filter)?;
            DynamicImage::ImageRgba8(crop_center(scaled, box_w, box_h))
        }
    };

    Ok(Image::from_inner(result))
}

/// Scale `in_*` to fit inside `box_*`, preserving aspect ratio. Result fits inside.
pub(crate) fn scale_contain(in_w: u32, in_h: u32, box_w: u32, box_h: u32) -> (u32, u32) {
    let r = f64::min(
        f64::from(box_w) / f64::from(in_w),
        f64::from(box_h) / f64::from(in_h),
    );
    (
        (f64::from(in_w) * r).round().max(1.0) as u32,
        (f64::from(in_h) * r).round().max(1.0) as u32,
    )
}

/// Scale `in_*` to cover `box_*`, preserving aspect ratio. Result covers box.
pub(crate) fn scale_cover(in_w: u32, in_h: u32, box_w: u32, box_h: u32) -> (u32, u32) {
    let r = f64::max(
        f64::from(box_w) / f64::from(in_w),
        f64::from(box_h) / f64::from(in_h),
    );
    (
        (f64::from(in_w) * r).round().max(1.0) as u32,
        (f64::from(in_h) * r).round().max(1.0) as u32,
    )
}

/// Place `src` centered inside a `box_w × box_h` canvas filled with `bg`.
pub(crate) fn pad_to(src: RgbaImage, box_w: u32, box_h: u32, bg: Color) -> RgbaImage {
    let mut canvas = RgbaImage::from_pixel(box_w, box_h, color_to_rgba(bg));
    let dx = (box_w.saturating_sub(src.width())) / 2;
    let dy = (box_h.saturating_sub(src.height())) / 2;
    image::imageops::overlay(&mut canvas, &src, i64::from(dx), i64::from(dy));
    canvas
}

/// Center-crop `src` to `box_w × box_h`. Assumes `src >= box` on both axes.
pub(crate) fn crop_center(src: RgbaImage, box_w: u32, box_h: u32) -> RgbaImage {
    let (sw, sh) = (src.width(), src.height());
    let cw = box_w.min(sw);
    let ch = box_h.min(sh);
    let x = (sw - cw) / 2;
    let y = (sh - ch) / 2;
    let mut out = RgbaImage::new(cw, ch);
    let view = src.view(x, y, cw, ch);
    out.copy_from(&*view, 0, 0)
        .expect("crop view fits in destination");
    out
}

fn color_to_rgba(c: Color) -> Rgba<u8> {
    Rgba([c.r, c.g, c.b, c.a])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contain_preserves_aspect_inside_box() {
        // 800×600 → into 400×400 box. r = min(0.5, 0.667) = 0.5. → 400×300.
        assert_eq!(scale_contain(800, 600, 400, 400), (400, 300));
    }

    #[test]
    fn cover_preserves_aspect_outside_box() {
        // 800×600 → cover 400×400 box. r = max(0.5, 0.667) = 0.667. → 533×400.
        assert_eq!(scale_cover(800, 600, 400, 400), (533, 400));
    }

    #[test]
    fn contain_landscape_into_square() {
        assert_eq!(scale_contain(1000, 500, 200, 200), (200, 100));
    }

    #[test]
    fn cover_portrait_into_square() {
        assert_eq!(scale_cover(500, 1000, 200, 200), (200, 400));
    }
}
