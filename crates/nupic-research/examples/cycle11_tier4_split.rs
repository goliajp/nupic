//! Cycle 11 — measure tier-4 content-split bench. For each of 4 photo
//! fixtures, compare d=0.5 (pre-Cycle-11 default for tier-4) vs d=0.7
//! (new tier-4b for textured) vs auto (now picks via var_diff signal).

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
        "04-photo-portrait.png",
        "05-photo-mountain.png",
        "06-photo-landscape.png",
        "07-photo-product.png",
    ];
    let tmpdir = std::env::temp_dir().join("cycle11-tier4");
    std::fs::create_dir_all(&tmpdir)?;

    println!(
        "{:<32} {:>10}{:>8}  {:>10}{:>8}  {:>10}{:>8}",
        "fixture", "size_d05", "SSIM_05", "size_d07", "SSIM_07", "size_auto", "SSIM_au"
    );
    println!("{}", "-".repeat(110));
    for fname in &fixtures {
        let path = root.join("assets/png-bench/inputs").join(fname);
        let (s05, q05) = run(&path, 0.5, "d05", &tmpdir)?;
        let (s07, q07) = run(&path, 0.7, "d07", &tmpdir)?;
        let (sau, qau) = run(&path, f32::NAN, "auto", &tmpdir)?;
        println!(
            "{:<32} {:>10}{:>8.2}  {:>10}{:>8.2}  {:>10}{:>8.2}",
            fname, s05, q05, s07, q07, sau, qau
        );
    }
    Ok(())
}
