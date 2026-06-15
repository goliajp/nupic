//! Perceptual and structural similarity metrics.
//!
//! v0.3 ships **DSSIM** (Kornel Lesiński's `dssim` crate, pure rust) as the
//! first working metric. **SSIMULACRA2** and **Butteraugli** signatures are
//! reserved here but currently return `Error::NotImplemented`; they land
//! when the self-built stone-layer perceptual pipeline does (see
//! `docs/roadmap.md` stage 4).
//!
//! The metrics are used by [`crate::ops::compress`] to drive
//! `Quality::Perceptual` quality-search loops, and exposed to CLI as
//! `nupic compare`.

use dssim::Dssim;
use imgref::ImgVec;
use rgb::{ComponentMap, RGBA};

use crate::error::{Error, Result};
use crate::image_handle::Image;

/// User-facing metric selector. Use with [`compute`] or the CLI's
/// `nupic compare --metric ...`.
///
/// `#[non_exhaustive]` — new metrics arrive here as they're implemented.
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Metric {
    Dssim,
    Ssimulacra2,
    Butteraugli,
}

impl Metric {
    /// Polarity of "better": `true` if a lower value means better quality.
    pub fn lower_is_better(self) -> bool {
        match self {
            Metric::Dssim | Metric::Butteraugli => true,
            Metric::Ssimulacra2 => false,
        }
    }
}

/// Compute `metric` between `reference` and `distorted`.
pub fn compute(metric: Metric, reference: &Image, distorted: &Image) -> Result<f64> {
    match metric {
        Metric::Dssim => dssim(reference, distorted),
        Metric::Ssimulacra2 => ssimulacra2(reference, distorted),
        Metric::Butteraugli => butteraugli(reference, distorted),
    }
}

/// Compute DSSIM between two same-sized images. Lower is better;
/// 0.0 means identical.
///
/// Typical interpretation:
/// - `< 0.005`  visually indistinguishable
/// - `< 0.02`   high quality
/// - `< 0.10`   noticeable artifacts
/// - `> 0.10`   strong degradation
pub fn dssim(reference: &Image, distorted: &Image) -> Result<f64> {
    if reference.size() != distorted.size() {
        return Err(Error::Invalid(format!(
            "DSSIM inputs must be the same size; got {:?} vs {:?}",
            reference.size(),
            distorted.size()
        )));
    }
    let ref_rgba = to_imgvec_rgba(reference);
    let dist_rgba = to_imgvec_rgba(distorted);

    let attr = Dssim::new();
    let ref_img = attr
        .create_image(&ref_rgba.as_ref())
        .ok_or_else(|| Error::Invalid("could not create DSSIM image from reference".into()))?;
    let dist_img = attr
        .create_image(&dist_rgba.as_ref())
        .ok_or_else(|| Error::Invalid("could not create DSSIM image from distorted".into()))?;
    let (val, _maps) = attr.compare(&ref_img, dist_img);
    Ok(f64::from(val))
}

/// SSIMULACRA2 score (higher is better, 0..=100). 0.5.0+ implementation
/// routes through the self-built [`nupic-ssimulacra`] stone crate,
/// which is bit-exact-class with the cement `ssimulacra2` v0.5.1 port
/// and ~21% faster on M2 via nested rayon. Typical thresholds:
/// - `≥ 90`  visually indistinguishable
/// - `≥ 70`  high quality
/// - `≥ 50`  medium quality
/// - `< 0`   catastrophic (well outside SSIMULACRA2's calibration band)
pub fn ssimulacra2(reference: &Image, distorted: &Image) -> Result<f64> {
    if reference.size() != distorted.size() {
        return Err(Error::Invalid(format!(
            "SSIMULACRA2 inputs must be the same size; got {:?} vs {:?}",
            reference.size(),
            distorted.size()
        )));
    }
    let ref_rgba = reference.inner().to_rgba8();
    let dist_rgba = distorted.inner().to_rgba8();
    let (w, h) = (ref_rgba.width(), ref_rgba.height());
    nupic_ssimulacra::ssimulacra2_score(
        ref_rgba.as_raw(),
        dist_rgba.as_raw(),
        w,
        h,
    )
    .map_err(|e| Error::Invalid(e.into()))
}

/// Butteraugli max-distance (lower is better). Reserved.
pub fn butteraugli(_reference: &Image, _distorted: &Image) -> Result<f64> {
    Err(Error::NotImplemented(
        "metrics::butteraugli — needs the stone-layer perceptual pipeline",
    ))
}

fn to_imgvec_rgba(img: &Image) -> ImgVec<RGBA<f32>> {
    let rgba = img.inner().to_rgba8();
    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
    let pixels: Vec<RGBA<f32>> = rgba
        .pixels()
        .map(|p| {
            RGBA {
                r: p.0[0],
                g: p.0[1],
                b: p.0[2],
                a: p.0[3],
            }
            .map(|c| f32::from(c) / 255.0)
        })
        .collect();
    ImgVec::new(pixels, w, h)
}
