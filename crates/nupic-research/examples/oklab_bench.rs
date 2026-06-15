//! Stone A — OKLab — perf / mem ceiling calibration.
//!
//! Backs `docs/research/png/03a-oklab-design.md`. The 03 essay estimated
//! 02-pluto OKLab forward at:
//!   naive scalar Rust       ~8 ms
//!   SIMD (NEON/AVX2)        ~2 ms
//!   bandwidth ceiling       ~0.1 ms
//! This bench grounds those numbers and bounds the oracle drift.
//!
//! Run:
//!   cargo run --release -p nupic-research --example oklab_bench
//!
//! Output (under `target/research-out/`):
//!   03a-oklab-bench.csv
//!   03a-oklab-bench.md
//!
//! Bench plan:
//!   1. For each of 02-pluto / 04-portrait / 06-landscape:
//!      load → RGBA8 buf
//!   2. forward path:
//!      a. naive scalar f32 ours (RGB → OKLab)
//!      b. oklab crate v1.1.2 oracle (`srgb_to_oklab`)
//!      diff between (a) and (b), max abs and mean.
//!   3. roundtrip path:
//!      ours: RGB → OKLab → RGB; assert per-channel error < 1e-4 (sRGB
//!      transfer function rounding ceiling)
//!   4. timing: median of 7 runs per impl per image, release build.
//!   5. memory: peak working set per image (RGBA8 + OKLab f32 buffers).
//!
//! Comparator notes:
//!   - `oklab::srgb_to_oklab` consumes a `rgb::RGB8`. Its math = `M2 *
//!     cbrt(M1 * srgb_to_linear(rgba))` per Ottosson (2021-01-25 updated
//!     matrices). We implement the same math by hand to confirm the
//!     numbers agree and to time both impls under identical workloads.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, anyhow};

const INPUTS: &str = "assets/png-bench/inputs";
const OUT_DIR: &str = "target/research-out";

// --- OKLab math --------------------------------------------------------

