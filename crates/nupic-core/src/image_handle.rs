use std::path::Path;

use crate::error::Result;
use crate::geom::Size;
use crate::ops::{circle, compress, crop, filter, fit, resize, watermark};

/// In-memory image handle.
///
/// Opaque. The current internal representation wraps `image::DynamicImage`,
/// but this is not part of the stable contract. Treat `Image` as a handle
/// and reach for the inherent methods below.
#[derive(Clone, Debug)]
pub struct Image {
    inner: image::DynamicImage,
}

impl Image {
    /// Decode an image from a filesystem path. Format is inferred from
    /// content + extension.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let inner = image::open(path)?;
        Ok(Self { inner })
    }

    /// Decode an image from an in-memory byte slice. Format is inferred from
    /// the content signature.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let inner = image::load_from_memory(bytes)?;
        Ok(Self { inner })
    }

    /// Write the image to `path`, picking an encoder from the extension.
    /// For full control over quality / metadata, use [`Image::compress`].
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        self.inner.save(path)?;
        Ok(())
    }

    pub fn width(&self) -> u32 {
        self.inner.width()
    }

    pub fn height(&self) -> u32 {
        self.inner.height()
    }

    pub fn size(&self) -> Size {
        Size::new(self.width(), self.height())
    }

    /// Resize the image. See [`crate::ResizeOpts`].
    pub fn resize(&self, opts: resize::ResizeOpts) -> Result<Self> {
        resize::resize(self.clone(), opts)
    }

    /// Fit the image into a target box. See [`crate::FitOpts`].
    pub fn fit(&self, opts: fit::FitOpts) -> Result<Self> {
        fit::fit(self.clone(), opts)
    }

    /// Mask the image into a circle. See [`crate::CircleOpts`].
    pub fn circle(&self, opts: circle::CircleOpts) -> Result<Self> {
        circle::circle(self.clone(), opts)
    }

    /// Crop to a rectangle. See [`crate::CropOpts`].
    pub fn crop(&self, opts: crop::CropOpts) -> Result<Self> {
        crop::crop(self.clone(), opts)
    }

    /// Apply a pixel-space filter (blur, sharpen, color adjustments, …).
    /// See [`crate::FilterOpts`].
    pub fn filter(&self, opts: filter::FilterOpts) -> Result<Self> {
        filter::filter(self.clone(), opts)
    }

    /// Overlay a watermark. See [`crate::WatermarkOpts`].
    pub fn watermark(&self, opts: watermark::WatermarkOpts) -> Result<Self> {
        watermark::watermark(self.clone(), opts)
    }

    /// Encode the image with format-aware compression. See [`crate::CompressOpts`].
    pub fn compress(&self, opts: compress::CompressOpts) -> Result<compress::EncodedImage> {
        compress::encode(self, opts)
    }
}

// Crate-internal access for op implementations. NOT part of the public API.
// Allowed dead_code: ops are stubbed today; these accessors light up as the
// cement-layer implementations land.
#[allow(dead_code)]
impl Image {
    pub(crate) fn inner(&self) -> &image::DynamicImage {
        &self.inner
    }

    pub(crate) fn into_inner(self) -> image::DynamicImage {
        self.inner
    }

    pub(crate) fn from_inner(inner: image::DynamicImage) -> Self {
        Self { inner }
    }
}
