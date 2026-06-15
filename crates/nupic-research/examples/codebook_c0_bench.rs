//! Stone C — C0 baseline bench.
//!
//! Trains a palette via `codebook_c0::train_palette_c0` on each lead
//! fixture (02-pluto / 04-portrait / 06-landscape), applies it via
//! hard argmin, encodes as indexed PNG (palette + tRNS via `png` crate
//! + oxipng pass), and measures:
//!   - training wall-clock (s)
//!   - inference wall-clock (ms)
//!   - output PNG bytes
//!   - SSIMULACRA2 score (decoded distorted vs original) via Stone B
//!
//! Compares against the imagequant baseline already exercised by
//! `pluto_sweep`. The cement (imagequant + oxipng @ q=95) numbers from
//! 02 essay's metric_sweep are the reference.
//!
//! Run:
//!   cargo run --release -p nupic-research --example codebook_c0_bench
//!
//! Output:
//!   target/research-out/03c-bis-codebook-c0-bench.csv
//!   target/research-out/03c-bis-codebook-c0-bench.md

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use nupic_research::codebook_c0::{Palette, TrainConfig, apply_palette, train_palette_c0};

const INPUTS: &str = "assets/png-bench/inputs";
const OUT_DIR: &str = "target/research-out";

#[derive(Debug)]
struct Row {
    image: String,
    n_pixels: usize,
    train_ms: f64,
    infer_ms: f64,
    bytes: usize,
    ssim_c0: f64,
    ssim_imagequant_baseline: f64,
    ssim_delta_vs_imagequant: f64,
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let inputs_dir = root.join(INPUTS);
    let out_dir = root.join(OUT_DIR);
    fs::create_dir_all(&out_dir)?;
    let names = [
        "01-png-transparency-demo.png",
        "02-pluto-transparent.png",
        "03-wikipedia-logo.png",
        "04-photo-portrait.png",
        "05-photo-mountain.png",
        "06-photo-landscape.png",
        "07-photo-product.png",
    ];

    let mut rows: Vec<Row> = Vec::new();
    for name in &names {
        let path = inputs_dir.join(name);
        let img = ::image::open(&path)?.to_rgba8();
        let (w, h) = (img.width() as usize, img.height() as usize);
        let n = w * h;
        let raw = img.into_raw();

        // train (C0 baseline cfg) — also test n_iters=0 to isolate
        // "imagequant init + hard-argmin no-dither apply" vs full C0.
        let cfg = TrainConfig::default();
        let t0 = Instant::now();
        let palette = train_palette_c0(&raw, w, h, cfg);
        let train_ms = t0.elapsed().as_secs_f64() * 1000.0;

        // Diagnostic: imagequant init only, 0 iters
        let cfg_no_train = TrainConfig { n_iters: 0, ..cfg };
        let palette_no_train = train_palette_c0(&raw, w, h, cfg_no_train);
        let (idx_nt, pal_nt) = apply_palette(&raw, w, h, &palette_no_train);
        let png_nt = encode_indexed_png(w, h, &idx_nt, &pal_nt, &raw);
        let opt_nt = oxipng::optimize_from_memory(&png_nt, &oxipng::Options::from_preset(5)).expect("oxipng");
        let dec_nt = ::image::load_from_memory_with_format(&opt_nt, ::image::ImageFormat::Png).expect("dec").to_rgba8().into_raw();
        let ssim_nt = nupic_ssimulacra::ssimulacra2_score(&raw, &dec_nt, w as u32, h as u32).unwrap();
        println!("[c0_bench] {name} no-train (imagequant init + hard argmin): SSIM {ssim_nt:.2}");

        // inference + indexed PNG encode
        let t0 = Instant::now();
        let (indices, palette_srgb) = apply_palette(&raw, w, h, &palette);
        let png_bytes = encode_indexed_png(w, h, &indices, &palette_srgb, &raw);
        let infer_ms = t0.elapsed().as_secs_f64() * 1000.0;

        // Apply oxipng (matches the cement post-encode step).
        let optimised = oxipng::optimize_from_memory(&png_bytes, &oxipng::Options::from_preset(5))
            .expect("oxipng");

        // SSIMULACRA2: decode optimised PNG, score vs original
        let decoded = ::image::load_from_memory_with_format(&optimised, ::image::ImageFormat::Png)?
            .to_rgba8()
            .into_raw();
        let ssim_c0 = nupic_ssimulacra::ssimulacra2_score(&raw, &decoded, w as u32, h as u32).unwrap();

        // baseline: imagequant + oxipng (mimics nupic 0.4 default)
        let (baseline_bytes, baseline_decoded) = imagequant_baseline(&raw, w, h);
        let ssim_imagequant_baseline = nupic_ssimulacra::ssimulacra2_score(&raw, &baseline_decoded, w as u32, h as u32).unwrap();

        rows.push(Row {
            image: name.to_string(),
            n_pixels: n,
            train_ms,
            infer_ms,
            bytes: optimised.len(),
            ssim_c0,
            ssim_imagequant_baseline,
            ssim_delta_vs_imagequant: ssim_c0 - ssim_imagequant_baseline,
        });
        let _ = baseline_bytes; // silenced
        println!("[c0_bench] done {name} (train {:.1}s, infer {:.0}ms, SSIM C0 {:.2} vs baseline {:.2})",
                 train_ms / 1000.0, infer_ms, ssim_c0, ssim_imagequant_baseline);
    }

