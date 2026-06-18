//! Cycle 111 — R6 multi-tile feasibility probe on 6 DSSIM-infeasible fixtures.
//!
//! Cycle 106 found that any global K∈{64..256} ∧ d∈{0,0.3,0.6} fails the
//! DSSIM gate on 6 Pile A fixtures (p125, p274, p214, p115, p175, p167).
//! Cycle 110 confirmed lossless fallback can't save them either
//! (ratios 1.36-1.95× tiny). The hypothesis: per-tile independent
//! quantization can capture different color regions per tile, breaking
//! the single-global-palette ceiling on DSSIM.
//!
//! This spike does NOT measure size — encoder is a downstream problem.
//! It measures whether **R6 quantization reconstruction DSSIM** can
//! drop below tiny_dssim. If yes, spatial-aware quantization is the
//! right path; if no, even R6 won't save these fixtures and Cycle 112+
//! moves to R3 VQ-VAE territory.
//!
//! Method:
//! 1. Split image into N×N tile grid.
//! 2. Per tile: imagequant K=64-128 quantize → quantized RGBA buffer.
//! 3. Reassemble quantized tiles into full RGBA.
//! 4. DSSIM(original, reassembled).
//! 5. Compare to tiny_dssim baseline.
//!
//! Wall target ≤ 5 min (6 fixtures × ~5 configs × 4 cores).

use std::path::PathBuf;

use image::ImageReader;
use imagequant::RGBA;
use rayon::prelude::*;

use nupic_core::metrics;
use nupic_core::Image;
use nupic_research::bench::{bench_pool, workspace_root};

/// Quantize a tile of RGBA pixels with imagequant, returning a new
/// RGBA buffer the same shape as the input (with quantized colors).
fn quantize_tile(rgba: &[u8], w: usize, h: usize, k: u32) -> Vec<u8> {
    assert_eq!(rgba.len(), w * h * 4);
    let pixels: Vec<RGBA> = rgba
        .chunks_exact(4)
        .map(|c| RGBA::new(c[0], c[1], c[2], c[3]))
        .collect();

    let mut attrs = imagequant::new();
    attrs.set_max_colors(k).expect("set_max_colors");
    attrs.set_speed(5).expect("set_speed");

    let mut img = attrs.new_image(&pixels[..], w, h, 0.0).expect("new_image");
    let mut res = attrs.quantize(&mut img).expect("quantize");
    res.set_dithering_level(0.3).expect("dither");

    let (palette, indexes) = res.remapped(&mut img).expect("remapped");
    let mut out = Vec::with_capacity(w * h * 4);
    for &idx in &indexes {
        let p = palette[idx as usize];
        out.push(p.r);
        out.push(p.g);
        out.push(p.b);
        out.push(p.a);
    }
    out
}

/// Tile the image into `tiles_x × tiles_y` grid; per-tile quantize at K;
/// reassemble into a full quantized RGBA buffer.
fn tile_quantize(rgba: &[u8], w: u32, h: u32, tiles_x: u32, tiles_y: u32, k: u32) -> Vec<u8> {
    let mut out = vec![0u8; (w * h * 4) as usize];
    let tw_base = w / tiles_x;
    let th_base = h / tiles_y;
    let extra_x = w % tiles_x;
    let extra_y = h % tiles_y;

    let mut y_start: u32 = 0;
    for ty in 0..tiles_y {
        let th = th_base + if ty < extra_y { 1 } else { 0 };
        let mut x_start: u32 = 0;
        for tx in 0..tiles_x {
            let tw = tw_base + if tx < extra_x { 1 } else { 0 };
            // Extract tile RGBA
            let mut tile = Vec::with_capacity((tw * th * 4) as usize);
            for ty_pix in 0..th {
                let row_start = ((y_start + ty_pix) * w + x_start) * 4;
                let row_end = row_start + tw * 4;
                tile.extend_from_slice(&rgba[row_start as usize..row_end as usize]);
            }
            // Quantize tile
            let q = quantize_tile(&tile, tw as usize, th as usize, k);
            // Write back
            for ty_pix in 0..th {
                let src_off = (ty_pix * tw * 4) as usize;
                let dst_off = (((y_start + ty_pix) * w + x_start) * 4) as usize;
                out[dst_off..dst_off + (tw * 4) as usize]
                    .copy_from_slice(&q[src_off..src_off + (tw * 4) as usize]);
            }
            x_start += tw;
        }
        y_start += th;
    }
    out
}

