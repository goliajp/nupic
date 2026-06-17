//! Cycle 55 — Skip Lloyd refine on stochastic content?
use std::path::PathBuf;
use std::time::Instant;
use image::ImageReader;
use nupic_quantize::{
    train_palette_rgba, refine_palette_kmeans_importance, refine_palette_kmeans_instrumented_strided,
    apply_palette_rgba, encode_indexed_png_with_alpha,
    classify_for_palette_size_with_importance,
};

fn run_with_iters(p: &PathBuf, refine_iters: usize) -> anyhow::Result<(usize, f64, f64)> {
    let img = ImageReader::open(p)?.with_guessed_format()?.decode()?;
    let r = img.to_rgba8();
    let w = r.width(); let h = r.height();
    let raw_rgba = r.into_raw();
    let (n_colors, alpha_imp) = classify_for_palette_size_with_importance(&raw_rgba, w as usize);
    let t0 = Instant::now();
    let (pi, ai) = train_palette_rgba(&raw_rgba, w, h, n_colors)?;
    let (pal, alpha) = if refine_iters == 0 {
        (pi, ai)
    } else if alpha_imp > 0.0 {
        refine_palette_kmeans_importance(&raw_rgba, w, h, &pi, &ai, refine_iters, alpha_imp)
    } else {
        let n_pix = (w as usize) * (h as usize);
        let stride = if n_pix >= 5_000_000 { 16 } else { 8 };
        let (pl, al, _) = refine_palette_kmeans_instrumented_strided(&raw_rgba, w, h, &pi, &ai, refine_iters, 0.0005, stride);
        (pl, al)
    };
    let (indices, ps) = apply_palette_rgba(&raw_rgba, w, h, &pal, &alpha);
    let trns = if alpha.iter().all(|&a| a == 255) { None } else { Some(alpha.as_slice()) };
    let raw_png = encode_indexed_png_with_alpha(w, h, &indices, &ps, trns)?;
    let n_pix = (w as usize) * (h as usize);
    let preset = if n_pix >= 5_000_000 { 1 } else { 5 };
    let mut o = oxipng::Options::from_preset(preset);
    o.strip = oxipng::StripChunks::Safe;
    let out = oxipng::optimize_from_memory(&raw_png, &o).unwrap();
    let dt = t0.elapsed().as_secs_f64() * 1000.0;
    // SSIM via external nupic compare
    let tmp = std::env::temp_dir();
    let outpath = tmp.join(format!("c55_iter{}.png", refine_iters));
    std::fs::write(&outpath, &out)?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();
    let nupic = root.join("target/release/nupic");
    let cmp = std::process::Command::new(&nupic).args(["compare", "-m", "ssimulacra2"]).arg(p).arg(&outpath).output()?;
    let s = String::from_utf8_lossy(&cmp.stdout);
    let ssim: f64 = s.lines().find_map(|l| l.strip_prefix("SSIMULACRA2: ").and_then(|v| v.split_whitespace().next()).and_then(|n| n.parse().ok())).unwrap_or(0.0);
    Ok((out.len(), dt, ssim))
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();
    let fixtures: &[(&str, &str)] = &[
        ("inputs/04-photo-portrait.png", "04 portrait"),
        ("inputs/05-photo-mountain.png", "05 mountain"),
        ("inputs/06-photo-landscape.png", "06 landscape"),
        ("inputs/07-photo-product.png", "07 product"),
        ("inputs-ext-real/17-aurora-5mp.png", "17 aurora"),
        ("inputs-ext-real/25-sofia-cathedral-5mp.png", "25 sofia"),
        ("inputs-ext-real/27-whale-tail-5mp.png", "27 whale"),
    ];
    println!("{:<14} {:>8} {:>10} {:>10} {:>10} {:>10}", "fixture", "iters=0", "iters=20", "iters=50", "iters=100", "Δ(0-100)");
    for &(rel, lbl) in fixtures {
        let p = root.join("assets/png-bench").join(rel);
        let (sz0, _t0, ss0) = run_with_iters(&p, 0)?;
        let (sz20, _t20, ss20) = run_with_iters(&p, 20)?;
        let (sz50, _t50, ss50) = run_with_iters(&p, 50)?;
        let (sz100, _t100, ss100) = run_with_iters(&p, 100)?;
        println!("{:<14}  {:>5.0}/{:.1}  {:>5.0}/{:.1}  {:>5.0}/{:.1}  {:>5.0}/{:.1}   Δssim={:+.2}",
            lbl,
            sz0 as f64/1024.0, ss0,
            sz20 as f64/1024.0, ss20,
            sz50 as f64/1024.0, ss50,
            sz100 as f64/1024.0, ss100,
            ss0 - ss100);
    }
    Ok(())
}
