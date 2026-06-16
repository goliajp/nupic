//! Pareto frontier sweep — cross-product of (refine_iters, dither_strength)
//! per fixture + dogfood inputs. Output: per-fixture size/SSIM matrix
//! used to identify true ceilings + inform whether tiered auto-dither
//! classifier or adaptive iter convergence would close gaps.
//!
//! Run:
//!   cargo run --release -p nupic-research --example pareto_sweep

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

fn process(src_path: &Path, dither_strength: f32, tmpdir: &Path) -> Result<(usize, f64)> {
    let img = ImageReader::open(src_path)?.with_guessed_format()?.decode()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let raw = rgba.into_raw();
    let opts = QuantizeOpts {
        n_colors: 256,
        oxipng_preset: 5,
        strip_metadata: true,
        dither_strength,
    };
    let png = quantize_indexed_png(&raw, w, h, opts).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let fname = src_path.file_name().unwrap().to_string_lossy();
    let out_path = tmpdir.join(format!("d{:.2}-{fname}", dither_strength));
    std::fs::write(&out_path, &png)?;
    let ssim = ssimulacra2(src_path, &out_path);
    Ok((png.len(), ssim))
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let dogfood_dir = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("Downloads");
    let inputs: Vec<PathBuf> = vec![
        root.join("assets/png-bench/inputs/01-png-transparency-demo.png"),
        root.join("assets/png-bench/inputs/02-pluto-transparent.png"),
        root.join("assets/png-bench/inputs/03-wikipedia-logo.png"),
        root.join("assets/png-bench/inputs/04-photo-portrait.png"),
        root.join("assets/png-bench/inputs/05-photo-mountain.png"),
        root.join("assets/png-bench/inputs/06-photo-landscape.png"),
        root.join("assets/png-bench/inputs/07-photo-product.png"),
        dogfood_dir.join("testflight.png"),
        dogfood_dir.join("vantage-staging.png"),
    ];

    let dither_strengths = [0.0f32, 0.25, 0.5, 0.75];
    let tmpdir = std::env::temp_dir().join("nupic-pareto");
    std::fs::create_dir_all(&tmpdir)?;

    println!("{}", "=".repeat(120));
    println!("Pareto frontier per fixture: (refine_iters=20 default) × dither {:?}", &dither_strengths);
    println!("{}", "=".repeat(120));
    println!(
        "{:<32} {:>10} {:>8}  {:>10} {:>8}  {:>10} {:>8}  {:>10} {:>8}",
        "input",
        "d=0 size", "d=0 SSIM",
        "d=.25 size", "d=.25 SSIM",
        "d=.5 size", "d=.5 SSIM",
        "d=.75 size", "d=.75 SSIM",
    );
    println!("{}", "-".repeat(120));

    println!();
    println!("Auxiliary signals for tier classifier:");
    println!("{:<32} {:>10} {:>10} {:>10}", "input", "opq_ratio", "mean_run", "uniq_ratio");
    for src in &inputs {
        if !src.exists() {
            continue;
        }
        let img = ImageReader::open(src)?.with_guessed_format()?.decode()?;
        let rgba = img.to_rgba8();
        let raw = rgba.into_raw();
        let n = raw.len() / 4;
        let n_opaque: usize = raw.chunks_exact(4).filter(|p| p[3] == 255).count();
        let opq_r = if n == 0 { 0.0 } else { n_opaque as f64 / n as f64 };
        // mean run length of consecutive RGB-identical pixels (row-major)
        let mut runs: u64 = 0;
        let mut total_runs: u64 = 0;
        let mut prev: [u8; 3] = [0, 0, 0];
        let mut cur_run: u64 = 0;
        for (i, p) in raw.chunks_exact(4).enumerate() {
            let rgb = [p[0], p[1], p[2]];
            if i > 0 && rgb == prev {
                cur_run += 1;
            } else {
                if cur_run > 0 {
                    runs += cur_run;
                    total_runs += 1;
                }
                cur_run = 1;
            }
            prev = rgb;
        }
        if cur_run > 0 {
            runs += cur_run;
            total_runs += 1;
        }
        let mean_run = if total_runs == 0 { 0.0 } else { runs as f64 / total_runs as f64 };
        // unique color ratio
        let mut uniq: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for p in raw.chunks_exact(4) {
            uniq.insert(u32::from(p[0]) | (u32::from(p[1]) << 8) | (u32::from(p[2]) << 16));
        }
        let uniq_r = if n == 0 { 0.0 } else { uniq.len() as f64 / n as f64 };
        let fname = src.file_name().unwrap().to_string_lossy();
        println!("{:<32} {:>10.4} {:>10.2} {:>10.4}",
            &fname[..fname.len().min(32)], opq_r, mean_run, uniq_r);
    }
    println!();
    println!("{}", "=".repeat(120));
    println!("Pareto sweep details:");
    println!("{}", "=".repeat(120));

    for src in &inputs {
        if !src.exists() {
            println!("{} — MISSING, skip", src.display());
            continue;
        }
        let fname = src.file_name().unwrap().to_string_lossy();
        let mut row: Vec<(usize, f64)> = Vec::new();
        for &d in &dither_strengths {
            row.push(process(src, d, &tmpdir)?);
        }
        print!("{:<32}", &fname[..fname.len().min(32)]);
        for (sz, ss) in &row {
            print!(" {:>10} {:>8.2}", sz, ss);
        }
        println!();
        // Per-row analysis: identify Pareto-optimal dither strengths
        // (a strength is Pareto-optimal if no other strength is both
        // smaller AND higher SSIM).
        let mut pareto: Vec<usize> = Vec::new();
        for i in 0..row.len() {
            let dominated = (0..row.len()).any(|j| j != i &&
                row[j].0 <= row[i].0 && row[j].1 >= row[i].1 &&
                (row[j].0 < row[i].0 || row[j].1 > row[i].1));
            if !dominated {
                pareto.push(i);
            }
        }
        print!("  pareto-optimal at:");
        for i in pareto {
            print!(" d={:.2}", dither_strengths[i]);
        }
        println!();
    }
    Ok(())
}
