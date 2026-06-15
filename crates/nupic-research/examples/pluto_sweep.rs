//! Sweep imagequant + oxipng over (`dither_level`, `quality_min`,
//! `quality_target`) on every PNG in `assets/png-bench/inputs/`.
//! Writes a CSV + markdown summary; numbers back
//! `docs/research/png/01-pluto-case.md`.
//!
//! Run:
//!   cargo run --release -p nupic-research --example pluto_sweep
//!
//! Output (under `target/research-out/`):
//!   01-pluto-sweep.csv
//!   01-pluto-sweep.md
//!
//! Reuses `nupic_core::Image` + `nupic_core::metrics::dssim` for the
//! reference loader / metric so the numbers are commensurable with
//! `nupic bench`. The quantise → indexed-PNG → oxipng pipeline is
//! re-implemented here so we can poke every knob; do **not** factor
//! this code into `nupic-core` without an essay justifying it.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use nupic_core::{Image, metrics};

const INPUTS: &str = "assets/png-bench/inputs";
const TINYPNG_DIR: &str = "assets/png-bench/tinypng-web";
const OUT_DIR: &str = "target/research-out";

#[derive(Clone, Copy, Debug)]
struct Cfg {
    dither: f32,
    q_min: u8,
    q_target: u8,
}

#[derive(Debug)]
struct Outcome {
    image: String,
    cfg: Cfg,
    palette_size: usize,
    post_oxipng_bytes: usize,
    dssim: f64,
    encode_ms: u128,
    note: &'static str,
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

    // Full grid on the lead case (02-pluto). Compact grid on the rest —
    // enough to confirm sweep conclusions aren't 02-specific.
    let lead = images
        .iter()
        .find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("02-")))
        .ok_or_else(|| anyhow!("02-pluto fixture missing"))?
        .clone();

    let full_grid: Vec<Cfg> = build_full_grid();
    let compact_grid: Vec<Cfg> = build_compact_grid();

    let mut rows: Vec<Outcome> = Vec::new();
    println!("[01-pluto-sweep] lead case: {}", lead.display());
    println!("[01-pluto-sweep] full grid = {} configs", full_grid.len());
    println!("[01-pluto-sweep] cross-check grid = {} configs × 6 images", compact_grid.len());

    let original = Image::open(&lead).context("open lead")?;
    for cfg in &full_grid {
        rows.push(run_one(&lead, &original, *cfg));
    }

    for path in images.iter().filter(|p| **p != lead) {
        let img = Image::open(path).with_context(|| format!("open {}", path.display()))?;
        for cfg in &compact_grid {
            rows.push(run_one(path, &img, *cfg));
        }
    }

    // Reference rows: nupic 0.4.0 default + TinyPNG baseline. Encoded
    // separately so the table is self-contained.
    let mut ref_rows: Vec<Outcome> = Vec::new();
    for path in &images {
        let img = Image::open(path)?;
        // nupic 0.4.0 default: dither 1.0, quality (70, 95) with QualityTooLow
        // fallback to (0, 95). Mirrors crates/nupic-core/src/ops/compress.rs.
        let cfg = Cfg { dither: 1.0, q_min: 70, q_target: 95 };
        ref_rows.push(run_with_fallback(path, &img, cfg, "nupic-0.4-default"));

        // TinyPNG baseline — read off disk.
        let tp_path = tinypng_dir.join(path.file_name().unwrap());
        if let Ok(bytes) = fs::read(&tp_path) {
            if let Ok(tp_decoded) = Image::decode(&bytes) {
                let dssim = metrics::dssim(&img, &tp_decoded).unwrap_or(f64::NAN);
                ref_rows.push(Outcome {
                    image: filename(path),
                    cfg: Cfg { dither: f32::NAN, q_min: 0, q_target: 0 },
                    palette_size: 0,
                    post_oxipng_bytes: bytes.len(),
                    dssim,
                    encode_ms: 0,
                    note: "tinypng-baseline",
                });
            }
        }
    }

    write_csv(&out_dir.join("01-pluto-sweep.csv"), &rows, &ref_rows)?;
    write_md(&out_dir.join("01-pluto-sweep.md"), &rows, &ref_rows, &lead)?;
    println!("[01-pluto-sweep] wrote {} sweep rows + {} reference rows",
             rows.len(), ref_rows.len());
    println!("[01-pluto-sweep] outputs in {}", out_dir.display());
    Ok(())
}

