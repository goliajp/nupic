//! `nupic-color` — self-built OKLab perceptual color space.
//!
//! Stone-layer crate (see `docs/research/png/03-perceptual-stone.md`).
//! Provides:
//!
//! - [`Oklab`] — three-channel f32 perceptual color (L, a, b)
//! - [`srgb_u8_to_oklab`] / [`oklab_to_srgb_u8`] — per-pixel converters
//! - [`srgb_u8_to_oklab_slice`] — tile-friendly bulk path; preserves
//!   per-pixel diff vs the per-pixel API to f32-epsilon
//!
//! The math is Björn Ottosson (2020, matrices updated 2021-01-25). The
//! implementation follows the codegen recipe drilled in
//! `docs/research/png/03a-bis-oklab-simd.md` §3.3:
//! - `f32::mul_add` on every matmul term (FMA → arm `vfmla` / x86 `vfmadd`)
//! - Lagny rational cbrt approximation (1 iter, ~24-bit precision)
//! - `#[inline(always)]` on hot path
//! - `rgb::Rgb<u8>` struct pass-by-value
//!
//! Measured on Apple M2 (release): 02-pluto (400 K px) **0.66 ms**.
//! See `crates/nupic-research/examples/oklab_simd_bench.rs` for the
//! ceiling history.

#![allow(clippy::excessive_precision)]
#![allow(clippy::inline_always)]
#![allow(clippy::similar_names)]
#![allow(clippy::many_single_char_names)]

use rgb::Rgb;

/// OKLab pixel. Layout matches the on-the-wire f32 triple used by
/// downstream stones (SSIMULACRA2 pyramid, codebook learner).
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
#[repr(C)]
pub struct Oklab {
    /// L — perceived lightness, roughly [0, 1].
    pub l: f32,
    /// a — green ↔ red, roughly [-0.4, 0.4].
    pub a: f32,
    /// b — blue ↔ yellow, roughly [-0.4, 0.4].
    pub b: f32,
}

impl Oklab {
    /// Construct from raw f32 triple.
    #[inline(always)]
    #[must_use]
    pub fn new(l: f32, a: f32, b: f32) -> Self {
        Self { l, a, b }
    }
}

// --- Lagny rational cube-root approximation (Ottosson 2020 §`cbrt`) ---
// 1 iteration achieves ~24-bit f32 precision. Replaces libm `cbrtf`.

#[inline(always)]
fn cbrt_lagny(x: f32) -> f32 {
    const B: u32 = 709_957_561;
    const C: f32 =  0.542_857_170_1;
    const D: f32 = -0.705_306_112_8;
    const E: f32 =  1.414_285_659_8;
    const F: f32 =  1.607_142_806_1;
    const G: f32 =  0.357_142_656_6;
    let mut t = f32::from_bits((x.to_bits() / 3).wrapping_add(B));
    let s = C + (t * t) * (t / x);
    t *= G + F / (s + E + D / s);
    t
}

// --- forward path ----------------------------------------------------

/// Convert linear sRGB (each channel in [0, 1]) to [`Oklab`]. Internal
/// helper; callers usually want [`srgb_u8_to_oklab`] which composes
/// `fast-srgb8` decode with this.
#[inline(always)]
#[must_use]
pub fn linear_srgb_to_oklab(rgb: Rgb<f32>) -> Oklab {
    // M1 (linear sRGB → LMS), Ottosson 2021-01-25 matrices.
    let l = 0.0514459929f32.mul_add(rgb.b, 0.4122214708f32.mul_add(rgb.r, 0.5363325363 * rgb.g));
    let m = 0.1073969566f32.mul_add(rgb.b, 0.2119034982f32.mul_add(rgb.r, 0.6806995451 * rgb.g));
    let s = 0.6299787005f32.mul_add(rgb.b, 0.0883024619f32.mul_add(rgb.r, 0.2817188376 * rgb.g));
    let lp = cbrt_lagny(l);
    let mp = cbrt_lagny(m);
    let sp = cbrt_lagny(s);
    // M2 (LMS' → OKLab).
    Oklab {
        l:   (-0.0040720468f32).mul_add(sp, 0.2104542553f32.mul_add(lp,  0.7936177850 * mp)),
        a:    0.4505937099f32 .mul_add(sp, 1.9779984951f32.mul_add(lp, -2.4285922050 * mp)),
        b:   (-0.8086757660f32).mul_add(sp, 0.0259040371f32.mul_add(lp,  0.7827717662 * mp)),
    }
}

/// Convert an 8-bit sRGB pixel to [`Oklab`].
///
/// Uses `fast-srgb8`'s 256-entry LUT for the sRGB → linear transfer,
/// followed by the Ottosson 2021-01-25 OKLab matmul + Lagny rational
/// cbrt. Numerically agrees with `oklab` crate v1.1.2 to f32 epsilon.
#[inline(always)]
#[must_use]
pub fn srgb_u8_to_oklab(c: Rgb<u8>) -> Oklab {
    linear_srgb_to_oklab(Rgb {
        r: fast_srgb8::srgb8_to_f32(c.r),
        g: fast_srgb8::srgb8_to_f32(c.g),
        b: fast_srgb8::srgb8_to_f32(c.b),
    })
}

// --- reverse path ----------------------------------------------------

