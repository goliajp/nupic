//! Stone A — OKLab — SIMD / LUT ceiling calibration (`A1`, `A2` phases).
//!
//! Backs `docs/research/png/03a-bis-oklab-simd.md`. The 03a essay
//! measured A0 (naive scalar) at 8.18 ms / 02-pluto. This bench
//! drills the next ceiling-attack rungs by adding:
//!
//!   A1a  scalar + `fast-srgb8` LUT for sRGB → linear
//!   A1b  scalar + LUT + cbrt polynomial Halley iteration
//!   A2   SIMD f32x4 via `wide` (sRGB LUT + matmul SIMD + cbrt scalar)
//!   A2+  SIMD f32x4 + cbrt SIMD via Halley iteration
//!
//! For each impl, on each of 02-pluto / 04-portrait / 06-landscape:
//!   - 7-run median forward-pass timing (release)
//!   - max abs diff vs the oklab v1.1.2 oracle on L, a, b
//!   - effective streaming bandwidth (4 B read + 12 B write per pixel)
//!
//! Run:
//!   cargo run --release -p nupic-research --example oklab_simd_bench
//!
//! Output (under `target/research-out/`):
//!   03a-bis-oklab-simd-bench.csv
//!   03a-bis-oklab-simd-bench.md

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use wide::f32x4;

const INPUTS: &str = "assets/png-bench/inputs";
const OUT_DIR: &str = "target/research-out";

// --- OKLab math constants (mirror oklab_bench.rs / Ottosson 2021-01-25)

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

// --- A0: scalar naive (re-defined here to make this bench self-contained)

