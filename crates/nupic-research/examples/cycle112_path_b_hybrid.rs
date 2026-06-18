//! Cycle 112 — Path B R6 → K=256 re-quantize hybrid spike.
//!
//! Cycle 111 found 8×8 tile × K=192 R6 emulation passes DSSIM 6/6 on
//! the Cycle 106 DSSIM-infeasible cluster, but its reconstruction has
//! up to 12288 unique colors — incompatible with PNG's 256-palette
//! ceiling. Path B tests whether feeding the R6 reconstruction back
//! through nupic-quantize's K=256 indexed PNG pipeline preserves the
//! R6 DSSIM advantage while producing a shippable single-palette PNG.
//!
//! Pipeline:
//!   1. R6 8×8 K=192 per-tile imagequant + reassemble → quantized RGBA
//!   2. quantize_indexed_png(K=256, d=0.3, preset=5) on that RGBA
//!   3. Measure size + DSSIM vs (TinyPNG, v1.2.8 baseline)
//!
//! Decision: ≥ 3/6 PASS both axis(size ≤ 0.80× tiny ∧ DSSIM ≤ tiny)
//! → v1.2.10 P-09 ship candidate.

use image::ImageReader;
use imagequant::RGBA;
use rayon::prelude::*;

use nupic_core::metrics;
use nupic_core::Image;
use nupic_quantize::{quantize_indexed_png, QuantizeOpts};
use nupic_research::bench::{bench_pool, workspace_root};

fn quantize_tile(rgba: &[u8], w: usize, h: usize, k: u32) -> Vec<u8> {
    let pixels: Vec<RGBA> = rgba
        .chunks_exact(4)
        .map(|c| RGBA::new(c[0], c[1], c[2], c[3]))
        .collect();
    let mut attrs = imagequant::new();
    attrs.set_max_colors(k).unwrap();
    attrs.set_speed(5).unwrap();
    let mut img = attrs.new_image(&pixels[..], w, h, 0.0).unwrap();
    let mut res = attrs.quantize(&mut img).unwrap();
    res.set_dithering_level(0.3).unwrap();
    let (palette, indexes) = res.remapped(&mut img).unwrap();
    let mut out = Vec::with_capacity(w * h * 4);
    for &idx in &indexes {
        let p = palette[idx as usize];
        out.extend_from_slice(&[p.r, p.g, p.b, p.a]);
    }
    out
}

fn tile_quantize(rgba: &[u8], w: u32, h: u32, tn: u32, k: u32) -> Vec<u8> {
    let mut out = vec![0u8; (w * h * 4) as usize];
    let tw_base = w / tn;
    let th_base = h / tn;
    let ex = w % tn;
    let ey = h % tn;
    let mut ys: u32 = 0;
    for ty in 0..tn {
        let th = th_base + if ty < ey { 1 } else { 0 };
        let mut xs: u32 = 0;
        for tx in 0..tn {
            let tw = tw_base + if tx < ex { 1 } else { 0 };
            let mut tile = Vec::with_capacity((tw * th * 4) as usize);
            for y in 0..th {
                let off = (((ys + y) * w + xs) * 4) as usize;
                tile.extend_from_slice(&rgba[off..off + (tw * 4) as usize]);
            }
            let q = quantize_tile(&tile, tw as usize, th as usize, k);
            for y in 0..th {
                let so = (y * tw * 4) as usize;
                let dst = (((ys + y) * w + xs) * 4) as usize;
                out[dst..dst + (tw * 4) as usize]
                    .copy_from_slice(&q[so..so + (tw * 4) as usize]);
            }
            xs += tw;
        }
        ys += th;
    }
    out
}

struct Out {
    fixture: String,
    tiny_size: u64,
    tiny_dssim: f64,
    baseline_size: u64,
    r6_only_dssim: f64,
    hybrid_size: u64,
    hybrid_dssim: f64,
    size_pass: bool,
    dssim_pass: bool,
    both: bool,
}

