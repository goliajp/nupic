//! Cycle 69 — Multi-scale weighted joint Lloyd-ICM (P4 integration).
//! Combines Cycle 44 multi-scale gradient importance weights with
//! Cycle 68 joint alternating optimization.
//!
//! Joint cost (multi-scale-weighted):
//!   f(P, I) = Σ_i w_i · ||p_i - P[I_i]||² + λ · Σ_{(i,n)∈E} [I_i ≠ I_n]
//!
//! where w_i = 1 / (1 + α · multi-scale-grad_i)

use std::path::PathBuf;
use image::ImageReader;
use rgb::Rgb;
use nupic_color::{Oklab, srgb_u8_to_oklab, oklab_to_srgb_u8};
use nupic_quantize::{
    train_palette_rgba, refine_palette_kmeans_importance, refine_palette_kmeans,
    apply_palette_rgba, encode_indexed_png_with_alpha,
    classify_for_palette_size_with_importance,
};

fn compute_ms_weights(src_rgba: &[u8], w: usize, h: usize, alpha: f32) -> Vec<f32> {
    let n = w * h;
    let luma: Vec<u8> = src_rgba.chunks_exact(4).map(|p| ((p[0] as u32 + p[1] as u32 + p[2] as u32) / 3) as u8).collect();
    const SCALES: [usize; 2] = [1, 2];
    let mut weights = vec![1.0f32; n];
    if alpha > 0.0 {
        for i in 0..n {
            let y = i / w; let x = i % w;
            let l0 = luma[i] as i32;
            let mut grad_sum = 0i32; let mut cnt = 0;
            for &s in &SCALES {
                if x + s < w { grad_sum += (l0 - luma[i + s] as i32).abs(); cnt += 1; }
                if y + s < h { grad_sum += (l0 - luma[(y + s) * w + x] as i32).abs(); cnt += 1; }
            }
            let mg = if cnt > 0 { grad_sum as f32 / cnt as f32 } else { 0.0 };
            weights[i] = 1.0 / (1.0 + alpha * mg);
        }
    }
    weights
}

fn icm_weighted(src_oklab: &[Oklab], weights: &[f32], w: usize, h: usize, palette: &[Oklab], indices: &mut [u8], lambda_sq: f32) {
    let k = palette.len();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let px = src_oklab[i];
            let wi = weights[i];
            let n_up = if y > 0 { indices[i - w] } else { 255 };
            let n_dn = if y + 1 < h { indices[i + w] } else { 255 };
            let n_lf = if x > 0 { indices[i - 1] } else { 255 };
            let n_rt = if x + 1 < w { indices[i + 1] } else { 255 };
            let mut best_j = indices[i];
            let mut best_cost = f32::INFINITY;
            for j in 0..k {
                let pj = palette[j];
                let dl = px.l - pj.l; let da = px.a - pj.a; let db = px.b - pj.b;
                let data = wi * (dl*dl + da*da + db*db);
                let mut s = 0u32;
                if n_up != j as u8 && n_up != 255 { s += 1; }
                if n_dn != j as u8 && n_dn != 255 { s += 1; }
                if n_lf != j as u8 && n_lf != 255 { s += 1; }
                if n_rt != j as u8 && n_rt != 255 { s += 1; }
                let cost = data + lambda_sq * (s as f32);
                if cost < best_cost { best_cost = cost; best_j = j as u8; }
            }
            indices[i] = best_j;
        }
    }
}

