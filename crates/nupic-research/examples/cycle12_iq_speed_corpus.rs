//! Cycle 12 — 7-fixture corpus sweep: imagequant speed=1 vs default speed=4.
//! On 05 alone, speed=1 gave +0.01 SSIM. Is it worth wiring effort=10 → s=1?

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use anyhow::Result;
use image::ImageReader;
use rgb::{Rgb, RGBA8};

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(PathBuf::from(m).ancestors().nth(2).unwrap().to_path_buf())
}

fn ssimulacra2(orig: &Path, cmp: &Path) -> f64 {
    let out = Command::new("nupic")
        .args(["compare", "-m", "ssimulacra2"])
        .arg(orig).arg(cmp).output().expect("nupic compare");
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines().find_map(|l| {
        l.strip_prefix("SSIMULACRA2: ")
            .and_then(|v| v.split_whitespace().next())
            .and_then(|n| n.parse::<f64>().ok())
    }).unwrap_or(0.0)
}

fn iq(raw: &[u8], w: u32, h: u32, speed: i32) -> Result<Vec<RGBA8>> {
    fn run(raw: &[u8], w: u32, h: u32, q_min: u8, speed: i32) -> Result<Vec<RGBA8>, String> {
        let pixels: Vec<RGBA8> = raw.chunks_exact(4)
            .map(|c| RGBA8 { r: c[0], g: c[1], b: c[2], a: c[3] }).collect();
        let mut attrs = imagequant::new();
        attrs.set_quality(q_min, 100).map_err(|e| format!("{e:?}"))?;
        attrs.set_speed(speed).map_err(|e| format!("{e:?}"))?;
        let mut img = attrs.new_image(pixels.as_slice(), w as usize, h as usize, 0.0)
            .map_err(|e| format!("{e:?}"))?;
        let mut q = attrs.quantize(&mut img).map_err(|e| format!("{e:?}"))?;
        let _ = q.set_dithering_level(0.0);
        let (p, _) = q.remapped(&mut img).map_err(|e| format!("{e:?}"))?;
        Ok(p)
    }
    run(raw, w, h, 70, speed)
        .or_else(|_| run(raw, w, h, 0, speed))
        .map_err(|e| anyhow::anyhow!("{e}"))
}

fn encode(raw: &[u8], w: u32, h: u32, palette: &[RGBA8], strength: f32,
          tmpdir: &Path, src: &Path, label: &str) -> Result<(usize, f64)> {
    let mut oklab: Vec<_> = palette.iter()
        .map(|c| nupic_color::srgb_u8_to_oklab(Rgb { r: c.r, g: c.g, b: c.b })).collect();
    let mut alpha: Vec<u8> = palette.iter().map(|c| c.a).collect();
    while oklab.len() < 256 { oklab.push(oklab[0]); alpha.push(alpha[0]); }
    let (oklab, alpha) = nupic_quantize::refine_palette_kmeans(raw, w, h, &oklab, &alpha, 100);
    let (indices, palette_srgb) = if strength > 0.0 {
        nupic_quantize::apply_palette_rgba_fs_dither(raw, w, h, &oklab, &alpha, strength)
    } else {
        nupic_quantize::apply_palette_rgba(raw, w, h, &oklab, &alpha)
    };
    let (indices, palette_srgb, palette_alpha) = nupic_quantize::compact_palette(indices, palette_srgb, alpha);
    let raw_png = nupic_quantize::encode_indexed_png_with_alpha(
        w, h, &indices, &palette_srgb, Some(&palette_alpha)
    ).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let oxopts = oxipng::Options::from_preset(5);
    let png = oxipng::optimize_from_memory(&raw_png, &oxopts)
        .map_err(|e| anyhow::anyhow!("oxipng: {e:?}"))?;
    let out = tmpdir.join(format!("{label}.png"));
    std::fs::write(&out, &png)?;
    Ok((png.len(), ssimulacra2(src, &out)))
}

fn auto_d(raw: &[u8], w: u32) -> f32 {
    nupic_quantize::classify_for_auto_dither(raw, w)
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let fixtures = [
        "01-png-transparency-demo.png",
        "02-pluto-transparent.png",
        "03-wikipedia-logo.png",
        "04-photo-portrait.png",
        "05-photo-mountain.png",
        "06-photo-landscape.png",
        "07-photo-product.png",
    ];
    let tmpdir = std::env::temp_dir().join("cycle12-corpus");
    std::fs::create_dir_all(&tmpdir)?;

    println!("Corpus sweep: speed=4 (baseline) vs speed=1 (slow); --dither auto");
    println!("{:<32} {:>10} {:>10} {:>8}    {:>10} {:>10} {:>8}",
        "fixture", "s4_size", "s4_SSIM", "s4_ms", "s1_size", "s1_SSIM", "s1_ms");
    let mut sum_s4 = 0.0f64; let mut sum_s1 = 0.0f64;
    for fname in &fixtures {
        let path = root.join("assets/png-bench/inputs").join(fname);
        let img = ImageReader::open(&path)?.with_guessed_format()?.decode()?;
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        let raw = rgba.into_raw();
        let d = auto_d(&raw, w);
        let t = Instant::now();
        let p4 = iq(&raw, w, h, 4)?; let (s4, q4) = encode(&raw, w, h, &p4, d, &tmpdir, &path, &format!("{fname}-s4"))?;
        let ms4 = t.elapsed().as_millis();
        let t = Instant::now();
        let p1 = iq(&raw, w, h, 1)?; let (s1, q1) = encode(&raw, w, h, &p1, d, &tmpdir, &path, &format!("{fname}-s1"))?;
        let ms1 = t.elapsed().as_millis();
        sum_s4 += q4; sum_s1 += q1;
        println!("{:<32} {:>10} {:>10.3} {:>8}    {:>10} {:>10.3} {:>8}",
            fname, s4, q4, ms4, s1, q1, ms1);
    }
    println!();
    println!("Mean SSIM: s4={:.3}, s1={:.3}, delta={:+.3}",
        sum_s4/7.0, sum_s1/7.0, (sum_s1-sum_s4)/7.0);
    Ok(())
}
