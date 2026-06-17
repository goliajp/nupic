//! Cycle 86 — R1 M-weighted Lloyd spike on 04 portrait, n=192
//!
//! Per Cycle 85 (R2 α-expansion) negative result: the OKLab L² + Potts
//! energy itself is SSIM-misaligned, so deeper optimization of the same
//! energy hits a ceiling (+0.25 SSIM vs ICM). R1 attacks the metric.
//!
//! Diagonal Mahalanobis k-means:
//!   - per-pixel scalar weight  b_i  from OKLab L channel multi-scale
//!     Gaussian-pyramid bandpass  (|DoG_σ1-σ2| + |DoG_σ2-σ4| + ε)
//!   - per-channel diagonal weight  {w_L, w_a, w_b}  on OKLab axes
//!   - d²(p, c) = b_i · (w_L Δl² + w_a Δa² + w_b Δb²)
//!
//! Centroid update has a closed form (per-channel w_d and per-pixel
//! b_i factor through the gradient):
//!   c_j[d] = Σ_{i in j} b_i · p_i[d]  /  Σ_{i in j} b_i
//! No inner-loop GD needed.
//!
//! Decision gate (from roadmap): ΔSSIM ≥ +1.0 vs ICM run here → R1 paper path
//!
//! Env knobs:
//!   MWL_ITERS  = 10     Lloyd iter cap
//!   MWL_NCOL   = 192    palette size
//!   MWL_WCHROM = 0.25   chroma channel weight (luma = 1.0)
//!   MWL_EPS    = 0.005  b_i floor (5% of typical OKLab-L range)

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use image::ImageReader;
use rgb::Rgb;

use nupic_color::{Oklab, oklab_to_srgb_u8, srgb_u8_to_oklab};
use nupic_quantize::{
    apply_palette_rgba, encode_indexed_png_with_alpha, refine_palette_kmeans,
    train_palette_rgba,
};

// ---------- Separable 5-tap Gaussian blur (σ ≈ 1) ----------
fn gauss5(src: &[f32], w: usize, h: usize) -> Vec<f32> {
    let k = [1.0f32, 4.0, 6.0, 4.0, 1.0];
    let norm = 16.0f32;
    let mut tmp = vec![0f32; w * h];
    let mut out = vec![0f32; w * h];

    for y in 0..h {
        let row = y * w;
        for x in 0..w {
            let mut s = 0.0;
            for (kk, &kv) in k.iter().enumerate() {
                let xx = (x as i32 + kk as i32 - 2).max(0).min(w as i32 - 1) as usize;
                s += src[row + xx] * kv;
            }
            tmp[row + x] = s / norm;
        }
    }
    for y in 0..h {
        for x in 0..w {
            let mut s = 0.0;
            for (kk, &kv) in k.iter().enumerate() {
                let yy = (y as i32 + kk as i32 - 2).max(0).min(h as i32 - 1) as usize;
                s += tmp[yy * w + x] * kv;
            }
            out[y * w + x] = s / norm;
        }
    }
    out
}

// ---------- M-weight per pixel from 3-scale L-channel bandpass ----------
fn compute_b_weight(src_oklab: &[Oklab], w: usize, h: usize, eps: f32) -> Vec<f32> {
    let l: Vec<f32> = src_oklab.iter().map(|o| o.l).collect();
    // σ ≈ 1, √2, 2 — three octaves via repeated 5-tap blur
    let g1 = gauss5(&l, w, h); //  σ ≈ 1
    let g2 = gauss5(&g1, w, h); // σ ≈ √2 cumulative
    let g3 = gauss5(&g2, w, h); // σ ≈ √3 cumulative
    let g4 = gauss5(&g3, w, h); // σ ≈ 2

    let n = w * h;
    let mut b = vec![0f32; n];
    for i in 0..n {
        let dog_low = (g1[i] - g2[i]).abs();
        let dog_high = (g2[i] - g4[i]).abs();
        b[i] = dog_low + dog_high + eps;
    }

    // log range for transparency
    let bmin = b.iter().cloned().fold(f32::INFINITY, f32::min);
    let bmax = b.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let bmean: f32 = b.iter().sum::<f32>() / n as f32;
    println!(
        "  b weight: min={:.4}  max={:.4}  mean={:.4}  (ε={:.3})",
        bmin, bmax, bmean, eps
    );
    b
}