fn palette_retrain_weighted(src_oklab: &[Oklab], weights: &[f32], palette: &mut [Oklab], indices: &[u8]) {
    let k = palette.len();
    let mut sum_l = vec![0f64; k]; let mut sum_a = vec![0f64; k]; let mut sum_b = vec![0f64; k];
    let mut sum_w = vec![0f64; k];
    for (i, (px, &idx)) in src_oklab.iter().zip(indices.iter()).enumerate() {
        let j = idx as usize;
        let wi = weights[i] as f64;
        sum_l[j] += wi * px.l as f64; sum_a[j] += wi * px.a as f64; sum_b[j] += wi * px.b as f64;
        sum_w[j] += wi;
    }
    for j in 0..k {
        if sum_w[j] > 1e-9 {
            palette[j] = Oklab { l: (sum_l[j] / sum_w[j]) as f32, a: (sum_a[j] / sum_w[j]) as f32, b: (sum_b[j] / sum_w[j]) as f32 };
        }
    }
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();
    let nupic = root.join("target/release/nupic");
    let tmp = std::env::temp_dir();
    let fixtures: &[(&str, &str)] = &[
        ("inputs/04-photo-portrait.png", "04 portrait"),
        ("inputs/05-photo-mountain.png", "05 mountain"),
        ("inputs/06-photo-landscape.png", "06 landscape"),
        ("inputs/07-photo-product.png", "07 product"),
    ];
    for &(rel, lbl) in fixtures {
        let p = root.join("assets/png-bench").join(rel);
        let img = ImageReader::open(&p)?.with_guessed_format()?.decode()?;
        let r = img.to_rgba8();
        let w = r.width(); let h = r.height();
        let raw_rgba = r.into_raw();
        let (n_colors, alpha_imp) = classify_for_palette_size_with_importance(&raw_rgba, w as usize);
        let (pi, ai) = train_palette_rgba(&raw_rgba, w, h, n_colors)?;
        let (pal_init, alpha) = if alpha_imp > 0.0 {
            refine_palette_kmeans_importance(&raw_rgba, w, h, &pi, &ai, 100, alpha_imp)
        } else {
            refine_palette_kmeans(&raw_rgba, w, h, &pi, &ai, 100)
        };
        let (indices_base, ps_init) = apply_palette_rgba(&raw_rgba, w, h, &pal_init, &alpha);
        let src_oklab: Vec<Oklab> = raw_rgba.chunks_exact(4).map(|p| srgb_u8_to_oklab(Rgb{r:p[0], g:p[1], b:p[2]})).collect();
        let trns = if alpha.iter().all(|&a| a == 255) { None } else { Some(alpha.as_slice()) };
        let mut o = oxipng::Options::from_preset(3);
        o.strip = oxipng::StripChunks::Safe;

        let raw_png = encode_indexed_png_with_alpha(w, h, &indices_base, &ps_init, trns)?;
        let out_base = oxipng::optimize_from_memory(&raw_png, &o).unwrap();
        let base_path = tmp.join(format!("c69_{}_base.png", lbl.replace(' ', "_")));
        std::fs::write(&base_path, &out_base)?;
        let cmp = std::process::Command::new(&nupic).args(["compare","-m","ssimulacra2"]).arg(&p).arg(&base_path).output()?;
        let s = String::from_utf8_lossy(&cmp.stdout);
        let ssim_base: f64 = s.lines().find_map(|l| l.strip_prefix("SSIMULACRA2: ").and_then(|v| v.split_whitespace().next()).and_then(|n| n.parse().ok())).unwrap_or(f64::NAN);
        println!("\n=== {} (joint i=3 only) ===", lbl);
        println!("  baseline                   {:>5.0}KB / SSIM {:.3}", out_base.len() as f64/1024.0, ssim_base);

        // Multi-scale weighted joint optimization at λ²=0.0001 i=3
        for ms_alpha in [0.0_f32, 0.1, 0.3, 0.5, 1.0] {
            let weights = compute_ms_weights(&raw_rgba, w as usize, h as usize, ms_alpha);
            let mut pal = pal_init.clone();
            let mut indices = indices_base.clone();
            for _ in 0..3 {
                icm_weighted(&src_oklab, &weights, w as usize, h as usize, &pal, &mut indices, 0.0001);
                palette_retrain_weighted(&src_oklab, &weights, &mut pal, &indices);
            }
            let palette_srgb: Vec<Rgb<u8>> = pal.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
            let raw_png = encode_indexed_png_with_alpha(w, h, &indices, &palette_srgb, trns)?;
            let out = oxipng::optimize_from_memory(&raw_png, &o).unwrap();
            let p_out = tmp.join(format!("c69_{}_ms{}.png", lbl.replace(' ',"_"), (ms_alpha*10.0) as u32));
            std::fs::write(&p_out, &out)?;
            let cmp = std::process::Command::new(&nupic).args(["compare","-m","ssimulacra2"]).arg(&p).arg(&p_out).output()?;
            let s = String::from_utf8_lossy(&cmp.stdout);
            let ssim: f64 = s.lines().find_map(|l| l.strip_prefix("SSIMULACRA2: ").and_then(|v| v.split_whitespace().next()).and_then(|n| n.parse().ok())).unwrap_or(f64::NAN);
            let dsz = (out.len() as f64 / out_base.len() as f64 - 1.0) * 100.0;
            let dss = ssim - ssim_base;
            let slope = if dss < 0.0 { dsz / (-dss) } else { 0.0 };
            let label = if ms_alpha == 0.0 { "joint (no MS)".to_string() } else { format!("joint + MS α={}", ms_alpha) };
            println!("  {:<22} {:>5.0}KB / {:.3}  Δsz={:+.2}% Δss={:+.3}  slope={:.1}",
                label, out.len() as f64/1024.0, ssim, dsz, dss, slope);
        }
    }
    Ok(())
}
