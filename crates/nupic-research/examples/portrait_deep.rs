//! Cycle 7 — 04-photo-portrait deep ceiling exploration. Sweep
//! n_colors × dither_strength × refine_iters on 04 only, find
//! absolute frontier — can we reach SSIMULACRA2 85+ (close TinyPNG
//! gap) within indexed PNG constraints?
//!
//! Run:
//!   cargo run --release -p nupic-research --example portrait_deep

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use image::ImageReader;
use nupic_quantize::{QuantizeOpts, quantize_indexed_png};

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
        .find_map(|l| l.strip_prefix("SSIMULACRA2: ").and_then(|v| v.split_whitespace().next()).and_then(|n| n.parse::<f64>().ok()))
        .unwrap_or(0.0)
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let src = root.join("assets/png-bench/inputs/04-photo-portrait.png");
    let tiny = root.join("assets/png-bench/tinypng-web/04-photo-portrait.png");
    let img = ImageReader::open(&src)?.with_guessed_format()?.decode()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let raw = rgba.into_raw();

    let tmpdir = std::env::temp_dir().join("portrait-deep");
    std::fs::create_dir_all(&tmpdir)?;

    println!("Reference: TinyPNG 04-portrait");
    let tiny_sz = std::fs::metadata(&tiny)?.len();
    let tiny_ssim = ssimulacra2(&src, &tiny);
    println!("  tiny size {} bytes, SSIM {:.2}", tiny_sz, tiny_ssim);
    println!();

    let n_colors_sweep = [128usize, 192, 256];
    let dither_sweep = [0.0f32, 0.25, 0.5, 0.75, 1.0];

    println!("{:<12} {:<10} {:>10} {:>10}  size/tiny  ssim_vs_tiny", "n_colors", "dither", "size", "SSIM");
    println!("{}", "-".repeat(80));

    let mut best_ssim = 0.0_f64;
    let mut best_combo = String::new();

    for &n_colors in &n_colors_sweep {
        for &dither in &dither_sweep {
            let opts = QuantizeOpts {
                n_colors,
                oxipng_preset: 5,
                strip_metadata: true,
                dither_strength: dither,
            ..Default::default()
        };
            let png = quantize_indexed_png(&raw, w, h, opts).map_err(|e| anyhow::anyhow!("{e:?}"))?;
            let out = tmpdir.join(format!("c{}_d{:.2}.png", n_colors, dither));
            std::fs::write(&out, &png)?;
            let ssim = ssimulacra2(&src, &out);
            let size_ratio = png.len() as f64 / tiny_sz as f64;
            let ssim_delta = ssim - tiny_ssim;
            println!(
                "{:<12} {:<10.2} {:>10} {:>10.2}  {:>9.3} {:>+11.2}",
                n_colors, dither, png.len(), ssim, size_ratio, ssim_delta
            );
            if ssim > best_ssim {
                best_ssim = ssim;
                best_combo = format!("n_colors={} dither={:.2} size={} ssim={:.2}", n_colors, dither, png.len(), ssim);
            }
        }
    }

    println!();
    println!("Best SSIM combo: {}", best_combo);
    println!("Tiny target: size {} SSIM {:.2}", tiny_sz, tiny_ssim);

    Ok(())
}