fn dssim_of_rgba(reference: &Image, rgba: &[u8], w: u32, h: u32) -> anyhow::Result<f64> {
    let buf = image::RgbaImage::from_vec(w, h, rgba.to_vec())
        .ok_or_else(|| anyhow::anyhow!("RgbaImage::from_vec"))?;
    let dyn_img = image::DynamicImage::ImageRgba8(buf);
    let mut bytes = Vec::new();
    use std::io::Cursor;
    dyn_img.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)?;
    let distorted = Image::decode(&bytes)?;
    Ok(metrics::dssim(reference, &distorted)?)
}

struct Result {
    fixture: String,
    tiles: u32,
    k: u32,
    dssim: f64,
    tiny_dssim: f64,
    passes_dssim: bool,
}

fn main() -> anyhow::Result<()> {
    let root = workspace_root();
    let corpus = root.join("assets/png-bench/corpus-500");

    // 6 DSSIM-infeasible from Cycle 106 + their tiny_dssim from baseline.
    let fixtures: &[(&str, f64)] = &[
        ("p125_1920x1080.png", 0.009766),
        ("p274_3840x2560.png", 0.003084),
        ("p214_2400x1600.png", 0.002845),
        ("p115_1024x768.png", 0.001970),
        ("p175_1920x1080.png", 0.001966),
        ("p167_1920x1080.png", 0.000880),
    ];

    // Tile counts × K configurations.
    let configs: &[(u32, u32)] = &[
        (2, 64), (2, 128), (3, 64), (3, 128), (4, 64), (4, 128),
        (6, 128), (8, 128), (8, 192),
    ];

    let pool = bench_pool()?;
    let t0 = std::time::Instant::now();
    let results: Vec<Result> = pool.install(|| {
        let mut all_jobs: Vec<(&str, f64, u32, u32)> = Vec::new();
        for &(name, tdss) in fixtures {
            for &(t, k) in configs {
                all_jobs.push((name, tdss, t, k));
            }
        }
        all_jobs.par_iter().filter_map(|(name, tdss, t, k)| {
            let orig = corpus.join(name);
            let img = ImageReader::open(&orig).ok()?.with_guessed_format().ok()?.decode().ok()?;
            let rgba = img.to_rgba8();
            let (w, h) = (rgba.width(), rgba.height());
            let raw = rgba.into_raw();
            let reassembled = tile_quantize(&raw, w, h, *t, *t, *k);
            let reference = Image::open(&orig).ok()?;
            let d = dssim_of_rgba(&reference, &reassembled, w, h).ok()?;
            Some(Result {
                fixture: name.to_string(),
                tiles: *t,
                k: *k,
                dssim: d,
                tiny_dssim: *tdss,
                passes_dssim: d <= *tdss,
            })
        }).collect()
    });
    let dt = t0.elapsed();
    eprintln!("wall = {:.1}s ({} jobs)", dt.as_secs_f64(), results.len());

    println!("fixture\ttiles_n\tK\tdssim\ttiny_dssim\tdssim_pass");
    for r in &results {
        println!("{}\t{}x{}\t{}\t{:.6}\t{:.6}\t{}",
            r.fixture, r.tiles, r.tiles, r.k, r.dssim, r.tiny_dssim,
            if r.passes_dssim { "Y" } else { "N" });
    }

    eprintln!();
    eprintln!("=== Cycle 111 R6 multi-tile feasibility ===");
    // Per fixture: did any tile config pass DSSIM?
    use std::collections::BTreeMap;
    let mut by_fix: BTreeMap<String, (bool, f64)> = BTreeMap::new();
    for r in &results {
        let entry = by_fix.entry(r.fixture.clone()).or_insert((false, f64::MAX));
        if r.passes_dssim { entry.0 = true; }
        if r.dssim < entry.1 { entry.1 = r.dssim; }
    }
    let mut passed = 0;
    for (fx, (any_pass, best_d)) in &by_fix {
        let tdss = fixtures.iter().find(|(n, _)| n == fx).map(|(_,d)| *d).unwrap_or(0.0);
        eprintln!("  {:<28} any_pass={} best_dssim={:.6} tiny_dssim={:.6} margin={:+.6}",
            fx, if *any_pass {"Y"} else {"N"}, best_d, tdss, *best_d - tdss);
        if *any_pass { passed += 1; }
    }
    eprintln!();
    eprintln!("R6-quantization DSSIM PASS: {}/{}", passed, fixtures.len());
    let verdict = if passed >= 4 { "GREEN — R6 quantization viable, Cycle 112 wire" }
                  else if passed >= 1 { "YELLOW — partial, Cycle 112 tune tiles/K" }
                  else { "RED — even R6 quantization stuck, transition to R3 VQ-VAE" };
    eprintln!("→ {}", verdict);

    Ok(())
}