// ---------- M-weighted Lloyd ----------
fn m_weighted_lloyd(
    src_oklab: &[Oklab],
    b: &[f32],
    palette_init: &[Oklab],
    w_l: f32,
    w_a: f32,
    w_b: f32,
    iters: usize,
) -> (Vec<Oklab>, Vec<u8>) {
    let n = src_oklab.len();
    let k = palette_init.len();
    let mut palette = palette_init.to_vec();
    let mut indices = vec![0u8; n];

    for it in 0..iters {
        // Assignment with diagonal Mahalanobis (b_i factors out per-pixel
        // → does not affect argmin; only w_d affects assignment direction)
        let mut changed = 0usize;
        for i in 0..n {
            let p = src_oklab[i];
            let mut best_j = 0u8;
            let mut best_d = f32::INFINITY;
            for j in 0..k {
                let c = palette[j];
                let dl = p.l - c.l;
                let da = p.a - c.a;
                let dbb = p.b - c.b;
                let d = w_l * dl * dl + w_a * da * da + w_b * dbb * dbb;
                if d < best_d {
                    best_d = d;
                    best_j = j as u8;
                }
            }
            if indices[i] != best_j {
                indices[i] = best_j;
                changed += 1;
            }
        }

        // Update — b_i-weighted mean (closed form for diagonal Mahalanobis)
        let mut sum_l = vec![0f64; k];
        let mut sum_a = vec![0f64; k];
        let mut sum_b = vec![0f64; k];
        let mut sum_w = vec![0f64; k];
        for i in 0..n {
            let j = indices[i] as usize;
            let bi = b[i] as f64;
            sum_l[j] += bi * src_oklab[i].l as f64;
            sum_a[j] += bi * src_oklab[i].a as f64;
            sum_b[j] += bi * src_oklab[i].b as f64;
            sum_w[j] += bi;
        }
        for j in 0..k {
            if sum_w[j] > 0.0 {
                let wj = sum_w[j];
                palette[j] = Oklab {
                    l: (sum_l[j] / wj) as f32,
                    a: (sum_a[j] / wj) as f32,
                    b: (sum_b[j] / wj) as f32,
                };
            }
        }

        if it < 3 || it % 2 == 0 || changed == 0 {
            println!("  iter {}: relabeled {}", it + 1, changed);
        }
        if changed == 0 {
            break;
        }
    }
    (palette, indices)
}

// ---------- ICM (Cycle 71 baseline; in-process head-to-head) ----------
fn icm_step(
    src_oklab: &[Oklab],
    w: usize,
    h: usize,
    palette: &[Oklab],
    indices: &mut [u8],
    lambda_sq: f32,
) {
    let k = palette.len();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let px = src_oklab[i];
            let n_up = if y > 0 { indices[i - w] } else { 255 };
            let n_dn = if y + 1 < h { indices[i + w] } else { 255 };
            let n_lf = if x > 0 { indices[i - 1] } else { 255 };
            let n_rt = if x + 1 < w { indices[i + 1] } else { 255 };
            let mut best_j = indices[i];
            let mut best_cost = f32::INFINITY;
            for j in 0..k {
                let pj = palette[j];
                let dl = px.l - pj.l;
                let da = px.a - pj.a;
                let db = px.b - pj.b;
                let data = dl * dl + da * da + db * db;
                let mut sc = 0u32;
                if n_up != j as u8 && n_up != 255 {
                    sc += 1;
                }
                if n_dn != j as u8 && n_dn != 255 {
                    sc += 1;
                }
                if n_lf != j as u8 && n_lf != 255 {
                    sc += 1;
                }
                if n_rt != j as u8 && n_rt != 255 {
                    sc += 1;
                }
                let cost = data + lambda_sq * (sc as f32);
                if cost < best_cost {
                    best_cost = cost;
                    best_j = j as u8;
                }
            }
            indices[i] = best_j;
        }
    }
}

fn palette_retrain(src_oklab: &[Oklab], palette: &mut [Oklab], indices: &[u8]) {
    let k = palette.len();
    let mut sl = vec![0f64; k];
    let mut sa = vec![0f64; k];
    let mut sb = vec![0f64; k];
    let mut ct = vec![0u32; k];
    for (px, &idx) in src_oklab.iter().zip(indices.iter()) {
        let j = idx as usize;
        sl[j] += px.l as f64;
        sa[j] += px.a as f64;
        sb[j] += px.b as f64;
        ct[j] += 1;
    }
    for j in 0..k {
        if ct[j] > 0 {
            let c = ct[j] as f64;
            palette[j] = Oklab {
                l: (sl[j] / c) as f32,
                a: (sa[j] / c) as f32,
                b: (sb[j] / c) as f32,
            };
        }
    }
}

