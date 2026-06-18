//! Research crate — experiments backing `docs/research/`.
//!
//! Each binary here is paired with an essay. Add new experiments as
//! `examples/<topic>_<n>_<slug>.rs` (or `src/bin/...` if they need to
//! share helpers via this lib). Keep numbers measurable, write the
//! essay alongside, and graduate the code into `nupic-core` or a
//! dedicated stone crate (`nupic-bits`, `nupic-deflate`, ...) when an
//! approach becomes load-bearing.
//!
//! Modules:
//! - [`ssim_b1`] — Stone B baseline reimpl(FMA + `#[inline(always)]`
//!   applied,yuvxyb for color conversion). Backs essay 03b-bis.
//! - [`bench`] — shared spike helpers: pre-loaded v1.2.8 corpus-500
//!   baseline (no tinypng_dssim re-compute), stratified sample by
//!   pile, 4-core rayon pool default. See [[feedback-no-long-sweeps-in-workflow]].

pub mod ssim_b1;
pub mod codebook_c0;
pub mod bench;
