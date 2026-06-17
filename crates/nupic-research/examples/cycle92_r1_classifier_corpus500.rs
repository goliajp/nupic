//! Cycle 92 — R1 classifier 506-corpus validation
//!
//! Cycle 91a found a 4-rule classifier hitting 9/10 on baseline-7 + 3×5MP. With
//! n=10 the thresholds are overfit-prone. This cycle validates on the larger
//! 506-corpus (`assets/png-bench/corpus-500/`).
//!
//! Plan (cheap-first):
//!   1. Compute 4 routing features on all 506 fixtures (sub-ms each).
//!   2. Apply Cycle 91a's 4-rule classifier (`FRIEND if trans_frac > 0 OR
//!      (chroma > 0.0166 AND edge_density > 0.1502 AND smoothness < 0.0614)`).
//!   3. Report FRIEND/HOSTILE distribution + size-bucket breakdown.
//!   4. Stratified-sample 20 (10 predicted-FRIEND + 10 predicted-HOSTILE),
//!      run R1+R8+R9 combined vs baseline → ground truth ΔSSIM → accuracy.
//!
//! Decision gate:
//!   - acc ≥ 85% on n=20 sample → Cycle 91a thresholds GREEN (paper §6 honest)
//!   - acc < 85%                → re-fit thresholds on the 20-sample, report
//!                                what changed

use std::collections::BTreeMap;
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

// ===== Cycle 91a feature computation (copy-paste, kept self-contained) =====

struct Features {
    mean_chroma: f32,
    smoothness: f32,
    edge_density: f32,
    trans_frac: f32,
}

fn compute_features(raw_rgba: &[u8], w: usize, h: usize) -> Features {
    let n = w * h;
    let mut alpha_count_lt = 0usize;
    let oklab: Vec<Oklab> = raw_rgba.chunks_exact(4).map(|p| {
        if p[3] < 255 { alpha_count_lt += 1; }
        srgb_u8_to_oklab(Rgb { r: p[0], g: p[1], b: p[2] })
    }).collect();
    let trans_frac = alpha_count_lt as f32 / n as f32;

    let sum_chroma: f64 = oklab.iter().map(|o| (o.a * o.a + o.b * o.b).sqrt() as f64).sum();
    let mean_chroma = (sum_chroma / n as f64) as f32;

    let mut sum_h = 0.0f64;
    let mut sum_v = 0.0f64;
    let mut count_h = 0usize;
    let mut count_v = 0usize;
    for y in 0..h {
        for x in 0..w-1 {
            let i = y * w + x;
            sum_h += (oklab[i].l - oklab[i + 1].l).abs() as f64;
            count_h += 1;
        }
    }
    if h >= 1 {
        for y in 0..h-1 {
            for x in 0..w {
                let i = y * w + x;
                sum_v += (oklab[i].l - oklab[i + w].l).abs() as f64;
                count_v += 1;
            }
        }
    }
    let smoothness = ((sum_h / count_h.max(1) as f64) + (sum_v / count_v.max(1) as f64)) as f32;

    let mut edge_count = 0usize;
    let mut edge_total = 0usize;
    if w >= 3 && h >= 3 {
        for y in 1..h-1 {
            for x in 1..w-1 {
                let i = y * w + x;
                let gx = oklab[i + 1].l - oklab[i - 1].l;
                let gy = oklab[i + w].l - oklab[i - w].l;
                let mag = (gx * gx + gy * gy).sqrt();
                if mag > 0.05 { edge_count += 1; }
                edge_total += 1;
            }
        }
    }
    let edge_density = edge_count as f32 / edge_total.max(1) as f32;

    Features { mean_chroma, smoothness, edge_density, trans_frac }
}

// Cycle 91a's best 4-rule classifier
fn predict_friend(f: &Features) -> bool {
    f.trans_frac > 0.0
        || (f.mean_chroma > 0.0166 && f.edge_density > 0.1502 && f.smoothness < 0.0614)
}

// ===== Combined pipeline (Cycle 90 [B]) for ground truth =====

struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self { Self(seed.wrapping_add(0xdeadbeef)) }
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
        let dl = p.l - c.l; let da = p.a - c.a; let db = p.b - c.b;
        dl*dl + da*da + db*db
    }).collect();
    for _ in 1..k {
        let total: f64 = min_dists.iter().map(|&v| v as f64).sum();
        if total <= 0.0 { centroids.push(samples[first_idx]); continue; }
        let pick = rng.next_f32() as f64 * total;
        let mut cumul = 0.0f64;
        let mut chosen = sample_size - 1;
        for (i, &d) in min_dists.iter().enumerate() {
            cumul += d as f64;
            if cumul >= pick { chosen = i; break; }
        }
        let new_c = samples[chosen];
        centroids.push(new_c);
        for (i, p) in samples.iter().enumerate() {
            let dl = p.l - new_c.l; let da = p.a - new_c.a; let db = p.b - new_c.b;
            let d = dl*dl + da*da + db*db;
            if d < min_dists[i] { min_dists[i] = d; }
        }
    }
    centroids
}

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
    let _g3 = gauss5(&g2, w, h);
    let g4 = gauss5(&g2, w, h);
    let g4 = gauss5(&g4, w, h);
    let n = w * h;
    let mut b = vec![0f32; n];
    for i in 0..n {
        let dog_low = (g1[i] - g2[i]).abs();
        let dog_high = (g2[i] - g4[i]).abs();
        b[i] = dog_low + dog_high + eps;
    }
    b
}

fn m_weighted_lloyd(src_oklab: &[Oklab], b: &[f32], palette_init: &[Oklab],
    w_l: f32, w_a: f32, w_b: f32, iters: usize) -> (Vec<Oklab>, Vec<u8>) {
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
                let dl = p.l - c.l; let da = p.a - c.a; let dbb = p.b - c.b;
                let d = w_l*dl*dl + w_a*da*da + w_b*dbb*dbb;
                if d < best_d { best_d = d; best_j = j as u8; }
            }
            if indices[i] != best_j { indices[i] = best_j; changed += 1; }
        }
        let mut sum_l = vec![0f64; k]; let mut sum_a = vec![0f64; k];
        let mut sum_b = vec![0f64; k]; let mut sum_w = vec![0f64; k];
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
        if changed == 0 { break; }
    }
    (palette, indices)
}

fn icm_step_scalar(src_oklab: &[Oklab], w: usize, h: usize, palette: &[Oklab],
    indices: &mut [u8], lambda_sq: f32) {
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
                let dl = px.l - pj.l; let da = px.a - pj.a; let db = px.b - pj.b;
                let data = dl*dl + da*da + db*db;
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

struct SoAPalette { l: Vec<f32>, a: Vec<f32>, b: Vec<f32>, k_pad: usize }
impl SoAPalette {
    fn from_oklab(pal: &[Oklab]) -> Self {
        let k_real = pal.len();
        let k_pad = (k_real + 3) & !3;
        let mut l = Vec::with_capacity(k_pad);
        let mut a = Vec::with_capacity(k_pad);
        let mut b = Vec::with_capacity(k_pad);
        for c in pal { l.push(c.l); a.push(c.a); b.push(c.b); }
        for _ in k_real..k_pad { l.push(1e9); a.push(1e9); b.push(1e9); }
        Self { l, a, b, k_pad }
    }
}

fn icm_step_simd(src_oklab: &[Oklab], w: usize, h: usize, pal: &SoAPalette,
    indices: &mut [u8], lambda_sq: f32) {
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
            let pl = f32x4::splat(px.l); let pa = f32x4::splat(px.a); let pb = f32x4::splat(px.b);
            let mut min_d2 = inf_f4;
            let mut min_idx = f32x4::from([0.0,1.0,2.0,3.0]);
            let mut idx_iter = f32x4::from([0.0,1.0,2.0,3.0]);
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
                let dl = pl - cl; let da = pa - ca; let db = pb - cb;
                let data = dl*dl + da*da + db*db;
                let mut smooth_count = zero_f4;
                if n_up_active { smooth_count += idx_iter.cmp_ne(nup_v).blend(one_f4, zero_f4); }
                if n_dn_active { smooth_count += idx_iter.cmp_ne(ndn_v).blend(one_f4, zero_f4); }
                if n_lf_active { smooth_count += idx_iter.cmp_ne(nlf_v).blend(one_f4, zero_f4); }
                if n_rt_active { smooth_count += idx_iter.cmp_ne(nrt_v).blend(one_f4, zero_f4); }
                let cost = data + lam_f4 * smooth_count;
                let mask = cost.cmp_lt(min_d2);
                min_d2 = mask.blend(cost, min_d2);
                min_idx = mask.blend(idx_iter, min_idx);
                idx_iter += four_f4;
                j += 4;
            }
            let arr_d = min_d2.to_array();
            let arr_i = min_idx.to_array();
            let mut best_d = arr_d[0]; let mut best_j = arr_i[0] as u8;
            for k in 1..4 { if arr_d[k] < best_d { best_d = arr_d[k]; best_j = arr_i[k] as u8; } }
            indices[i] = best_j;
        }
    }
}

