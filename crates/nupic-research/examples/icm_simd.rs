//! Cycle 89 — R9 ICM SIMD spike (perf engineering)
//!
//! Cycle 71 ICM inner loop is scalar OKLab L² + Potts smoothness across
//! all K palette entries per pixel. With K=192-256 and N=1-25M pixels,
//! this is the hottest loop in the pipeline.
//!
//! R9 spike: SoA palette (Vec<f32> L, a, b each) + wide::f32x4 4-lane
//! distance and smoothness computation, blend-based argmin, same as
//! Cycle 82/83 pattern in nupic-quantize::apply_palette_rgba.
//!
//! Gate per R9 roadmap: baseline-7 04/06/07 -50-200ms total ICM time;
//! 5MP+ proportionally larger savings.

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use image::ImageReader;
use rgb::Rgb;
use wide::{f32x4, CmpLt, CmpNe};

use nupic_color::{Oklab, oklab_to_srgb_u8, srgb_u8_to_oklab};
use nupic_quantize::{
    apply_palette_rgba, classify_for_palette_size_with_importance,
    encode_indexed_png_with_alpha, refine_palette_kmeans, refine_palette_kmeans_importance,
    train_palette_rgba,
};

// ---------- Scalar ICM (cycle 71 reference) ----------
fn icm_step_scalar(
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
                if n_up != j as u8 && n_up != 255 { sc += 1; }
                if n_dn != j as u8 && n_dn != 255 { sc += 1; }
                if n_lf != j as u8 && n_lf != 255 { sc += 1; }
                if n_rt != j as u8 && n_rt != 255 { sc += 1; }
                let cost = data + lambda_sq * (sc as f32);
                if cost < best_cost { best_cost = cost; best_j = j as u8; }
            }
            indices[i] = best_j;
        }
    }
}

// ---------- SoA palette padded to multiple of 4 ----------
struct SoAPalette {
    l: Vec<f32>,
    a: Vec<f32>,
    b: Vec<f32>,
    k_real: usize,
    k_pad: usize, // multiple of 4
}
impl SoAPalette {
    fn from_oklab(pal: &[Oklab]) -> Self {
        let k_real = pal.len();
        let k_pad = (k_real + 3) & !3usize;
        let mut l = Vec::with_capacity(k_pad);
        let mut a = Vec::with_capacity(k_pad);
        let mut b = Vec::with_capacity(k_pad);
        for c in pal {
            l.push(c.l);
            a.push(c.a);
            b.push(c.b);
        }
        // Pad with +INF position to never win argmin
        for _ in k_real..k_pad {
            l.push(1.0e9);
            a.push(1.0e9);
            b.push(1.0e9);
        }
        Self { l, a, b, k_real, k_pad }
    }
}

