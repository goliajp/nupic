//! Fixture-level Stone C contract:every fixture in
//! `assets/png-bench/inputs/` must achieve **SSIMULACRA2 ≥ cement
//! imagequant baseline − 2 points** (small tolerance for the photo
//! fixtures where Stone C ties).
//!
//! Stone C's claim from 03c-bis is "near or above cement on every
//! fixture";this test enforces the contract.

use std::path::PathBuf;

use nupic_quantize::{quantize_indexed_png, QuantizeOpts};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf()
}

fn load(name: &str) -> (Vec<u8>, u32, u32) {
    let p = root().join("assets/png-bench/inputs").join(name);
    let img = ::image::open(&p).unwrap_or_else(|e| panic!("{p:?}: {e}")).to_rgba8();
    let (w, h) = (img.width(), img.height());
    (img.into_raw(), w, h)
}

fn ssim_of(raw_ref: &[u8], distorted_png: &[u8], w: u32, h: u32) -> f64 {
    let decoded = ::image::load_from_memory_with_format(distorted_png, ::image::ImageFormat::Png)
        .expect("decode")
        .to_rgba8()
        .into_raw();
    nupic_ssimulacra::ssimulacra2_score(raw_ref, &decoded, w, h).unwrap()
}

fn cement_baseline_png(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    let pixels: Vec<rgb::RGBA8> = rgba.chunks_exact(4)
        .map(|c| rgb::RGBA8 { r: c[0], g: c[1], b: c[2], a: c[3] })
        .collect();
    let mut attrs = imagequant::new();
    let _ = attrs.set_quality(70, 95);
    let _ = attrs.set_speed(4);
    let mut img = attrs.new_image(pixels.as_slice(), w as usize, h as usize, 0.0).expect("iq");
    let mut quant = match attrs.quantize(&mut img) {
        Ok(q) => q,
        Err(_) => {
            let mut a2 = imagequant::new();
            let _ = a2.set_quality(0, 95);
            let _ = a2.set_speed(4);
            img = a2.new_image(pixels.as_slice(), w as usize, h as usize, 0.0).expect("iq");
            attrs = a2;
            attrs.quantize(&mut img).expect("iq fallback")
        }
    };
    let _ = quant.set_dithering_level(1.0);
    let (palette, indexed) = quant.remapped(&mut img).expect("iq remap");
    let mut rgb_palette: Vec<u8> = Vec::with_capacity(palette.len() * 3);
    let mut alphas: Vec<u8> = Vec::with_capacity(palette.len());
    for c in &palette {
        rgb_palette.push(c.r);
        rgb_palette.push(c.g);
        rgb_palette.push(c.b);
        alphas.push(c.a);
    }
    while alphas.last() == Some(&255) { alphas.pop(); }
    let mut raw = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut raw, w, h);
        enc.set_color(png::ColorType::Indexed);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_palette(rgb_palette);
        if !alphas.is_empty() { enc.set_trns(alphas); }
        let mut writer = enc.write_header().expect("png header");
        writer.write_image_data(&indexed).expect("png data");
    }
    oxipng::optimize_from_memory(&raw, &oxipng::Options::from_preset(5)).expect("oxipng")
}

fn check(name: &str, tolerance: f64) {
    let (raw, w, h) = load(name);
    let nupic_png = quantize_indexed_png(&raw, w, h, QuantizeOpts::default()).unwrap();
    let cement_png = cement_baseline_png(&raw, w, h);
    let nupic_ssim = ssim_of(&raw, &nupic_png, w, h);
    let cement_ssim = ssim_of(&raw, &cement_png, w, h);
    let diff = nupic_ssim - cement_ssim;
    assert!(
        diff >= -tolerance,
        "{name}: nupic SSIM {nupic_ssim:.2} < cement {cement_ssim:.2} - {tolerance} (diff {diff:+.2})",
    );
    eprintln!("[{name}] nupic={nupic_ssim:.2} cement={cement_ssim:.2} diff={diff:+.2}");
}

#[test] fn fixture_01_transparency_demo()  { check("01-png-transparency-demo.png", 2.0); }
#[test] fn fixture_02_pluto_transparent()  { check("02-pluto-transparent.png", 2.0); }
#[test] fn fixture_03_wikipedia_logo()     { check("03-wikipedia-logo.png", 2.0); }
#[test] fn fixture_04_photo_portrait()     { check("04-photo-portrait.png", 2.0); }
#[test] fn fixture_05_photo_mountain()     { check("05-photo-mountain.png", 2.0); }
#[test] fn fixture_06_photo_landscape()    { check("06-photo-landscape.png", 2.0); }
#[test] fn fixture_07_photo_product()      { check("07-photo-product.png", 2.0); }
