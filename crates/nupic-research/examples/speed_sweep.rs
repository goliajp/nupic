//! Cycle 53 stage 2 — idat_recoding=false on baseline-7 + 5MP
use std::path::PathBuf;
use std::time::Instant;
use image::ImageReader;
use nupic_quantize::{
    train_palette_rgba, refine_palette_kmeans_importance, refine_palette_kmeans,
    apply_palette_rgba, encode_indexed_png_with_alpha,
    classify_for_palette_size_with_importance,
};

fn run_pipeline(p: &PathBuf, idat_recoding: bool, preset: u8) -> anyhow::Result<(usize, f64)> {
    let img = ImageReader::open(p)?.with_guessed_format()?.decode()?;
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
    let (indices, ps) = apply_palette_rgba(&raw_rgba, w, h, &pal, &alpha);
    let trns = if alpha.iter().all(|&a| a == 255) { None } else { Some(alpha.as_slice()) };
    let raw_png = encode_indexed_png_with_alpha(w, h, &indices, &ps, trns)?;
    let t0 = Instant::now();
    let mut o = oxipng::Options::from_preset(preset);
    o.idat_recoding = idat_recoding;
    o.strip = oxipng::StripChunks::Safe;
    let out = oxipng::optimize_from_memory(&raw_png, &o).unwrap();
    let dt = t0.elapsed().as_secs_f64() * 1000.0;
    Ok((out.len(), dt))
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();
    let fixtures: &[(&str, u8)] = &[
        // (path, preset_used_by_current_pipeline)
        ("inputs/01-png-transparency-demo.png", 5),
        ("inputs/02-pluto-transparent.png", 5),
        ("inputs/03-wikipedia-logo.png", 5),
        ("inputs/04-photo-portrait.png", 5),
        ("inputs/05-photo-mountain.png", 5),
        ("inputs/06-photo-landscape.png", 5),
        ("inputs/07-photo-product.png", 5),
        ("inputs-ext-real/25-sofia-cathedral-5mp.png", 1),
        ("inputs-ext-real/27-whale-tail-5mp.png", 1),
    ];
    println!("{:<32} {:>10} {:>10} {:>10} {:>8}", "fixture", "default_KB", "noidat_KB", "Δ%", "Δtime");
    for &(rel, preset) in fixtures {
        let p = root.join("assets/png-bench").join(rel);
        let (sz_def, dt_def) = run_pipeline(&p, true, preset)?;
        let (sz_no, dt_no) = run_pipeline(&p, false, preset)?;
        let pct = (sz_no as f64 / sz_def as f64 - 1.0) * 100.0;
        let dt_pct = (dt_no - dt_def) / dt_def * 100.0;
        let name = rel.split('/').last().unwrap();
        println!("{:<32} {:>10.0} {:>10.0} {:>+9.2}% {:>+7.0}%",
            name, sz_def as f64 / 1024.0, sz_no as f64 / 1024.0, pct, dt_pct);
    }
    Ok(())
}
