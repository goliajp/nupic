//! Cycle 48 stage 2 — smarter raw encoder
use std::path::PathBuf;
use std::time::Instant;
use image::ImageReader;
use nupic_quantize::{train_palette_rgba, refine_palette_kmeans, apply_palette_rgba};
use rgb::Rgb;

fn encode_smart(w: u32, h: u32, indices: &[u8], pal_srgb: &[Rgb<u8>], pal_alpha: Option<&[u8]>, filter: png::Filter, compression: png::Compression) -> Vec<u8> {
    let mut rgb_palette: Vec<u8> = Vec::with_capacity(pal_srgb.len() * 3);
    for c in pal_srgb { rgb_palette.push(c.r); rgb_palette.push(c.g); rgb_palette.push(c.b); }
    let mut raw = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut raw, w, h);
        enc.set_color(png::ColorType::Indexed);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_palette(rgb_palette);
        enc.set_filter(filter);
        enc.set_compression(compression);
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
    let fixtures = ["inputs-ext-real/25-sofia-cathedral-5mp.png", "inputs-ext-real/17-aurora-5mp.png"];
    let configs: &[(&str, png::Filter, png::Compression)] = &[
        ("None+Bal(curr)", png::Filter::NoFilter, png::Compression::Balanced),
        ("None+High", png::Filter::NoFilter, png::Compression::High),
        ("Paeth+Bal", png::Filter::Paeth, png::Compression::Balanced),
        ("Paeth+High", png::Filter::Paeth, png::Compression::High),
        ("Sub+Bal", png::Filter::Sub, png::Compression::Balanced),
        ("Sub+High", png::Filter::Sub, png::Compression::High),
        ("Up+Bal", png::Filter::Up, png::Compression::Balanced),
        ("Avg+Bal", png::Filter::Avg, png::Compression::Balanced),
    ];
    for rel in fixtures {
        let p = root.join("assets/png-bench").join(rel);
        let img = ImageReader::open(&p)?.with_guessed_format()?.decode()?;
        let r = img.to_rgba8();
        let w = r.width(); let h = r.height();
        let raw_rgba = r.into_raw();
        let (pi, ai) = train_palette_rgba(&raw_rgba, w, h, 208)?;
        let (pal, alpha) = refine_palette_kmeans(&raw_rgba, w, h, &pi, &ai, 100);
        let (indices, ps) = apply_palette_rgba(&raw_rgba, w, h, &pal, &alpha);
        let trns = if alpha.iter().all(|&a| a == 255) { None } else { Some(alpha.as_slice()) };
        println!("\n=== {} ===", rel);
        let raw_default = encode_smart(w, h, &indices, &ps, trns, png::Filter::NoFilter, png::Compression::Balanced);
        let t0 = Instant::now();
        let oxi = oxipng::optimize_from_memory(&raw_default, &oxipng::Options::from_preset(1)).unwrap();
        let dt_oxi = t0.elapsed().as_secs_f64() * 1000.0;
        println!("REFERENCE oxipng-p1:               {:>5.0} KB  ({:.0}ms)", oxi.len() as f64/1024.0, dt_oxi);
        for &(lbl, f, c) in configs {
            let t0 = Instant::now();
            let p = encode_smart(w, h, &indices, &ps, trns, f, c);
            let dt = t0.elapsed().as_secs_f64() * 1000.0;
            println!("  {:<18} {:>5.0} KB  ({:.0}ms)  vs_oxi={:.3}x", lbl, p.len() as f64/1024.0, dt, p.len() as f64 / oxi.len() as f64);
        }
    }
    Ok(())
}

// Note: png crate default Filter is Adaptive (picks best per row)
