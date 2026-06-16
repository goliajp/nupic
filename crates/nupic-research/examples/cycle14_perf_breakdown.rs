//! Cycle 14 — perf breakdown on 05-photo-mountain to identify bottleneck.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use image::ImageReader;

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(PathBuf::from(m).ancestors().nth(2).unwrap().to_path_buf())
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let src = root.join("assets/png-bench/inputs/05-photo-mountain.png");

    // Stage 1: image decode
    let t = Instant::now();
    let img = ImageReader::open(&src)?.with_guessed_format()?.decode()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let raw = rgba.into_raw();
    let decode_ms = t.elapsed().as_millis();

    // Stage 2: train palette (imagequant)
    let t = Instant::now();
    let (oklab, alpha) = nupic_quantize::train_palette_rgba(&raw, w, h, 256)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let train_ms = t.elapsed().as_millis();

    // Stage 3: refine via Lloyd's (iter=100)
    let t = Instant::now();
    let (oklab, alpha) = nupic_quantize::refine_palette_kmeans(&raw, w, h, &oklab, &alpha, 100);
    let refine_ms = t.elapsed().as_millis();

    // Stage 4a: apply palette no dither
    let t = Instant::now();
    let (indices, palette_srgb) = nupic_quantize::apply_palette_rgba(&raw, w, h, &oklab, &alpha);
    let apply_ms = t.elapsed().as_millis();

    // Stage 4b: apply palette WITH FS dither (0.7)
    let t = Instant::now();
    let (_indices_d, _palette_d) = nupic_quantize::apply_palette_rgba_fs_dither(&raw, w, h, &oklab, &alpha, 0.7);
    let dither_ms = t.elapsed().as_millis();

    // Stage 5: encode indexed PNG (png crate, no oxipng polish)
    let t = Instant::now();
    let (indices, palette_srgb, palette_alpha) = nupic_quantize::compact_palette(indices, palette_srgb, alpha.clone());
    let raw_png = nupic_quantize::encode_indexed_png_with_alpha(
        w, h, &indices, &palette_srgb, Some(&palette_alpha)
    ).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let png_encode_ms = t.elapsed().as_millis();
    let raw_png_size = raw_png.len();

    // Stage 6: oxipng polish
    let t = Instant::now();
    let oxopts = oxipng::Options::from_preset(5);
    let polished = oxipng::optimize_from_memory(&raw_png, &oxopts)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let oxipng_ms = t.elapsed().as_millis();

    println!("Perf breakdown for 05-photo-mountain (1200x800):");
    println!("{:<24} {:>8}", "stage", "ms");
    println!("{:<24} {:>8}", "image decode", decode_ms);
    println!("{:<24} {:>8}", "train_palette (IQ)", train_ms);
    println!("{:<24} {:>8}", "refine_kmeans (100 iter)", refine_ms);
    println!("{:<24} {:>8}", "apply_palette (no dith)", apply_ms);
    println!("{:<24} {:>8}", "apply_palette_fs_dith", dither_ms);
    println!("{:<24} {:>8}", "encode_indexed_png", png_encode_ms);
    println!("{:<24} {:>8}", "oxipng preset=5", oxipng_ms);
    println!();
    println!("Raw indexed PNG size: {} bytes", raw_png_size);
    println!("Polished PNG size:    {} bytes", polished.len());
    println!("Total (decode + IQ + Lloyd + apply + png + oxipng): {} ms",
        decode_ms + train_ms + refine_ms + apply_ms + png_encode_ms + oxipng_ms);
    Ok(())
}
