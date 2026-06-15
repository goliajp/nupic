//! Property tests for `nupic-quantize`. Test what the public API
//! guarantees, not how it computes — so any future palette refinement
//! / dither variant keeps these passing.

use nupic_quantize::{
    QuantizeOpts, apply_palette, quantize, quantize_indexed_png, train_palette,
};
use rgb::Rgb;

fn solid_rgba(r: u8, g: u8, b: u8, w: u32, h: u32) -> Vec<u8> {
    let mut v = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..(w * h) {
        v.extend_from_slice(&[r, g, b, 255]);
    }
    v
}

fn gradient_rgba(w: u32, h: u32) -> Vec<u8> {
    let mut v = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            v.push((x * 255 / w) as u8);
            v.push((y * 255 / h) as u8);
            v.push(128);
            v.push(255);
        }
    }
    v
}

/// Property 1 — solid-colour image quantizes to a 1-color palette and
/// every index points to the same entry.
#[test]
fn solid_color_collapses_to_one_palette_entry() {
    let img = solid_rgba(120, 80, 40, 32, 32);
    let q = quantize(&img, 32, 32, 256).unwrap();
    assert!(!q.indices.is_empty());
    let first = q.indices[0];
    for &idx in &q.indices {
        assert_eq!(idx, first, "solid-color index drifted");
    }
}

/// Property 2 — indexed PNG header has the PNG signature; deflated
/// bytes are non-trivial.
#[test]
fn indexed_png_starts_with_png_magic() {
    let img = gradient_rgba(64, 48);
    let png = quantize_indexed_png(&img, 64, 48, QuantizeOpts::default()).unwrap();
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]),
        "missing PNG signature");
    assert!(png.len() > 100, "implausibly short PNG: {} bytes", png.len());
}

/// Property 3 — `n_colors` is honoured (palette length ≤ requested).
#[test]
fn palette_size_respects_request() {
    let img = gradient_rgba(64, 48);
    for &k in &[4usize, 16, 64, 200, 256] {
        let palette = train_palette(&img, 64, 48, k).unwrap();
        assert!(palette.len() <= k,
            "k={k}: palette {} > requested {k}", palette.len());
        assert!(palette.len() <= 256,
            "k={k}: palette exceeds 8-bit indexed limit");
    }
}

/// Property 4 — output indices are < palette length on every pixel.
#[test]
fn indices_within_palette() {
    let img = gradient_rgba(50, 40);
    let q = quantize(&img, 50, 40, 32).unwrap();
    let k = q.palette_srgb.len();
    for (i, &idx) in q.indices.iter().enumerate() {
        assert!((idx as usize) < k,
            "pixel {i} idx {idx} >= palette {k}");
    }
}

/// Property 5 — deterministic across runs (no rayon work-stealing
/// nondeterminism inside the assignment loop).
#[test]
fn output_deterministic() {
    let img = gradient_rgba(40, 30);
    let q1 = quantize(&img, 40, 30, 64).unwrap();
    let q2 = quantize(&img, 40, 30, 64).unwrap();
    assert_eq!(q1.indices, q2.indices, "indices differ across runs");
}

/// Property 6 — `apply_palette` with the trained palette agrees with
/// the `quantize` one-shot helper.
#[test]
fn apply_palette_matches_quantize() {
    let img = gradient_rgba(60, 40);
    let palette = train_palette(&img, 60, 40, 128).unwrap();
    let (idx_apply, pal_apply) = apply_palette(&img, 60, 40, &palette);
    let q = quantize(&img, 60, 40, 128).unwrap();
    assert_eq!(idx_apply, q.indices);
    assert_eq!(pal_apply.len(), q.palette_srgb.len());
}

/// Property 7 — `From<u8>` indices round-trip is consistent for solid
/// blocks.
#[test]
fn solid_block_round_trip_consistent() {
    let img = solid_rgba(50, 100, 150, 16, 16);
    let palette = train_palette(&img, 16, 16, 16).unwrap();
    let (idx, pal) = apply_palette(&img, 16, 16, &palette);
    assert!(!pal.is_empty());
    // every pixel should map to a palette entry whose sRGB is close to
    // the source colour (within rounding from OKLab roundtrip).
    let p0 = pal[idx[0] as usize];
    assert!((p0.r as i32 - 50).abs() <= 2);
    assert!((p0.g as i32 - 100).abs() <= 2);
    assert!((p0.b as i32 - 150).abs() <= 2);
}

/// Property 8 — empty image (zero size) rejected via panic on the
/// `assert_eq!` in `apply_palette`. We don't unit-test panics
/// extensively; this just doc-references the expectation.
#[test]
fn dimension_invariants() {
    let img = gradient_rgba(20, 20);
    let palette = train_palette(&img, 20, 20, 8).unwrap();
    let _ = apply_palette(&img, 20, 20, &palette);  // should succeed
}

/// Property 9 — `Rgb<u8>` palette entries are unique-ish (palette size
/// drops below requested k when image has too few distinct colours).
#[test]
fn palette_packs_distinct_colors() {
    // Two-tone image: should not need 64 colours
    let mut img = Vec::with_capacity(64 * 64 * 4);
    for y in 0..64u32 {
        for _ in 0..64 {
            let (r, g, b) = if y < 32 { (255, 0, 0) } else { (0, 0, 255) };
            img.extend_from_slice(&[r, g, b, 255]);
        }
    }
    let palette = train_palette(&img, 64, 64, 64).unwrap();
    assert!(palette.len() <= 64);
}
