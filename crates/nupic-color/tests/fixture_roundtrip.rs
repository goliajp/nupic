//! Five fixture-level roundtrip tests against real PNGs in
//! `assets/png-bench/inputs/`. Each asserts that decoding a fixture,
//! converting to OKLab via the bulk slice API and back to sRGB, the
//! resulting RGB channels are within ≤ 1 of the original on every
//! pixel and the global mean-abs-diff stays below 0.5.
//!
//! Acts as the integration-level corollary to `properties.rs` —
//! verifies the stone behaves on real image data, not just synthetic
//! sweeps.

use std::path::PathBuf;

use nupic_color::{Oklab, oklab_to_srgb_u8_slice, srgb_u8_to_oklab_slice};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .join("assets")
        .join("png-bench")
        .join("inputs")
}

fn check_fixture(name: &str) {
    let path = fixture_dir().join(name);
    let rgba = ::image::open(&path)
        .unwrap_or_else(|e| panic!("open {}: {e}", path.display()))
        .to_rgba8();
    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
    let n = w * h;
    let raw = rgba.into_raw();

    let mut lab = vec![Oklab::new(0.0, 0.0, 0.0); n];
    srgb_u8_to_oklab_slice(&raw, &mut lab);
    let mut back = vec![0u8; raw.len()];
    oklab_to_srgb_u8_slice(&lab, &mut back);

    let mut max_diff = 0i32;
    let mut sum_diff = 0u64;
    let mut over_one = 0u64;
    for i in 0..n {
        for c in 0..3 {
            let d = (back[i*4+c] as i32 - raw[i*4+c] as i32).abs();
            if d > max_diff { max_diff = d; }
            sum_diff += d as u64;
            if d > 1 { over_one += 1; }
        }
    }
    let mean = sum_diff as f64 / (3.0 * n as f64);
    assert!(
        max_diff <= 1,
        "{name}: max channel diff = {max_diff} > 1 (over_one_count = {over_one}, mean = {mean:.4})",
    );
    assert!(
        mean < 0.5,
        "{name}: mean channel diff = {mean:.4} > 0.5",
    );
}

#[test] fn fixture_01_transparency_demo()  { check_fixture("01-png-transparency-demo.png"); }
#[test] fn fixture_02_pluto_transparent()  { check_fixture("02-pluto-transparent.png"); }
#[test] fn fixture_03_wikipedia_logo()     { check_fixture("03-wikipedia-logo.png"); }
#[test] fn fixture_04_photo_portrait()     { check_fixture("04-photo-portrait.png"); }
#[test] fn fixture_05_photo_mountain()     { check_fixture("05-photo-mountain.png"); }