fn palette_retrain(src_oklab: &[Oklab], palette: &mut [Oklab], indices: &[u8]) {
    let k = palette.len();
    let mut sl = vec![0f64; k]; let mut sa = vec![0f64; k];
    let mut sb = vec![0f64; k]; let mut ct = vec![0u32; k];
    for (px, &idx) in src_oklab.iter().zip(indices.iter()) {
        let j = idx as usize;
        sl[j] += px.l as f64; sa[j] += px.a as f64; sb[j] += px.b as f64; ct[j] += 1;
    }
    for j in 0..k {
        if ct[j] > 0 {
            let c = ct[j] as f64;
            palette[j] = Oklab { l: (sl[j]/c) as f32, a: (sa[j]/c) as f32, b: (sb[j]/c) as f32 };
        }
    }
}

fn ssim_via_nupic(orig: &PathBuf, cmp_path: &PathBuf, nupic: &PathBuf) -> f64 {
    let out = Command::new(nupic)
        .args(["compare", "-m", "ssimulacra2"])
        .arg(orig).arg(cmp_path)
        .output().expect("nupic compare");
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines().find_map(|l| {
        l.strip_prefix("SSIMULACRA2: ")
            .and_then(|v| v.split_whitespace().next())
            .and_then(|n| n.parse::<f64>().ok())
    }).unwrap_or(f64::NAN)
}

