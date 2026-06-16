//! Per-image quality sweep across q_target with **both** DSSIM and
//! SSIMULACRA2 measured at every point. Cross-references the nupic 0.4
//! default vs the TinyPNG baseline on the same metric stack.
//!
//! Backs `docs/research/png/02-perceptual-metrics.md`. Answers the
//! question 01 essay leaves open: is the imagequant-on-02-pluto
//! "metric ceiling" actually visible to a stronger perceptual metric?
//!
//! Run:
//!   cargo run --release -p nupic-research --example metric_sweep
//!
//! Output (under `target/research-out/`):
//!   02-metric-sweep.csv
//!   02-metric-sweep.md
//!
//! Caveat: SSIMULACRA2 is opinionated about input color space. The
//! crate is told `TransferCharacteristic::SRGB` + `ColorPrimaries::BT709`
//! (== sRGB primaries) since every PNG fixture is sRGB on the wire.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use nupic_core::{Image, metrics};
use ssimulacra2::{ColorPrimaries, Rgb, TransferCharacteristic};

const INPUTS: &str = "assets/png-bench/inputs";
const TINYPNG_DIR: &str = "assets/png-bench/tinypng-web";
const OUT_DIR: &str = "target/research-out";

/// q_target values to sweep — chosen to cover the 02-pluto curve in 01
/// (q=10..=100, with the 80–95 elbow region densely sampled).
const Q_TARGETS: &[u8] = &[10, 30, 50, 70, 80, 90, 95, 100];

#[derive(Debug)]
struct Row {
    image: String,
    label: String, // "nupic-0.4-default" | "tinypng" | "sweep:q_target=80" | ...
    palette: usize,
    bytes: usize,
    dssim: f64,
    ssimulacra2: f64,
    encode_ms: u128,
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let inputs_dir = root.join(INPUTS);
    let tinypng_dir = root.join(TINYPNG_DIR);
    let out_dir = root.join(OUT_DIR);
    fs::create_dir_all(&out_dir).context("create research output dir")?;

    let mut images: Vec<PathBuf> = fs::read_dir(&inputs_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "png"))
        .collect();
    images.sort();
    if images.is_empty() {
        return Err(anyhow!("no fixtures in {}", inputs_dir.display()));
    }

    let mut rows: Vec<Row> = Vec::new();
    for path in &images {
        let name = filename(path);
        let original = Image::open(path).with_context(|| format!("open {}", path.display()))?;
        let original_rgba = nupic_to_rgba8(&original);

        // 1. nupic 0.4 default
        let t0 = Instant::now();
        let (pal, default_bytes) = nupic_default_encode(&original_rgba)
            .with_context(|| format!("nupic-default on {name}"))?;
        let dssim_default = decoded_dssim(&original, &default_bytes).unwrap_or(f64::NAN);
        let ssimulacra2_default = decoded_ssimulacra2(&original_rgba, &default_bytes)
            .unwrap_or(f64::NAN);
        rows.push(Row {
            image: name.clone(),
            label: "nupic-0.4-default".into(),
            palette: pal,
            bytes: default_bytes.len(),
            dssim: dssim_default,
            ssimulacra2: ssimulacra2_default,
            encode_ms: t0.elapsed().as_millis(),
        });

        // 2. TinyPNG baseline (read from disk if present)
        let tp_path = tinypng_dir.join(path.file_name().unwrap());
        if let Ok(tp_bytes) = fs::read(&tp_path) {
            let tp_dssim = decoded_dssim(&original, &tp_bytes).unwrap_or(f64::NAN);
            let tp_ssim = decoded_ssimulacra2(&original_rgba, &tp_bytes).unwrap_or(f64::NAN);
            rows.push(Row {
                image: name.clone(),
                label: "tinypng".into(),
                palette: 0,
                bytes: tp_bytes.len(),
                dssim: tp_dssim,
                ssimulacra2: tp_ssim,
                encode_ms: 0,
            });
        }

        // 3. q_target sweep (dither=1.0, q_min=0)
        for &q in Q_TARGETS {
            let t = Instant::now();
            match sweep_encode(&original_rgba, q) {
                Ok((pal, bytes)) => {
                    let dssim = decoded_dssim(&original, &bytes).unwrap_or(f64::NAN);
                    let ssim = decoded_ssimulacra2(&original_rgba, &bytes).unwrap_or(f64::NAN);
                    rows.push(Row {
                        image: name.clone(),
                        label: format!("sweep:q_target={q}"),
                        palette: pal,
                        bytes: bytes.len(),
                        dssim,
                        ssimulacra2: ssim,
                        encode_ms: t.elapsed().as_millis(),
                    });
                }
                Err(e) => eprintln!("sweep q={q} on {name}: {e}"),
            }
        }
        println!("[02-metric-sweep] done {name}");
    }

    write_csv(&out_dir.join("02-metric-sweep.csv"), &rows)?;
    write_md(&out_dir.join("02-metric-sweep.md"), &rows)?;
    println!("[02-metric-sweep] wrote {} rows to {}", rows.len(), out_dir.display());
    Ok(())
}

