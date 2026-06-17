//! Cycle 54 — png crate Compression level on raw encode
use std::path::PathBuf;
use std::time::Instant;
use image::ImageReader;
use rgb::Rgb;
use nupic_quantize::{
    train_palette_rgba, refine_palette_kmeans_importance, refine_palette_kmeans,
    apply_palette_rgba, classify_for_palette_size_with_importance,
};

fn encode_with_comp(w: u32, h: u32, indices: &[u8], pal_srgb: &[Rgb<u8>], pal_alpha: Option<&[u8]>, comp: png::Compression) -> Vec<u8> {
    let mut rgb_palette: Vec<u8> = Vec::with_capacity(pal_srgb.len() * 3);
    for c in pal_srgb { rgb_palette.push(c.r); rgb_palette.push(c.g); rgb_palette.push(c.b); }
    let mut raw = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut raw, w, h);
        enc.set_color(png::ColorType::Indexed);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_palette(rgb_palette);
        enc.set_compression(comp);
        if let Some(a) = pal_alpha {
            let last = a.iter().rposition(|&v| v != 255);
            if let Some(i) = last { enc.set_trns(a[..=i].to_vec()); }
        }
        let mut writer = enc.write_header().unwrap();
        writer.write_image_data(indices).unwrap();
    }
    raw
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();
    let fixtures = ["inputs-ext-real/25-sofia-cathedral-5mp.png", "inputs/04-photo-portrait.png"];
    for rel in fixtures {
        let p = root.join("assets/png-bench").join(rel);
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
        let (indices, ps) = apply_palette_rgba(&raw_rgba, w, h, &pal, &alpha);
        let trns = if alpha.iter().all(|&a| a == 255) { None } else { Some(alpha.as_slice()) };

        let n_pix = (w as usize) * (h as usize);
        let preset = if n_pix >= 5_000_000 { 1 } else { 5 };
        let mut oxi_opts = oxipng::Options::from_preset(preset);
        oxi_opts.strip = oxipng::StripChunks::Safe;

        println!("\n=== {} ({}MP, preset={}) ===", rel, n_pix / 1_000_000, preset);
        for &(label, comp) in &[
            ("Fast", png::Compression::Fast),
            ("Balanced(curr)", png::Compression::Balanced),
            ("High", png::Compression::High),
        ] {
            let t_enc = Instant::now();
            let raw_png = encode_with_comp(w, h, &indices, &ps, trns, comp);
            let dt_enc = t_enc.elapsed().as_secs_f64() * 1000.0;
            let t_oxi = Instant::now();
            let out = oxipng::optimize_from_memory(&raw_png, &oxi_opts).unwrap();
            let dt_oxi = t_oxi.elapsed().as_secs_f64() * 1000.0;
            println!("  {:<14}  raw={:>5.0}KB({:>3.0}ms) → oxi={:>5.0}KB({:>4.0}ms)  total={:>4.0}ms",
                label, raw_png.len() as f64/1024.0, dt_enc, out.len() as f64/1024.0, dt_oxi, dt_enc + dt_oxi);
        }
    }
    Ok(())
}
