//! Cycle 59 — Luma-sorted palette: maybe LZ77 locality wins
use std::path::PathBuf;
use image::ImageReader;
use rgb::Rgb;
use nupic_quantize::{
    train_palette_rgba, refine_palette_kmeans_importance, refine_palette_kmeans,
    apply_palette_rgba, encode_indexed_png_with_alpha,
    classify_for_palette_size_with_importance,
};

fn reorder_by_luma(indices: &[u8], palette: &[Rgb<u8>], alpha: &[u8]) -> (Vec<u8>, Vec<Rgb<u8>>, Vec<u8>) {
    let n = palette.len();
    // Compute luma for each palette entry
    let lumas: Vec<i32> = palette.iter().map(|c| (c.r as i32 + c.g as i32 + c.b as i32) / 3).collect();
    // Permutation: order by luma ascending
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| lumas[i]);
    let mut inv_map = vec![0u8; n];
    for (new_i, &old_i) in order.iter().enumerate() { inv_map[old_i] = new_i as u8; }
    let new_indices: Vec<u8> = indices.iter().map(|&i| inv_map[i as usize]).collect();
    let new_palette: Vec<Rgb<u8>> = order.iter().map(|&old_i| palette[old_i]).collect();
    let new_alpha: Vec<u8> = order.iter().map(|&old_i| alpha[old_i]).collect();
    (new_indices, new_palette, new_alpha)
}

fn pipeline(p: &PathBuf, reorder: bool) -> anyhow::Result<usize> {
    let img = ImageReader::open(p)?.with_guessed_format()?.decode()?;
    let r = img.to_rgba8();
    let w = r.width(); let h = r.height();
    let raw_rgba = r.into_raw();
    let (n_colors, alpha_imp) = classify_for_palette_size_with_importance(&raw_rgba, w as usize);
    let (pi, ai) = train_palette_rgba(&raw_rgba, w, h, n_colors)?;
    let n_pix = (w as usize) * (h as usize);
    let refine_cap = if n_pix >= 5_000_000 { 20 } else { 100 };
    let (pal, alpha) = if alpha_imp > 0.0 {
        refine_palette_kmeans_importance(&raw_rgba, w, h, &pi, &ai, refine_cap, alpha_imp)
    } else {
        refine_palette_kmeans(&raw_rgba, w, h, &pi, &ai, refine_cap)
    };
    let (mut indices, mut palette_srgb) = apply_palette_rgba(&raw_rgba, w, h, &pal, &alpha);
    let mut alpha_vec = alpha.clone();
    if reorder {
        let (i, p, a) = reorder_by_luma(&indices, &palette_srgb, &alpha_vec);
        indices = i; palette_srgb = p; alpha_vec = a;
    }
    let trns = if alpha_vec.iter().all(|&a| a == 255) { None } else { Some(alpha_vec.as_slice()) };
    let raw_png = encode_indexed_png_with_alpha(w, h, &indices, &palette_srgb, trns)?;
    let preset = if n_pix >= 5_000_000 { 1 } else { 5 };
    let mut o = oxipng::Options::from_preset(preset);
    o.strip = oxipng::StripChunks::Safe;
    let out = oxipng::optimize_from_memory(&raw_png, &o).unwrap();
    Ok(out.len())
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();
    let fixtures: &[(&str, &str)] = &[
        ("inputs/04-photo-portrait.png", "04 portrait"),
        ("inputs/05-photo-mountain.png", "05 mountain"),
        ("inputs/06-photo-landscape.png", "06 landscape"),
        ("inputs/07-photo-product.png", "07 product"),
        ("inputs-ext-real/17-aurora-5mp.png", "17 aurora 5MP"),
        ("inputs-ext-real/25-sofia-cathedral-5mp.png", "25 sofia 5MP"),
        ("inputs-ext-real/27-whale-tail-5mp.png", "27 whale 5MP"),
    ];
    println!("=== Cycle 59 — luma-sorted palette ===");
    println!("{:<18} {:>12} {:>14} {:>10}", "fixture", "baseline_KB", "luma_sort_KB", "Δ%");
    let mut sum_b = 0; let mut sum_r = 0;
    for &(rel, lbl) in fixtures {
        let p = root.join("assets/png-bench").join(rel);
        let sz_base = pipeline(&p, false)?;
        let sz_re = pipeline(&p, true)?;
        sum_b += sz_base; sum_r += sz_re;
        let pct = (sz_re as f64 / sz_base as f64 - 1.0) * 100.0;
        println!("{:<18} {:>12.0} {:>14.0} {:>+9.2}%",
            lbl, sz_base as f64/1024.0, sz_re as f64/1024.0, pct);
    }
    println!();
    println!("TOTAL: {} KB → {} KB  ({:+.2}%)",
        sum_b/1024, sum_r/1024, (sum_r as f64 / sum_b as f64 - 1.0) * 100.0);
    Ok(())
}
