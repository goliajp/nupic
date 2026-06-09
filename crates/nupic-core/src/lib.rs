//! nupic-core — ceiling-first image-operation API.
//!
//! # Design stance
//!
//! The public surface here is designed for the **end-state** — research-grade
//! self-built pipelines, perceptual quality targets, future container formats
//! (AVIF / JPEG XL). Today's implementations are stubs (`NotImplemented` /
//! mature-crate wrappers); the signatures are intended to stay.
//!
//! # Opacity
//!
//! [`Image`] is an opaque newtype. The current internal representation is
//! `image::DynamicImage`, but this is not promised. Callers who need pixel
//! access go through inherent methods, not by reaching for the underlying
//! type. This is what makes the eventual swap-in of self-built pipelines a
//! pure implementation change.
//!
//! # Extensibility
//!
//! Format / Filter / FitMode / Position / Quality / WatermarkContent are all
//! `#[non_exhaustive]`. New variants (e.g. `Format::Jxl`, new perceptual
//! targets) can be added in minor versions without breaking downstream
//! callers' `match` blocks.

mod color;
mod error;
mod font;
mod format;
mod geom;
mod image_handle;
mod text;

pub mod detect;
pub mod metrics;
pub mod ops;

pub use color::Color;
pub use error::{Error, Result};
pub use font::Font;
pub use format::{Filter, FitMode, Format, Position};
pub use geom::{Point, Rect, Size};
pub use image_handle::Image;

pub use metrics::Metric;
pub use ops::circle::CircleOpts;
pub use ops::compress::{CompressOpts, EncodedImage, PerceptualTarget, Quality};
pub use ops::fit::FitOpts;
pub use ops::mock::{MockOpts, MockStyle};
pub use ops::resize::{ResizeMode, ResizeOpts};
pub use ops::watermark::{WatermarkContent, WatermarkOpts};
