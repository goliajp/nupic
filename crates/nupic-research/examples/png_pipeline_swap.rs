//! End-to-end PNG pipeline bench:for each fixture,run nupic-quantize
//! to get a palette + index buffer,then compare:
//!
//! - Path **A** (current): `nupic-quantize::quantize_indexed_png` →
//!   raw indexed PNG via `png` crate → `oxipng::optimize_from_memory`
//!   (libdeflate near-optimal).
//! - Path **B** (proposed): `nupic-quantize::quantize` → palette + indices →
//!   `nupic-png::encode_indexed_png` (filter try-all + nupic-deflate
//!   Level::Best).
//!
//! Run:
//!   cargo run --release -p nupic-research --example png_pipeline_swap

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use image::ImageReader;
use nupic_color::oklab_to_srgb_u8;
use nupic_png::{FilterStrategy, IndexedImage, encode_indexed_png, encode_indexed_png_with};
use nupic_quantize::{QuantizeOpts, quantize, quantize_indexed_png};
use rgb::Rgb;

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(Path::new(m).ancestors().nth(2).context("workspace root")?.to_path_buf())
}

fn process(root: &Path, fname: &str) -> Result<(String, usize, usize, usize, usize)> {
    let path = root.join("assets/png-bench/inputs").join(fname);
    let img = ImageReader::open(&path)?.with_guessed_format()?.decode()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let raw = rgba.into_raw();
    let raw_size = raw.len();

    // Path A: current production pipeline.
    let opts = QuantizeOpts { n_colors: 256, oxipng_preset: 5, strip_metadata: true, dither_strength: 0.0 };
    let path_a_png = quantize_indexed_png(&raw, w, h, opts).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let size_a = path_a_png.len();

    // Path B: nupic-quantize → nupic-png (no oxipng).
    let qi = quantize(&raw, w, h, 256).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let palette_srgb: Vec<Rgb<u8>> = qi.palette_srgb.into_iter().collect();
    // Convert OKLab palette to sRGB for the indexed PNG (the quantize
    // function already returns sRGB via palette_srgb).
    // Actually quantize returns (indices, palette_srgb) where palette_srgb
    // is Vec<Rgb<u8>> sRGB. Use directly.
    let png_img = IndexedImage {
        width: w,
        height: h,
        palette: palette_srgb,
        indices: qi.indices,
        trns: None,
    };
    let path_b_png = encode_indexed_png(&png_img);
    let size_b = path_b_png.len();
    let path_c_png = encode_indexed_png_with(&png_img, FilterStrategy::DeflateAware);
    let size_c = path_c_png.len();

    let _ = oklab_to_srgb_u8;

    Ok((fname.to_string(), raw_size, size_a, size_b, size_c))
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let fixtures = [
        "01-png-transparency-demo.png",
        "02-pluto-transparent.png",
        "03-wikipedia-logo.png",
        "04-photo-portrait.png",
        "05-photo-mountain.png",
        "06-photo-landscape.png",
        "07-photo-product.png",
    ];

    println!(
        "{:<32} {:>10} {:>10} {:>10} {:>10}   {:>6} {:>6}",
        "fixture", "raw_rgba", "A_oxipng", "B_bestof", "C_dfl_aw", "B/A", "C/A"
    );
    println!("{}", "-".repeat(96));

    let (mut sum_a, mut sum_b, mut sum_c) = (0usize, 0usize, 0usize);
    for fname in fixtures {
        match process(&root, fname) {
            Ok((name, raw, a, b, c)) => {
                let rb = b as f64 / a as f64;
                let rc = c as f64 / a as f64;
                println!(
                    "{:<32} {:>10} {:>10} {:>10} {:>10}   {:>5.2}× {:>5.2}×",
                    name, raw, a, b, c, rb, rc
                );
                sum_a += a;
                sum_b += b;
                sum_c += c;
            }
            Err(e) => println!("{:<32} ERROR: {e}", fname),
        }
    }
    println!("{}", "-".repeat(96));
    println!(
        "{:<32} {:>10} {:>10} {:>10} {:>10}   {:>5.2}× {:>5.2}×",
        "TOTAL",
        "",
        sum_a,
        sum_b,
        sum_c,
        sum_b as f64 / sum_a.max(1) as f64,
        sum_c as f64 / sum_a.max(1) as f64,
    );
    Ok(())
}
