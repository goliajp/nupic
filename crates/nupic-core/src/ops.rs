//! Image operations.
//!
//! Each submodule defines an `Opts` struct + a free function. The free
//! function is the canonical entry point; convenience methods on [`Image`]
//! (see `image_handle.rs`) delegate here so that callers can fluently chain
//! `img.resize(...)?.fit(...)?.compress(...)?`.
//!
//! Implementations are stubbed (`Error::NotImplemented`) until the
//! cement-layer versions land. The public signatures are intended to stay
//! across the later swap-in of self-built pipelines.
//!
//! [`Image`]: crate::Image

pub mod circle;
pub mod compress;
pub mod fit;
pub mod mock;
pub mod resize;
pub mod watermark;