/// Convert [`Oklab`] back to linear sRGB. Outputs may be < 0 or > 1
/// when the OKLab pixel was outside the sRGB gamut.
#[inline(always)]
#[must_use]
pub fn oklab_to_linear_srgb(c: Oklab) -> Rgb<f32> {
    // M2 inverse.
    let lp =  0.2158037573f32 .mul_add(c.b,  0.3963377774f32 .mul_add(c.a, c.l));
    let mp = (-0.0638541728f32).mul_add(c.b, (-0.1055613458f32).mul_add(c.a, c.l));
    let sp = (-1.2914855480f32).mul_add(c.b, (-0.0894841775f32).mul_add(c.a, c.l));
    let l = lp * lp * lp;
    let m = mp * mp * mp;
    let s = sp * sp * sp;
    // M1 inverse.
    Rgb {
        r:  0.2309699292f32 .mul_add(s,  4.0767416621f32 .mul_add(l, -3.3077115913 * m)),
        g: (-0.3413193965f32).mul_add(s, (-1.2684380046f32).mul_add(l,  2.6097574011 * m)),
        b:  1.7076147010f32 .mul_add(s, (-0.0041960863f32).mul_add(l, -0.7034186147 * m)),
    }
}

/// Convert [`Oklab`] to 8-bit sRGB, clamped to [0, 255]. Uses
/// `fast-srgb8`'s reverse LUT for the gamma encode.
#[inline(always)]
#[must_use]
pub fn oklab_to_srgb_u8(c: Oklab) -> Rgb<u8> {
    let lin = oklab_to_linear_srgb(c);
    Rgb {
        r: fast_srgb8::f32_to_srgb8(lin.r),
        g: fast_srgb8::f32_to_srgb8(lin.g),
        b: fast_srgb8::f32_to_srgb8(lin.b),
    }
}

// --- bulk path (tile-aware, streaming-friendly) -----------------------

/// Convert a packed `&[u8]` RGBA8 buffer into a `&mut [Oklab]` buffer.
/// Alpha is dropped (caller must round-trip alpha separately).
///
/// Panics if `out.len() * 4 != rgba.len()`.
///
/// Implementation is a tight scalar loop relying on the per-pixel
/// converter being `#[inline(always)]`; LLVM auto-vectorises the
/// resulting matmul chain on arm M2 to ~9.7 GB/s effective bandwidth
/// (M-bound on the LUT load + division chain in Lagny cbrt). For 4K
/// inputs (~33 MB OKLab buffer) callers should tile externally to keep
/// the working set in L2.
pub fn srgb_u8_to_oklab_slice(rgba: &[u8], out: &mut [Oklab]) {
    assert_eq!(out.len() * 4, rgba.len(),
               "rgba buffer must be 4 bytes per Oklab output");
    for (i, slot) in out.iter_mut().enumerate() {
        let off = i * 4;
        *slot = srgb_u8_to_oklab(Rgb {
            r: rgba[off],
            g: rgba[off + 1],
            b: rgba[off + 2],
        });
    }
}

/// Pixel count per tile recommended for memory-aware bulk conversion.
///
/// Rationale (Apple M2 measurements, see
/// `docs/research/png/03a-ter-oklab-graduation.md` §2):
/// - 16 384 pixels × 16 byte (RGBA8 in + OKLab f32 out) = 256 KB
///   working set, fits comfortably in M2 L2 (12 MB) and stays in L1
///   for the OKLab output side (192 KB at this size).
/// - Larger tiles work but yield diminishing returns; smaller tiles
///   inflate per-call overhead.
///
/// Callers handling 4K+ images **should** drive
/// [`srgb_u8_to_oklab_tiled`] (or call [`srgb_u8_to_oklab_slice`]
/// repeatedly with chunks ≤ this constant) instead of allocating one
/// 33 MB OKLab buffer.
pub const RECOMMENDED_TILE_PIXELS: usize = 16_384;

/// Tile-aware bulk path. Internally calls
/// [`srgb_u8_to_oklab_slice`] over chunks of
/// [`RECOMMENDED_TILE_PIXELS`] pixels each.
///
/// Numerically identical to [`srgb_u8_to_oklab_slice`] applied to the
/// whole buffer — the only difference is the working set bound during
/// execution.
pub fn srgb_u8_to_oklab_tiled(rgba: &[u8], out: &mut [Oklab]) {
    assert_eq!(out.len() * 4, rgba.len());
    let stride_in = RECOMMENDED_TILE_PIXELS * 4;
    for (in_chunk, out_chunk) in
        rgba.chunks(stride_in).zip(out.chunks_mut(RECOMMENDED_TILE_PIXELS))
    {
        srgb_u8_to_oklab_slice(in_chunk, out_chunk);
    }
}

/// Reverse bulk path. Writes packed RGBA8 (alpha=255) for each
/// [`Oklab`] input. Panics if `rgba.len() != lab.len() * 4`.
pub fn oklab_to_srgb_u8_slice(lab: &[Oklab], rgba: &mut [u8]) {
    assert_eq!(lab.len() * 4, rgba.len());
    for (i, c) in lab.iter().enumerate() {
        let off = i * 4;
        let p = oklab_to_srgb_u8(*c);
        rgba[off] = p.r;
        rgba[off + 1] = p.g;
        rgba[off + 2] = p.b;
        rgba[off + 3] = 255;
    }
}

// --- conversions from foreign types ---------------------------------

impl From<Rgb<u8>> for Oklab {
    #[inline(always)]
    fn from(rgb: Rgb<u8>) -> Self {
        srgb_u8_to_oklab(rgb)
    }
}

impl From<Oklab> for Rgb<u8> {
    #[inline(always)]
    fn from(lab: Oklab) -> Self {
        oklab_to_srgb_u8(lab)
    }
}
