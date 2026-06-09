use std::path::Path;

/// Resampling filter for resize / fit.
///
/// `#[non_exhaustive]` — perceptually-optimized / learned filters may be added.
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Filter {
    Nearest,
    Triangle,
    CatmullRom,
    Gaussian,
    Lanczos3,
}

/// How an image is positioned inside a target box. Mirrors CSS `object-fit`
/// plus `Inside` / `Outside` from `sharp`.
///
/// `#[non_exhaustive]` — additional modes may be added.
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FitMode {
    /// Scale to fit inside the box, preserving aspect ratio. Pads with `background`.
    Contain,
    /// Scale to cover the box, preserving aspect ratio. Crops overflow.
    Cover,
    /// Stretch to fill the box. Ignores aspect ratio.
    Fill,
    /// Resize only when the image exceeds the box on either dimension.
    Inside,
    /// Resize only when the image is smaller than the box on either dimension.
    Outside,
}

/// Anchor position for watermark placement, mockup label, etc.
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Position {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// Output image format.
///
/// `Auto` infers from the output path extension; otherwise falls back to the
/// input format.
///
/// `#[non_exhaustive]` — new containers slot in here without SemVer break.
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Format {
    Auto,
    Png,
    Jpeg,
    Webp,
    Gif,
    Bmp,
    Tiff,
    Avif,
    Jxl,
}

impl Format {
    /// Detect format from a path's extension. Returns `None` if the extension
    /// is missing or unrecognized.
    pub fn from_path(path: impl AsRef<Path>) -> Option<Self> {
        let ext = path.as_ref().extension()?.to_str()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "png" => Self::Png,
            "jpg" | "jpeg" => Self::Jpeg,
            "webp" => Self::Webp,
            "gif" => Self::Gif,
            "bmp" => Self::Bmp,
            "tif" | "tiff" => Self::Tiff,
            "avif" => Self::Avif,
            "jxl" => Self::Jxl,
            _ => return None,
        })
    }

    /// The canonical extension (no leading dot). `Auto` returns `""`.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Auto => "",
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::Gif => "gif",
            Self::Bmp => "bmp",
            Self::Tiff => "tiff",
            Self::Avif => "avif",
            Self::Jxl => "jxl",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_detection() {
        assert_eq!(Format::from_path("foo.png"), Some(Format::Png));
        assert_eq!(Format::from_path("foo.JPG"), Some(Format::Jpeg));
        assert_eq!(Format::from_path("foo.tiff"), Some(Format::Tiff));
        assert_eq!(Format::from_path("foo.tif"), Some(Format::Tiff));
        assert_eq!(Format::from_path("foo"), None);
        assert_eq!(Format::from_path("foo.unknown"), None);
    }

    #[test]
    fn extension_round_trips_for_every_concrete_format() {
        // For each non-Auto format, the canonical extension must map back.
        let concrete = [
            Format::Png,
            Format::Jpeg,
            Format::Webp,
            Format::Gif,
            Format::Bmp,
            Format::Tiff,
            Format::Avif,
            Format::Jxl,
        ];
        for f in concrete {
            let ext = f.extension();
            assert!(!ext.is_empty(), "{f:?} produced empty extension");
            let path = format!("name.{ext}");
            assert_eq!(
                Format::from_path(&path),
                Some(f),
                "round-trip failed: {f:?} → {ext:?} → ?"
            );
        }
    }

    #[test]
    fn auto_has_no_extension() {
        assert_eq!(Format::Auto.extension(), "");
    }
}
