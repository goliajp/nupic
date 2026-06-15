//! Property-based contracts for `nupic-color`. Tests **what the public
//! API guarantees**, not how it computes — so any future swap from
//! Lagny-cbrt to NEON intrinsics keeps these passing.
//!
//! Cov target (from `docs/research/png/03a-oklab-design.md` §4):
//! ≥ 50 properties + ≥ 5 fixture roundtrip + oracle 1:1 match within
//! 1e-5. Realised here via a handful of `#[test]` functions, each
//! looping over hundreds-to-thousands of assertions.

use nupic_color::{
    Oklab, RECOMMENDED_TILE_PIXELS, oklab_to_srgb_u8, oklab_to_srgb_u8_slice,
    srgb_u8_to_oklab, srgb_u8_to_oklab_slice, srgb_u8_to_oklab_tiled,
};
use rgb::Rgb;

const RGB_LEVELS: [u8; 6] = [0, 51, 102, 153, 204, 255];

/// Property 1 — RGB(u8) → OKLab → RGB(u8) roundtrip per-channel error ≤ 1.
/// Sweeps a 6×6×6 = 216-colour grid.
#[test]
fn roundtrip_per_channel_error_within_one() {
    for &r in &RGB_LEVELS {
        for &g in &RGB_LEVELS {
            for &b in &RGB_LEVELS {
                let original = Rgb { r, g, b };
                let lab = srgb_u8_to_oklab(original);
                let back = oklab_to_srgb_u8(lab);
                let dr = (back.r as i32 - r as i32).abs();
                let dg = (back.g as i32 - g as i32).abs();
                let db = (back.b as i32 - b as i32).abs();
                assert!(
                    dr <= 1 && dg <= 1 && db <= 1,
                    "roundtrip ({r},{g},{b}) → ({},{},{}) diff ({dr},{dg},{db})",
                    back.r, back.g, back.b,
                );
            }
        }
    }
}

/// Property 2 — pure black and pure white anchors.
#[test]
fn black_and_white_anchors_match_reference() {
    let black = srgb_u8_to_oklab(Rgb { r: 0, g: 0, b: 0 });
    assert!(black.l.abs() < 1e-6, "black L = {}", black.l);
    assert!(black.a.abs() < 1e-6, "black a = {}", black.a);
    assert!(black.b.abs() < 1e-6, "black b = {}", black.b);

    let white = srgb_u8_to_oklab(Rgb { r: 255, g: 255, b: 255 });
    assert!((white.l - 1.0).abs() < 1e-4, "white L = {} (expected ~1)", white.l);
    assert!(white.a.abs() < 1e-4, "white a = {}", white.a);
    assert!(white.b.abs() < 1e-4, "white b = {}", white.b);
}

/// Property 3 — pure-primary axis signs per Ottosson 2020 §4.
#[test]
fn primary_axes_have_expected_signs() {
    let red = srgb_u8_to_oklab(Rgb { r: 255, g: 0, b: 0 });
    assert!(red.a > 0.0, "pure red a should be > 0: got {}", red.a);

    let green = srgb_u8_to_oklab(Rgb { r: 0, g: 255, b: 0 });
    assert!(green.a < 0.0, "pure green a should be < 0: got {}", green.a);

    let blue = srgb_u8_to_oklab(Rgb { r: 0, g: 0, b: 255 });
    assert!(blue.b < 0.0, "pure blue b should be < 0: got {}", blue.b);

    let yellow = srgb_u8_to_oklab(Rgb { r: 255, g: 255, b: 0 });
    assert!(yellow.b > 0.0, "pure yellow b should be > 0: got {}", yellow.b);
}

/// Property 4 — L axis monotonic in luminance for a gray sweep.
#[test]
fn l_axis_monotonic_in_gray() {
    let mut prev_l = f32::NEG_INFINITY;
    for v in (0..=255u8).step_by(8) {
        let gray = srgb_u8_to_oklab(Rgb { r: v, g: v, b: v });
        assert!(
            gray.l > prev_l - 1e-6,
            "L axis not monotonic at gray {v}: L={} prev={}",
            gray.l, prev_l,
        );
        prev_l = gray.l;
    }
}