struct Rgba8Buf {
    width: u32,
    height: u32,
    pixels_u8: Vec<u8>, // RGBA8 packed
}

fn nupic_to_rgba8(image: &Image) -> Rgba8Buf {
    // Re-use the lossless-encode → decode trick from pluto_sweep to avoid
    // touching pub(crate) Image::inner.
    let encoded = image
        .compress(nupic_core::CompressOpts {
            format: nupic_core::Format::Png,
            quality: nupic_core::Quality::Lossless,
            strip_metadata: true,
            effort: 0,
            use_nupic_png: false,
            dither_strength: 0.0,
        })
        .expect("lossless encode of fixture must succeed");
    let dec = ::image::load_from_memory_with_format(&encoded.bytes, ::image::ImageFormat::Png)
        .expect("decode round-tripped PNG")
        .to_rgba8();
    let (w, h) = (dec.width(), dec.height());
    Rgba8Buf { width: w, height: h, pixels_u8: dec.into_raw() }
}

fn nupic_default_encode(rgba: &Rgba8Buf) -> Result<(usize, Vec<u8>)> {
    encode_lossy(rgba, 1.0, 70, 95)
        .or_else(|_| encode_lossy(rgba, 1.0, 0, 95))
        .map_err(|e| anyhow!("nupic-default fallback also failed: {e:?}"))
}

fn sweep_encode(rgba: &Rgba8Buf, q_target: u8) -> Result<(usize, Vec<u8>)> {
    encode_lossy(rgba, 1.0, 0, q_target)
        .map_err(|e| anyhow!("sweep encode q={q_target} failed: {e:?}"))
}

#[derive(Debug)]
enum EncodeErr { QualityTooLow, Other(String) }

fn encode_lossy(
    rgba: &Rgba8Buf,
    dither: f32,
    q_min: u8,
    q_target: u8,
) -> std::result::Result<(usize, Vec<u8>), EncodeErr> {
    let width = rgba.width as usize;
    let height = rgba.height as usize;
    let pixels: Vec<rgb::RGBA8> = rgba.pixels_u8
        .chunks_exact(4)
        .map(|c| rgb::RGBA8 { r: c[0], g: c[1], b: c[2], a: c[3] })
        .collect();

    let mut attrs = imagequant::new();
    attrs.set_quality(q_min, q_target).map_err(map_iq_err)?;
    attrs.set_speed(4).map_err(map_iq_err)?;
    let mut img = attrs.new_image(pixels.as_slice(), width, height, 0.0).map_err(map_iq_err)?;
    let mut quant = attrs.quantize(&mut img).map_err(map_iq_err)?;
    quant.set_dithering_level(dither).map_err(map_iq_err)?;
    let (palette, indexed) = quant.remapped(&mut img).map_err(map_iq_err)?;

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
        let mut encoder = png::Encoder::new(&mut raw, width as u32, height as u32);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_palette(rgb_palette);
        if !alphas.is_empty() { encoder.set_trns(alphas); }
        let mut writer = encoder.write_header()
            .map_err(|e| EncodeErr::Other(e.to_string()))?;
        writer.write_image_data(&indexed)
            .map_err(|e| EncodeErr::Other(e.to_string()))?;
    }

    let optimised = oxipng::optimize_from_memory(&raw, &oxipng::Options::from_preset(5))
        .map_err(|e| EncodeErr::Other(e.to_string()))?;
    Ok((palette.len(), optimised))
}

