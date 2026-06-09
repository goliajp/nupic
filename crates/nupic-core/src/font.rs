//! User-supplied font handles for text rendering.
//!
//! `Font` wraps the raw TTF / OTF bytes in an `Arc` so it's cheap to clone
//! and thread-safe. A [`Font::default_font`] handle yields the bundled
//! **Source Sans 3 Regular** (Latin-only). For CJK / emoji / custom typography,
//! load any TTF/OTF with [`Font::from_path`] or [`Font::from_bytes`].

use std::path::Path;
use std::sync::{Arc, OnceLock};

use crate::error::{Error, Result};

const BUNDLED_FONT: &[u8] = include_bytes!("../assets/SourceSans3-Regular.ttf");

#[derive(Clone, Debug)]
pub struct Font {
    bytes: Arc<Vec<u8>>,
}

impl Font {
    /// The bundled default font (Source Sans 3 Regular, SIL OFL 1.1).
    pub fn default_font() -> Self {
        static ARC: OnceLock<Arc<Vec<u8>>> = OnceLock::new();
        let arc = ARC.get_or_init(|| Arc::new(BUNDLED_FONT.to_vec()));
        Self {
            bytes: Arc::clone(arc),
        }
    }

    /// Load a font from a TTF / OTF file on disk.
    ///
    /// Returns `Error::Codec` if the file isn't a parseable font.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(bytes)
    }

    /// Wrap caller-owned TTF / OTF bytes.
    ///
    /// Validates at construction by parsing once; subsequent rasterization
    /// can skip the validity check.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        ab_glyph::FontVec::try_from_vec(bytes.clone())
            .map_err(|e| Error::Codec(Box::new(e)))?;
        Ok(Self {
            bytes: Arc::new(bytes),
        })
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        self.bytes.as_slice()
    }
}

impl Default for Font {
    fn default() -> Self {
        Self::default_font()
    }
}
