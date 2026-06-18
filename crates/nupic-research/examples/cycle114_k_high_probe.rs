//! Cycle 114 — global K-high probe.
//!
//! Cycle 113 found .nupic minimal palette overhead is 47 KB fixed
//! (64 tiles × 192 colors × 4 bytes), and index zlib dominates total
//! .nupic size on small images. Before going to palette-sharing
//! engineering, ask the simpler question: **does a single global
//! K=512 / K=1024 / K=4096 imagequant pass DSSIM on the 6 infeasible
//! fixtures?**
//!
//! If yes, we don't need R6 multi-tile at all — just K>256 in a
//! tile-aware container. If no, R6 is the only path and palette
//! sharing engineering follows.

use image::ImageReader;
use imagequant::RGBA;
use rayon::prelude::*;

use nupic_core::metrics;
use nupic_core::Image;
use nupic_research::bench::{bench_pool, workspace_root};

fn quantize_full(rgba: &[u8], w: usize, h: usize, k: u32) -> Vec<u8> {
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

fn dssim_of_rgba(reference: &Image, rgba: &[u8], w: u32, h: u32) -> anyhow::Result<f64> {
    let buf = image::RgbaImage::from_vec(w, h, rgba.to_vec())
        .ok_or_else(|| anyhow::anyhow!("from_vec"))?;
    let dyn_img = image::DynamicImage::ImageRgba8(buf);
    let mut bytes = Vec::new();
    use std::io::Cursor;
    dyn_img.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)?;
    Ok(metrics::dssim(reference, &Image::decode(&bytes)?)?)
}

fn main() -> anyhow::Result<()> {
    let root = workspace_root();
    let corpus = root.join("assets/png-bench/corpus-500");

    let fixtures: &[(&str, f64)] = &[
        ("p115_1024x768.png", 0.001970),
        ("p125_1920x1080.png", 0.009766),
        ("p167_1920x1080.png", 0.000880),
        ("p175_1920x1080.png", 0.001966),
        ("p214_2400x1600.png", 0.002845),
        ("p274_3840x2560.png", 0.003084),
    ];
    let ks: &[u32] = &[256, 512, 1024, 4096];

    let pool = bench_pool()?;
    let t0 = std::time::Instant::now();

    let mut jobs = Vec::new();
    for &(name, tdss) in fixtures {
        for &k in ks {
            jobs.push((name, tdss, k));
        }
    }
    let results: Vec<_> = pool.install(|| {
        jobs.par_iter().filter_map(|(name, tdss, k)| {
            let orig = corpus.join(name);
            let img = ImageReader::open(&orig).ok()?.with_guessed_format().ok()?.decode().ok()?;
            let rgba = img.to_rgba8();
            let (w, h) = (rgba.width(), rgba.height());
            let raw = rgba.into_raw();
            let q = quantize_full(&raw, w as usize, h as usize, *k);
            let reference = Image::open(&orig).ok()?;
            let d = dssim_of_rgba(&reference, &q, w, h).ok()?;
            Some((name.to_string(), *k, d, *tdss, d <= *tdss))
        }).collect()
    });
    let dt = t0.elapsed();
    eprintln!("wall = {:.1}s ({} jobs)", dt.as_secs_f64(), results.len());

    println!("fixture\tK\tdssim\ttiny_dssim\tdssim_pass");
    for (fx, k, d, tdss, p) in &results {
        println!("{}\t{}\t{:.6}\t{:.6}\t{}", fx, k, d, tdss, if *p {"Y"} else {"N"});
    }

    eprintln!();
    eprintln!("=== Cycle 114 global K-high probe ===");
    use std::collections::BTreeMap;
    let mut by_fx: BTreeMap<String, (f64, u32, bool)> = BTreeMap::new();
    for (fx, k, d, _, p) in &results {
        let e = by_fx.entry(fx.clone()).or_insert((f64::MAX, 0, false));
        if *d < e.0 { *e = (*d, *k, *p); }
    }
    let mut passed = 0;
    for (fx, (best_d, best_k, p)) in &by_fx {
        let tdss = fixtures.iter().find(|(n,_)| n == fx).unwrap().1;
        eprintln!("  {:<28} best_K={} best_dssim={:.6} tiny={:.6} margin={:+.6} pass={}",
            fx, best_k, best_d, tdss, best_d - tdss, if *p {"Y"} else {"N"});
        if *p { passed += 1; }
    }
    eprintln!();
    eprintln!("DSSIM PASS: {}/{}", passed, by_fx.len());
    let verdict = if passed == 6 {
        "GREEN — global K=high passes 6/6, .nupic only needs K>256 single-palette container (simpler than R6)"
    } else if passed >= 3 {
        "YELLOW — partial; some fixtures still need R6 spatial-aware"
    } else {
        "RED — global K-high doesn't break ceiling, R6 multi-tile is fundamental"
    };
    eprintln!("→ {}", verdict);

    Ok(())
}