fn build_full_grid() -> Vec<Cfg> {
    let mut g = Vec::new();
    let dithers = [0.0f32, 0.25, 0.5, 0.75, 1.0];
    let pairs: &[(u8, u8)] = &[
        (0, 10),
        (0, 30),
        (0, 50),
        (0, 80),
        (0, 90),
        (0, 95),
        (0, 100),
        (50, 90),
        (50, 95),
        (70, 95),
        (80, 95),
        (80, 100),
    ];
    for &d in &dithers {
        for &(qm, qt) in pairs {
            g.push(Cfg { dither: d, q_min: qm, q_target: qt });
        }
    }
    g
}

fn build_compact_grid() -> Vec<Cfg> {
    // Six configs that span the trade-off: low-dither / high-dither
    // crossed with three quality-target levels.
    vec![
        Cfg { dither: 0.5, q_min: 0, q_target: 80 },
        Cfg { dither: 0.5, q_min: 0, q_target: 95 },
        Cfg { dither: 0.5, q_min: 0, q_target: 100 },
        Cfg { dither: 1.0, q_min: 0, q_target: 80 },
        Cfg { dither: 1.0, q_min: 0, q_target: 95 },
        Cfg { dither: 1.0, q_min: 0, q_target: 100 },
    ]
}

fn run_one(path: &Path, reference: &Image, cfg: Cfg) -> Outcome {
    run_with_fallback(path, reference, cfg, "")
}

/// Encode + measure. On QualityTooLow, retry with `q_min = 0` and tag
/// the row so the essay can call it out.
fn run_with_fallback(path: &Path, reference: &Image, cfg: Cfg, note: &'static str) -> Outcome {
    let t0 = Instant::now();
    match encode_lossy(reference, cfg) {
        Ok((palette_size, bytes)) => {
            let elapsed = t0.elapsed().as_millis();
            let dssim = decoded_dssim(reference, &bytes).unwrap_or(f64::NAN);
            Outcome {
                image: filename(path),
                cfg,
                palette_size,
                post_oxipng_bytes: bytes.len(),
                dssim,
                encode_ms: elapsed,
                note,
            }
        }
        Err(EncodeErr::QualityTooLow) if cfg.q_min > 0 => {
            let fallback_cfg = Cfg { q_min: 0, ..cfg };
            let elapsed_before = t0.elapsed().as_millis();
            match encode_lossy(reference, fallback_cfg) {
                Ok((palette_size, bytes)) => {
                    let elapsed = t0.elapsed().as_millis();
                    let dssim = decoded_dssim(reference, &bytes).unwrap_or(f64::NAN);
                    Outcome {
                        image: filename(path),
                        cfg,
                        palette_size,
                        post_oxipng_bytes: bytes.len(),
                        dssim,
                        encode_ms: elapsed,
                        note: tag_quality_fallback(note),
                    }
                }
                Err(_) => Outcome {
                    image: filename(path),
                    cfg,
                    palette_size: 0,
                    post_oxipng_bytes: 0,
                    dssim: f64::NAN,
                    encode_ms: elapsed_before,
                    note: "ERR (fallback also failed)",
                },
            }
        }
        Err(EncodeErr::QualityTooLow) => Outcome {
            image: filename(path),
            cfg,
            palette_size: 0,
            post_oxipng_bytes: 0,
            dssim: f64::NAN,
            encode_ms: t0.elapsed().as_millis(),
            note: "ERR QualityTooLow (q_min=0)",
        },
        Err(EncodeErr::Other(e)) => Outcome {
            image: filename(path),
            cfg,
            palette_size: 0,
            post_oxipng_bytes: 0,
            dssim: f64::NAN,
            encode_ms: t0.elapsed().as_millis(),
            note: Box::leak(format!("ERR {e}").into_boxed_str()),
        },
    }
}