    write_csv(&out_dir.join("03c-bis-codebook-c0-bench.csv"), &rows)?;
    write_md(&out_dir.join("03c-bis-codebook-c0-bench.md"), &rows)?;
    println!("[c0_bench] wrote {} rows", rows.len());
    Ok(())
}

fn encode_indexed_png(
    w: usize,
    h: usize,
    indices: &[u8],
    palette_srgb: &[rgb::Rgb<u8>],
    original_rgba: &[u8],
) -> Vec<u8> {
    // Build tRNS per-palette: take the most-frequent alpha per palette
    // entry from the original RGBA. C0 doesn't model alpha; for this
    // bench we set alpha = 255 for all palette entries (drop alpha
    // channel — same as imagequant baseline for opaque-only fixtures).
    let _ = original_rgba;
    let mut rgb_palette: Vec<u8> = Vec::with_capacity(palette_srgb.len() * 3);
    for c in palette_srgb {
        rgb_palette.push(c.r);
        rgb_palette.push(c.g);
        rgb_palette.push(c.b);
    }
    let mut raw = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut raw, w as u32, h as u32);
        enc.set_color(png::ColorType::Indexed);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_palette(rgb_palette);
        let mut writer = enc.write_header().expect("png header");
        writer.write_image_data(indices).expect("png data");
    }
    raw
}

fn imagequant_baseline(rgba: &[u8], w: usize, h: usize) -> (Vec<u8>, Vec<u8>) {
    let pixels: Vec<rgb::RGBA8> = rgba.chunks_exact(4)
        .map(|c| rgb::RGBA8 { r: c[0], g: c[1], b: c[2], a: c[3] })
        .collect();
    let mut attrs = imagequant::new();
    // start (70, 95); fall back to (0, 95) on QualityTooLow.
    let mut q_min = 70u8;
    attrs.set_quality(q_min, 95).expect("iq quality");
    attrs.set_speed(4).expect("iq speed");
    let mut img = attrs.new_image(pixels.as_slice(), w, h, 0.0).expect("iq image");
    let mut quant = match attrs.quantize(&mut img) {
        Ok(q) => q,
        Err(_) => {
            q_min = 0;
            let mut attrs2 = imagequant::new();
            attrs2.set_quality(q_min, 95).expect("iq quality fallback");
            attrs2.set_speed(4).expect("iq speed");
            img = attrs2.new_image(pixels.as_slice(), w, h, 0.0).expect("iq image fallback");
            attrs = attrs2;
            attrs.quantize(&mut img).expect("iq fallback quant")
        }
    };
    let _ = q_min;
    quant.set_dithering_level(1.0).expect("iq dither");
    let (palette, indexed) = quant.remapped(&mut img).expect("iq remap");
    let mut rgb_palette: Vec<u8> = Vec::with_capacity(palette.len() * 3);
    let mut alphas: Vec<u8> = Vec::with_capacity(palette.len());
    for c in &palette {
        rgb_palette.push(c.r);
        rgb_palette.push(c.g);
        rgb_palette.push(c.b);
        alphas.push(c.a);
    }
    while alphas.last() == Some(&255) { alphas.pop(); }
    let mut raw = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut raw, w as u32, h as u32);
        enc.set_color(png::ColorType::Indexed);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_palette(rgb_palette);
        if !alphas.is_empty() { enc.set_trns(alphas); }
        let mut writer = enc.write_header().expect("png header");
        writer.write_image_data(&indexed).expect("png data");
    }
    let optimised = oxipng::optimize_from_memory(&raw, &oxipng::Options::from_preset(5))
        .expect("oxipng");
    let decoded = ::image::load_from_memory_with_format(&optimised, ::image::ImageFormat::Png)
        .expect("decode")
        .to_rgba8()
        .into_raw();
    (optimised, decoded)
}

fn write_csv(path: &Path, rows: &[Row]) -> Result<()> {
    use std::io::Write;
    let mut f = fs::File::create(path)?;
    writeln!(f, "image,n_pixels,train_ms,infer_ms,bytes,ssim_c0,ssim_imagequant,delta")?;
    for r in rows {
        writeln!(f, "{},{},{:.1},{:.1},{},{:.3},{:.3},{:.3}",
            r.image, r.n_pixels, r.train_ms, r.infer_ms,
            r.bytes, r.ssim_c0, r.ssim_imagequant_baseline,
            r.ssim_delta_vs_imagequant)?;
    }
    Ok(())
}

fn write_md(path: &Path, rows: &[Row]) -> Result<()> {
    use std::fmt::Write;
    let mut s = String::new();
    writeln!(&mut s, "# 03c-bis-codebook-c0-bench — raw output\n")?;
    writeln!(&mut s, "Generated by `cargo run --release -p nupic-research --example codebook_c0_bench`.\n")?;
    writeln!(&mut s, "| image | n_px | train s | infer ms | bytes | C0 SSIM | imagequant SSIM | Δ |")?;
    writeln!(&mut s, "|---|---:|---:|---:|---:|---:|---:|---:|")?;
    for r in rows {
        writeln!(&mut s,
            "| `{}` | {} | {:.1} | {:.0} | {} | {:.2} | {:.2} | {:+.2} |",
            r.image, r.n_pixels, r.train_ms / 1000.0, r.infer_ms,
            r.bytes, r.ssim_c0, r.ssim_imagequant_baseline,
            r.ssim_delta_vs_imagequant)?;
    }
    fs::write(path, s)?;
    Ok(())
}

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(Path::new(m).ancestors().nth(2).context("workspace root")?.to_path_buf())
}
