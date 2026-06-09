use image::{DynamicImage, Rgba, RgbaImage};

use crate::color::Color;
use crate::error::{Error, Result};
use crate::geom::Size;
use crate::image_handle::Image;
use crate::text;

/// Placeholder visual style.
///
/// `#[non_exhaustive]` — additional styles may be added.
#[derive(Copy, Clone, Debug)]
#[non_exhaustive]
pub enum MockStyle {
    /// Background color with a very faint 45° diagonal-stripe overlay.
    /// The default — reads as soft drafting / wireframe paper.
    Stripes,
    /// Flat `background`. No texture.
    Solid,
    /// Vertical fade from `background` (top) to a darker tint (bottom).
    Gradient,
    /// Subtle two-tone checkerboard; `tile` is the cell size in pixels.
    Checker { tile: u32 },
}

#[derive(Clone, Debug)]
pub struct MockOpts {
    pub size: Size,
    pub style: MockStyle,
    pub background: Color,
    pub foreground: Color,
    /// Override the main label. `None` = render `"<width> × <height>"`.
    pub text: Option<String>,
}

impl MockOpts {
    pub fn new(size: Size) -> Self {
        Self {
            size,
            style: MockStyle::Stripes,
            background: Color::rgb(0xf8, 0xfa, 0xfc), // slate-50
            foreground: Color::rgb(0x33, 0x41, 0x55), // slate-700
            text: None,
        }
    }
}

/// Render a placeholder image: textured background + centered dimensions label.
pub fn render(opts: MockOpts) -> Result<Image> {
    let (w, h) = (opts.size.width, opts.size.height);
    if w == 0 || h == 0 {
        return Err(Error::Invalid(format!(
            "mock size must be non-zero, got {w}×{h}"
        )));
    }
    let mut canvas = RgbaImage::new(w, h);
    fill_background(&mut canvas, &opts);
    draw_label(&mut canvas, &opts);
    Ok(Image::from_inner(DynamicImage::ImageRgba8(canvas)))
}

// ===== background =====

fn fill_background(canvas: &mut RgbaImage, opts: &MockOpts) {
    let (w, h) = (canvas.width(), canvas.height());
    let bg_px = color_to_rgba(opts.background);
    for p in canvas.pixels_mut() {
        *p = bg_px;
    }
    match opts.style {
        MockStyle::Solid => {}
        MockStyle::Stripes => paint_stripes(canvas, opts.foreground),
        MockStyle::Gradient => paint_gradient(canvas, opts.background, w, h),
        MockStyle::Checker { tile } => paint_checker(canvas, opts.background, tile, w, h),
    }
}

/// Faint 45° diagonal hatching. Single-pixel lines, ~14px spacing, ~6% alpha.
/// Reads as drafting-paper texture, never competes with the dimensions text.
fn paint_stripes(canvas: &mut RgbaImage, color: Color) {
    let (w, h) = (canvas.width(), canvas.height());
    let spacing: u32 = stripe_spacing(w, h);
    let alpha: f32 = 0.07;
    for y in 0..h {
        for x in 0..w {
            if (x + y) % spacing == 0 {
                text::blend_pixel(canvas, x as i32, y as i32, color, alpha);
            }
        }
    }
}

fn stripe_spacing(w: u32, h: u32) -> u32 {
    // ~1.5% of the smaller dimension, clamped to a tasteful range.
    let s = (w.min(h) / 60).max(10);
    s.min(20)
}

fn paint_gradient(canvas: &mut RgbaImage, bg: Color, w: u32, h: u32) {
    // Overlay a gradient *over* the already-painted flat bg, so we don't
    // touch each pixel twice.
    let bottom = darken(bg, 0.08);
    let denom = h.saturating_sub(1).max(1) as f32;
    for y in 0..h {
        let t = (y as f32) / denom;
        let row = lerp_color(bg, bottom, t);
        let px = color_to_rgba(row);
        for x in 0..w {
            canvas.put_pixel(x, y, px);
        }
    }
}