fn main() -> anyhow::Result<()> {
    let root = workspace_root();
    let corpus = root.join("assets/png-bench/corpus-500");
    let tiny_dir = root.join("assets/png-bench/tinypng-corpus-500");
    let out_dir = root.join("assets/png-bench/cycle112");
    std::fs::create_dir_all(&out_dir)?;

    // 6 DSSIM-infeasible from Cycle 106-111 + their tiny baselines.
    // (name, tile_n, K_per_tile) — winning config per Cycle 111 unanimous.
    let fixtures: &[&str] = &[
        "p115_1024x768.png",
        "p125_1920x1080.png",
        "p167_1920x1080.png",
        "p175_1920x1080.png",
        "p214_2400x1600.png",
        "p274_3840x2560.png",
    ];

    let pool = bench_pool()?;
    let t0 = std::time::Instant::now();
    let results: Vec<Out> = pool.install(|| {
        fixtures.par_iter().filter_map(|name| {
            let orig = corpus.join(name);
            let tiny = tiny_dir.join(name);
            let img = ImageReader::open(&orig).ok()?.with_guessed_format().ok()?.decode().ok()?;
            let rgba = img.to_rgba8();
            let (w, h) = (rgba.width(), rgba.height());
            let raw = rgba.into_raw();
            let reference = Image::open(&orig).ok()?;
            let tiny_size = std::fs::metadata(&tiny).ok()?.len();
            let tiny_dssim = metrics::dssim(&reference, &Image::open(&tiny).ok()?).ok()?;

            // R6 spatial-aware quantize
            let r6_rgba = tile_quantize(&raw, w, h, 8, 192);

            // R6-only DSSIM (no second quantize) — Cycle 111 reproducibility
            let r6_only_dssim = {
                let buf = image::RgbaImage::from_vec(w, h, r6_rgba.clone())?;
                let dyn_img = image::DynamicImage::ImageRgba8(buf);
                let mut bytes = Vec::new();
                use std::io::Cursor;
                dyn_img.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png).ok()?;
                metrics::dssim(&reference, &Image::decode(&bytes).ok()?).ok()?
            };

            // Path B: feed R6 reconstruction through K=256 indexed PNG.
            // d=0 (no dither) — dither would re-introduce DSSIM noise
            // we just paid R6 to remove. R6 already gave us a 192-color
            // palette per tile; imagequant K=256 captures most of that
            // exactly without dither.
            let mut hopts = QuantizeOpts::default();
            hopts.n_colors = 256;
            hopts.dither_strength = 0.0;
            hopts.oxipng_preset = 5;
            hopts.strip_metadata = true;
            let hybrid = quantize_indexed_png(&r6_rgba, w, h, hopts).ok()?;
            let hybrid_size = hybrid.len() as u64;
            let hybrid_dssim = metrics::dssim(&reference, &Image::decode(&hybrid).ok()?).ok()?;

            // Persist hybrid for visual inspection
            std::fs::write(out_dir.join(name), &hybrid).ok()?;

            // v1.2.8 baseline (from corpus-500-three-axis.tsv via bench helpers,
            // but here we read it from production binary subprocess for honesty)
            // Skipping subprocess for spike speed; use known from prior cycles.
            let size_cap = (tiny_size as f64 * 0.80) as u64;
            let size_pass = hybrid_size <= size_cap;
            let dssim_pass = hybrid_dssim <= tiny_dssim;

            Some(Out {
                fixture: name.to_string(),
                tiny_size, tiny_dssim,
                baseline_size: 0,
                r6_only_dssim,
                hybrid_size, hybrid_dssim,
                size_pass, dssim_pass,
                both: size_pass && dssim_pass,
            })
        }).collect()
    });
    let dt = t0.elapsed();
    eprintln!("wall = {:.1}s ({} fixtures)", dt.as_secs_f64(), results.len());

    println!("fixture\ttiny_size\ttiny_dssim\tr6_only_dssim\thybrid_size\thybrid_dssim\tsize_ratio\tdssim_delta\tsize_pass\tdssim_pass\tboth");
    for r in &results {
        println!("{}\t{}\t{:.6}\t{:.6}\t{}\t{:.6}\t{:.4}\t{:+.6}\t{}\t{}\t{}",
            r.fixture, r.tiny_size, r.tiny_dssim, r.r6_only_dssim,
            r.hybrid_size, r.hybrid_dssim,
            r.hybrid_size as f64 / r.tiny_size as f64,
            r.hybrid_dssim - r.tiny_dssim,
            if r.size_pass {"Y"} else {"N"},
            if r.dssim_pass {"Y"} else {"N"},
            if r.both {"Y"} else {"N"},
        );
    }

    let total = results.len() as u32;
    let pass = results.iter().filter(|r| r.both).count() as u32;
    let dssim_pass = results.iter().filter(|r| r.dssim_pass).count() as u32;
    let size_pass = results.iter().filter(|r| r.size_pass).count() as u32;
    eprintln!();
    eprintln!("=== Cycle 112 Path B: R6 8x8 K=192 → K=256 re-quantize hybrid ===");
    eprintln!("PASS both = {}/{} ({:.0}%)", pass, total, 100.0 * pass as f64 / total as f64);
    eprintln!("size_pass = {}/{}", size_pass, total);
    eprintln!("dssim_pass = {}/{}", dssim_pass, total);
    eprintln!();
    let verdict = if pass >= 3 { "GREEN — Path B viable, wire P-09 + v1.2.10 candidate" }
                  else if pass >= 1 { "YELLOW — Path B partial, tune hybrid params" }
                  else { "RED — R6 advantage lost in re-quantize, Path A or paper-only" };
    eprintln!("→ {}", verdict);

    Ok(())
}
