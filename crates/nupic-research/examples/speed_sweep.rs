//! Cycle 70b — Verify anneal-ultra-fine on baseline-7
use std::path::PathBuf;
use image::ImageReader;
use rgb::Rgb;
use nupic_color::{Oklab, srgb_u8_to_oklab, oklab_to_srgb_u8};
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
        ("inputs/01-png-transparency-demo.png", "01 trans"),
        ("inputs/02-pluto-transparent.png", "02 pluto"),
        ("inputs/03-wikipedia-logo.png", "03 wiki"),
        ("inputs/04-photo-portrait.png", "04 portrait"),
        ("inputs/05-photo-mountain.png", "05 mountain"),
        ("inputs/06-photo-landscape.png", "06 landscape"),
        ("inputs/07-photo-product.png", "07 product"),
    ];
    let mut total_base = 0u64; let mut total_anneal = 0u64;
    let mut all_positive = true;
    println!("Cycle 70b — anneal ultra-fine (λ²=0.0001, 0.00005, 0.00002) on baseline-7");
    println!("{:<13} {:>10} {:>11} {:>11} {:>9}", "fixture", "base_KB/SS", "anneal_KB/SS", "Δsize/Δss", "Pareto?");
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
        let base_path = tmp.join(format!("c70b_{}_base.png", lbl.replace(' ', "_")));
        std::fs::write(&base_path, &out_base)?;
        let cmp = std::process::Command::new(&nupic).args(["compare","-m","ssimulacra2"]).arg(&p).arg(&base_path).output()?;
        let s = String::from_utf8_lossy(&cmp.stdout);
        let ssim_base: f64 = s.lines().find_map(|l| l.strip_prefix("SSIMULACRA2: ").and_then(|v| v.split_whitespace().next()).and_then(|n| n.parse().ok())).unwrap_or(f64::NAN);

        let lambdas = [0.0001f32, 0.00005, 0.00002];
        let mut pal = pal_init.clone();
        let mut indices = indices_base.clone();
        for &lam in &lambdas {
            icm_step(&src_oklab, w as usize, h as usize, &pal, &mut indices, lam);
            palette_retrain(&src_oklab, &mut pal, &indices);
        }
        let palette_srgb: Vec<Rgb<u8>> = pal.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
        let raw_png = encode_indexed_png_with_alpha(w, h, &indices, &palette_srgb, trns)?;
        let out_anneal = oxipng::optimize_from_memory(&raw_png, &o).unwrap();
        let an_path = tmp.join(format!("c70b_{}_anneal.png", lbl.replace(' ', "_")));
        std::fs::write(&an_path, &out_anneal)?;
        let cmp = std::process::Command::new(&nupic).args(["compare","-m","ssimulacra2"]).arg(&p).arg(&an_path).output()?;
        let s = String::from_utf8_lossy(&cmp.stdout);
        let ssim_an: f64 = s.lines().find_map(|l| l.strip_prefix("SSIMULACRA2: ").and_then(|v| v.split_whitespace().next()).and_then(|n| n.parse().ok())).unwrap_or(f64::NAN);
        let dsz = (out_anneal.len() as f64 / out_base.len() as f64 - 1.0) * 100.0;
        let dss = ssim_an - ssim_base;
        total_base += out_base.len() as u64; total_anneal += out_anneal.len() as u64;
        let strict_pareto = dsz < 0.0 && dss >= -0.05;  // within 0.05 SSIM ≈ noise
        if !strict_pareto { all_positive = false; }
        let mark = if dsz < 0.0 && dss >= 0.0 { "STRICT++" } else if strict_pareto { "noise-OK" } else { "DEGRADE" };
        println!("{:<13} {:>4}KB/{:5.2} {:>5}KB/{:5.2} {:+6.2}%/{:+5.2} {:>9}",
            lbl, out_base.len()/1024, ssim_base, out_anneal.len()/1024, ssim_an, dsz, dss, mark);
    }
    println!();
    println!("TOTAL: base={}KB anneal={}KB Δ={:+.2}%", total_base/1024, total_anneal/1024, (total_anneal as f64/total_base as f64 - 1.0)*100.0);
    println!("All positive Pareto: {}", all_positive);
    Ok(())
}
