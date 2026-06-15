//! Stone B — B1 baseline reimpl vs cement.
//!
//! Compares `nupic_research::ssim_b1::ssimulacra2_score_srgb` against
//! `ssimulacra2` crate v0.5.1 on the three lead images. Emits a
//! markdown + CSV summary with score diff + timing diff.
//!
//! Run:
//!   cargo run --release -p nupic-research --example ssim_b1_bench

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use nupic_research::ssim_b1::{ssimulacra2_score_srgb, ssimulacra2_score_srgb_chunked};
use ssimulacra2::{ColorPrimaries, Rgb, TransferCharacteristic, compute_frame_ssimulacra2};

const INPUTS: &str = "assets/png-bench/inputs";
const TINYPNG: &str = "assets/png-bench/tinypng-web";
const OUT_DIR: &str = "target/research-out";

#[derive(Debug)]
struct Row {
    image: String,
    n_pixels: usize,
    pass: &'static str,
    cement_ms: f64,
    b1_ms: f64,
    b2_ms: f64,
    cement_score: f64,
    b1_score: f64,
    b2_score: f64,
    b1_diff: f64,
    b2_diff: f64,
    b1_over_cement: f64,
    b2_over_cement: f64,
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let inputs_dir = root.join(INPUTS);
    let tinypng_dir = root.join(TINYPNG);
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
        let img = ::image::open(&p)?.to_rgba8();
        let (w, h) = (img.width() as usize, img.height() as usize);
        let n = w * h;
        let raw = img.into_raw();
        let srgb_f32: Vec<[f32; 3]> = raw
            .chunks_exact(4)
            .map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0])
            .collect();

        // self-vs-self
        let (cm, cs) = time_cement(&srgb_f32, &srgb_f32, w, h, 5);
        let (b1m, b1s) = time_b1(&srgb_f32, &srgb_f32, w, h, 5);
        let (b2m, b2s) = time_b2(&srgb_f32, &srgb_f32, w, h, 5);
        rows.push(make_row(name, n, "self", cm, b1m, b2m, cs, b1s, b2s));

        // vs tinypng
        let tp = tinypng_dir.join(name);
        if let Ok(tp_bytes) = fs::read(&tp) {
            if let Ok(d) = ::image::load_from_memory_with_format(&tp_bytes, ::image::ImageFormat::Png) {
                let d_rgba = d.to_rgba8();
                if (d_rgba.width() * d_rgba.height()) as usize == n {
                    let d_raw = d_rgba.into_raw();
                    let d_f32: Vec<[f32; 3]> = d_raw
                        .chunks_exact(4)
                        .map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0])
                        .collect();
                    let (cm, cs) = time_cement(&srgb_f32, &d_f32, w, h, 5);
                    let (b1m, b1s) = time_b1(&srgb_f32, &d_f32, w, h, 5);
                    let (b2m, b2s) = time_b2(&srgb_f32, &d_f32, w, h, 5);
                    rows.push(make_row(name, n, "vs-tinypng", cm, b1m, b2m, cs, b1s, b2s));
                }
            }
        }
        println!("[ssim_b1_bench] done {name}");
    }
    write_csv(&out_dir.join("03b-bis-ssim-b1-bench.csv"), &rows)?;
    write_md(&out_dir.join("03b-bis-ssim-b1-bench.md"), &rows)?;
    println!("[ssim_b1_bench] wrote {} rows", rows.len());
    Ok(())
}

fn time_cement(r: &[[f32; 3]], d: &[[f32; 3]], w: usize, h: usize, runs: usize) -> (f64, f64) {
    let mut ts = Vec::with_capacity(runs);
    let mut score = 0.0f64;
    for _ in 0..runs {
        let rr = Rgb::new(r.to_vec(), w, h, TransferCharacteristic::SRGB, ColorPrimaries::BT709).expect("rgb");
        let dr = Rgb::new(d.to_vec(), w, h, TransferCharacteristic::SRGB, ColorPrimaries::BT709).expect("rgb");
        let t0 = Instant::now();
        score = compute_frame_ssimulacra2(rr, dr).expect("metric");
        ts.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    ts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (ts[ts.len() / 2], score)
}

fn time_b1(r: &[[f32; 3]], d: &[[f32; 3]], w: usize, h: usize, runs: usize) -> (f64, f64) {
    let mut ts = Vec::with_capacity(runs);
    let mut score = 0.0f64;
    for _ in 0..runs {
        let t0 = Instant::now();
        score = ssimulacra2_score_srgb(r, d, w, h).expect("b1");
        ts.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    ts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (ts[ts.len() / 2], score)
}

fn time_b2(r: &[[f32; 3]], d: &[[f32; 3]], w: usize, h: usize, runs: usize) -> (f64, f64) {
    let mut ts = Vec::with_capacity(runs);
    let mut score = 0.0f64;
    for _ in 0..runs {
        let t0 = Instant::now();
        score = ssimulacra2_score_srgb_chunked(r, d, w, h).expect("b2");
        ts.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    ts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (ts[ts.len() / 2], score)
}

#[allow(clippy::too_many_arguments)]
fn make_row(
    name: &str, n: usize, pass: &'static str,
    cm: f64, b1m: f64, b2m: f64,
    cs: f64, b1s: f64, b2s: f64,
) -> Row {
    Row {
        image: name.to_string(),
        n_pixels: n,
        pass,
        cement_ms: cm,
        b1_ms: b1m,
        b2_ms: b2m,
        cement_score: cs,
        b1_score: b1s,
        b2_score: b2s,
        b1_diff: (cs - b1s).abs(),
        b2_diff: (cs - b2s).abs(),
        b1_over_cement: b1m / cm,
        b2_over_cement: b2m / cm,
    }
}

fn write_csv(path: &Path, rows: &[Row]) -> Result<()> {
    use std::io::Write;
    let mut f = fs::File::create(path)?;
    writeln!(f, "image,n_pixels,pass,cement_ms,b1_ms,b2_ms,cement_score,b1_score,b2_score,b1_diff,b2_diff,b1_over_cement,b2_over_cement")?;
    for r in rows {
        writeln!(f, "{},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.4},{:.4},{:.2},{:.2}",
            r.image, r.n_pixels, r.pass, r.cement_ms, r.b1_ms, r.b2_ms,
            r.cement_score, r.b1_score, r.b2_score,
            r.b1_diff, r.b2_diff, r.b1_over_cement, r.b2_over_cement)?;
    }
    Ok(())
}

fn write_md(path: &Path, rows: &[Row]) -> Result<()> {
    use std::fmt::Write;
    let mut s = String::new();
    writeln!(&mut s, "# 03b-bis-ssim-b1-bench — B1 vs B2 vs cement\n")?;
    writeln!(&mut s, "Generated by `cargo run --release -p nupic-research --example ssim_b1_bench`.\n")?;
    writeln!(&mut s, "| image | pass | cement_ms | B1_ms | B2_ms | B1/cement | B2/cement | B1_diff | B2_diff |")?;
    writeln!(&mut s, "|---|---|---:|---:|---:|---:|---:|---:|---:|")?;
    for r in rows {
        writeln!(&mut s,
            "| `{}` | {} | {:.3} | {:.3} | {:.3} | {:.2}× | {:.2}× | {:.4} | {:.4} |",
            r.image, r.pass,
            r.cement_ms, r.b1_ms, r.b2_ms,
            r.b1_over_cement, r.b2_over_cement,
            r.b1_diff, r.b2_diff)?;
    }
    fs::write(path, s)?;
    Ok(())
}

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(Path::new(m).ancestors().nth(2).context("workspace root")?.to_path_buf())
}
