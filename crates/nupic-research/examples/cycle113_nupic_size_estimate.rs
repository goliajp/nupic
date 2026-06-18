//! Cycle 113 — `.nupic` minimal container size estimate.
//!
//! Cycle 112 RED at strict DSSIM gate: R6 → K=256 PNG re-quantize loses
//! the R6 DSSIM headroom because PNG palette caps at 256. Cycle 111
//! showed R6-only reconstruction passes strict DSSIM 6/6 with margins
//! -0.00072 to -0.00825. The only strict-gate ship path is a
//! tile-aware container (`.nupic` format) that ships R6 tile palettes
//! + indexes directly.
//!
//! This spike estimates byte size of a minimal `.nupic` format without
//! implementing full encoder/decoder, just to check feasibility:
//!
//! Layout:
//!   header(20B): magic(5B) + width(u32) + height(u32) + tile_n(u32)
//!   per-tile(64 tiles for 8×8):
//!     K(u8) + palette(K × 4 RGBA bytes)
//!   global tile index stream: concat all per-tile indexes (u8 each),
//!     then zlib-compress the entire concat
//!
//! decision: ≥ 3/6 fixtures with `.nupic` size ≤ 0.80× tiny → Cycle 114
//! writes full encoder + decoder + round-trip lossless validation.

use image::ImageReader;
use imagequant::RGBA;
use rayon::prelude::*;

use nupic_research::bench::{bench_pool, workspace_root};

fn quantize_tile_palette_indexes(rgba: &[u8], w: usize, h: usize, k: u32) -> (Vec<u8>, Vec<u8>) {
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
    let mut pal_bytes = Vec::with_capacity(palette.len() * 4);
    for p in &palette {
        pal_bytes.extend_from_slice(&[p.r, p.g, p.b, p.a]);
    }
    (pal_bytes, indexes)
}

fn zlib_compress(bytes: &[u8]) -> Vec<u8> {
    // Cycle 114: env-toggle zopfli for stronger entropy coding.
    // CYCLE114_ZOPFLI=1 → use zopfli iterations=15 (~3-8% tighter than
    // zlib best, ~10-30× wall — acceptable for size budget check).
    let use_zopfli = std::env::var("CYCLE114_ZOPFLI").ok().as_deref() == Some("1");
    if use_zopfli {
        let opts = zopfli::Options {
            iteration_count: std::num::NonZeroU64::new(15).unwrap(),
            ..Default::default()
        };
        let mut out = Vec::new();
        zopfli::compress(opts, zopfli::Format::Zlib, bytes, &mut out).unwrap();
        return out;
    }
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;
    let mut e = ZlibEncoder::new(Vec::new(), Compression::best());
    e.write_all(bytes).unwrap();
    e.finish().unwrap()
}

struct Out {
    fixture: String,
    tiny_size: u64,
    palette_total_bytes: usize,
    index_raw_bytes: usize,
    index_zlib_bytes: usize,
    nupic_total_bytes: usize,
    ratio_vs_tiny: f64,
    size_pass_080: bool,
}

fn main() -> anyhow::Result<()> {
    let root = workspace_root();
    let corpus = root.join("assets/png-bench/corpus-500");
    let tiny_dir = root.join("assets/png-bench/tinypng-corpus-500");

    let fixtures: &[&str] = &[
        "p115_1024x768.png",
        "p125_1920x1080.png",
        "p167_1920x1080.png",
        "p175_1920x1080.png",
        "p214_2400x1600.png",
        "p274_3840x2560.png",
    ];
    let tn: u32 = 8;
    let k: u32 = 192;

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
            let tiny_size = std::fs::metadata(&tiny).ok()?.len();

            // tile loop: collect per-tile palette bytes + index bytes
            let mut palette_bytes: Vec<u8> = Vec::new(); // K + palette bytes per tile concat
            let mut index_raw: Vec<u8> = Vec::new();    // raw indexes concat
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
                        tile.extend_from_slice(&raw[off..off + (tw * 4) as usize]);
                    }
                    let (pal, idx) = quantize_tile_palette_indexes(&tile, tw as usize, th as usize, k);
                    // tile palette format: 1 byte K_used + palette bytes
                    let k_used = (pal.len() / 4) as u8;
                    palette_bytes.push(k_used);
                    palette_bytes.extend_from_slice(&pal);
                    index_raw.extend_from_slice(&idx);
                    xs += tw;
                }
                ys += th;
            }

            // zlib-compress the global index stream
            let index_zlib = zlib_compress(&index_raw);

            // total .nupic bytes
            let header_bytes = 5 + 4 + 4 + 4; // magic+w+h+tile_n = 17
            let nupic_total = header_bytes + palette_bytes.len() + index_zlib.len();
            let ratio = nupic_total as f64 / tiny_size as f64;
            let size_pass = ratio <= 0.80;

            Some(Out {
                fixture: name.to_string(),
                tiny_size,
                palette_total_bytes: palette_bytes.len(),
                index_raw_bytes: index_raw.len(),
                index_zlib_bytes: index_zlib.len(),
                nupic_total_bytes: nupic_total,
                ratio_vs_tiny: ratio,
                size_pass_080: size_pass,
            })
        }).collect()
    });
    let dt = t0.elapsed();
    eprintln!("wall = {:.1}s ({} fixtures)", dt.as_secs_f64(), results.len());

    println!("fixture\ttiny_KB\tpalette_KB\tindex_raw_KB\tindex_zlib_KB\tnupic_total_KB\tratio_vs_tiny\tsize_pass_080");
    for r in &results {
        println!("{}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.4}\t{}",
            r.fixture,
            r.tiny_size as f64 / 1024.0,
            r.palette_total_bytes as f64 / 1024.0,
            r.index_raw_bytes as f64 / 1024.0,
            r.index_zlib_bytes as f64 / 1024.0,
            r.nupic_total_bytes as f64 / 1024.0,
            r.ratio_vs_tiny,
            if r.size_pass_080 { "Y" } else { "N" },
        );
    }

    let total = results.len() as u32;
    let pass = results.iter().filter(|r| r.size_pass_080).count() as u32;
    let mean_ratio: f64 = results.iter().map(|r| r.ratio_vs_tiny).sum::<f64>() / total as f64;
    eprintln!();
    eprintln!("=== Cycle 113 .nupic size estimate (8x8 tile × K=192, zlib-best per global index stream) ===");
    eprintln!("PASS (size ≤ 0.80x tiny): {}/{}", pass, total);
    eprintln!("mean ratio vs tiny: {:.4}x", mean_ratio);
    eprintln!();
    let verdict = if pass >= 3 {
        "GREEN — .nupic format feasible, Cycle 114 writes full encoder/decoder"
    } else if pass >= 1 {
        "YELLOW — .nupic partially feasible, tune palette sharing / entropy coder"
    } else {
        "RED — .nupic minimal format larger than 0.80x tiny on all 6, paper writeup only"
    };
    eprintln!("→ {}", verdict);

    Ok(())
}