fn map_iq_err(e: imagequant::Error) -> EncodeErr {
    if matches!(e, imagequant::Error::QualityTooLow) {
        EncodeErr::QualityTooLow
    } else {
        EncodeErr::Other(format!("{e:?}"))
    }
}

fn decoded_dssim(reference: &Image, bytes: &[u8]) -> Option<f64> {
    let distorted = Image::decode(bytes).ok()?;
    metrics::dssim(reference, &distorted).ok()
}

fn decoded_ssimulacra2(reference_rgba: &Rgba8Buf, encoded_bytes: &[u8]) -> Option<f64> {
    let distorted = ::image::load_from_memory_with_format(encoded_bytes, ::image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    let dist_rgba = Rgba8Buf {
        width: distorted.width(),
        height: distorted.height(),
        pixels_u8: distorted.into_raw(),
    };
    compute_ssimulacra2(reference_rgba, &dist_rgba)
}

fn compute_ssimulacra2(reference: &Rgba8Buf, distorted: &Rgba8Buf) -> Option<f64> {
    if reference.width != distorted.width || reference.height != distorted.height {
        return None;
    }
    let to_rgb = |buf: &Rgba8Buf| -> Rgb {
        let pixels: Vec<[f32; 3]> = buf.pixels_u8
            .chunks_exact(4)
            .map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0])
            .collect();
        Rgb::new(
            pixels,
            buf.width as usize,
            buf.height as usize,
            TransferCharacteristic::SRGB,
            ColorPrimaries::BT709,
        )
        .expect("Rgb::new with sRGB/BT.709 cannot fail for non-empty buffer")
    };
    let r = to_rgb(reference);
    let d = to_rgb(distorted);
    ssimulacra2::compute_frame_ssimulacra2(r, d).ok()
}

fn filename(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn write_csv(path: &Path, rows: &[Row]) -> Result<()> {
    use std::io::Write;
    let mut f = fs::File::create(path)?;
    writeln!(f, "image,label,palette,bytes,dssim,ssimulacra2,encode_ms")?;
    for r in rows {
        let dssim = if r.dssim.is_nan() { "nan".into() } else { format!("{:.6}", r.dssim) };
        let ssim = if r.ssimulacra2.is_nan() { "nan".into() } else { format!("{:.3}", r.ssimulacra2) };
        writeln!(f, "{},{},{},{},{},{},{}",
            r.image, r.label, r.palette, r.bytes, dssim, ssim, r.encode_ms)?;
    }
    Ok(())
}

fn write_md(path: &Path, rows: &[Row]) -> Result<()> {
    use std::fmt::Write;
    let mut s = String::new();
    writeln!(&mut s, "# 02-metric-sweep — raw output\n")?;
    writeln!(&mut s, "Generated by `cargo run --release -p nupic-research --example metric_sweep`.")?;
    writeln!(&mut s, "")?;
    writeln!(&mut s, "## Reference rows (nupic-0.4-default vs tinypng)\n")?;
    writeln!(&mut s, "| image | source | bytes | DSSIM | SSIMULACRA2 |")?;
    writeln!(&mut s, "|---|---|---:|---:|---:|")?;
    for r in rows.iter().filter(|r| r.label == "nupic-0.4-default" || r.label == "tinypng") {
        writeln!(&mut s, "| `{}` | {} | {} | {:.6} | {:.3} |",
            r.image, r.label, r.bytes, r.dssim, r.ssimulacra2)?;
    }
    writeln!(&mut s, "\n## q_target sweep (dither=1.0, q_min=0)\n")?;
    writeln!(&mut s, "| image | q_target | palette | bytes | DSSIM | SSIMULACRA2 | ms |")?;
    writeln!(&mut s, "|---|---:|---:|---:|---:|---:|---:|")?;
    for r in rows.iter().filter(|r| r.label.starts_with("sweep:")) {
        let q = r.label.trim_start_matches("sweep:q_target=");
        writeln!(&mut s, "| `{}` | {} | {} | {} | {:.6} | {:.3} | {} |",
            r.image, q, r.palette, r.bytes, r.dssim, r.ssimulacra2, r.encode_ms)?;
    }
    fs::write(path, s)?;
    Ok(())
}

fn workspace_root() -> Result<PathBuf> {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Ok(Path::new(manifest).ancestors().nth(2)
        .ok_or_else(|| anyhow!("no workspace root"))?.to_path_buf())
}