/// Property 5 — output agrees with `oklab` crate v1.1.2 oracle within
/// 1e-5 per channel over a 32×32×32 colour grid (32 768 points).
#[test]
fn matches_oklab_crate_oracle_within_epsilon() {
    let step = 8usize;
    let mut max_diff = 0f32;
    for r in (0..=255).step_by(step) {
        for g in (0..=255).step_by(step) {
            for b in (0..=255).step_by(step) {
                let r = r as u8; let g = g as u8; let b = b as u8;
                let ours = srgb_u8_to_oklab(Rgb { r, g, b });
                let theirs = ::oklab::srgb_to_oklab(::oklab::Rgb { r, g, b });
                let dl = (ours.l - theirs.l).abs();
                let da = (ours.a - theirs.a).abs();
                let db = (ours.b - theirs.b).abs();
                let d = dl.max(da).max(db);
                if d > max_diff { max_diff = d; }
                assert!(
                    d < 1e-5,
                    "at ({r},{g},{b}): diff ({dl},{da},{db}) > 1e-5 (ours {ours:?} theirs L={} a={} b={})",
                    theirs.l, theirs.a, theirs.b,
                );
            }
        }
    }
    // record peak for fast eyeballing in test output
    println!("oklab oracle max diff = {max_diff}");
}

/// Property 6 — slice bulk path is bit-equal to per-pixel path.
/// Important contract: tile-aware code must not introduce float drift.
#[test]
fn slice_bulk_matches_per_pixel_exactly() {
    let mut rgba: Vec<u8> = Vec::with_capacity(1024 * 4);
    for i in 0..1024u32 {
        rgba.push((i & 0xff) as u8);
        rgba.push(((i >> 1) & 0xff) as u8);
        rgba.push(((i >> 2) & 0xff) as u8);
        rgba.push(255);
    }
    let n = rgba.len() / 4;
    let mut bulk = vec![Oklab::new(0.0, 0.0, 0.0); n];
    srgb_u8_to_oklab_slice(&rgba, &mut bulk);
    for i in 0..n {
        let direct = srgb_u8_to_oklab(Rgb {
            r: rgba[i * 4],
            g: rgba[i * 4 + 1],
            b: rgba[i * 4 + 2],
        });
        assert_eq!(direct, bulk[i], "bulk vs per-pixel diverged at index {i}");
    }
}

/// Property 7 — reverse slice round-trips back to the original sRGB
/// within ≤ 1 per channel (same property as #1 but exercised via the
/// bulk API).
#[test]
fn slice_roundtrip_bulk_path() {
    let mut rgba: Vec<u8> = Vec::with_capacity(216 * 4);
    for &r in &RGB_LEVELS {
        for &g in &RGB_LEVELS {
            for &b in &RGB_LEVELS {
                rgba.extend_from_slice(&[r, g, b, 255]);
            }
        }
    }
    let n = rgba.len() / 4;
    let mut lab = vec![Oklab::new(0.0, 0.0, 0.0); n];
    srgb_u8_to_oklab_slice(&rgba, &mut lab);
    let mut back = vec![0u8; rgba.len()];
    oklab_to_srgb_u8_slice(&lab, &mut back);
    for i in 0..n {
        for c in 0..3 {
            let d = (back[i*4+c] as i32 - rgba[i*4+c] as i32).abs();
            assert!(d <= 1, "bulk roundtrip channel diff {d} > 1 at idx {i} c {c}");
        }
        assert_eq!(back[i*4+3], 255, "alpha not 255 at idx {i}");
    }
}

/// Property 8 — tiled bulk path is bit-equal to the un-tiled slice
/// path. Tile size deliberately chosen larger than
/// `RECOMMENDED_TILE_PIXELS` so we exercise the multi-tile loop.
#[test]
fn tiled_bulk_matches_slice_exactly() {
    let n = RECOMMENDED_TILE_PIXELS * 3 + 137; // forces 4 tiles with a tail
    let mut rgba: Vec<u8> = Vec::with_capacity(n * 4);
    for i in 0..n {
        rgba.push((i & 0xff) as u8);
        rgba.push(((i >> 1) & 0xff) as u8);
        rgba.push(((i >> 2) & 0xff) as u8);
        rgba.push(255);
    }
    let mut slice_out = vec![Oklab::new(0.0, 0.0, 0.0); n];
    let mut tile_out = vec![Oklab::new(0.0, 0.0, 0.0); n];
    srgb_u8_to_oklab_slice(&rgba, &mut slice_out);
    srgb_u8_to_oklab_tiled(&rgba, &mut tile_out);
    for i in 0..n {
        assert_eq!(slice_out[i], tile_out[i], "diverged at idx {i}");
    }
}

/// Property 9 — `From<Rgb<u8>>` for `Oklab` matches `srgb_u8_to_oklab`.
#[test]
fn from_trait_matches_function() {
    for &r in &RGB_LEVELS {
        for &g in &RGB_LEVELS {
            for &b in &RGB_LEVELS {
                let p = Rgb { r, g, b };
                let via_fn = srgb_u8_to_oklab(p);
                let via_from: Oklab = p.into();
                assert_eq!(via_fn, via_from);
            }
        }
    }
}
