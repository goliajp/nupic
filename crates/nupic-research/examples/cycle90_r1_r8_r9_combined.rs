//! Cycle 90 — R1 + R8 + R9 combined bench (paper §5 main result)
//!
//! Stack three axes from Cycles 86-89:
//!   R1 (Cycle 86) — M-weighted Lloyd (w_chrom=0.5, ε=0.001, 10 iters) on OKLab
//!   R8 (Cycle 88) — k-means++ init replacing imagequant median-cut
//!   R9 (Cycle 89) — SoA + f32x4 SIMD ICM (1.67× bit-exact)
//!
//! Two pipelines compared head-to-head on each fixture:
//!   [A] Baseline           : imagequant init → refine(Lloyd) → ICM scalar (Cycle 71 anneal)
//!   [B] R1+R8+R9 Combined  : kmeans++ init  → refine(Lloyd) → M-weighted Lloyd → ICM SIMD (anneal)
//!
//! Hypothesis: three axes stack.
//!   - R1 raises quality on portraits / chroma-rich (Cycle 87 GREEN cohort).
//!   - R8 cuts time on most fixtures (Cycle 88 5/7 dual-win).
//!   - R9 cuts time on every ICM step (Cycle 89 1.67× clean).
//!
//! Gate per Cycle 90:
//!   mean ΔSSIM ≥ +0.5 AND mean Δtotal_ms ≤ 0  → GREEN (paper §5 headline)
//!   mean ΔSSIM ≥ 0    AND mean Δtotal_ms ≤ 0  → YELLOW (Pareto, routing in Cycle 91)
//!   mean ΔSSIM < 0     OR mean Δtotal_ms > 0  → RED (revisit composition order)

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

// ============================================================
// R8: k-means++ init on subsample (from cycle 88)
// ============================================================
struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_add(0xdeadbeef))
    }
    fn next_f32(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as u32) as f32 / (u32::MAX as f32)
    }
}

fn kmeans_pp_init_oklab(src_oklab: &[Oklab], k: usize, seed: u64) -> Vec<Oklab> {
    let n = src_oklab.len();
    let sample_size = 20_000.min(n);
    let stride = (n / sample_size).max(1);
    let samples: Vec<Oklab> = (0..sample_size).map(|i| src_oklab[i * stride]).collect();

    let mut rng = Lcg::new(seed);
    let mut centroids: Vec<Oklab> = Vec::with_capacity(k);
    let first_idx = (rng.next_f32() * sample_size as f32) as usize % sample_size;
    centroids.push(samples[first_idx]);

    let mut min_dists: Vec<f32> = samples.iter().map(|p| {
        let c = centroids[0];
        let dl = p.l - c.l;
        let da = p.a - c.a;
        let db = p.b - c.b;
        dl * dl + da * da + db * db
    }).collect();

    for _ in 1..k {
        let total: f64 = min_dists.iter().map(|&v| v as f64).sum();
        if total <= 0.0 {
            centroids.push(samples[first_idx]);
            continue;
        }
        let pick = rng.next_f32() as f64 * total;
        let mut cumul = 0.0f64;
        let mut chosen = sample_size - 1;
        for (i, &d) in min_dists.iter().enumerate() {
            cumul += d as f64;
            if cumul >= pick {
                chosen = i;
                break;
            }
        }
        let new_c = samples[chosen];
        centroids.push(new_c);
        for (i, p) in samples.iter().enumerate() {
            let dl = p.l - new_c.l;
            let da = p.a - new_c.a;
            let db = p.b - new_c.b;
            let d = dl * dl + da * da + db * db;
            if d < min_dists[i] {
                min_dists[i] = d;
            }
        }
    }
    centroids
}

// ============================================================
// R1: M-weighted Lloyd (from cycle 86/87)
// ============================================================
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

// ============================================================
// ICM scalar (Cycle 71 reference)
// ============================================================
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

