//! Cycle 87 — R1 M-weighted Lloyd cross-corpus validation on baseline-7
//!
//! Cycle 86 gave R1 GREEN on 04 portrait (+2.59 SSIM vs ICM at
//! w_chrom=0.5, ε=0.001). This validates whether the win generalises
//! across the baseline-7 corpus (different content classes:
//! transparency, photo, landscape, UI/logo, product).
//!
//! Decision gate:
//!   mean ΔSSIM across 7 fixtures ≥ +0.5  → R1 productionization candidate (Cycle 91 wiring)
//!   mean ≥ 0 but < +0.5                  → per-content routing needed (Cycle 91 classifier)
//!   any fixture < 0 with no obvious class → revisit hyperparams / metric

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use image::ImageReader;
use rgb::Rgb;

use nupic_color::{Oklab, oklab_to_srgb_u8, srgb_u8_to_oklab};
use nupic_quantize::{
    apply_palette_rgba, classify_for_palette_size_with_importance,
    encode_indexed_png_with_alpha, refine_palette_kmeans, refine_palette_kmeans_importance,
    train_palette_rgba,
};

// ===== gauss5 + b-weight + M-weighted Lloyd =====
// (identical to examples/m_weighted_lloyd.rs)

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

fn compute_b_weight(src_oklab: &[Oklab], w: usize, h: usize, eps: f32) -> Vec<f32> {
    let l: Vec<f32> = src_oklab.iter().map(|o| o.l).collect();
    let g1 = gauss5(&l, w, h);
    let g2 = gauss5(&g1, w, h);
    let g3 = gauss5(&g2, w, h);
    let g4 = gauss5(&g3, w, h);
    let n = w * h;
    let mut b = vec![0f32; n];
    for i in 0..n {
        let dog_low = (g1[i] - g2[i]).abs();
        let dog_high = (g2[i] - g4[i]).abs();
        b[i] = dog_low + dog_high + eps;
    }
    b
}

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

    for _ in 0..iters {
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
        if changed == 0 {
            break;
        }
    }
    (palette, indices)
}

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

    // Cycle 86 best config
    let w_chrom: f32 = 0.5;
    let eps: f32 = 0.001;
    let mwl_iters: usize = 10;

    let fixtures: &[(&str, &str)] = &[
        ("inputs/01-png-transparency-demo.png", "01 trans"),
        ("inputs/02-pluto-transparent.png", "02 pluto"),
        ("inputs/03-wikipedia-logo.png", "03 wiki"),
        ("inputs/04-photo-portrait.png", "04 portrait"),
        ("inputs/05-photo-mountain.png", "05 mountain"),
        ("inputs/06-photo-landscape.png", "06 landscape"),
        ("inputs/07-photo-product.png", "07 product"),
    ];

    println!("Cycle 87 — R1 M-weighted Lloyd cross-corpus on baseline-7");
    println!("  config: w_chrom={}  ε={}  iters={}  (Cycle 86 best)", w_chrom, eps, mwl_iters);
    println!("  baseline (ICM Cycle 71 anneal) uses classify-picked n_colors per fixture");
    println!();
    println!(
        "{:<13} {:>4} {:>4} {:>9} {:>9} {:>9} {:>9} {:>8} {:>8}",
        "fixture", "n", "imp", "icm KB", "icm SSIM", "mwl KB", "mwl SSIM", "ΔSSIM", "Δsize%"
    );

    let mut total_icm_kb: u64 = 0;
    let mut total_mwl_kb: u64 = 0;
    let mut sum_d_ssim: f64 = 0.0;
    let mut n_green = 0;
    let mut n_pareto = 0;
    let mut n_red = 0;

    for &(rel, lbl) in fixtures {
        let p = root.join("assets/png-bench").join(rel);
        let img = ImageReader::open(&p)?.with_guessed_format()?.decode()?;
        let r = img.to_rgba8();
        let w = r.width();
        let h = r.height();
        let raw_rgba = r.into_raw();

        // classifier-driven n_colors + alpha-importance (matches Cycle 71 / speed_sweep.rs)
        let (n_colors, alpha_imp) =
            classify_for_palette_size_with_importance(&raw_rgba, w as usize);
        let (pi, ai) = train_palette_rgba(&raw_rgba, w, h, n_colors)?;
        let (pal_init, alpha) = if alpha_imp > 0.0 {
            refine_palette_kmeans_importance(&raw_rgba, w, h, &pi, &ai, 100, alpha_imp)
        } else {
            refine_palette_kmeans(&raw_rgba, w, h, &pi, &ai, 100)
        };
        let (indices_init, ps_init) =
            apply_palette_rgba(&raw_rgba, w, h, &pal_init, &alpha);
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

        // [B] ICM Cycle 71 anneal
        let lambdas_icm = [0.0001f32, 0.00005, 0.00002];
        let mut pal_icm = pal_init.clone();
        let mut idx_icm = indices_init.clone();
        for &lam in &lambdas_icm {
            icm_step(&src_oklab, w as usize, h as usize, &pal_icm, &mut idx_icm, lam);
            palette_retrain(&src_oklab, &mut pal_icm, &idx_icm);
        }
        let pal_icm_srgb: Vec<Rgb<u8>> =
            pal_icm.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
        let raw_png = encode_indexed_png_with_alpha(w, h, &idx_icm, &pal_icm_srgb, trns)?;
        let out_icm = oxipng::optimize_from_memory(&raw_png, &oxi).unwrap();
        let icm_path = tmp.join(format!("c87_{}_icm.png", lbl.replace(' ', "_")));
        std::fs::write(&icm_path, &out_icm)?;
        let ssim_icm = ssim_via_nupic(&p, &icm_path, &nupic);

        // [D] M-Lloyd → ICM (best Cycle 86 config + post-ICM)
        let b_weight = compute_b_weight(&src_oklab, w as usize, h as usize, eps);
        let (pal_d_init, idx_d_init) =
            m_weighted_lloyd(&src_oklab, &b_weight, &pal_init, 1.0, w_chrom, w_chrom, mwl_iters);
        let mut pal_d = pal_d_init;
        let mut idx_d = idx_d_init;
        for &lam in &lambdas_icm {
            icm_step(&src_oklab, w as usize, h as usize, &pal_d, &mut idx_d, lam);
            palette_retrain(&src_oklab, &mut pal_d, &idx_d);
        }
        let pal_d_srgb: Vec<Rgb<u8>> = pal_d.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
        let raw_png = encode_indexed_png_with_alpha(w, h, &idx_d, &pal_d_srgb, trns)?;
        let out_d = oxipng::optimize_from_memory(&raw_png, &oxi).unwrap();
        let d_path = tmp.join(format!("c87_{}_mwl.png", lbl.replace(' ', "_")));
        std::fs::write(&d_path, &out_d)?;
        let ssim_d = ssim_via_nupic(&p, &d_path, &nupic);

        let d_ssim = ssim_d - ssim_icm;
        let d_size_pct =
            (out_d.len() as f64 / out_icm.len() as f64 - 1.0) * 100.0;
        let mark = if d_ssim >= 0.5 {
            n_green += 1;
            "GREEN"
        } else if d_ssim >= 0.0 {
            n_pareto += 1;
            "≥0"
        } else {
            n_red += 1;
            "RED"
        };
        println!(
            "{:<13} {:>4} {:>4.2} {:>7} KB {:>9.4} {:>7} KB {:>9.4} {:>+8.3} {:>+7.2}% {}",
            lbl,
            n_colors,
            alpha_imp,
            out_icm.len() / 1024,
            ssim_icm,
            out_d.len() / 1024,
            ssim_d,
            d_ssim,
            d_size_pct,
            mark
        );

        total_icm_kb += out_icm.len() as u64;
        total_mwl_kb += out_d.len() as u64;
        sum_d_ssim += d_ssim;
    }

    println!();
    let n = fixtures.len() as f64;
    let mean_d = sum_d_ssim / n;
    let total_size_pct = (total_mwl_kb as f64 / total_icm_kb as f64 - 1.0) * 100.0;
    println!(
        "TOTAL: icm={}KB  mwl={}KB  Δsize={:+.2}%  mean ΔSSIM={:+.3}",
        total_icm_kb / 1024,
        total_mwl_kb / 1024,
        total_size_pct,
        mean_d
    );
    println!("       GREEN(≥+0.5) = {}/7    ≥0 = {}/7    RED = {}/7", n_green, n_pareto, n_red);
    println!();

    if mean_d >= 0.5 {
        println!(">>> GREEN  (mean ΔSSIM ≥ +0.5): R1 productionization candidate");
    } else if mean_d >= 0.0 {
        println!(">>> YELLOW (mean ≥0 but < +0.5): per-content routing needed");
    } else {
        println!(">>> RED    (mean < 0): revisit hyperparams / metric");
    }
    Ok(())
}