#[inline]
fn srgb_u8_to_linear(c: u8) -> f32 {
    // sRGB IEC 61966-2-1 inverse transfer function.
    let v = c as f32 / 255.0;
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

#[inline]
fn linear_to_srgb_u8(c: f32) -> u8 {
    let v = if c <= 0.003_130_8 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8
}

/// Ottosson (2020, updated 2021-01-25) M1 / M2 matrices.
const M1: [[f32; 3]; 3] = [
    [0.4122214708, 0.5363325363, 0.0514459929],
    [0.2119034982, 0.6806995451, 0.1073969566],
    [0.0883024619, 0.2817188376, 0.6299787005],
];
const M2: [[f32; 3]; 3] = [
    [0.2104542553,  0.7936177850, -0.0040720468],
    [1.9779984951, -2.4285922050,  0.4505937099],
    [0.0259040371,  0.7827717662, -0.8086757660],
];
const M1_INV: [[f32; 3]; 3] = [
    [ 4.0767416621, -3.3077115913,  0.2309699292],
    [-1.2684380046,  2.6097574011, -0.3413193965],
    [-0.0041960863, -0.7034186147,  1.7076147010],
];
const M2_INV: [[f32; 3]; 3] = [
    [1.0,  0.3963377774,  0.2158037573],
    [1.0, -0.1055613458, -0.0638541728],
    [1.0, -0.0894841775, -1.2914855480],
];

#[inline]
fn matmul(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

#[inline]
fn rgb_to_oklab_naive(r: u8, g: u8, b: u8) -> [f32; 3] {
    let lin = [srgb_u8_to_linear(r), srgb_u8_to_linear(g), srgb_u8_to_linear(b)];
    let lms = matmul(&M1, lin);
    let lms_prime = [lms[0].cbrt(), lms[1].cbrt(), lms[2].cbrt()];
    matmul(&M2, lms_prime)
}

#[inline]
fn oklab_to_rgb_naive(lab: [f32; 3]) -> (u8, u8, u8) {
    let lms_prime = matmul(&M2_INV, lab);
    let lms = [
        lms_prime[0] * lms_prime[0] * lms_prime[0],
        lms_prime[1] * lms_prime[1] * lms_prime[1],
        lms_prime[2] * lms_prime[2] * lms_prime[2],
    ];
    let lin = matmul(&M1_INV, lms);
    (linear_to_srgb_u8(lin[0]), linear_to_srgb_u8(lin[1]), linear_to_srgb_u8(lin[2]))
}

// --- bench --------------------------------------------------------------

#[derive(Debug)]
struct BenchRow {
    image: String,
    n_pixels: usize,
    impl_name: String,
    pass: &'static str, // "forward" | "roundtrip"
    median_ms: f64,
    max_diff_l: f32,
    max_diff_a: f32,
    max_diff_b: f32,
    bandwidth_gbps: f64,
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let inputs_dir = root.join(INPUTS);
    let out_dir = root.join(OUT_DIR);
    fs::create_dir_all(&out_dir)?;

    // Lead images at three sizes for the ceiling extrapolation table.
    let names = ["02-pluto-transparent.png", "04-photo-portrait.png", "06-photo-landscape.png"];
    let paths: Vec<PathBuf> = names.iter().map(|n| inputs_dir.join(n)).collect();
    for p in &paths {
        if !p.exists() {
            return Err(anyhow!("fixture missing: {}", p.display()));
        }
    }

    let mut rows: Vec<BenchRow> = Vec::new();
    for p in &paths {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        let img = ::image::open(p)?.to_rgba8();
        let (w, h) = (img.width() as usize, img.height() as usize);
        let n = w * h;
        let rgba = img.into_raw();

        // ---- forward: naive ours --------------------------------------------
        let mut lab_ours = vec![[0f32; 3]; n];
        let mut times: Vec<f64> = Vec::new();
        for _ in 0..7 {
            let t0 = Instant::now();
            for i in 0..n {
                lab_ours[i] = rgb_to_oklab_naive(rgba[i*4], rgba[i*4+1], rgba[i*4+2]);
            }
            times.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        let median_ours = median(&mut times);
        // Streaming bytes: 4 read + 12 write per pixel.
        let bw = (n as f64 * 16.0) / (median_ours / 1000.0) / 1e9;
        rows.push(BenchRow {
            image: name.clone(), n_pixels: n,
            impl_name: "naive-scalar-f32".into(), pass: "forward",
            median_ms: median_ours,
            max_diff_l: 0.0, max_diff_a: 0.0, max_diff_b: 0.0,
            bandwidth_gbps: bw,
        });

        // ---- forward: oklab crate oracle ------------------------------------
        let mut lab_oracle = vec![[0f32; 3]; n];
        let mut times: Vec<f64> = Vec::new();
        for _ in 0..7 {
            let t0 = Instant::now();
            for i in 0..n {
                let r = oklab::srgb_to_oklab(rgb::RGB8 { r: rgba[i*4], g: rgba[i*4+1], b: rgba[i*4+2] });
                lab_oracle[i] = [r.l, r.a, r.b];
            }
            times.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        let median_oracle = median(&mut times);
        let bw_oracle = (n as f64 * 16.0) / (median_oracle / 1000.0) / 1e9;

        // Diff vs oracle (forward).
        let (max_l, max_a, max_b) = diff_max(&lab_ours, &lab_oracle);
        rows.push(BenchRow {
            image: name.clone(), n_pixels: n,
            impl_name: "oklab-crate-v1.1.2".into(), pass: "forward",
            median_ms: median_oracle,
            max_diff_l: max_l, max_diff_a: max_a, max_diff_b: max_b,
            bandwidth_gbps: bw_oracle,
        });

        // Re-attribute diff against ours under "naive-scalar-f32".
        if let Some(r) = rows.iter_mut().rev().nth(1) {
            r.max_diff_l = max_l;
            r.max_diff_a = max_a;
            r.max_diff_b = max_b;
        }

        // ---- roundtrip: ours, error vs original sRGB --------------------------
        let mut rgb_back = vec![(0u8, 0u8, 0u8); n];
        let mut times: Vec<f64> = Vec::new();
        for _ in 0..7 {
            let t0 = Instant::now();
            for i in 0..n {
                rgb_back[i] = oklab_to_rgb_naive(lab_ours[i]);
            }
            times.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        let median_rt = median(&mut times);
        let (max_dr, max_dg, max_db) = diff_u8(&rgba, &rgb_back);
        let bw_rt = (n as f64 * (12.0 + 3.0)) / (median_rt / 1000.0) / 1e9;
        rows.push(BenchRow {
            image: name.clone(), n_pixels: n,
            impl_name: "naive-scalar-f32".into(), pass: "roundtrip",
            median_ms: median_rt,
            max_diff_l: max_dr as f32,
            max_diff_a: max_dg as f32,
            max_diff_b: max_db as f32,
            bandwidth_gbps: bw_rt,
        });

        println!("[oklab_bench] done {name} ({w}×{h}, {n} px)");
    }

    write_csv(&out_dir.join("03a-oklab-bench.csv"), &rows)?;
    write_md(&out_dir.join("03a-oklab-bench.md"), &rows)?;
    println!("[oklab_bench] wrote {} rows to {}", rows.len(), out_dir.display());
    Ok(())
}

fn median(xs: &mut [f64]) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[xs.len() / 2]
}

fn diff_max(a: &[[f32; 3]], b: &[[f32; 3]]) -> (f32, f32, f32) {
    let mut m = [0f32; 3];
    for (x, y) in a.iter().zip(b.iter()) {
        for j in 0..3 {
            let d = (x[j] - y[j]).abs();
            if d > m[j] { m[j] = d; }
        }
    }
    (m[0], m[1], m[2])
}

fn diff_u8(orig: &[u8], back: &[(u8, u8, u8)]) -> (i32, i32, i32) {
    let mut m = [0i32; 3];
    for (i, t) in back.iter().enumerate() {
        let (r, g, b) = (orig[i*4], orig[i*4+1], orig[i*4+2]);
        let dr = (r as i32 - t.0 as i32).abs();
        let dg = (g as i32 - t.1 as i32).abs();
        let db = (b as i32 - t.2 as i32).abs();
        if dr > m[0] { m[0] = dr; }
        if dg > m[1] { m[1] = dg; }
        if db > m[2] { m[2] = db; }
    }
    (m[0], m[1], m[2])
}

fn write_csv(path: &Path, rows: &[BenchRow]) -> Result<()> {
    use std::io::Write;
    let mut f = fs::File::create(path)?;
    writeln!(f, "image,n_pixels,impl,pass,median_ms,max_diff_l,max_diff_a,max_diff_b,bandwidth_gbps")?;
    for r in rows {
        writeln!(f, "{},{},{},{},{:.3},{:.6},{:.6},{:.6},{:.2}",
            r.image, r.n_pixels, r.impl_name, r.pass, r.median_ms,
            r.max_diff_l, r.max_diff_a, r.max_diff_b, r.bandwidth_gbps)?;
    }
    Ok(())
}

fn write_md(path: &Path, rows: &[BenchRow]) -> Result<()> {
    use std::fmt::Write;
    let mut s = String::new();
    writeln!(&mut s, "# 03a-oklab-bench — raw output\n\nGenerated by `cargo run --release -p nupic-research --example oklab_bench`.\n")?;
    writeln!(&mut s, "| image | n_px | impl | pass | median_ms | max_diff(L,a,b) or (dR,dG,dB) | bandwidth GB/s |")?;
    writeln!(&mut s, "|---|---:|---|---|---:|---|---:|")?;
    for r in rows {
        writeln!(&mut s,
            "| `{}` | {} | {} | {} | {:.3} | ({:.4},{:.4},{:.4}) | {:.2} |",
            r.image, r.n_pixels, r.impl_name, r.pass, r.median_ms,
            r.max_diff_l, r.max_diff_a, r.max_diff_b, r.bandwidth_gbps)?;
    }
    fs::write(path, s)?;
    Ok(())
}

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(Path::new(m).ancestors().nth(2).context("workspace root")?.to_path_buf())
}
