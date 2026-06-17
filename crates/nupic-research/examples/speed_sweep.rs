//! Cycle 50 — Per-stage RSS profile on 5MP.
use std::path::PathBuf;
use std::mem::MaybeUninit;
use image::ImageReader;
use nupic_quantize::{
    train_palette_rgba, refine_palette_kmeans_importance,
    refine_palette_kmeans, apply_palette_rgba,
    encode_indexed_png_with_alpha, classify_for_palette_size_with_importance,
};

fn rss_mb() -> u64 {
    unsafe {
        let mut ru: MaybeUninit<libc::rusage> = MaybeUninit::uninit();
        libc::getrusage(libc::RUSAGE_SELF, ru.as_mut_ptr());
        ru.assume_init().ru_maxrss as u64 / 1024 / 1024
    }
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();
    let fixtures = ["inputs-ext-real/25-sofia-cathedral-5mp.png", "inputs-ext-real/27-whale-tail-5mp.png"];
    for rel in fixtures {
        let p = root.join("assets/png-bench").join(rel);
        println!("\n=== {} ===", rel);
        println!("RSS start: {} MB", rss_mb());
        let img = ImageReader::open(&p)?.with_guessed_format()?.decode()?;
        let r = img.to_rgba8();
        let w = r.width(); let h = r.height();
        let raw = r.into_raw();
        println!("RSS after decode (raw={}MB): {} MB", raw.len() / 1024 / 1024, rss_mb());

        let (n_colors, alpha) = classify_for_palette_size_with_importance(&raw, w as usize);
        let (pi, ai) = train_palette_rgba(&raw, w, h, n_colors)?;
        println!("RSS after train_palette: {} MB (n={})", rss_mb(), n_colors);

        let (pal, alph) = if alpha > 0.0 {
            refine_palette_kmeans_importance(&raw, w, h, &pi, &ai, 100, alpha)
        } else {
            refine_palette_kmeans(&raw, w, h, &pi, &ai, 100)
        };
        println!("RSS after refine (α={}): {} MB", alpha, rss_mb());

        let (indices, ps) = apply_palette_rgba(&raw, w, h, &pal, &alph);
        println!("RSS after apply: {} MB", rss_mb());

        let trns = if alph.iter().all(|&a| a == 255) { None } else { Some(alph.as_slice()) };
        let raw_png = encode_indexed_png_with_alpha(w, h, &indices, &ps, trns)?;
        println!("RSS after encode_png (raw_png={}MB): {} MB", raw_png.len() / 1024 / 1024, rss_mb());

        let preset = if (w as usize) * (h as usize) >= 5_000_000 { 1 } else { 5 };
        let oxi = oxipng::optimize_from_memory(&raw_png, &oxipng::Options::from_preset(preset)).unwrap();
        println!("RSS after oxipng-p{} (final={}KB): {} MB", preset, oxi.len() / 1024, rss_mb());

        drop(img); drop(raw); drop(pal); drop(alph); drop(indices); drop(ps); drop(raw_png); drop(oxi);
        println!("RSS after explicit drops: {} MB", rss_mb());
    }
    Ok(())
}
