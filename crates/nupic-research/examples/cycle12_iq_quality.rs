//! Cycle 12 — test if imagequant quality_max=100 / speed=1 gives more
//! SSIM on 05-photo-mountain (palette-saturated, currently 76.82).
//! Goes around the existing `train_palette_rgba` API by reimplementing
//! it locally with tunable IQ params, then runs the full Lloyd's +
//! dither + oxipng pipeline.

use std::path::{Path, PathBuf};
use std::process::Command;
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

fn quantize_with_iq(raw: &[u8], w: u32, h: u32, q_min: u8, q_max: u8, speed: i32)
    -> Result<Vec<RGBA8>>
{
    let pixels: Vec<RGBA8> = raw.chunks_exact(4)
        .map(|c| RGBA8 { r: c[0], g: c[1], b: c[2], a: c[3] })
        .collect();
    let mut attrs = imagequant::new();
    attrs.set_quality(q_min, q_max).map_err(|e| anyhow::anyhow!("set_quality: {e:?}"))?;
    attrs.set_speed(speed).map_err(|e| anyhow::anyhow!("set_speed: {e:?}"))?;
    let mut img = attrs.new_image(pixels.as_slice(), w as usize, h as usize, 0.0)
        .map_err(|e| anyhow::anyhow!("new_image: {e:?}"))?;
    let mut quant = attrs.quantize(&mut img).map_err(|e| anyhow::anyhow!("quantize: {e:?}"))?;
    let _ = quant.set_dithering_level(0.0);
    let (palette, _) = quant.remapped(&mut img).map_err(|e| anyhow::anyhow!("remapped: {e:?}"))?;
    Ok(palette)
}

fn encode_via_palette(raw: &[u8], w: u32, h: u32, palette_rgba: &[RGBA8],
                     strength: f32, tmpdir: &Path, label: &str)
    -> Result<(usize, f64)>
{
    let oklab: Vec<_> = palette_rgba.iter()
        .map(|c| nupic_color::srgb_u8_to_oklab(Rgb { r: c.r, g: c.g, b: c.b }))
        .collect();
    let alpha: Vec<u8> = palette_rgba.iter().map(|c| c.a).collect();
    let mut oklab = oklab;
    let mut alpha = alpha;
    while oklab.len() < 256 {
        oklab.push(oklab[0]);
        alpha.push(alpha[0]);
    }
    let (oklab, alpha) = nupic_quantize::refine_palette_kmeans(
        raw, w, h, &oklab, &alpha, 100,
    );
    let (indices, palette_srgb) = if strength > 0.0 {
        nupic_quantize::apply_palette_rgba_fs_dither(raw, w, h, &oklab, &alpha, strength)
    } else {
        nupic_quantize::apply_palette_rgba(raw, w, h, &oklab, &alpha)
    };
    let (indices, palette_srgb, palette_alpha) = nupic_quantize::compact_palette(
        indices, palette_srgb, alpha,
    );
    let raw_png = nupic_quantize::encode_indexed_png_with_alpha(
        w, h, &indices, &palette_srgb, Some(&palette_alpha),
    ).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    // Optimize via oxipng to match real pipeline
    let oxopts = oxipng::Options::from_preset(5);
    let png_bytes = oxipng::optimize_from_memory(&raw_png, &oxopts)
        .map_err(|e| anyhow::anyhow!("oxipng: {e:?}"))?;
    let out = tmpdir.join(format!("{label}.png"));
    std::fs::write(&out, &png_bytes)?;
    let src = tmpdir.join("source.png");
    // src already written by caller
    let ssim = ssimulacra2(&src, &out);
    Ok((png_bytes.len(), ssim))
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let src = root.join("assets/png-bench/inputs/05-photo-mountain.png");
    let img = ImageReader::open(&src)?.with_guessed_format()?.decode()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let raw = rgba.into_raw();

    let tmpdir = std::env::temp_dir().join("cycle12-iq");
    std::fs::create_dir_all(&tmpdir)?;
    // For SSIM compare, write source as PNG to tmpdir
    std::fs::copy(&src, tmpdir.join("source.png"))?;

    println!("05-photo-mountain.png imagequant params sweep (dither=0.7, Lloyd=100):");
    println!("{:<24} {:>14} {:>10}", "params", "size", "SSIM");

    for (q_min, q_max, speed, label) in &[
        (70u8, 95u8, 4i32, "baseline_q70-95_s4"),
        (70u8, 100u8, 4i32, "qmax100_s4"),
        (70u8, 100u8, 1i32, "qmax100_s1"),
        (70u8, 100u8, 10i32, "qmax100_s10"),
        (0u8, 100u8, 1i32, "q0-100_s1"),
    ] {
        match quantize_with_iq(&raw, w, h, *q_min, *q_max, *speed)
            .and_then(|p| encode_via_palette(&raw, w, h, &p, 0.7, &tmpdir, label))
        {
            Ok((size, ssim)) => println!("{:<24} {:>14} {:>10.3}", label, size, ssim),
            Err(e) => println!("{:<24} ERROR: {}", label, e),
        }
    }
    Ok(())
}