fn paint_checker(canvas: &mut RgbaImage, bg: Color, tile: u32, w: u32, h: u32) {
    let tile = tile.max(4);
    let dark = color_to_rgba(darken(bg, 0.04));
    for y in 0..h {
        let cy = y / tile;
        for x in 0..w {
            let cx = x / tile;
            if (cx + cy) % 2 == 1 {
                canvas.put_pixel(x, y, dark);
            }
        }
    }
}

// ===== centered dimensions label =====

fn draw_label(canvas: &mut RgbaImage, opts: &MockOpts) {
    let (w, h) = (canvas.width(), canvas.height());
    let label = opts
        .text
        .clone()
        .unwrap_or_else(|| format!("{} × {}", opts.size.width, opts.size.height));
    if label.is_empty() {
        return;
    }

    // Target the label to fill ~55% of the canvas width, capped by height.
    let max_w = (w as f32) * 0.55;
    let max_h = (h as f32) * 0.20;
    let mut px_size = ((h as f32) * 0.16).clamp(28.0, 160.0);
    while text::text_width(&label, px_size) > max_w && px_size > 12.0 {
        px_size -= 1.0;
    }
    let cap = text::cap_height(px_size);
    if cap > max_h {
        px_size *= max_h / cap;
    }
    let cap = text::cap_height(px_size);
    let text_w = text::text_width(&label, px_size);
    let baseline_offset = (text::ascent(px_size) - cap).round() as i32;

    let x = ((w as f32 - text_w) / 2.0).round() as i32;
    let y_cap_top = ((h as f32 - cap) / 2.0).round() as i32;
    let y = y_cap_top - baseline_offset;

    text::draw_text(canvas, &label, x, y, px_size, opts.foreground, 1.0);
}

// ===== color helpers =====

fn color_to_rgba(c: Color) -> Rgba<u8> {
    Rgba([c.r, c.g, c.b, c.a])
}

fn darken(c: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let factor = 1.0 - t;
    Color::rgba(
        ((c.r as f32) * factor).round().clamp(0.0, 255.0) as u8,
        ((c.g as f32) * factor).round().clamp(0.0, 255.0) as u8,
        ((c.b as f32) * factor).round().clamp(0.0, 255.0) as u8,
        c.a,
    )
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let lerp = |x: u8, y: u8| -> u8 {
        let xv = f32::from(x);
        let yv = f32::from(y);
        (xv + (yv - xv) * t).round().clamp(0.0, 255.0) as u8
    };
    Color::rgba(
        lerp(a.r, b.r),
        lerp(a.g, b.g),
        lerp(a.b, b.b),
        lerp(a.a, b.a),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_size_matches_opts() {
        let img = render(MockOpts::new(Size::new(133, 77))).unwrap();
        assert_eq!(img.size(), Size::new(133, 77));
    }

    #[test]
    fn zero_size_rejected() {
        let err = render(MockOpts::new(Size::new(0, 100))).unwrap_err();
        assert!(matches!(err, Error::Invalid(_)));
        let err = render(MockOpts::new(Size::new(100, 0))).unwrap_err();
        assert!(matches!(err, Error::Invalid(_)));
    }

    #[test]
    fn custom_text_does_not_panic() {
        let mut opts = MockOpts::new(Size::new(200, 100));
        opts.text = Some("custom label".to_string());
        let img = render(opts).unwrap();
        assert_eq!(img.size(), Size::new(200, 100));
    }

    #[test]
    fn every_style_renders_at_requested_size() {
        for style in [
            MockStyle::Stripes,
            MockStyle::Solid,
            MockStyle::Gradient,
            MockStyle::Checker { tile: 8 },
        ] {
            let mut opts = MockOpts::new(Size::new(120, 80));
            opts.style = style;
            let img = render(opts).unwrap_or_else(|e| panic!("{style:?} failed: {e:?}"));
            assert_eq!(img.size(), Size::new(120, 80));
        }
    }

    #[test]
    fn tiny_size_skips_decoration_but_still_renders() {
        // Below the decoration / brand thresholds the function still produces
        // a valid image of the requested size.
        let img = render(MockOpts::new(Size::new(40, 20))).unwrap();
        assert_eq!(img.size(), Size::new(40, 20));
    }
}
