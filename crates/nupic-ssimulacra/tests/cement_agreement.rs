//! Bit-exact-class agreement against the `ssimulacra2` crate v0.5.1
//! reference implementation. On the 7 fixtures from
//! `assets/png-bench/inputs/`:
//!
//! - self-vs-self score must equal 100.000 (both nupic and cement)
//! - self-vs-tinypng-output score diff between nupic and cement must
//!   be below 0.001 (well under the 0.5-point Stone B graduation
//!   tolerance from `docs/research/png/03b-ssimulacra2-design.md` §6.1)
//!
//! Acts as the integration-level corollary to `properties.rs` — verifies
//! the stone behaves on real image data, not just synthetic sweeps.

use std::path::PathBuf;

use nupic_ssimulacra::ssimulacra2_score;
use ssimulacra2::{ColorPrimaries, Rgb, TransferCharacteristic, compute_frame_ssimulacra2};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf()
}

fn load(p: &str) -> (Vec<u8>, u32, u32) {
    let img = ::image::open(root().join(p)).unwrap_or_else(|e| panic!("{p}: {e}")).to_rgba8();
    let (w, h) = (img.width(), img.height());
    (img.into_raw(), w, h)
}

fn cement_score(rgba: &[u8], w: u32, h: u32) -> f64 {
    let f32_pixels: Vec<[f32; 3]> = rgba.chunks_exact(4)
        .map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0])
        .collect();
    let r = Rgb::new(f32_pixels.clone(), w as usize, h as usize, TransferCharacteristic::SRGB, ColorPrimaries::BT709).expect("rgb");
    let d = Rgb::new(f32_pixels, w as usize, h as usize, TransferCharacteristic::SRGB, ColorPrimaries::BT709).expect("rgb");
    compute_frame_ssimulacra2(r, d).expect("cement")
}

fn check(name: &str) {
    let (raw, w, h) = load(&format!("assets/png-bench/inputs/{name}"));

    // self-vs-self
    let ours = ssimulacra2_score(&raw, &raw, w, h).unwrap();
    assert!((ours - 100.0).abs() < 1e-6,
        "{name} self-vs-self: nupic = {ours}");

    // vs tinypng
    let tp_path = format!("assets/png-bench/tinypng-web/{name}");
    let Ok(tp_img) = ::image::open(root().join(&tp_path)) else { return };
    let tp_rgba = tp_img.to_rgba8();
    if tp_rgba.width() != w || tp_rgba.height() != h {
        return;
    }
    let tp_raw = tp_rgba.into_raw();

    let ours_vs_tp = ssimulacra2_score(&raw, &tp_raw, w, h).unwrap();
    let cement_score_vs_tp = cement_vs_distorted(&raw, &tp_raw, w, h);
    let diff = (ours_vs_tp - cement_score_vs_tp).abs();
    assert!(diff < 0.001,
        "{name}: nupic={ours_vs_tp:.6} cement={cement_score_vs_tp:.6} diff={diff}");

    // sanity: cement self-vs-self = 100 too
    let _ = cement_score(&raw, w, h);
}

fn cement_vs_distorted(ref_rgba: &[u8], dist_rgba: &[u8], w: u32, h: u32) -> f64 {
    let r_f: Vec<[f32; 3]> = ref_rgba.chunks_exact(4)
        .map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0])
        .collect();
    let d_f: Vec<[f32; 3]> = dist_rgba.chunks_exact(4)
        .map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0])
        .collect();
    let r = Rgb::new(r_f, w as usize, h as usize, TransferCharacteristic::SRGB, ColorPrimaries::BT709).expect("rgb");
    let d = Rgb::new(d_f, w as usize, h as usize, TransferCharacteristic::SRGB, ColorPrimaries::BT709).expect("rgb");
    compute_frame_ssimulacra2(r, d).expect("cement")
}

#[test] fn fixture_01_transparency_demo()  { check("01-png-transparency-demo.png"); }
#[test] fn fixture_02_pluto_transparent()  { check("02-pluto-transparent.png"); }
#[test] fn fixture_03_wikipedia_logo()     { check("03-wikipedia-logo.png"); }
#[test] fn fixture_04_photo_portrait()     { check("04-photo-portrait.png"); }
#[test] fn fixture_05_photo_mountain()     { check("05-photo-mountain.png"); }
#[test] fn fixture_06_photo_landscape()    { check("06-photo-landscape.png"); }
#[test] fn fixture_07_photo_product()      { check("07-photo-product.png"); }
