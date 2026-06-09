//! TTF text rasterization for mockup labels and text watermarks.
//!
//! Every entry point takes a `&Font` so callers can swap in CJK / display /
//! custom fonts via `Font::from_path` without touching nupic-core.

use ab_glyph::{Font as _, FontRef, PxScale, ScaleFont, point};
use image::{Rgba, RgbaImage};

use crate::color::Color;
use crate::font::Font;

fn font_ref(font: &Font) -> FontRef<'_> {
    FontRef::try_from_slice(font.as_slice())
        .expect("Font bytes were validated at construction")
}

/// Width in pixels of a single line of `text` rasterized at `px_size`.
pub(crate) fn text_width(font: &Font, text: &str, px_size: f32) -> f32 {
    let f = font_ref(font);
    let s = f.as_scaled(PxScale::from(px_size));
    let mut w = 0.0f32;
    for c in text.chars() {
        w += s.h_advance(f.glyph_id(c));
    }
    w
}

/// Distance from baseline to top edge (positive, in pixels).
pub(crate) fn ascent(font: &Font, px_size: f32) -> f32 {
    let f = font_ref(font);
    f.as_scaled(PxScale::from(px_size)).ascent()
}

/// Cap height: the height of capital letters / digits in pixels. Used for
/// centering text optically rather than by full em-height.
pub(crate) fn cap_height(font: &Font, px_size: f32) -> f32 {
    let f = font_ref(font);
    let scaled = f.as_scaled(PxScale::from(px_size));
    let id = f.glyph_id('H');
    if let Some(outline) = f.outline_glyph(id.with_scale_and_position(
        PxScale::from(px_size),
        point(0.0, scaled.ascent()),
    )) {
        let b = outline.px_bounds();
        (b.max.y - b.min.y).abs()
    } else {
        scaled.height() * 0.7
    }
}

/// Rasterize `text` onto `canvas` with its top-left corner at `(x, y)` and
/// the glyphs alpha-blended in `color` (modulated by `alpha_factor`).
pub(crate) fn draw_text(
    canvas: &mut RgbaImage,
    text: &str,
    x: i32,
    y: i32,
    px_size: f32,
    color: Color,
    alpha_factor: f32,
    font: &Font,
) {
    let f = font_ref(font);
    let scale = PxScale::from(px_size);
    let scaled = f.as_scaled(scale);
    let mut cursor_x = x as f32;
    let baseline_y = y as f32 + scaled.ascent();
    let alpha_factor = alpha_factor.clamp(0.0, 1.0);
    for c in text.chars() {
        let glyph_id = f.glyph_id(c);
        let glyph = glyph_id.with_scale_and_position(scale, point(cursor_x, baseline_y));
        if let Some(outline) = f.outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            let base_x = bounds.min.x.floor() as i32;
            let base_y = bounds.min.y.floor() as i32;
            outline.draw(|px, py, coverage| {
                let cx = base_x + px as i32;
                let cy = base_y + py as i32;
                blend_pixel(canvas, cx, cy, color, alpha_factor * coverage);
            });
        }
        cursor_x += scaled.h_advance(glyph_id);
    }
}

/// Alpha-over blend a single pixel. Public to the crate so other ops can
/// share the same blending path (e.g. circle's antialiased ring,
/// watermark image composite).
pub(crate) fn blend_pixel(canvas: &mut RgbaImage, x: i32, y: i32, color: Color, alpha: f32) {
    if alpha <= 0.0 || x < 0 || y < 0 {
        return;
    }
    let (w, h) = (canvas.width() as i32, canvas.height() as i32);
    if x >= w || y >= h {
        return;
    }
    let src_a = (f32::from(color.a) / 255.0) * alpha;
    if src_a <= 0.0 {
        return;
    }
    let p = canvas.get_pixel_mut(x as u32, y as u32);
    let dst_a = f32::from(p.0[3]) / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= 0.0 {
        return;
    }
    let blend = |s: u8, d: u8| -> u8 {
        let sv = f32::from(s) / 255.0;
        let dv = f32::from(d) / 255.0;
        let out = (sv * src_a + dv * dst_a * (1.0 - src_a)) / out_a;
        (out * 255.0).round().clamp(0.0, 255.0) as u8
    };
    *p = Rgba([
        blend(color.r, p.0[0]),
        blend(color.g, p.0[1]),
        blend(color.b, p.0[2]),
        (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
    ]);
}
