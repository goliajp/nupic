//! Cycle 57 — Comparison: nupic full pipeline vs imagequant standalone
//! Paper P1 material: Table 1 of GoliaPNG benchmark.

use std::path::PathBuf;
use std::process::Command;
use image::ImageReader;
use rgb::Rgb;

// Path A: full nupic pipeline (current production)
fn nupic_pipeline(in_path: &PathBuf, out_path: &PathBuf) -> anyhow::Result<()> {
    let nupic = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().join("target/release/nupic");
    Command::new(&nupic).args(["compress", "-o"]).arg(out_path).args(["--dither", "auto"]).arg(in_path).status()?;
    Ok(())
}

// Path B: imagequant standalone (Lab L2 + FS dither, default)
fn imagequant_pipeline(in_path: &PathBuf, out_path: &PathBuf) -> anyhow::Result<()> {
    let img = ImageReader::open(in_path)?.with_guessed_format()?.decode()?;
    let r = img.to_rgba8();
    let w = r.width(); let h = r.height();
    let raw = r.into_raw();
    let pixels: &[rgb::RGBA8] = unsafe { std::slice::from_raw_parts(raw.as_ptr() as *const rgb::RGBA8, raw.len() / 4) };

    let mut attrs = imagequant::new();
    attrs.set_quality(0, 95)?;
    attrs.set_speed(4)?;
    attrs.set_max_colors(256)?;
    let mut img2 = attrs.new_image(pixels, w as usize, h as usize, 0.0)?;
    let mut quant = attrs.quantize(&mut img2)?;
    let _ = quant.set_dithering_level(1.0); // default Floyd-Steinberg
    let (palette, indices) = quant.remapped(&mut img2)?;

    // Build raw PNG via png crate
    let mut rgb_palette: Vec<u8> = Vec::with_capacity(palette.len() * 3);
    let mut alpha_vec: Vec<u8> = Vec::with_capacity(palette.len());
    for c in &palette { rgb_palette.push(c.r); rgb_palette.push(c.g); rgb_palette.push(c.b); alpha_vec.push(c.a); }
    let last_nonopaque = alpha_vec.iter().rposition(|&v| v != 255);
    let trns = last_nonopaque.map(|i| alpha_vec[..=i].to_vec());

    let mut raw_png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut raw_png, w, h);
        enc.set_color(png::ColorType::Indexed);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_palette(rgb_palette);
        enc.set_compression(png::Compression::Fast);
        if let Some(t) = trns.as_ref() { if !t.is_empty() { enc.set_trns(t.clone()); } }
        let mut writer = enc.write_header()?;
        writer.write_image_data(&indices)?;
    }
    // Same oxipng as nupic pipeline (preset=1 if 5MP, else 5)
    let n_pix = (w as usize) * (h as usize);
    let preset = if n_pix >= 5_000_000 { 1 } else { 5 };
    let mut o = oxipng::Options::from_preset(preset);
    o.strip = oxipng::StripChunks::Safe;
    let out = oxipng::optimize_from_memory(&raw_png, &o).unwrap();
    std::fs::write(out_path, out)?;
    Ok(())
}

fn ssim(input: &PathBuf, encoded: &PathBuf) -> f64 {
    let nupic = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().join("target/release/nupic");
    let cmd = Command::new(&nupic).args(["compare", "-m", "ssimulacra2"]).arg(input).arg(encoded).output();
    if let Ok(o) = cmd {
        let s = String::from_utf8_lossy(&o.stdout);
        s.lines().find_map(|l| l.strip_prefix("SSIMULACRA2: ").and_then(|v| v.split_whitespace().next()).and_then(|n| n.parse().ok())).unwrap_or(f64::NAN)
    } else { f64::NAN }
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();
    let tmp = std::env::temp_dir();
    let fixtures: &[(&str, &str)] = &[
        ("inputs/01-png-transparency-demo.png", "01 trans"),
        ("inputs/02-pluto-transparent.png", "02 pluto"),
        ("inputs/03-wikipedia-logo.png", "03 wiki"),
        ("inputs/04-photo-portrait.png", "04 portrait"),
        ("inputs/05-photo-mountain.png", "05 mountain"),
        ("inputs/06-photo-landscape.png", "06 landscape"),
        ("inputs/07-photo-product.png", "07 product"),
        ("inputs-ext-real/17-aurora-5mp.png", "17 aurora"),
        ("inputs-ext-real/25-sofia-cathedral-5mp.png", "25 sofia"),
        ("inputs-ext-real/27-whale-tail-5mp.png", "27 whale"),
    ];
    println!("=== Cycle 57: nupic full pipeline vs imagequant standalone ===");
    println!("{:<15} {:>13} {:>13} {:>10}", "fixture", "nupic KB/SSIM", "iq KB/SSIM", "n_ratio");
    let mut sum_nup = 0u64; let mut sum_iq = 0u64;
    for &(rel, lbl) in fixtures {
        let p = root.join("assets/png-bench").join(rel);
        let np = tmp.join(format!("c57_n_{}.png", lbl.replace(' ', "_")));
        let ip = tmp.join(format!("c57_i_{}.png", lbl.replace(' ', "_")));
        nupic_pipeline(&p, &np)?;
        imagequant_pipeline(&p, &ip)?;
        let nss = ssim(&p, &np);
        let iss = ssim(&p, &ip);
        let ns = std::fs::metadata(&np)?.len();
        let is_ = std::fs::metadata(&ip)?.len();
        sum_nup += ns; sum_iq += is_;
        println!("{:<15} {:>5.0}/{:.1}   {:>5.0}/{:.1}   {:.3}",
            lbl, ns as f64/1024.0, nss, is_ as f64/1024.0, iss, ns as f64 / is_ as f64);
    }
    println!();
    println!("TOTAL: nupic={:.0}KB, iq={:.0}KB, nupic/iq = {:.3} ({:+.2}%)",
        sum_nup as f64/1024.0, sum_iq as f64/1024.0, sum_nup as f64 / sum_iq as f64, (sum_nup as f64 / sum_iq as f64 - 1.0) * 100.0);
    Ok(())
}