// ============================================================
// R9: ICM SIMD (from cycle 89)
// ============================================================
struct SoAPalette {
    l: Vec<f32>,
    a: Vec<f32>,
    b: Vec<f32>,
    #[allow(dead_code)]
    k_real: usize,
    k_pad: usize,
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
        for _ in k_real..k_pad {
            l.push(1.0e9);
            a.push(1.0e9);
            b.push(1.0e9);
        }
        Self { l, a, b, k_real, k_pad }
    }
}

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

struct FixtureResult {
    label: String,
    n_pixels: usize,
    n_colors: usize,
    ssim_a: f64,
    ssim_b: f64,
    time_a_ms: f64,
    time_b_ms: f64,
    size_a: u64,
    size_b: u64,
}

fn run_fixture(
    fixture_path: &PathBuf,
    nupic: &PathBuf,
    label: &str,
) -> anyhow::Result<FixtureResult> {
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

    let src_oklab: Vec<Oklab> = raw_rgba
        .chunks_exact(4)
        .map(|p| srgb_u8_to_oklab(Rgb { r: p[0], g: p[1], b: p[2] }))
        .collect();
    let lambdas = [0.0001f32, 0.00005, 0.00002];

    // ===================================================================
    // [A] Baseline pipeline: imagequant → refine → ICM scalar
    // ===================================================================
    let t_a = Instant::now();
    let (pi_a, ai_a) = train_palette_rgba(&raw_rgba, w, h, n_colors)?;
    let (pal_init_a, alpha_a) = if alpha_imp > 0.0 {
        refine_palette_kmeans_importance(&raw_rgba, w, h, &pi_a, &ai_a, 100, alpha_imp)
    } else {
        refine_palette_kmeans(&raw_rgba, w, h, &pi_a, &ai_a, 100)
    };
    let (idx_init_a, _ps_init_a) = apply_palette_rgba(&raw_rgba, w, h, &pal_init_a, &alpha_a);
    let mut pal_a = pal_init_a;
    let mut idx_a = idx_init_a;
    for &lam in &lambdas {
        icm_step_scalar(&src_oklab, w as usize, h as usize, &pal_a, &mut idx_a, lam);
        palette_retrain(&src_oklab, &mut pal_a, &idx_a);
    }
    let time_a_ms = t_a.elapsed().as_secs_f64() * 1000.0;
    let pal_a_srgb: Vec<Rgb<u8>> = pal_a.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
    let trns_a = if alpha_a.iter().all(|&a| a == 255) { None } else { Some(alpha_a.as_slice()) };
    let raw_png_a = encode_indexed_png_with_alpha(w, h, &idx_a, &pal_a_srgb, trns_a)?;
    let out_a = oxipng::optimize_from_memory(&raw_png_a, &oxi).unwrap();
    let path_a = tmp.join(format!("c90_{}_baseline.png", label.replace(' ', "_")));
    std::fs::write(&path_a, &out_a)?;
    let ssim_a = ssim_via_nupic(fixture_path, &path_a, &nupic);

    // ===================================================================
    // [B] R1+R8+R9 combined: kmeans++ → refine → M-weighted Lloyd → ICM SIMD
    // ===================================================================
    let t_b = Instant::now();
    let pi_b = kmeans_pp_init_oklab(&src_oklab, n_colors, label.len() as u64 * 31 + 7);
    let ai_b = ai_a.clone();
    let (pal_init_b, alpha_b) = if alpha_imp > 0.0 {
        refine_palette_kmeans_importance(&raw_rgba, w, h, &pi_b, &ai_b, 100, alpha_imp)
    } else {
        refine_palette_kmeans(&raw_rgba, w, h, &pi_b, &ai_b, 100)
    };
    let w_chrom = 0.5f32;
    let eps = 0.001f32;
    let mwl_iters = 10usize;
    let b_weight = compute_b_weight(&src_oklab, w as usize, h as usize, eps);
    let (pal_mwl, idx_mwl) =
        m_weighted_lloyd(&src_oklab, &b_weight, &pal_init_b, 1.0, w_chrom, w_chrom, mwl_iters);
    let mut pal_b = pal_mwl;
    let mut idx_b = idx_mwl;
    for &lam in &lambdas {
        let soa = SoAPalette::from_oklab(&pal_b);
        icm_step_simd(&src_oklab, w as usize, h as usize, &soa, &mut idx_b, lam);
        palette_retrain(&src_oklab, &mut pal_b, &idx_b);
    }
    let time_b_ms = t_b.elapsed().as_secs_f64() * 1000.0;
    let pal_b_srgb: Vec<Rgb<u8>> = pal_b.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
    let trns_b = if alpha_b.iter().all(|&a| a == 255) { None } else { Some(alpha_b.as_slice()) };
    let raw_png_b = encode_indexed_png_with_alpha(w, h, &idx_b, &pal_b_srgb, trns_b)?;
    let out_b = oxipng::optimize_from_memory(&raw_png_b, &oxi).unwrap();
    let path_b = tmp.join(format!("c90_{}_combined.png", label.replace(' ', "_")));
    std::fs::write(&path_b, &out_b)?;
    let ssim_b = ssim_via_nupic(fixture_path, &path_b, &nupic);

    let dms = time_b_ms - time_a_ms;
    let dssim = ssim_b - ssim_a;
    let dsize_pct = (out_b.len() as f64 / out_a.len() as f64 - 1.0) * 100.0;
    let mark = if dssim >= 0.5 && dms <= 0.0 {
        "GREEN"
    } else if dssim >= 0.0 && dms <= 0.0 {
        "YELLOW"
    } else if dssim >= 0.5 && dms > 0.0 {
        "Q+T-"
    } else if dssim < 0.0 && dms <= 0.0 {
        "T+Q-"
    } else {
        "RED"
    };
    println!(
        "{:<24} {:>2}MP n={:>3} | A {:>7.0}ms SSIM {:>7.3} | B {:>7.0}ms SSIM {:>7.3} | Δt {:>+7.0}ms ΔSSIM {:>+6.3} Δsize {:>+5.2}% {}",
        label,
        n_pixels / 1_000_000,
        n_colors,
        time_a_ms,
        ssim_a,
        time_b_ms,
        ssim_b,
        dms,
        dssim,
        dsize_pct,
        mark,
    );
    Ok(FixtureResult {
        label: label.to_string(),
        n_pixels,
        n_colors,
        ssim_a,
        ssim_b,
        time_a_ms,
        time_b_ms,
        size_a: out_a.len() as u64,
        size_b: out_b.len() as u64,
    })
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();
    let nupic = root.join("target/release/nupic");

    let fixtures: &[(&str, &str)] = &[
        ("inputs/01-png-transparency-demo.png", "01 trans"),
        ("inputs/02-pluto-transparent.png", "02 pluto"),
        ("inputs/03-wikipedia-logo.png", "03 wiki"),
        ("inputs/04-photo-portrait.png", "04 portrait"),
        ("inputs/05-photo-mountain.png", "05 mountain"),
        ("inputs/06-photo-landscape.png", "06 landscape"),
        ("inputs/07-photo-product.png", "07 product"),
        ("inputs-ext-real/17-aurora-5mp.png", "17 aurora 5.9MP"),
        ("inputs-ext-real/25-sofia-cathedral-5mp.png", "25 sofia 5.5MP"),
        ("inputs-ext-real/27-whale-tail-5mp.png", "27 whale 5.5MP"),
    ];

    println!("Cycle 90 — R1 + R8 + R9 combined bench");
    println!("  [A] imagequant init → refine(Lloyd) → ICM scalar (Cycle 71 anneal)");
    println!("  [B] kmeans++ init  → refine(Lloyd) → M-weighted Lloyd (w_chrom=0.5,ε=0.001,10 iters) → ICM SIMD");
    println!("  gate: mean ΔSSIM ≥ +0.5 AND mean Δt ≤ 0  → GREEN  (paper §5 headline)");
    println!();
    println!(
        "{:<24} {:>4} {:>5} | {:>9} {:>10} | {:>9} {:>10} | {:>9} {:>9} {:>7} {}",
        "fixture", "MP", "n", "A wall", "A SSIM", "B wall", "B SSIM", "Δt", "ΔSSIM", "Δsize", "gate"
    );

    let mut results: Vec<FixtureResult> = Vec::new();
    for &(rel, lbl) in fixtures {
        let path = root.join("assets/png-bench").join(rel);
        if !path.exists() {
            println!("  (skip {}: not found)", lbl);
            continue;
        }
        match run_fixture(&path, &nupic, lbl) {
            Ok(r) => results.push(r),
            Err(e) => println!("  ERR {}: {}", lbl, e),
        }
    }
    println!();

    // Aggregates: baseline-7 (idx 0..7) and 5MP cohort (idx 7..)
    fn agg(rows: &[&FixtureResult], name: &str) {
        if rows.is_empty() { return; }
        let n = rows.len() as f64;
        let mean_dssim: f64 = rows.iter().map(|r| r.ssim_b - r.ssim_a).sum::<f64>() / n;
        let mean_dms: f64 = rows.iter().map(|r| r.time_b_ms - r.time_a_ms).sum::<f64>() / n;
        let sum_a: u64 = rows.iter().map(|r| r.size_a).sum();
        let sum_b: u64 = rows.iter().map(|r| r.size_b).sum();
        let dsize_pct = (sum_b as f64 / sum_a as f64 - 1.0) * 100.0;
        let n_green = rows.iter().filter(|r| (r.ssim_b - r.ssim_a) >= 0.5 && (r.time_b_ms - r.time_a_ms) <= 0.0).count();
        let n_yellow = rows.iter().filter(|r| (r.ssim_b - r.ssim_a) >= 0.0 && (r.time_b_ms - r.time_a_ms) <= 0.0).count();
        let n_qup_tdown = rows.iter().filter(|r| (r.ssim_b - r.ssim_a) >= 0.5 && (r.time_b_ms - r.time_a_ms) > 0.0).count();
        let n_tup_qdown = rows.iter().filter(|r| (r.ssim_b - r.ssim_a) < 0.0 && (r.time_b_ms - r.time_a_ms) <= 0.0).count();
        let n_red = rows.iter().filter(|r| (r.ssim_b - r.ssim_a) < 0.0 && (r.time_b_ms - r.time_a_ms) > 0.0).count();
        println!(
            "[{}] mean ΔSSIM {:+.3}  mean Δt {:+.0}ms  Δsize {:+.2}%   |   GREEN {}/{}  YELLOW {}/{}  Q+T- {}/{}  T+Q- {}/{}  RED {}/{}",
            name, mean_dssim, mean_dms, dsize_pct,
            n_green, rows.len(),
            n_yellow, rows.len(),
            n_qup_tdown, rows.len(),
            n_tup_qdown, rows.len(),
            n_red, rows.len(),
        );
    }

    let baseline_7: Vec<&FixtureResult> = results.iter().filter(|r| !r.label.contains("MP")).collect();
    let fivemp: Vec<&FixtureResult> = results.iter().filter(|r| r.label.contains("MP")).collect();
    let all: Vec<&FixtureResult> = results.iter().collect();
    agg(&baseline_7, "baseline-7");
    agg(&fivemp, "5MP cohort  ");
    agg(&all, "ALL          ");

    println!();
    let n = all.len() as f64;
    let overall_mean_dssim: f64 = all.iter().map(|r| r.ssim_b - r.ssim_a).sum::<f64>() / n;
    let overall_mean_dms: f64 = all.iter().map(|r| r.time_b_ms - r.time_a_ms).sum::<f64>() / n;
    let verdict = if overall_mean_dssim >= 0.5 && overall_mean_dms <= 0.0 {
        ">>> GREEN  — three axes stack; R1+R8+R9 paper §5 headline ready"
    } else if overall_mean_dssim >= 0.0 && overall_mean_dms <= 0.0 {
        ">>> YELLOW — Pareto across corpus, per-content routing in Cycle 91"
    } else {
        ">>> RED    — composition order or interaction issue; revisit"
    };
    println!("{}", verdict);
    Ok(())
}