fn ssim_via_nupic(orig: &PathBuf, cmp_path: &PathBuf, nupic: &PathBuf) -> f64 {
    let out = Command::new(nupic)
        .args(["compare", "-m", "ssimulacra2"])
        .arg(orig)
        .arg(cmp_path)
        .output()
        .expect("nupic compare");
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines()
        .find_map(|l| {
            l.strip_prefix("SSIMULACRA2: ")
                .and_then(|v| v.split_whitespace().next())
                .and_then(|n| n.parse::<f64>().ok())
        })
        .unwrap_or(f64::NAN)
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();
    let nupic = root.join("target/release/nupic");
    let tmp = std::env::temp_dir();
    let img_path = root.join("assets/png-bench/inputs/04-photo-portrait.png");

    let n_colors: usize = std::env::var("MWL_NCOL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(192);
    let iters: usize = std::env::var("MWL_ITERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    let w_chrom: f32 = std::env::var("MWL_WCHROM")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.25);
    let eps: f32 = std::env::var("MWL_EPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.005);

    println!("Cycle 86 — R1 M-weighted Lloyd spike on 04 portrait");
    println!("  baseline:  Cycle 71 joint anneal → 86.19 SSIMULACRA2");
    println!(
        "  config:    n_colors={}  iters={}  w_L=1.0  w_chroma={}  ε={}",
        n_colors, iters, w_chrom, eps
    );
    println!("  gate:      ΔSSIM ≥ +1.0 vs ICM here → R1 path\n");

    let img = ImageReader::open(&img_path)?.with_guessed_format()?.decode()?;
    let r = img.to_rgba8();
    let w = r.width();
    let h = r.height();
    let raw_rgba = r.into_raw();
    println!("input: {}×{} = {} px", w, h, (w * h));

    // Init — imagequant + refine_palette_kmeans (matches speed_sweep.rs)
    let (pi, ai) = train_palette_rgba(&raw_rgba, w, h, n_colors)?;
    let (pal_init, alpha) = refine_palette_kmeans(&raw_rgba, w, h, &pi, &ai, 100);
    let (indices_init, ps_init) = apply_palette_rgba(&raw_rgba, w, h, &pal_init, &alpha);
    let trns = if alpha.iter().all(|&a| a == 255) {
        None
    } else {
        Some(alpha.as_slice())
    };
    let src_oklab: Vec<Oklab> = raw_rgba
        .chunks_exact(4)
        .map(|p| {
            srgb_u8_to_oklab(Rgb {
                r: p[0],
                g: p[1],
                b: p[2],
            })
        })
        .collect();

    let mut oxi = oxipng::Options::from_preset(3);
    oxi.strip = oxipng::StripChunks::Safe;

    // [A] init only
    let raw_png = encode_indexed_png_with_alpha(w, h, &indices_init, &ps_init, trns)?;
    let out_init = oxipng::optimize_from_memory(&raw_png, &oxi).unwrap();
    let init_path = tmp.join("c86_mwl_init.png");
    std::fs::write(&init_path, &out_init)?;
    let ssim_init = ssim_via_nupic(&img_path, &init_path, &nupic);
    println!(
        "[A] imagequant init only:    {} KB   SSIM {:.4}",
        out_init.len() / 1024,
        ssim_init
    );

    // [B] ICM (cycle 71 anneal)
    let lambdas_icm = [0.0001f32, 0.00005, 0.00002];
    let mut pal_icm = pal_init.clone();
    let mut idx_icm = indices_init.clone();
    let t0 = Instant::now();
    for &lam in &lambdas_icm {
        icm_step(&src_oklab, w as usize, h as usize, &pal_icm, &mut idx_icm, lam);
        palette_retrain(&src_oklab, &mut pal_icm, &idx_icm);
    }
    let icm_time = t0.elapsed().as_secs_f64();
    let pal_icm_srgb: Vec<Rgb<u8>> = pal_icm.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
    let raw_png = encode_indexed_png_with_alpha(w, h, &idx_icm, &pal_icm_srgb, trns)?;
    let out_icm = oxipng::optimize_from_memory(&raw_png, &oxi).unwrap();
    let icm_path = tmp.join("c86_mwl_icm.png");
    std::fs::write(&icm_path, &out_icm)?;
    let ssim_icm = ssim_via_nupic(&img_path, &icm_path, &nupic);
    println!(
        "[B] ICM (Cycle 71 anneal):   {} KB   SSIM {:.4}   ({:.2}s)",
        out_icm.len() / 1024,
        ssim_icm,
        icm_time
    );

    // [C] M-weighted Lloyd
    println!("\n[C] M-weighted Lloyd:");
    let t0 = Instant::now();
    let b_weight = compute_b_weight(&src_oklab, w as usize, h as usize, eps);
    let b_time = t0.elapsed().as_secs_f64();
    println!("  b weight precomputed in {:.2}s", b_time);

    let t0 = Instant::now();
    let (pal_mwl, idx_mwl) =
        m_weighted_lloyd(&src_oklab, &b_weight, &pal_init, 1.0, w_chrom, w_chrom, iters);
    let mwl_time = t0.elapsed().as_secs_f64();
    let pal_mwl_srgb: Vec<Rgb<u8>> = pal_mwl.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
    let raw_png = encode_indexed_png_with_alpha(w, h, &idx_mwl, &pal_mwl_srgb, trns)?;
    let out_mwl = oxipng::optimize_from_memory(&raw_png, &oxi).unwrap();
    let mwl_path = tmp.join("c86_mwl_lloyd.png");
    std::fs::write(&mwl_path, &out_mwl)?;
    let ssim_mwl = ssim_via_nupic(&img_path, &mwl_path, &nupic);
    println!(
        "  → {} KB   SSIM {:.4}   ({:.2}s lloyd + {:.2}s b-precompute)",
        out_mwl.len() / 1024,
        ssim_mwl,
        mwl_time,
        b_time
    );

    // [D] M-Lloyd then ICM (does ICM smoothness add on top?)
    let mut pal_d = pal_mwl.clone();
    let mut idx_d = idx_mwl.clone();
    let t0 = Instant::now();
    for &lam in &lambdas_icm {
        icm_step(&src_oklab, w as usize, h as usize, &pal_d, &mut idx_d, lam);
        palette_retrain(&src_oklab, &mut pal_d, &idx_d);
    }
    let d_time = t0.elapsed().as_secs_f64();
    let pal_d_srgb: Vec<Rgb<u8>> = pal_d.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
    let raw_png = encode_indexed_png_with_alpha(w, h, &idx_d, &pal_d_srgb, trns)?;
    let out_d = oxipng::optimize_from_memory(&raw_png, &oxi).unwrap();
    let d_path = tmp.join("c86_mwl_then_icm.png");
    std::fs::write(&d_path, &out_d)?;
    let ssim_d = ssim_via_nupic(&img_path, &d_path, &nupic);
    println!(
        "[D] M-Lloyd → ICM:           {} KB   SSIM {:.4}   ({:.2}s)",
        out_d.len() / 1024,
        ssim_d,
        d_time
    );

    let dssim_vs_icm = ssim_mwl - ssim_icm;
    let dssim_d_vs_icm = ssim_d - ssim_icm;
    let dssim_vs_init = ssim_mwl - ssim_init;
    let dssim_vs_c71 = ssim_mwl - 86.19;
    println!("\n=== Δ summary ===");
    println!("M-Lloyd        vs ICM here:        {:+.3}", dssim_vs_icm);
    println!("M-Lloyd → ICM  vs ICM here:        {:+.3}", dssim_d_vs_icm);
    println!("M-Lloyd        vs init:            {:+.3}", dssim_vs_init);
    println!("M-Lloyd        vs Cycle-71 (86.19): {:+.3}", dssim_vs_c71);
    println!();
    let best = dssim_vs_icm.max(dssim_d_vs_icm);
    if best >= 1.0 {
        println!(">>> GREEN  (best ΔSSIM ≥ +1.0): R1 paper path");
    } else if best >= 0.5 {
        println!(">>> YELLOW (+0.5..+1.0): write essay, tune metric, decide");
    } else {
        println!(">>> RED    (best ΔSSIM < +0.5): R1 simple variant ruled out — escalate or P3 engineering paper");
    }

    Ok(())
}
