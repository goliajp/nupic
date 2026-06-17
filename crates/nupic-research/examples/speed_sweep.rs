//! Cycle 65 stage 2 — much smaller λ
use std::path::PathBuf;
use std::time::Instant;
use image::ImageReader;
use rgb::Rgb;
use nupic_color::{Oklab, srgb_u8_to_oklab};
use nupic_quantize::{
    train_palette_rgba, refine_palette_kmeans_importance, refine_palette_kmeans,
    apply_palette_rgba, encode_indexed_png_with_alpha,
    classify_for_palette_size_with_importance,
};

fn icm_refine(src_oklab: &[Oklab], w: usize, h: usize, palette: &[Oklab], indices: &mut [u8], lambda_sq: f32, iters: usize) {
    let k = palette.len();
    for _ in 0..iters {
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
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();
    let nupic = root.join("target/release/nupic");
    let tmp = std::env::temp_dir();
    let fixture = "inputs/04-photo-portrait.png";
    let p = root.join("assets/png-bench").join(fixture);
    let img = ImageReader::open(&p)?.with_guessed_format()?.decode()?;
    let r = img.to_rgba8();
    let w = r.width(); let h = r.height();
    let raw_rgba = r.into_raw();
    let (n_colors, alpha_imp) = classify_for_palette_size_with_importance(&raw_rgba, w as usize);
    let (pi, ai) = train_palette_rgba(&raw_rgba, w, h, n_colors)?;
    let (pal, alpha) = if alpha_imp > 0.0 {
        refine_palette_kmeans_importance(&raw_rgba, w, h, &pi, &ai, 100, alpha_imp)
    } else {
        refine_palette_kmeans(&raw_rgba, w, h, &pi, &ai, 100)
    };
    let (indices_base, ps) = apply_palette_rgba(&raw_rgba, w, h, &pal, &alpha);
    let src_oklab: Vec<Oklab> = raw_rgba.chunks_exact(4).map(|p| srgb_u8_to_oklab(Rgb{r:p[0], g:p[1], b:p[2]})).collect();
    let trns = if alpha.iter().all(|&a| a == 255) { None } else { Some(alpha.as_slice()) };
    let raw_png = encode_indexed_png_with_alpha(w, h, &indices_base, &ps, trns)?;
    let mut o = oxipng::Options::from_preset(3);
    o.strip = oxipng::StripChunks::Safe;
    let out_base = oxipng::optimize_from_memory(&raw_png, &o).unwrap();
    let base_path = tmp.join("c65b_base.png");
    std::fs::write(&base_path, &out_base)?;
    let cmp = std::process::Command::new(&nupic).args(["compare","-m","ssimulacra2"]).arg(&p).arg(&base_path).output()?;
    let s = String::from_utf8_lossy(&cmp.stdout);
    let ssim_base: f64 = s.lines().find_map(|l| l.strip_prefix("SSIMULACRA2: ").and_then(|v| v.split_whitespace().next()).and_then(|n| n.parse().ok())).unwrap_or(f64::NAN);
    println!("=== 04 portrait — finer λ ===");
    println!("baseline             {:>5.0}KB / SSIM {:.3}", out_base.len() as f64/1024.0, ssim_base);
    // λ ranges much smaller
    for lambda_sq in [0.0001_f32, 0.0005, 0.001, 0.002, 0.005] {
        let mut indices = indices_base.clone();
        icm_refine(&src_oklab, w as usize, h as usize, &pal, &mut indices, lambda_sq, 1);
        let raw_png = encode_indexed_png_with_alpha(w, h, &indices, &ps, trns)?;
        let out = oxipng::optimize_from_memory(&raw_png, &o).unwrap();
        let p_out = tmp.join(format!("c65b_l{}.png", (lambda_sq*100000.0) as u32));
        std::fs::write(&p_out, &out)?;
        let cmp = std::process::Command::new(&nupic).args(["compare","-m","ssimulacra2"]).arg(&p).arg(&p_out).output()?;
        let s = String::from_utf8_lossy(&cmp.stdout);
        let ssim: f64 = s.lines().find_map(|l| l.strip_prefix("SSIMULACRA2: ").and_then(|v| v.split_whitespace().next()).and_then(|n| n.parse().ok())).unwrap_or(f64::NAN);
        let dsz = (out.len() as f64 / out_base.len() as f64 - 1.0) * 100.0;
        println!("  λ²={:.5}  {:>5.0}KB / SSIM {:.3}  Δsize={:+.2}% Δssim={:+.3}",
            lambda_sq, out.len() as f64/1024.0, ssim, dsz, ssim - ssim_base);
    }
    Ok(())
}
