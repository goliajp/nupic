//! Detection operations: bbox capture, salience maps, content-aware crop.
//!
//! v0.3 ships **alpha-bbox**: find the tightest rectangle around pixels
//! whose alpha exceeds a threshold. Future work (saliency, learned
//! detection) slots in alongside without API churn.

use crate::error::{Error, Result};
use crate::geom::Rect;
use crate::image_handle::Image;

#[derive(Copy, Clone, Debug)]
pub struct AlphaBboxOpts {
    /// Alpha threshold (0..=255). Pixels with alpha *strictly greater* than
    /// this value are considered "content". Default 0 (any non-transparent).
    pub threshold: u8,
}

impl Default for AlphaBboxOpts {
    fn default() -> Self {
        Self { threshold: 0 }
    }
}

/// Find the tightest rectangle enclosing the input's non-transparent pixels.
///
/// Returns `Error::Invalid` if no pixel exceeds the threshold (i.e. the
/// image is entirely below it).
pub fn alpha_bbox(img: &Image, opts: AlphaBboxOpts) -> Result<Rect> {
    let inner = img.inner();
    let rgba = inner.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let threshold = opts.threshold;

    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    let mut any = false;

    for (x, y, p) in rgba.enumerate_pixels() {
        if p.0[3] > threshold {
            if x < min_x {
                min_x = x;
            }
            if y < min_y {
                min_y = y;
            }
            if x > max_x {
                max_x = x;
            }
            if y > max_y {
                max_y = y;
            }
            any = true;
        }
    }

    if !any {
        return Err(Error::Invalid(format!(
            "bbox: no pixel exceeds alpha threshold {threshold}"
        )));
    }
    let _ = (w, h);
    Ok(Rect::from_xywh(
        min_x as i32,
        min_y as i32,
        max_x - min_x + 1,
        max_y - min_y + 1,
    ))
}