fn tag_quality_fallback(prev: &'static str) -> &'static str {
    if prev.is_empty() { "q_min→0 fallback" } else { prev }
}

#[derive(Debug)]
enum EncodeErr {
    QualityTooLow,
    Other(String),
}

fn encode_lossy(image: &Image, cfg: Cfg) -> std::result::Result<(usize, Vec<u8>), EncodeErr> {
    let rgba = nupic_to_rgba8(image);
    let width = rgba.width as usize;
    let height = rgba.height as usize;
    let pixels: &[rgb::RGBA8] = &rgba.pixels;

    let mut attrs = imagequant::new();
    attrs
        .set_quality(cfg.q_min, cfg.q_target)
        .map_err(map_iq_err)?;
    attrs.set_speed(4).map_err(map_iq_err)?;

    let mut img_iq = attrs
        .new_image(pixels, width, height, 0.0)
        .map_err(map_iq_err)?;
    let mut quant = attrs.quantize(&mut img_iq).map_err(map_iq_err)?;
    quant.set_dithering_level(cfg.dither).map_err(map_iq_err)?;
    let (palette, indexed_pixels) = quant.remapped(&mut img_iq).map_err(map_iq_err)?;

    // Write indexed PNG via the same scheme as nupic-core::encode_png_lossy.
    let mut rgb_palette: Vec<u8> = Vec::with_capacity(palette.len() * 3);
    let mut alphas: Vec<u8> = Vec::with_capacity(palette.len());
    for c in &palette {
        rgb_palette.push(c.r);
        rgb_palette.push(c.g);
        rgb_palette.push(c.b);
        alphas.push(c.a);
    }
    while alphas.last() == Some(&255) {
        alphas.pop();
    }

    let mut raw = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut raw, width as u32, height as u32);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_palette(rgb_palette);
        if !alphas.is_empty() {
            encoder.set_trns(alphas);
        }
        let mut writer = encoder
            .write_header()
            .map_err(|e| EncodeErr::Other(e.to_string()))?;
        writer
            .write_image_data(&indexed_pixels)
            .map_err(|e| EncodeErr::Other(e.to_string()))?;
    }

    // oxipng preset 5 — matches nupic compress default (effort=5 → preset 5).
    let oxipng_opts = oxipng::Options::from_preset(5);
    let optimised = oxipng::optimize_from_memory(&raw, &oxipng_opts)
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

struct Rgba8Buf {
    width: u32,
    height: u32,
    pixels: Vec<rgb::RGBA8>,
}

fn nupic_to_rgba8(image: &Image) -> Rgba8Buf {
    // Round-trip through encode → decode to get raw RGBA without touching
    // `Image::inner` (pub(crate) only). encode_png lossless is the cheapest
    // valid path and bit-exact preserves pixels.
    let encoded = image
        .compress(nupic_core::CompressOpts {
            format: nupic_core::Format::Png,
            quality: nupic_core::Quality::Lossless,
            strip_metadata: true,
            effort: 0,
        })
        .expect("lossless encode of fixture must succeed");
    let dec = ::image::load_from_memory_with_format(&encoded.bytes, ::image::ImageFormat::Png)
        .expect("decode round-tripped PNG")
        .to_rgba8();
    let (w, h) = (dec.width(), dec.height());
    let pixels: Vec<rgb::RGBA8> = dec
        .pixels()
        .map(|p| rgb::RGBA8 { r: p[0], g: p[1], b: p[2], a: p[3] })
        .collect();
    Rgba8Buf { width: w, height: h, pixels }
}

fn decoded_dssim(reference: &Image, encoded_bytes: &[u8]) -> Option<f64> {
    let distorted = Image::decode(encoded_bytes).ok()?;
    metrics::dssim(reference, &distorted).ok()
}

