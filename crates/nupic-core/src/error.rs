use crate::format::Format;

pub type Result<T> = std::result::Result<T, Error>;

/// Errors returned by nupic-core.
///
/// `#[non_exhaustive]` — new variants may be added in minor versions.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A codec-layer failure. Source error is downcast-able for diagnostics,
    /// but the concrete type is not part of the stable API.
    #[error("codec error: {0}")]
    Codec(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),

    #[error("unsupported format: {0:?}")]
    UnsupportedFormat(Format),

    #[error("invalid color: {0}")]
    InvalidColor(String),

    #[error("invalid input: {0}")]
    Invalid(String),

    /// The requested operation is part of the API surface but not implemented
    /// yet. Will disappear as the cement-layer implementations land.
    #[error("not yet implemented: {0}")]
    NotImplemented(&'static str),
}

impl From<image::ImageError> for Error {
    fn from(e: image::ImageError) -> Self {
        Self::Codec(Box::new(e))
    }
}
