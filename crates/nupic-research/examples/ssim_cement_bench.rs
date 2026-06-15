//! Stone B — cement baseline timing for SSIMULACRA2.
//!
//! Isolates the pure metric cost: no quantize / no oxipng / no DSSIM
//! in the timed region. Same image set as oklab_bench so timings are
//! directly comparable to Stone A.
//!
//! Backs `docs/research/png/03b-ssimulacra2-design.md`. The 03 essay
//! had estimated cement ~100 ms / 02-pluto inferred from metric_sweep
//! total time; this bench measures the pure metric cost directly.
//!
//! Run:
//!   cargo run --release -p nupic-research --example ssim_cement_bench
//!
//! Output:
//!   target/research-out/03b-ssim-cement-bench.csv
//!   target/research-out/03b-ssim-cement-bench.md

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use ssimulacra2::{ColorPrimaries, Rgb, TransferCharacteristic, compute_frame_ssimulacra2};

const INPUTS: &str = "assets/png-bench/inputs";
const OUT_DIR: &str = "target/research-out";

#[derive(Debug)]
struct Row {
    image: String,
    n_pixels: usize,
    pass: &'static str, // "self-vs-self" | "vs-tinypng"
    median_ms: f64,
    score: f64,
    bandwidth_gbps: f64,
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let inputs_dir = root.join(INPUTS);
    let tinypng_dir = root.join("assets/png-bench/tinypng-web");
    let out_dir = root.join(OUT_DIR);
    fs::create_dir_all(&out_dir)?;

    let names = [
        "02-pluto-transparent.png",
        "04-photo-portrait.png",
        "06-photo-landscape.png",
    ];

    let mut rows: Vec<Row> = Vec::new();
    for name in &names {
        let p = inputs_dir.join(name);
        if !p.exists() { return Err(anyhow!("missing fixture {}", p.display())); }
        let rgba = ::image::open(&p)?.to_rgba8();
        let n = (rgba.width() * rgba.height()) as usize;
        let raw = rgba.into_raw();

        // ---- pass 1: original vs original (lower bound = best case) ---
        let times = run_metric(&raw, &raw, n, 5);
        let m = median(&times);
        let score = score_once(&raw, &raw, n);
        let bw = pyramid_bandwidth(n, m);
        rows.push(Row {
            image: name.to_string(),
            n_pixels: n,
            pass: "self-vs-self",
            median_ms: m,
            score,
            bandwidth_gbps: bw,
        });

        // ---- pass 2: original vs TinyPNG output (realistic) ----
        let tp = tinypng_dir.join(name);
        if let Ok(tp_bytes) = fs::read(&tp) {
            if let Ok(dist) = ::image::load_from_memory_with_format(&tp_bytes, ::image::ImageFormat::Png) {
                let dist_rgba = dist.to_rgba8();
                if (dist_rgba.width() * dist_rgba.height()) as usize == n {
                    let dist_raw = dist_rgba.into_raw();
                    let times = run_metric(&raw, &dist_raw, n, 5);
                    let m = median(&times);
                    let score = score_once(&raw, &dist_raw, n);
                    let bw = pyramid_bandwidth(n, m);
                    rows.push(Row {
                        image: name.to_string(),
                        n_pixels: n,
                        pass: "vs-tinypng",
                        median_ms: m,
                        score,
                        bandwidth_gbps: bw,
                    });
                }
            }
        }

        println!("[ssim_cement_bench] done {name} ({n} px)");
    }

    write_csv(&out_dir.join("03b-ssim-cement-bench.csv"), &rows)?;
    write_md(&out_dir.join("03b-ssim-cement-bench.md"), &rows)?;
    println!("[ssim_cement_bench] wrote {} rows to {}", rows.len(), out_dir.display());
    Ok(())
}

fn run_metric(ref_rgba: &[u8], dist_rgba: &[u8], n: usize, runs: usize) -> Vec<f64> {
    let (w, h) = guess_wh(n);
    let mut times = Vec::with_capacity(runs);
    for _ in 0..runs {
        let r = build_rgb(ref_rgba, w, h);
        let d = build_rgb(dist_rgba, w, h);
        let t0 = Instant::now();
        let _ = compute_frame_ssimulacra2(r, d).expect("metric");
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    times
}

fn score_once(ref_rgba: &[u8], dist_rgba: &[u8], n: usize) -> f64 {
    let (w, h) = guess_wh(n);
    let r = build_rgb(ref_rgba, w, h);
    let d = build_rgb(dist_rgba, w, h);
    compute_frame_ssimulacra2(r, d).expect("metric")
}

fn build_rgb(rgba: &[u8], w: usize, h: usize) -> Rgb {
    let pixels: Vec<[f32; 3]> = rgba.chunks_exact(4)
        .map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0])
        .collect();
    Rgb::new(pixels, w, h, TransferCharacteristic::SRGB, ColorPrimaries::BT709)
        .expect("rgb")
}

fn guess_wh(n: usize) -> (usize, usize) {
    match n {
        399_424 => (632, 632),
        960_000 => (1200, 800),
        1_440_000 => (1600, 900),
        _ => panic!("unknown pixel count {n}"),
    }
}

fn median(xs: &[f64]) -> f64 {
    let mut s = xs.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s[s.len() / 2]
}

fn pyramid_bandwidth(n: usize, median_ms: f64) -> f64 {
    // 6-scale pyramid + 3 maps each + 1-norm + 4-norm aggregation.
    // Memory traffic estimate: pyramid build ~ 1.33 × N × 4 byte × 3
    // channel × 2 images = ~32 byte/pixel-equivalent streaming. This
    // is a rough upper bound; the actual reads exceed it because
    // SSIM windows over-read.
    let bytes = n as f64 * 32.0;
    bytes / (median_ms / 1000.0) / 1e9
}

fn write_csv(path: &Path, rows: &[Row]) -> Result<()> {
    use std::io::Write;
    let mut f = fs::File::create(path)?;
    writeln!(f, "image,n_pixels,pass,median_ms,score,bandwidth_gbps")?;
    for r in rows {
        writeln!(f, "{},{},{},{:.3},{:.3},{:.2}",
            r.image, r.n_pixels, r.pass, r.median_ms, r.score, r.bandwidth_gbps)?;
    }
    Ok(())
}

fn write_md(path: &Path, rows: &[Row]) -> Result<()> {
    use std::fmt::Write;
    let mut s = String::new();
    writeln!(&mut s, "# 03b-ssim-cement-bench — raw output\n")?;
    writeln!(&mut s, "Generated by `cargo run --release -p nupic-research --example ssim_cement_bench`.\n")?;
    writeln!(&mut s, "Pure SSIMULACRA2 metric cost (no quant, no oxipng).\n")?;
    writeln!(&mut s, "| image | n_px | pass | median_ms | score | est bw GB/s |")?;
    writeln!(&mut s, "|---|---:|---|---:|---:|---:|")?;
    for r in rows {
        writeln!(&mut s,
            "| `{}` | {} | {} | {:.3} | {:.3} | {:.2} |",
            r.image, r.n_pixels, r.pass, r.median_ms, r.score, r.bandwidth_gbps)?;
    }
    fs::write(path, s)?;
    Ok(())
}

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(Path::new(m).ancestors().nth(2).context("workspace root")?.to_path_buf())
}
