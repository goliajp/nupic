//! Cycle 68 — Joint quantize-encode optimization.
//! Alternating minimization:
//!   1. Standard Lloyd → palette + indices
//!   2. ICM step with λ → updated indices (smoother)
//!   3. Palette re-training: centroid = mean of pixels now assigned
//!   4. Repeat steps 2-3
//!
//! This is true joint optimization of (palette, indices) under the
//! cost f(P, I) = Σ_i ||p_i - P[I_i]||² + λ · Σ_{neighbors} [I ≠ I_n]

use std::path::PathBuf;
use image::ImageReader;
use rgb::Rgb;
use nupic_color::{Oklab, srgb_u8_to_oklab};
use nupic_quantize::{
    train_palette_rgba, refine_palette_kmeans_importance, refine_palette_kmeans,
    apply_palette_rgba, encode_indexed_png_with_alpha,
    classify_for_palette_size_with_importance,
};

fn icm_step(src_oklab: &[Oklab], w: usize, h: usize, palette: &[Oklab], indices: &mut [u8], lambda_sq: f32) {
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

fn palette_retrain(src_oklab: &[Oklab], palette: &mut [Oklab], indices: &[u8]) {
    let k = palette.len();
    let mut sum_l = vec![0f64; k]; let mut sum_a = vec![0f64; k]; let mut sum_b = vec![0f64; k];
    let mut count = vec![0u32; k];
    for (px, &idx) in src_oklab.iter().zip(indices.iter()) {
        let j = idx as usize;
        sum_l[j] += px.l as f64; sum_a[j] += px.a as f64; sum_b[j] += px.b as f64;
        count[j] += 1;
    }
    for j in 0..k {
        if count[j] > 0 {
            let c = count[j] as f64;
            palette[j] = Oklab { l: (sum_l[j] / c) as f32, a: (sum_a[j] / c) as f32, b: (sum_b[j] / c) as f32 };
        }
    }
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();
    let nupic = root.join("target/release/nupic");
    let tmp = std::env::temp_dir();
    let fixtures: &[(&str, &str)] = &[
        ("inputs/04-photo-portrait.png", "04 portrait"),
        ("inputs/06-photo-landscape.png", "06 landscape"),
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

        // Baseline
        let raw_png = encode_indexed_png_with_alpha(w, h, &indices_base, &ps_init, trns)?;
        let out_base = oxipng::optimize_from_memory(&raw_png, &o).unwrap();
        let base_path = tmp.join(format!("c68_{}_base.png", lbl.replace(' ', "_")));
        std::fs::write(&base_path, &out_base)?;
        let cmp = std::process::Command::new(&nupic).args(["compare","-m","ssimulacra2"]).arg(&p).arg(&base_path).output()?;
        let s = String::from_utf8_lossy(&cmp.stdout);
        let ssim_base: f64 = s.lines().find_map(|l| l.strip_prefix("SSIMULACRA2: ").and_then(|v| v.split_whitespace().next()).and_then(|n| n.parse().ok())).unwrap_or(f64::NAN);
        println!("\n=== {} ===", lbl);
        println!("  baseline                       {:>5.0}KB / SSIM {:.3}", out_base.len() as f64/1024.0, ssim_base);

        for &lambda_sq in &[0.0001_f32, 0.0003, 0.001] {
            // 3 rounds of joint optimization
            for joint_iters in [1usize, 2, 3] {
                let mut pal = pal_init.clone();
                let mut indices = indices_base.clone();
                for _ in 0..joint_iters {
                    icm_step(&src_oklab, w as usize, h as usize, &pal, &mut indices, lambda_sq);
                    palette_retrain(&src_oklab, &mut pal, &indices);
                }
                // Final apply (re-assign with new palette via standard L2)
                use nupic_color::oklab_to_srgb_u8;
                let palette_srgb: Vec<Rgb<u8>> = pal.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
                let raw_png = encode_indexed_png_with_alpha(w, h, &indices, &palette_srgb, trns)?;
                let out = oxipng::optimize_from_memory(&raw_png, &o).unwrap();
                let p_out = tmp.join(format!("c68_{}_l{}_it{}.png", lbl.replace(' ',"_"), (lambda_sq*100000.0) as u32, joint_iters));
                std::fs::write(&p_out, &out)?;
                let cmp = std::process::Command::new(&nupic).args(["compare","-m","ssimulacra2"]).arg(&p).arg(&p_out).output()?;
                let s = String::from_utf8_lossy(&cmp.stdout);
                let ssim: f64 = s.lines().find_map(|l| l.strip_prefix("SSIMULACRA2: ").and_then(|v| v.split_whitespace().next()).and_then(|n| n.parse().ok())).unwrap_or(f64::NAN);
                let dsz = (out.len() as f64 / out_base.len() as f64 - 1.0) * 100.0;
                let dss = ssim - ssim_base;
                let slope = if dss < 0.0 { dsz / (-dss) } else { 0.0 };
                println!("  λ²={:.5} joint_iters={}   {:>5.0}KB / {:.3}  Δsz={:+.2}% Δss={:+.3}  slope={:.1}",
                    lambda_sq, joint_iters, out.len() as f64/1024.0, ssim, dsz, dss, slope);
            }
        }
    }
    Ok(())
}
