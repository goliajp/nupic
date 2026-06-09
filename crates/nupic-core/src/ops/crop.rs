use crate::error::{Error, Result};
use crate::geom::Rect;
use crate::image_handle::Image;

#[derive(Copy, Clone, Debug)]
pub struct CropOpts {
    pub rect: Rect,
}

impl CropOpts {
    pub fn new(rect: Rect) -> Self {
        Self { rect }
    }
}

pub fn crop(img: Image, opts: CropOpts) -> Result<Image> {
    let inner = img.into_inner();
    let (iw, ih) = (inner.width() as i32, inner.height() as i32);
    let r = opts.rect;
    let x1 = r.left().clamp(0, iw);
    let y1 = r.top().clamp(0, ih);
    let x2 = r.right().clamp(0, iw);
    let y2 = r.bottom().clamp(0, ih);
    if x2 <= x1 || y2 <= y1 {
        return Err(Error::Invalid(format!(
            "crop rect produced an empty result: rect={r:?}, image={iw}×{ih}"
        )));
    }
    let cw = (x2 - x1) as u32;
    let ch = (y2 - y1) as u32;
    let cropped = inner.crop_imm(x1 as u32, y1 as u32, cw, ch);
    Ok(Image::from_inner(cropped))
}
