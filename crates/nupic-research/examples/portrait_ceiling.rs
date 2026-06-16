//! 04-photo-portrait SSIMULACRA2 ceiling exploration:
//! - Sweep Lloyd k-means n_iters {5, 10, 20, 50, 100} to find Stone D plateau.
//! - Try larger initial palette (1024 colors clipped to 256 post-refine).
//! - Measure each variant's size + SSIMULACRA2 vs source.
//!
//! Run:
//!   cargo run --release -p nupic-research --example portrait_ceiling

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use image::ImageReader;
use nupic_quantize::{quantize_with, QuantizeOpts, encode_indexed_png_with_alpha};

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(PathBuf::from(m).ancestors().nth(2).unwrap().to_path_buf())
}

fn ssimulacra2(orig: &Path, cmp: &Path) -> f64 {
    let out = Command::new("nupic")
        .args(["compare", "-m", "ssimulacra2"])
        .arg(orig)
        .arg(cmp)
        .output()
        .expect("nupic compare");
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines()
        .find_map(|l| {
            l.strip_prefix("SSIMULACRA2: ")
                .and_then(|v| v.split_whitespace().next())
                .and_then(|n| n.parse::<f64>().ok())
        })
        .unwrap_or(0.0)
}

fn encode_with_iters(src_rgba: &[u8], w: u32, h: u32, n_iters: usize, n_colors: usize) -> Result<Vec<u8>> {
    let qi = quantize_with(src_rgba, w, h, n_colors, n_iters)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let trns = if qi.palette_alpha.iter().all(|&a| a == 255) {
        None
    } else {
        Some(qi.palette_alpha.as_slice())
    };
    let raw = encode_indexed_png_with_alpha(w, h, &qi.indices, &qi.palette_srgb, trns)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    // No oxipng — bench algorithm-only output to isolate effect.
    let _ = QuantizeOpts::default();
    Ok(raw)
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let src_path = root.join("assets/png-bench/inputs/04-photo-portrait.png");
    let img = ImageReader::open(&src_path)?.with_guessed_format()?.decode()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let raw = rgba.into_raw();

    let tmpdir = std::env::temp_dir().join("nupic-portrait-ceiling");
    std::fs::create_dir_all(&tmpdir)?;

    println!("{:<32} {:>10} {:>10}", "config", "size", "SSIM");
    println!("{}", "-".repeat(60));

    // Sweep n_iters at 256 colors
    for &iters in &[5usize, 10, 20, 50, 100] {
        let png = encode_with_iters(&raw, w, h, iters, 256)?;
        let path = tmpdir.join(format!("256c-iter{}.png", iters));
        std::fs::write(&path, &png)?;
        let ssim = ssimulacra2(&src_path, &path);
        println!("{:<32} {:>10} {:>10.4}", format!("256 colors, iter={}", iters), png.len(), ssim);
    }

    // What about 192 / 128 colors with more iters? (smaller palette might
    // benefit more from refinement because each centroid has more pixels)
    for &(colors, iters) in &[(192usize, 20), (128, 50)] {
        let png = encode_with_iters(&raw, w, h, iters, colors)?;
        let path = tmpdir.join(format!("{}c-iter{}.png", colors, iters));
        std::fs::write(&path, &png)?;
        let ssim = ssimulacra2(&src_path, &path);
        println!("{:<32} {:>10} {:>10.4}", format!("{} colors, iter={}", colors, iters), png.len(), ssim);
    }

    // For reference: raw (no quantization) — best possible quality, big size
    // (PNG truecolor encoding via image crate)
    {
        let mut raw_png: Vec<u8> = Vec::new();
        let buf = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(w, h, raw.clone())
            .context("rgba buffer")?;
        buf.write_to(&mut std::io::Cursor::new(&mut raw_png), image::ImageFormat::Png)?;
        let path = tmpdir.join("truecolor.png");
        std::fs::write(&path, &raw_png)?;
        let ssim = ssimulacra2(&src_path, &path);
        println!("{:<32} {:>10} {:>10.4}", "truecolor (no quant)", raw_png.len(), ssim);
    }

    Ok(())
}
