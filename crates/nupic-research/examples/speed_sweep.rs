//! Cycle 56 — Cross-format experiment: same palette → PNG vs GIF
//! Paper P4 material: demonstrates SSIMULACRA2-aware quantization
//! is format-independent.

use std::path::PathBuf;
use std::time::Instant;
use image::ImageReader;
use rgb::Rgb;
use nupic_quantize::{
    train_palette_rgba, refine_palette_kmeans_importance, refine_palette_kmeans,
    apply_palette_rgba, encode_indexed_png_with_alpha,
    classify_for_palette_size_with_importance,
};

fn encode_gif(w: u32, h: u32, indices: &[u8], pal_srgb: &[Rgb<u8>], pal_alpha: Option<&[u8]>) -> Vec<u8> {
    // Pad palette to 256 for GIF (must be power of 2: 2, 4, 8, ..., 256)
    let n = pal_srgb.len();
    let pad_size: usize = match n {
        n if n <= 2 => 2,
        n if n <= 4 => 4,
        n if n <= 8 => 8,
        n if n <= 16 => 16,
        n if n <= 32 => 32,
        n if n <= 64 => 64,
        n if n <= 128 => 128,
        _ => 256,
    };
    let mut palette_bytes: Vec<u8> = Vec::with_capacity(pad_size * 3);
    for c in pal_srgb { palette_bytes.push(c.r); palette_bytes.push(c.g); palette_bytes.push(c.b); }
    while palette_bytes.len() < pad_size * 3 { palette_bytes.push(0); }

    let mut out: Vec<u8> = Vec::new();
    {
        let mut enc = gif::Encoder::new(&mut out, w as u16, h as u16, &palette_bytes).unwrap();
        // Optional: tRNS via transparent index (use first non-opaque entry or 0 fallback)
        let trans_idx: Option<u8> = pal_alpha.and_then(|a| {
            a.iter().position(|&v| v == 0).map(|i| i as u8)
        });
        let mut frame = gif::Frame::default();
        frame.width = w as u16;
        frame.height = h as u16;
        frame.buffer = std::borrow::Cow::Borrowed(indices);
        if let Some(ti) = trans_idx { frame.transparent = Some(ti); }
        enc.write_frame(&frame).unwrap();
    }
    out
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
        ("inputs-ext-real/25-sofia-cathedral-5mp.png", "25 sofia"),
    ];
    println!("=== Cross-format Cycle 56: same palette, PNG vs GIF ===");
    println!("{:<14} {:>12} {:>12} {:>10} {:>10}", "fixture", "png_KB/SSIM", "gif_KB/SSIM", "gif/png", "Δ_SSIM");
    for &(rel, lbl) in fixtures {
        let p = root.join("assets/png-bench").join(rel);
        let img = ImageReader::open(&p)?.with_guessed_format()?.decode()?;
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
        let (indices, ps) = apply_palette_rgba(&raw_rgba, w, h, &pal, &alpha);
        let trns = if alpha.iter().all(|&a| a == 255) { None } else { Some(alpha.as_slice()) };

        // PNG output (full pipeline)
        let raw_png = encode_indexed_png_with_alpha(w, h, &indices, &ps, trns)?;
        let preset = if n_pix >= 5_000_000 { 1 } else { 5 };
        let mut o = oxipng::Options::from_preset(preset);
        o.strip = oxipng::StripChunks::Safe;
        let png_out = oxipng::optimize_from_memory(&raw_png, &o).unwrap();
        let png_path = tmp.join(format!("c56_{}.png", lbl.replace(' ', "_")));
        std::fs::write(&png_path, &png_out)?;

        // GIF output (same indices + palette)
        let t0 = Instant::now();
        let gif_out = encode_gif(w, h, &indices, &ps, trns);
        let _dt_gif = t0.elapsed().as_secs_f64() * 1000.0;
        let gif_path = tmp.join(format!("c56_{}.gif", lbl.replace(' ', "_")));
        std::fs::write(&gif_path, &gif_out)?;

        // SSIM via nupic compare (works on both png and gif)
        let png_ssim_cmd = std::process::Command::new(&nupic).args(["compare", "-m", "ssimulacra2"]).arg(&p).arg(&png_path).output()?;
        let gif_ssim_cmd = std::process::Command::new(&nupic).args(["compare", "-m", "ssimulacra2"]).arg(&p).arg(&gif_path).output()?;
        let s1 = String::from_utf8_lossy(&png_ssim_cmd.stdout);
        let s2 = String::from_utf8_lossy(&gif_ssim_cmd.stdout);
        let parse = |s: &str| s.lines().find_map(|l| l.strip_prefix("SSIMULACRA2: ").and_then(|v| v.split_whitespace().next()).and_then(|n| n.parse::<f64>().ok())).unwrap_or(f64::NAN);
        let png_ssim = parse(&s1);
        let gif_ssim = parse(&s2);
        println!("{:<14} {:>5.0}/{:.1}  {:>5.0}/{:.1}   {:.2}x   {:+.2}",
            lbl,
            png_out.len() as f64/1024.0, png_ssim,
            gif_out.len() as f64/1024.0, gif_ssim,
            gif_out.len() as f64 / png_out.len() as f64,
            gif_ssim - png_ssim);
    }
    Ok(())
}
