//! Stone E content-adaptive dither — per-image classifier picks
//! dither strength based on cheap content statistic (unique RGB color
//! count / total pixels). Goal: auto-Pick equivalent of `--dither 0.5`
//! for photo-class images, 0.0 for logos / transparent, no user
//! trade-off needed. Validates threshold on 7-fixture corpus + dogfood.
//!
//! Run:
//!   cargo run --release -p nupic-research --example stone_e_auto

use std::collections::HashSet;
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
        .find_map(|l| {
            l.strip_prefix("SSIMULACRA2: ")
                .and_then(|v| v.split_whitespace().next())
                .and_then(|n| n.parse::<f64>().ok())
        })
        .unwrap_or(0.0)
}

/// Multi-feature classifier:
/// - `opaque_ratio`:fraction of pixels with alpha = 255.
/// - `unique_color_count`:unique opaque RGB colors.
/// - `n_pixels`:total pixel count.
///
/// Auto-dither heuristic ships:
///   `opaque_ratio >= 0.95 && n_pixels >= 200_000` → use dither
/// Rationale: photo-class workloads are fully-opaque large rasters
/// (04/05/06/07 fixtures all satisfy). Transparent-bearing inputs
/// (01/02) skip because dither noise on alpha boundaries hurts;
/// small logos (03) skip because palette overhead per byte dominates.
fn classify(src_rgba: &[u8]) -> (f64, usize, usize, bool) {
    let mut set: HashSet<u32> = HashSet::new();
    let mut n_opaque = 0usize;
    let mut n_total = 0usize;
    for px in src_rgba.chunks_exact(4) {
        n_total += 1;
        if px[3] == 255 {
            n_opaque += 1;
            let key = u32::from(px[0]) | (u32::from(px[1]) << 8) | (u32::from(px[2]) << 16);
            set.insert(key);
        }
    }
    let opaque_ratio = if n_total == 0 { 0.0 } else { n_opaque as f64 / n_total as f64 };
    let auto_dither = opaque_ratio >= 0.95 && n_total >= 200_000;
    (opaque_ratio, set.len(), n_total, auto_dither)
}

fn run(src_path: &Path, strength: f32, label: &str, tmpdir: &Path) -> Result<(usize, f64)> {
    let img = ImageReader::open(src_path)?.with_guessed_format()?.decode()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let raw = rgba.into_raw();
    let opts = QuantizeOpts {
        n_colors: 256,
        oxipng_preset: 5,
        strip_metadata: true,
        dither_strength: strength,
            ..Default::default()
        };
    let png = quantize_indexed_png(&raw, w, h, opts).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let out = tmpdir.join(format!("{label}-{}", src_path.file_name().unwrap().to_string_lossy()));
    std::fs::write(&out, &png)?;
    let ssim = ssimulacra2(src_path, &out);
    Ok((png.len(), ssim))
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let fixtures = [
        ("01-png-transparency-demo.png", "assets/png-bench/inputs"),
        ("02-pluto-transparent.png", "assets/png-bench/inputs"),
        ("03-wikipedia-logo.png", "assets/png-bench/inputs"),
        ("04-photo-portrait.png", "assets/png-bench/inputs"),
        ("05-photo-mountain.png", "assets/png-bench/inputs"),
        ("06-photo-landscape.png", "assets/png-bench/inputs"),
        ("07-photo-product.png", "assets/png-bench/inputs"),
    ];
    let tmpdir = std::env::temp_dir().join("stone-e-auto");
    std::fs::create_dir_all(&tmpdir)?;

    println!(
        "{:<32} {:>8} {:>10}  {:>10} {:>8}  {:>10} {:>8}  decision",
        "fixture", "opq_r", "n_pixels", "size_d0", "SSIM_d0", "size_d05", "SSIM_d05"
    );
    println!("{}", "-".repeat(120));
    for (fname, dir) in &fixtures {
        let path = root.join(dir).join(fname);
        let raw = {
            let img = ImageReader::open(&path)?.with_guessed_format()?.decode()?;
            img.to_rgba8().into_raw()
        };
        let (opq_r, _ucnt, npx, auto) = classify(&raw);
        let (s_d0, ssim_d0) = run(&path, 0.0, "d0", &tmpdir)?;
        let (s_d05, ssim_d05) = run(&path, 0.5, "d05", &tmpdir)?;
        let pick = if auto { "DITHER(0.5)" } else { "skip" };
        println!(
            "{:<32} {:>8.4} {:>10}  {:>10} {:>8.2}  {:>10} {:>8.2}  {}",
            fname, opq_r, npx, s_d0, ssim_d0, s_d05, ssim_d05, pick
        );
    }
    println!();
    println!("rule: opaque_ratio >= 0.95 && n_pixels >= 200_000 → dither_strength = 0.5");
    Ok(())
}
