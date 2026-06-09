use image::DynamicImage;

use crate::error::{Error, Result};
use crate::image_handle::Image;

#[derive(Copy, Clone, Debug)]
pub struct CircleOpts {
    /// Circle radius in pixels. `None` = inscribed circle of the input image
    /// (i.e. `min(width, height) / 2`).
    pub radius: Option<u32>,
    /// Anti-aliasing feather width at the edge in pixels.
    pub feather: u32,
}

impl Default for CircleOpts {
    fn default() -> Self {
        Self {
            radius: None,
            feather: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Size;

    fn fixture(w: u32, h: u32) -> Image {
        let raw = image::RgbaImage::from_pixel(w, h, image::Rgba([100, 150, 200, 255]));
        Image::from_inner(image::DynamicImage::ImageRgba8(raw))
    }

    #[test]
    fn output_keeps_input_dimensions() {
        let img = fixture(80, 60);
        let out = circle(img, CircleOpts::default()).unwrap();
        assert_eq!(out.size(), Size::new(80, 60));
    }

    #[test]
    fn explicit_radius_keeps_dimensions() {
        let img = fixture(80, 60);
        let out = circle(
            img,
            CircleOpts {
                radius: Some(20),
                feather: 0,
            },
        )
        .unwrap();
        assert_eq!(out.size(), Size::new(80, 60));
    }

    #[test]
    fn zero_radius_rejected() {
        let img = fixture(80, 60);
        let err = circle(
            img,
            CircleOpts {
                radius: Some(0),
                feather: 0,
            },
        )
        .unwrap_err();
        assert!(matches!(err, Error::Invalid(_)), "got: {err:?}");
    }
}

pub fn circle(img: Image, opts: CircleOpts) -> Result<Image> {
    let inner = img.into_inner();
    let mut rgba = inner.into_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    if w == 0 || h == 0 {
        return Err(Error::Invalid("circle input image has zero area".into()));
    }

    let cx = (w as f32) / 2.0;
    let cy = (h as f32) / 2.0;
    let inscribed = (w.min(h) as f32) / 2.0;
    let r_outer = opts.radius.map(|v| v as f32).unwrap_or(inscribed);
    if r_outer <= 0.0 {
        return Err(Error::Invalid(format!(
            "circle radius must be > 0, got {r_outer}"
        )));
    }
    let feather = opts.feather as f32;
    let r_inner = (r_outer - feather).max(0.0);
    let feather_span = (r_outer - r_inner).max(1.0e-6);

    for y in 0..h {
        for x in 0..w {
            let dx = (x as f32 + 0.5) - cx;
            let dy = (y as f32 + 0.5) - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let alpha_factor = if dist <= r_inner {
                1.0
            } else if dist >= r_outer {
                0.0
            } else {
                1.0 - (dist - r_inner) / feather_span
            };
            if alpha_factor < 1.0 {
                let p = rgba.get_pixel_mut(x, y);
                let new_a = (f32::from(p.0[3]) * alpha_factor)
                    .round()
                    .clamp(0.0, 255.0) as u8;
                p.0[3] = new_a;
            }
        }
    }

    Ok(Image::from_inner(DynamicImage::ImageRgba8(rgba)))
}