// ===== Bench one fixture: returns ΔSSIM for combined vs baseline =====
fn bench_fixture(fixture_path: &PathBuf, nupic: &PathBuf, label: &str) -> Option<f64> {
    let img = ImageReader::open(fixture_path).ok()?.with_guessed_format().ok()?.decode().ok()?;
    let r = img.to_rgba8();
    let w = r.width(); let h = r.height();
    let raw_rgba = r.into_raw();
    if w < 4 || h < 4 { return None; }
    let (n_colors, alpha_imp) = classify_for_palette_size_with_importance(&raw_rgba, w as usize);
    let tmp = std::env::temp_dir();
    let mut oxi = oxipng::Options::from_preset(3);
    oxi.strip = oxipng::StripChunks::Safe;
    let src_oklab: Vec<Oklab> = raw_rgba.chunks_exact(4).map(|p|
        srgb_u8_to_oklab(Rgb { r: p[0], g: p[1], b: p[2] })).collect();
    let lambdas = [0.0001f32, 0.00005, 0.00002];

    // [A] Baseline
    let (pi_a, ai_a) = train_palette_rgba(&raw_rgba, w, h, n_colors).ok()?;
    let (pal_init_a, alpha_a) = if alpha_imp > 0.0 {
        refine_palette_kmeans_importance(&raw_rgba, w, h, &pi_a, &ai_a, 100, alpha_imp)
    } else {
        refine_palette_kmeans(&raw_rgba, w, h, &pi_a, &ai_a, 100)
    };
    let (idx_init_a, _) = apply_palette_rgba(&raw_rgba, w, h, &pal_init_a, &alpha_a);
    let mut pal_a = pal_init_a;
    let mut idx_a = idx_init_a;
    for &lam in &lambdas {
        icm_step_scalar(&src_oklab, w as usize, h as usize, &pal_a, &mut idx_a, lam);
        palette_retrain(&src_oklab, &mut pal_a, &idx_a);
    }
    let pal_a_srgb: Vec<Rgb<u8>> = pal_a.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
    let trns_a = if alpha_a.iter().all(|&a| a == 255) { None } else { Some(alpha_a.as_slice()) };
    let raw_a = encode_indexed_png_with_alpha(w, h, &idx_a, &pal_a_srgb, trns_a).ok()?;
    let out_a = oxipng::optimize_from_memory(&raw_a, &oxi).ok()?;
    let path_a = tmp.join(format!("c92_{}_a.png", label));
    std::fs::write(&path_a, &out_a).ok()?;
    let ssim_a = ssim_via_nupic(fixture_path, &path_a, nupic);

    // [B] Combined
    let pi_b = kmeans_pp_init_oklab(&src_oklab, n_colors, label.len() as u64 * 31 + 7);
    let ai_b = ai_a.clone();
    let (pal_init_b, alpha_b) = if alpha_imp > 0.0 {
        refine_palette_kmeans_importance(&raw_rgba, w, h, &pi_b, &ai_b, 100, alpha_imp)
    } else {
        refine_palette_kmeans(&raw_rgba, w, h, &pi_b, &ai_b, 100)
    };
    let b_weight = compute_b_weight(&src_oklab, w as usize, h as usize, 0.001);
    let (pal_mwl, idx_mwl) =
        m_weighted_lloyd(&src_oklab, &b_weight, &pal_init_b, 1.0, 0.5, 0.5, 10);
    let mut pal_b = pal_mwl;
    let mut idx_b = idx_mwl;
    for &lam in &lambdas {
        let soa = SoAPalette::from_oklab(&pal_b);
        icm_step_simd(&src_oklab, w as usize, h as usize, &soa, &mut idx_b, lam);
        palette_retrain(&src_oklab, &mut pal_b, &idx_b);
    }
    let pal_b_srgb: Vec<Rgb<u8>> = pal_b.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
    let trns_b = if alpha_b.iter().all(|&a| a == 255) { None } else { Some(alpha_b.as_slice()) };
    let raw_b = encode_indexed_png_with_alpha(w, h, &idx_b, &pal_b_srgb, trns_b).ok()?;
    let out_b = oxipng::optimize_from_memory(&raw_b, &oxi).ok()?;
    let path_b = tmp.join(format!("c92_{}_b.png", label));
    std::fs::write(&path_b, &out_b).ok()?;
    let ssim_b = ssim_via_nupic(fixture_path, &path_b, nupic);

    Some(ssim_b - ssim_a)
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors().nth(2).unwrap().to_path_buf();
    let nupic = root.join("target/release/nupic");
    let corpus_dir = root.join("assets/png-bench/corpus-500");

    let mut entries: Vec<PathBuf> = std::fs::read_dir(&corpus_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map_or(false, |e| e == "png"))
        .collect();
    entries.sort();
    println!("Cycle 92 — R1 classifier 506-corpus validation");
    println!("  loaded {} PNG fixtures from {}", entries.len(), corpus_dir.display());
    println!();

    // === Stage 1: features on all ===
    let t0 = Instant::now();
    let mut feats_all: Vec<(PathBuf, Features, (u32, u32))> = Vec::with_capacity(entries.len());
    let mut skipped = 0usize;
    for path in &entries {
        let img = match ImageReader::open(path).and_then(|r| r.with_guessed_format()) {
            Ok(r) => match r.decode() { Ok(i) => i, Err(_) => { skipped += 1; continue; } },
            Err(_) => { skipped += 1; continue; }
        };
        let r = img.to_rgba8();
        let w = r.width(); let h = r.height();
        if w < 4 || h < 4 { skipped += 1; continue; }
        let raw = r.into_raw();
        let f = compute_features(&raw, w as usize, h as usize);
        feats_all.push((path.clone(), f, (w, h)));
    }
    println!(
        "Stage 1 (features): {} fixtures processed, {} skipped, took {:.1}s",
        feats_all.len(), skipped, t0.elapsed().as_secs_f64()
    );
    println!();

    // === Stage 2: predict + distribution ===
    let mut n_friend = 0usize;
    let mut n_hostile = 0usize;
    let mut by_size: BTreeMap<&'static str, (usize, usize)> = BTreeMap::new();
    let bucket = |w: u32, h: u32| -> &'static str {
        let n = (w as u64) * (h as u64);
        if n < 100_000 { "S<0.1MP" }
        else if n < 1_000_000 { "M<1MP" }
        else if n < 5_000_000 { "L<5MP" }
        else { "XL≥5MP" }
    };
    for (_, f, (w, h)) in &feats_all {
        let friend = predict_friend(f);
        if friend { n_friend += 1; } else { n_hostile += 1; }
        let b = bucket(*w, *h);
        let e = by_size.entry(b).or_insert((0, 0));
        if friend { e.0 += 1; } else { e.1 += 1; }
    }
    println!("Stage 2 (classifier predictions):");
    println!("  FRIEND total: {}  ({:.1}%)", n_friend, 100.0 * n_friend as f64 / feats_all.len() as f64);
    println!("  HOSTILE total: {} ({:.1}%)", n_hostile, 100.0 * n_hostile as f64 / feats_all.len() as f64);
    println!("  size bucket distribution:");
    for (b, (f, h)) in &by_size {
        let tot = f + h;
        println!("    {:<10} {:>4} fixtures  FRIEND {:>3} ({:>4.1}%)  HOSTILE {:>3} ({:>4.1}%)",
                 b, tot, f, 100.0 * *f as f64 / tot as f64,
                 h, 100.0 * *h as f64 / tot as f64);
    }
    println!();

    // === Stage 3: stratified sample for ground truth ===
    // Pick small/medium fixtures only (no XL) to keep autorun budget reasonable.
    let candidates_friend: Vec<usize> = feats_all.iter().enumerate()
        .filter(|(_, (_, f, (w, h)))| predict_friend(f) && (*w as u64 * *h as u64) < 1_500_000)
        .map(|(i, _)| i).collect();
    let candidates_hostile: Vec<usize> = feats_all.iter().enumerate()
        .filter(|(_, (_, f, (w, h)))| !predict_friend(f) && (*w as u64 * *h as u64) < 1_500_000)
        .map(|(i, _)| i).collect();
    println!("Stage 3 sample pool: candidates FRIEND={}, HOSTILE={} (size<1.5MP only)",
             candidates_friend.len(), candidates_hostile.len());

    // Deterministic stride sample 10 from each
    fn stride_sample(pool: &[usize], n: usize) -> Vec<usize> {
        if pool.len() <= n { return pool.to_vec(); }
        let stride = pool.len() / n;
        (0..n).map(|i| pool[i * stride]).collect()
    }
    let sample_friend = stride_sample(&candidates_friend, 10);
    let sample_hostile = stride_sample(&candidates_hostile, 10);
    let sample_indices: Vec<usize> = sample_friend.iter().chain(sample_hostile.iter()).copied().collect();
    println!("Sampled {} fixtures ({} pred-FRIEND + {} pred-HOSTILE) for ground truth",
             sample_indices.len(), sample_friend.len(), sample_hostile.len());
    println!();

    // === Stage 4: ground truth ===
    println!("Stage 4 (ground truth bench, R1+R8+R9 combined vs baseline):");
    println!(
        "{:<28} {:>5} {:>6} {:>8} {:>9} {:>9} {:>10}",
        "fixture", "MP", "chroma", "edge", "smooth", "ΔSSIM", "predict/actual"
    );

    let t_gt = Instant::now();
    let mut results: Vec<(String, &Features, bool, Option<f64>)> = Vec::new();
    for &idx in &sample_indices {
        let (path, f, (w, h)) = &feats_all[idx];
        let label_owned = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?").to_string();
        let pred_friend = predict_friend(f);
        let d_ssim = bench_fixture(path, &nupic, &label_owned);
        let actual_friend = match d_ssim { Some(d) => d >= 0.5, None => false };
        let verdict = if d_ssim.is_none() { "ERR".to_string() }
            else if pred_friend == actual_friend { "OK".to_string() }
            else if pred_friend { "FP".to_string() } else { "FN".to_string() };
        println!(
            "{:<28} {:>5} {:>6.3} {:>8.4} {:>9.4} {:>+9.2} {:>4}/{:<5} {}",
            label_owned,
            (*w as u64 * *h as u64) / 1_000_000,
            f.mean_chroma, f.edge_density, f.smoothness,
            d_ssim.unwrap_or(0.0),
            if pred_friend { "F" } else { "H" },
            if actual_friend { "F" } else { "H" },
            verdict
        );
        results.push((label_owned, f, pred_friend, d_ssim));
    }
    println!();
    println!("Stage 4 took {:.1}s", t_gt.elapsed().as_secs_f64());

    // Accuracy
    let valid: Vec<&(String, &Features, bool, Option<f64>)> = results.iter()
        .filter(|(_, _, _, d)| d.is_some()).collect();
    let n_val = valid.len();
    let correct = valid.iter()
        .filter(|(_, _, p, d)| (d.unwrap() >= 0.5) == *p).count();
    let fps = valid.iter()
        .filter(|(_, _, p, d)| *p && d.unwrap() < 0.5).count();
    let fns_ = valid.iter()
        .filter(|(_, _, p, d)| !*p && d.unwrap() >= 0.5).count();
    println!();
    println!("=== Final accuracy: {}/{} ({:.1}%) ===", correct, n_val, 100.0 * correct as f64 / n_val.max(1) as f64);
    println!("  FPs (predicted FRIEND, actual HOSTILE) = {}", fps);
    println!("  FNs (predicted HOSTILE, actual FRIEND) = {}", fns_);
    let acc_pct = 100.0 * correct as f64 / n_val.max(1) as f64;
    if acc_pct >= 85.0 {
        println!(">>> GREEN — Cycle 91a thresholds generalize");
    } else if acc_pct >= 70.0 {
        println!(">>> YELLOW — thresholds partially generalize, re-fit recommended");
    } else {
        println!(">>> RED — thresholds overfit on Cycle 91a's 10 fixtures, re-fit needed");
    }

    Ok(())
}