// ---------- SIMD ICM step ----------
fn icm_step_simd(
    src_oklab: &[Oklab],
    w: usize,
    h: usize,
    pal: &SoAPalette,
    indices: &mut [u8],
    lambda_sq: f32,
) {
    let one_f4 = f32x4::splat(1.0);
    let zero_f4 = f32x4::splat(0.0);
    let four_f4 = f32x4::splat(4.0);
    let lam_f4 = f32x4::splat(lambda_sq);
    let inf_f4 = f32x4::splat(f32::INFINITY);

    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let px = src_oklab[i];
            let n_up_u = if y > 0 { indices[i - w] } else { 255u8 };
            let n_dn_u = if y + 1 < h { indices[i + w] } else { 255u8 };
            let n_lf_u = if x > 0 { indices[i - 1] } else { 255u8 };
            let n_rt_u = if x + 1 < w { indices[i + 1] } else { 255u8 };

            let pl_f4 = f32x4::splat(px.l);
            let pa_f4 = f32x4::splat(px.a);
            let pb_f4 = f32x4::splat(px.b);

            let mut min_d2 = inf_f4;
            let mut min_idx = f32x4::from([0.0, 1.0, 2.0, 3.0]);
            let mut idx_iter = f32x4::from([0.0, 1.0, 2.0, 3.0]);

            // Precompute boundary masks once per pixel
            let n_up_active = n_up_u != 255;
            let n_dn_active = n_dn_u != 255;
            let n_lf_active = n_lf_u != 255;
            let n_rt_active = n_rt_u != 255;
            let nup_v = if n_up_active { f32x4::splat(n_up_u as f32) } else { inf_f4 };
            let ndn_v = if n_dn_active { f32x4::splat(n_dn_u as f32) } else { inf_f4 };
            let nlf_v = if n_lf_active { f32x4::splat(n_lf_u as f32) } else { inf_f4 };
            let nrt_v = if n_rt_active { f32x4::splat(n_rt_u as f32) } else { inf_f4 };

            let mut j = 0usize;
            while j < pal.k_pad {
                let cl = f32x4::new([pal.l[j], pal.l[j+1], pal.l[j+2], pal.l[j+3]]);
                let ca = f32x4::new([pal.a[j], pal.a[j+1], pal.a[j+2], pal.a[j+3]]);
                let cb = f32x4::new([pal.b[j], pal.b[j+1], pal.b[j+2], pal.b[j+3]]);
                let dl = pl_f4 - cl;
                let da = pa_f4 - ca;
                let db = pb_f4 - cb;
                let data = dl * dl + da * da + db * db;

                // Smoothness count: per lane, +1 if active neighbor != lane_idx
                let mut smooth_count = zero_f4;
                if n_up_active {
                    let neq = idx_iter.cmp_ne(nup_v);
                    smooth_count += neq.blend(one_f4, zero_f4);
                }
                if n_dn_active {
                    let neq = idx_iter.cmp_ne(ndn_v);
                    smooth_count += neq.blend(one_f4, zero_f4);
                }
                if n_lf_active {
                    let neq = idx_iter.cmp_ne(nlf_v);
                    smooth_count += neq.blend(one_f4, zero_f4);
                }
                if n_rt_active {
                    let neq = idx_iter.cmp_ne(nrt_v);
                    smooth_count += neq.blend(one_f4, zero_f4);
                }
                let cost = data + lam_f4 * smooth_count;

                let mask = cost.cmp_lt(min_d2);
                min_d2 = mask.blend(cost, min_d2);
                min_idx = mask.blend(idx_iter, min_idx);

                idx_iter += four_f4;
                j += 4;
            }
            // Reduce 4 lanes
            let arr_d = min_d2.to_array();
            let arr_i = min_idx.to_array();
            let mut best_d = arr_d[0];
            let mut best_j = arr_i[0] as u8;
            for k in 1..4 {
                if arr_d[k] < best_d {
                    best_d = arr_d[k];
                    best_j = arr_i[k] as u8;
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

fn run_fixture(
    fixture_path: &PathBuf,
    nupic: &PathBuf,
    label: &str,
) -> anyhow::Result<()> {
    let img = ImageReader::open(fixture_path)?.with_guessed_format()?.decode()?;
    let r = img.to_rgba8();
    let w = r.width();
    let h = r.height();
    let n_pixels = (w as usize) * (h as usize);
    let raw_rgba = r.into_raw();
    let (n_colors, alpha_imp) =
        classify_for_palette_size_with_importance(&raw_rgba, w as usize);

    let tmp = std::env::temp_dir();
    let mut oxi = oxipng::Options::from_preset(3);
    oxi.strip = oxipng::StripChunks::Safe;

    let (pi, ai) = train_palette_rgba(&raw_rgba, w, h, n_colors)?;
    let (pal_init, alpha) = if alpha_imp > 0.0 {
        refine_palette_kmeans_importance(&raw_rgba, w, h, &pi, &ai, 100, alpha_imp)
    } else {
        refine_palette_kmeans(&raw_rgba, w, h, &pi, &ai, 100)
    };
    let (indices_init, _ps_init) = apply_palette_rgba(&raw_rgba, w, h, &pal_init, &alpha);
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
    let lambdas = [0.0001f32, 0.00005, 0.00002];

    // ---------- [A] Scalar ICM (Cycle 71) ----------
    let mut pal_a = pal_init.clone();
    let mut idx_a = indices_init.clone();
    let t_a = Instant::now();
    for &lam in &lambdas {
        icm_step_scalar(&src_oklab, w as usize, h as usize, &pal_a, &mut idx_a, lam);
        palette_retrain(&src_oklab, &mut pal_a, &idx_a);
    }
    let time_a = t_a.elapsed().as_secs_f64();
    let pal_a_srgb: Vec<Rgb<u8>> = pal_a.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
    let raw_png = encode_indexed_png_with_alpha(w, h, &idx_a, &pal_a_srgb, trns)?;
    let out_a = oxipng::optimize_from_memory(&raw_png, &oxi).unwrap();
    let path_a = tmp.join(format!("c89_{}_scalar.png", label.replace(' ', "_")));
    std::fs::write(&path_a, &out_a)?;
    let ssim_a = ssim_via_nupic(fixture_path, &path_a, &nupic);

    // ---------- [B] SIMD ICM ----------
    let mut pal_b = pal_init.clone();
    let mut idx_b = indices_init.clone();
    let t_b = Instant::now();
    for &lam in &lambdas {
        let soa = SoAPalette::from_oklab(&pal_b);
        icm_step_simd(&src_oklab, w as usize, h as usize, &soa, &mut idx_b, lam);
        palette_retrain(&src_oklab, &mut pal_b, &idx_b);
    }
    let time_b = t_b.elapsed().as_secs_f64();
    let pal_b_srgb: Vec<Rgb<u8>> = pal_b.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
    let raw_png = encode_indexed_png_with_alpha(w, h, &idx_b, &pal_b_srgb, trns)?;
    let out_b = oxipng::optimize_from_memory(&raw_png, &oxi).unwrap();
    let path_b = tmp.join(format!("c89_{}_simd.png", label.replace(' ', "_")));
    std::fs::write(&path_b, &out_b)?;
    let ssim_b = ssim_via_nupic(fixture_path, &path_b, &nupic);

    let dms = (time_b - time_a) * 1000.0;
    let dssim = ssim_b - ssim_a;
    let dsize_pct = (out_b.len() as f64 / out_a.len() as f64 - 1.0) * 100.0;
    let speedup = time_a / time_b;
    println!(
        "{:<24} {:>2}MP n={:>3} | scalar {:>7.1}ms SSIM {:>7.3} | simd {:>7.1}ms SSIM {:>7.3} | Δtime {:>+7.1}ms ({:>4.2}× speedup) ΔSSIM {:+6.3} Δsize {:>+5.2}%",
        label,
        n_pixels / 1_000_000,
        n_colors,
        time_a * 1000.0,
        ssim_a,
        time_b * 1000.0,
        ssim_b,
        dms,
        speedup,
        dssim,
        dsize_pct,
    );
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();
    let nupic = root.join("target/release/nupic");

    let fixtures: &[(&str, &str)] = &[
        ("inputs/04-photo-portrait.png", "04 portrait"),
        ("inputs/06-photo-landscape.png", "06 landscape"),
        ("inputs/07-photo-product.png", "07 product"),
        ("inputs-ext-real/17-aurora-5mp.png", "17 aurora 5.9MP"),
        ("inputs-ext-real/25-sofia-cathedral-5mp.png", "25 sofia 5.5MP"),
        ("inputs-ext-real/27-whale-tail-5mp.png", "27 whale 5.5MP"),
    ];

    println!("Cycle 89 — R9 ICM SIMD spike (wide::f32x4 4-lane)");
    println!("  bench: scalar (Cycle 71) vs SoA + f32x4 SIMD, 3-step anneal schedule");
    println!("  gate (per R9 roadmap): baseline-7 04/06/07 -50-200ms total ICM time");
    println!();
    for &(rel, lbl) in fixtures {
        let path = root.join("assets/png-bench").join(rel);
        if !path.exists() {
            println!("  (skip {}: not found)", lbl);
            continue;
        }
        run_fixture(&path, &nupic, lbl)?;
    }
    Ok(())
}