#[inline]
fn srgb_u8_to_linear_naive(c: u8) -> f32 {
    let v = c as f32 / 255.0;
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

#[inline]
fn rgb_to_oklab_a0(r: u8, g: u8, b: u8) -> [f32; 3] {
    let lin = [
        srgb_u8_to_linear_naive(r),
        srgb_u8_to_linear_naive(g),
        srgb_u8_to_linear_naive(b),
    ];
    let l = M1[0][0] * lin[0] + M1[0][1] * lin[1] + M1[0][2] * lin[2];
    let m = M1[1][0] * lin[0] + M1[1][1] * lin[1] + M1[1][2] * lin[2];
    let s = M1[2][0] * lin[0] + M1[2][1] * lin[1] + M1[2][2] * lin[2];
    let lp = l.cbrt();
    let mp = m.cbrt();
    let sp = s.cbrt();
    [
        M2[0][0] * lp + M2[0][1] * mp + M2[0][2] * sp,
        M2[1][0] * lp + M2[1][1] * mp + M2[1][2] * sp,
        M2[2][0] * lp + M2[2][1] * mp + M2[2][2] * sp,
    ]
}

// --- A1a: scalar + fast-srgb8 LUT

#[inline]
fn rgb_to_oklab_a1a(r: u8, g: u8, b: u8) -> [f32; 3] {
    let lin = [
        fast_srgb8::srgb8_to_f32(r),
        fast_srgb8::srgb8_to_f32(g),
        fast_srgb8::srgb8_to_f32(b),
    ];
    let l = M1[0][0] * lin[0] + M1[0][1] * lin[1] + M1[0][2] * lin[2];
    let m = M1[1][0] * lin[0] + M1[1][1] * lin[1] + M1[1][2] * lin[2];
    let s = M1[2][0] * lin[0] + M1[2][1] * lin[1] + M1[2][2] * lin[2];
    let lp = l.cbrt();
    let mp = m.cbrt();
    let sp = s.cbrt();
    [
        M2[0][0] * lp + M2[0][1] * mp + M2[0][2] * sp,
        M2[1][0] * lp + M2[1][1] * mp + M2[1][2] * sp,
        M2[2][0] * lp + M2[2][1] * mp + M2[2][2] * sp,
    ]
}

// --- A1b: scalar + LUT + Halley-cbrt(faster than libm `cbrt`)
// Halley iteration on cube root:
//   given y = cbrt(x), iterate y' = y * (y³ + 2x) / (2y³ + x)
//   1-2 iterations from a rough initial guess achieve f32 precision.

#[inline]
fn cbrt_halley(x: f32) -> f32 {
    if x == 0.0 { return 0.0; }
    let sign = x.signum();
    let ax = x.abs();
    // Initial guess: bit-trick approximation
    //   For positive x, log2(cbrt(x)) = log2(x) / 3
    //   Approximation via floating-point bit manipulation (Quake-style).
    let bits = ax.to_bits();
    // Bias-corrected divide-by-3 in the exponent field.
    let init_bits = bits / 3 + (0x3f800000u32 / 3) * 2;
    let mut y = f32::from_bits(init_bits);
    // Two Halley iterations to converge to ~24-bit precision.
    for _ in 0..2 {
        let y3 = y * y * y;
        y = y * (y3 + 2.0 * ax) / (2.0 * y3 + ax);
    }
    sign * y
}

#[inline]
fn rgb_to_oklab_a1b(r: u8, g: u8, b: u8) -> [f32; 3] {
    let lin = [
        fast_srgb8::srgb8_to_f32(r),
        fast_srgb8::srgb8_to_f32(g),
        fast_srgb8::srgb8_to_f32(b),
    ];
    let l = M1[0][0] * lin[0] + M1[0][1] * lin[1] + M1[0][2] * lin[2];
    let m = M1[1][0] * lin[0] + M1[1][1] * lin[1] + M1[1][2] * lin[2];
    let s = M1[2][0] * lin[0] + M1[2][1] * lin[1] + M1[2][2] * lin[2];
    let lp = cbrt_halley(l);
    let mp = cbrt_halley(m);
    let sp = cbrt_halley(s);
    [
        M2[0][0] * lp + M2[0][1] * mp + M2[0][2] * sp,
        M2[1][0] * lp + M2[1][1] * mp + M2[1][2] * sp,
        M2[2][0] * lp + M2[2][1] * mp + M2[2][2] * sp,
    ]
}

// --- A2: SIMD f32x4 batch (4 pixels per iter), cbrt via scalar Halley.
// `wide::f32x4` portable SIMD wrapper.

#[inline]
fn cbrt_halley_simd(x: f32x4) -> f32x4 {
    // SIMD-able rendition of cbrt_halley. Sign handling assumes inputs
    // are non-negative (linear sRGB before matrix is ≥ 0; after M1 also ≥ 0
    // because M1 row sums > 0 and entries ≥ 0).
    let bits: [u32; 4] = unsafe {
        std::mem::transmute(x)  // f32x4 -> [f32; 4] -> [u32; 4]
    };
    let init_bits = [
        bits[0] / 3 + (0x3f800000u32 / 3) * 2,
        bits[1] / 3 + (0x3f800000u32 / 3) * 2,
        bits[2] / 3 + (0x3f800000u32 / 3) * 2,
        bits[3] / 3 + (0x3f800000u32 / 3) * 2,
    ];
    let mut y: f32x4 = unsafe { std::mem::transmute(init_bits) };
    for _ in 0..2 {
        let y3 = y * y * y;
        let num = y * (y3 + f32x4::splat(2.0) * x);
        let den = f32x4::splat(2.0) * y3 + x;
        y = num / den;
    }
    y
}

fn rgb_chunk4_to_oklab_a2(chunk: &[u8; 16], out: &mut [[f32; 3]; 4]) {
    // chunk holds 4 RGBA8 pixels = 16 bytes
    let r = f32x4::from([
        fast_srgb8::srgb8_to_f32(chunk[0]),
        fast_srgb8::srgb8_to_f32(chunk[4]),
        fast_srgb8::srgb8_to_f32(chunk[8]),
        fast_srgb8::srgb8_to_f32(chunk[12]),
    ]);
    let g = f32x4::from([
        fast_srgb8::srgb8_to_f32(chunk[1]),
        fast_srgb8::srgb8_to_f32(chunk[5]),
        fast_srgb8::srgb8_to_f32(chunk[9]),
        fast_srgb8::srgb8_to_f32(chunk[13]),
    ]);
    let b = f32x4::from([
        fast_srgb8::srgb8_to_f32(chunk[2]),
        fast_srgb8::srgb8_to_f32(chunk[6]),
        fast_srgb8::srgb8_to_f32(chunk[10]),
        fast_srgb8::srgb8_to_f32(chunk[14]),
    ]);
    let l = f32x4::splat(M1[0][0]) * r + f32x4::splat(M1[0][1]) * g + f32x4::splat(M1[0][2]) * b;
    let m = f32x4::splat(M1[1][0]) * r + f32x4::splat(M1[1][1]) * g + f32x4::splat(M1[1][2]) * b;
    let s = f32x4::splat(M1[2][0]) * r + f32x4::splat(M1[2][1]) * g + f32x4::splat(M1[2][2]) * b;
    let lp = cbrt_halley_simd(l);
    let mp = cbrt_halley_simd(m);
    let sp = cbrt_halley_simd(s);
    let l_okl = f32x4::splat(M2[0][0]) * lp + f32x4::splat(M2[0][1]) * mp + f32x4::splat(M2[0][2]) * sp;
    let a_okl = f32x4::splat(M2[1][0]) * lp + f32x4::splat(M2[1][1]) * mp + f32x4::splat(M2[1][2]) * sp;
    let b_okl = f32x4::splat(M2[2][0]) * lp + f32x4::splat(M2[2][1]) * mp + f32x4::splat(M2[2][2]) * sp;
    let la: [f32; 4] = l_okl.into();
    let aa: [f32; 4] = a_okl.into();
    let ba: [f32; 4] = b_okl.into();
    for i in 0..4 {
        out[i] = [la[i], aa[i], ba[i]];
    }
}

// --- bench harness

#[derive(Debug)]
struct Row {
    image: String,
    n_pixels: usize,
    impl_name: String,
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
    let names = ["02-pluto-transparent.png", "04-photo-portrait.png", "06-photo-landscape.png"];
    let mut rows: Vec<Row> = Vec::new();

    for name in &names {
        let p = inputs_dir.join(name);
        if !p.exists() { return Err(anyhow!("missing fixture {}", p.display())); }
        let img = ::image::open(&p)?.to_rgba8();
        let n = (img.width() * img.height()) as usize;
        let rgba = img.into_raw();

        // Compute oracle (oklab crate) once for diff.
        let mut oracle: Vec<[f32; 3]> = Vec::with_capacity(n);
        for i in 0..n {
            let r = oklab::srgb_to_oklab(rgb::RGB8 {
                r: rgba[i*4], g: rgba[i*4+1], b: rgba[i*4+2],
            });
            oracle.push([r.l, r.a, r.b]);
        }

        // A0 — scalar naive
        rows.push(time_impl(name, n, "A0-naive-scalar", &rgba, &oracle, |rgba, out| {
            for i in 0..out.len() {
                out[i] = rgb_to_oklab_a0(rgba[i*4], rgba[i*4+1], rgba[i*4+2]);
            }
        }));

        // A1a — scalar + LUT sRGB
        rows.push(time_impl(name, n, "A1a-LUT-srgb", &rgba, &oracle, |rgba, out| {
            for i in 0..out.len() {
                out[i] = rgb_to_oklab_a1a(rgba[i*4], rgba[i*4+1], rgba[i*4+2]);
            }
        }));

        // A1b — scalar + LUT + Halley cbrt
        rows.push(time_impl(name, n, "A1b-LUT-Halley", &rgba, &oracle, |rgba, out| {
            for i in 0..out.len() {
                out[i] = rgb_to_oklab_a1b(rgba[i*4], rgba[i*4+1], rgba[i*4+2]);
            }
        }));

        // A2 — SIMD f32x4 batch (LUT + Halley SIMD)
        rows.push(time_impl(name, n, "A2-SIMD-f32x4", &rgba, &oracle, |rgba, out| {
            let mut i = 0;
            let blocks = out.len() / 4;
            for blk in 0..blocks {
                let offset = blk * 16;
                let chunk: &[u8; 16] = rgba[offset..offset + 16].try_into().unwrap();
                let mut buf = [[0f32; 3]; 4];
                rgb_chunk4_to_oklab_a2(chunk, &mut buf);
                for j in 0..4 {
                    out[blk * 4 + j] = buf[j];
                    i += 1;
                }
            }
            // tail
            while i < out.len() {
                out[i] = rgb_to_oklab_a1b(rgba[i*4], rgba[i*4+1], rgba[i*4+2]);
                i += 1;
            }
        }));

        println!("[oklab_simd_bench] done {name} ({n} px)");
    }

    write_csv(&out_dir.join("03a-bis-oklab-simd-bench.csv"), &rows)?;
    write_md(&out_dir.join("03a-bis-oklab-simd-bench.md"), &rows)?;
    println!("[oklab_simd_bench] wrote {} rows to {}", rows.len(), out_dir.display());
    Ok(())
}

fn time_impl(
    name: &str,
    n: usize,
    label: &str,
    rgba: &[u8],
    oracle: &[[f32; 3]],
    f: impl Fn(&[u8], &mut [[f32; 3]]),
) -> Row {
    let mut out = vec![[0f32; 3]; n];
    let mut times = Vec::with_capacity(7);
    for _ in 0..7 {
        let t0 = Instant::now();
        f(rgba, &mut out);
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = times[times.len() / 2];
    let (dl, da, db) = diff_max(&out, oracle);
    let bw = (n as f64 * 16.0) / (median / 1000.0) / 1e9;
    Row {
        image: name.to_string(),
        n_pixels: n,
        impl_name: label.to_string(),
        median_ms: median,
        max_diff_l: dl,
        max_diff_a: da,
        max_diff_b: db,
        bandwidth_gbps: bw,
    }
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

fn write_csv(path: &Path, rows: &[Row]) -> Result<()> {
    use std::io::Write;
    let mut f = fs::File::create(path)?;
    writeln!(f, "image,n_pixels,impl,median_ms,max_diff_l,max_diff_a,max_diff_b,bandwidth_gbps")?;
    for r in rows {
        writeln!(f, "{},{},{},{:.3},{:.6},{:.6},{:.6},{:.2}",
            r.image, r.n_pixels, r.impl_name, r.median_ms,
            r.max_diff_l, r.max_diff_a, r.max_diff_b, r.bandwidth_gbps)?;
    }
    Ok(())
}

fn write_md(path: &Path, rows: &[Row]) -> Result<()> {
    use std::fmt::Write;
    let mut s = String::new();
    writeln!(&mut s, "# 03a-bis-oklab-simd-bench — raw output\n")?;
    writeln!(&mut s, "Generated by `cargo run --release -p nupic-research --example oklab_simd_bench`.\n")?;
    writeln!(&mut s, "| image | n_px | impl | median_ms | max_diff(L,a,b) | bandwidth GB/s |")?;
    writeln!(&mut s, "|---|---:|---|---:|---|---:|")?;
    for r in rows {
        writeln!(&mut s,
            "| `{}` | {} | {} | {:.3} | ({:.5},{:.5},{:.5}) | {:.2} |",
            r.image, r.n_pixels, r.impl_name, r.median_ms,
            r.max_diff_l, r.max_diff_a, r.max_diff_b, r.bandwidth_gbps)?;
    }
    fs::write(path, s)?;
    Ok(())
}

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(Path::new(m).ancestors().nth(2).context("workspace root")?.to_path_buf())
}
