use image::{DynamicImage, Rgba, RgbaImage};

use crate::color::Color;
use crate::error::{Error, Result};
use crate::font::Font;
use crate::format::{Filter, Position};
use crate::image_handle::Image;
use crate::ops::resize::resize_rgba;
use crate::text;

/// Watermark payload.
///
/// `#[non_exhaustive]` — future variants (SVG vector, repeated tile, etc.) may be added.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum WatermarkContent {
    /// Rendered text. Font selection is the implementation's call until the
    /// API grows a `font` field.
    Text { text: String },
    /// Image overlay.
    Image(Image),
}

#[derive(Clone, Debug)]
pub struct WatermarkOpts {
    pub content: WatermarkContent,
    pub position: Position,
    /// `0.0` = invisible, `1.0` = fully opaque.
    pub opacity: f32,
    /// Margin from the anchor edge in pixels.
    pub margin: u32,
    /// Image-watermark scale, `0.0..=1.0` of the base image width.
    /// Ignored for text watermarks.
    pub scale: f32,
    /// Text-watermark color. Ignored for image watermarks.
    pub text_color: Color,
    /// Font used for text watermarks. Default = bundled Source Sans 3 Regular.
    pub font: Font,
}

impl WatermarkOpts {
    pub fn text(text: impl Into<String>) -> Self {
        Self::new(WatermarkContent::Text { text: text.into() })
    }

    pub fn image(image: Image) -> Self {
        Self::new(WatermarkContent::Image(image))
    }

    fn new(content: WatermarkContent) -> Self {
        Self {
            content,
            position: Position::BottomRight,
            opacity: 0.5,
            margin: 16,
            scale: 0.2,
            text_color: Color::WHITE,
            font: Font::default_font(),
        }
    }
}

pub fn watermark(img: Image, opts: WatermarkOpts) -> Result<Image> {
    let inner = img.into_inner();
    let mut canvas = inner.into_rgba8();
    let opacity = opts.opacity.clamp(0.0, 1.0);

    match opts.content {
        WatermarkContent::Text { text } => {
            draw_text_watermark(
                &mut canvas,
                &text,
                opts.position,
                opts.margin,
                opacity,
                opts.text_color,
                &opts.font,
            );
        }
        WatermarkContent::Image(overlay) => {
            draw_image_watermark(&mut canvas, overlay, opts.position, opts.margin, opts.scale, opacity)?;
        }
    }

    Ok(Image::from_inner(DynamicImage::ImageRgba8(canvas)))
}

fn draw_text_watermark(
    canvas: &mut RgbaImage,
    text: &str,
    position: Position,
    margin: u32,
    opacity: f32,
    color: Color,
    font: &Font,
) {
    let (cw, ch) = (canvas.width(), canvas.height());
    // Watermark text size: 3% of the canvas's smaller dimension, clamped.
    let px_size = (cw.min(ch) as f32 * 0.03).clamp(14.0, 64.0);
    let tw = text::text_width(font, text, px_size).ceil() as u32;
    let th = text::cap_height(font, px_size).ceil() as u32;
    let (x, y) = anchor_position(cw, ch, tw, th, position, margin);
    text::draw_text(canvas, text, x, y, px_size, color, opacity, font);
}

fn draw_image_watermark(
    canvas: &mut RgbaImage,
    overlay: Image,
    position: Position,
    margin: u32,
    scale: f32,
    opacity: f32,
) -> Result<()> {
    let (cw, ch) = (canvas.width(), canvas.height());
    let scale = scale.clamp(0.0, 1.0);
    if scale <= 0.0 {
        return Err(Error::Invalid(format!(
            "watermark scale must be > 0, got {scale}"
        )));
    }
    let overlay_inner = overlay.into_inner();
    let (ow, oh) = (overlay_inner.width(), overlay_inner.height());
    if ow == 0 || oh == 0 {
        return Err(Error::Invalid("watermark image has zero area".into()));
    }

    let target_w = ((cw as f32) * scale).round().max(1.0) as u32;
    let target_h = ((oh as f64) * (target_w as f64) / (ow as f64))
        .round()
        .max(1.0) as u32;
    let resized = resize_rgba(overlay_inner, target_w, target_h, Filter::Lanczos3)?;
    let (x, y) = anchor_position(cw, ch, target_w, target_h, position, margin);
    composite(canvas, &resized, x, y, opacity);
    Ok(())
}

fn anchor_position(cw: u32, ch: u32, ow: u32, oh: u32, pos: Position, margin: u32) -> (i32, i32) {
    let cw_i = cw as i32;
    let ch_i = ch as i32;
    let ow_i = ow as i32;
    let oh_i = oh as i32;
    let m = margin as i32;
    match pos {
        Position::TopLeft => (m, m),
        Position::TopCenter => ((cw_i - ow_i) / 2, m),
        Position::TopRight => (cw_i - ow_i - m, m),
        Position::CenterLeft => (m, (ch_i - oh_i) / 2),
        Position::Center => ((cw_i - ow_i) / 2, (ch_i - oh_i) / 2),
        Position::CenterRight => (cw_i - ow_i - m, (ch_i - oh_i) / 2),
        Position::BottomLeft => (m, ch_i - oh_i - m),
        Position::BottomCenter => ((cw_i - ow_i) / 2, ch_i - oh_i - m),
        Position::BottomRight => (cw_i - ow_i - m, ch_i - oh_i - m),
    }
}

/// Alpha-over composite `overlay` onto `canvas` at `(dx, dy)` with `opacity`
/// multiplying the source alpha.
fn composite(canvas: &mut RgbaImage, overlay: &RgbaImage, dx: i32, dy: i32, opacity: f32) {
    let (ow, oh) = (overlay.width(), overlay.height());
    let (cw, ch) = (canvas.width(), canvas.height());
    for oy in 0..oh {
        let cy = dy + (oy as i32);
        if cy < 0 {
            continue;
        }
        if cy as u32 >= ch {
            break;
        }
        for ox in 0..ow {
            let cx = dx + (ox as i32);
            if cx < 0 {
                continue;
            }
            if cx as u32 >= cw {
                break;
            }
            let src = overlay.get_pixel(ox, oy);
            let src_a = (f32::from(src.0[3]) / 255.0) * opacity;
            if src_a <= 0.0 {
                continue;
            }
            let dst = canvas.get_pixel_mut(cx as u32, cy as u32);
            let dst_a = f32::from(dst.0[3]) / 255.0;
            let out_a = src_a + dst_a * (1.0 - src_a);
            if out_a <= 0.0 {
                continue;
            }
            let blend = |s: u8, d: u8| -> u8 {
                let sv = f32::from(s) / 255.0;
                let dv = f32::from(d) / 255.0;
                let out = (sv * src_a + dv * dst_a * (1.0 - src_a)) / out_a;
                (out * 255.0).round().clamp(0.0, 255.0) as u8
            };
            *dst = Rgba([
                blend(src.0[0], dst.0[0]),
                blend(src.0[1], dst.0[1]),
                blend(src.0[2], dst.0[2]),
                (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
            ]);
        }
    }
}