fn filename(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn write_csv(path: &Path, rows: &[Outcome], refs: &[Outcome]) -> Result<()> {
    use std::io::Write;
    let mut f = fs::File::create(path)?;
    writeln!(f, "image,dither,q_min,q_target,palette,post_oxipng_bytes,dssim,encode_ms,note")?;
    for r in rows.iter().chain(refs.iter()) {
        let dither = if r.cfg.dither.is_nan() { String::from("nan") } else { format!("{:.2}", r.cfg.dither) };
        let dssim = if r.dssim.is_nan() { String::from("nan") } else { format!("{:.6}", r.dssim) };
        writeln!(
            f,
            "{},{},{},{},{},{},{},{},{}",
            r.image, dither, r.cfg.q_min, r.cfg.q_target,
            r.palette_size, r.post_oxipng_bytes, dssim, r.encode_ms, r.note,
        )?;
    }
    Ok(())
}

fn write_md(path: &Path, rows: &[Outcome], refs: &[Outcome], lead: &Path) -> Result<()> {
    use std::fmt::Write;
    let mut s = String::new();
    let lead_name = filename(lead);
    writeln!(
        &mut s,
        "# 01-pluto-sweep — raw output\n\nGenerated by `cargo run --release -p nupic-research --example pluto_sweep`.\n"
    )?;
    writeln!(&mut s, "## Reference rows\n")?;
    writeln!(&mut s, "| image | source | dither | q_min | q_target | palette | bytes | DSSIM | ms |")?;
    writeln!(&mut s, "|---|---|---:|---:|---:|---:|---:|---:|---:|")?;
    for r in refs {
        let dither_cell = if r.cfg.dither.is_nan() { String::from("—") } else { format!("{:.2}", r.cfg.dither) };
        writeln!(
            &mut s,
            "| `{}` | {} | {} | {} | {} | {} | {} | {:.6} | {} |",
            r.image, r.note, dither_cell, r.cfg.q_min, r.cfg.q_target,
            r.palette_size, r.post_oxipng_bytes, r.dssim, r.encode_ms,
        )?;
    }
    writeln!(&mut s, "\n## Lead case ({})\n\nFull grid: 5 dither × 9 (q_min, q_target).\n", lead_name)?;
    writeln!(&mut s, "| dither | q_min | q_target | palette | bytes | DSSIM | ms | note |")?;
    writeln!(&mut s, "|---:|---:|---:|---:|---:|---:|---:|---|")?;
    for r in rows.iter().filter(|r| r.image == lead_name) {
        writeln!(
            &mut s,
            "| {:.2} | {} | {} | {} | {} | {:.6} | {} | {} |",
            r.cfg.dither, r.cfg.q_min, r.cfg.q_target,
            r.palette_size, r.post_oxipng_bytes, r.dssim, r.encode_ms,
            if r.note.is_empty() { "" } else { r.note },
        )?;
    }
    writeln!(&mut s, "\n## Cross-check (other 6 images)\n")?;
    writeln!(&mut s, "| image | dither | q_min | q_target | palette | bytes | DSSIM | ms |")?;
    writeln!(&mut s, "|---|---:|---:|---:|---:|---:|---:|---:|")?;
    for r in rows.iter().filter(|r| r.image != lead_name) {
        writeln!(
            &mut s,
            "| `{}` | {:.2} | {} | {} | {} | {} | {:.6} | {} |",
            r.image, r.cfg.dither, r.cfg.q_min, r.cfg.q_target,
            r.palette_size, r.post_oxipng_bytes, r.dssim, r.encode_ms,
        )?;
    }
    fs::write(path, s)?;
    Ok(())
}

fn workspace_root() -> Result<PathBuf> {
    // CARGO_MANIFEST_DIR points at the research crate; workspace root is two
    // levels up.
    let manifest = env!("CARGO_MANIFEST_DIR");
    let root = Path::new(manifest)
        .ancestors()
        .nth(2)
        .ok_or_else(|| anyhow!("could not derive workspace root from {}", manifest))?;
    Ok(root.to_path_buf())
}
