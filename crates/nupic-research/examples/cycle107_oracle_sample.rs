//! Cycle 107 — oracle K×d sweep on same stratified sample (100 fixtures).
//!
//! Per-fixture best-of K∈{96,128,160,192,224,256} × d∈{0,0.3,0.6} × p=6
//! to gauge ROUTING TABLE upper bound. If oracle PASS on this sample
//! projects to GREEN (35% cohort), R4 routing is alive (Cycle 108 builds
//! input-feature classifier). If projects ≤ YELLOW even with oracle, R4
//! routing is dead — must switch to R6 multi-tile or lossless fallback.
//!
//! rayon-parallel, target ≤ 3 min wall.

use std::path::{Path, PathBuf};

use image::ImageReader;
use rayon::prelude::*;

use nupic_core::metrics;
use nupic_core::Image;
use nupic_quantize::{quantize_indexed_png, QuantizeOpts};

fn dssim_of(reference: &Image, png_bytes: &[u8]) -> anyhow::Result<f64> {
    let d = Image::decode(png_bytes)?;
    Ok(metrics::dssim(reference, &d)?)
}

fn dssim_of_path(reference: &Image, p: &Path) -> anyhow::Result<f64> {
    let d = Image::open(p)?;
    Ok(metrics::dssim(reference, &d)?)
}

fn quantize(raw: &[u8], w: u32, h: u32, k: usize, d: f32, p: u8) -> Vec<u8> {
    let mut opts = QuantizeOpts::default();
    opts.n_colors = k;
    opts.dither_strength = d;
    opts.oxipng_preset = p;
    opts.strip_metadata = true;
    quantize_indexed_png(raw, w, h, opts).expect("quantize")
}

#[derive(Clone)]
struct Entry { name: String, pile: String }

fn read_classification(p: &Path) -> anyhow::Result<Vec<Entry>> {
    let txt = std::fs::read_to_string(p)?;
    let mut out = Vec::new();
    for (i, line) in txt.lines().enumerate() {
        if i == 0 || line.trim().is_empty() { continue; }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 3 { continue; }
        out.push(Entry { name: cols[0].to_string(), pile: cols[2].to_string() });
    }
    Ok(out)
}

fn stratify(entries: Vec<Entry>, per_pile: usize) -> Vec<Entry> {
    use std::collections::BTreeMap;
    let mut by_pile: BTreeMap<String, Vec<Entry>> = BTreeMap::new();
    for e in entries { by_pile.entry(e.pile.clone()).or_default().push(e); }
    let mut out = Vec::new();
    for (_p, rows) in by_pile {
        let take = per_pile.min(rows.len());
        if take == rows.len() { out.extend(rows); }
        else {
            let stride = rows.len() as f64 / take as f64;
            for i in 0..take {
                let idx = (i as f64 * stride) as usize;
                out.push(rows[idx].clone());
            }
        }
    }
    out
}

#[derive(Clone)]
struct Best {
    name: String,
    pile: String,
    tiny_size: u64,
    tiny_dssim: f64,
    best_k: usize,
    best_d: f32,
    best_size: u64,
    best_dssim: f64,
    pass: bool,
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();
    let class_tsv = root.join("assets/png-bench/cycle107/pile_classification.tsv");
    let corpus = root.join("assets/png-bench/corpus-500");
    let tiny_dir = root.join("assets/png-bench/tinypng-corpus-500");

    let all = read_classification(&class_tsv)?;
    let sample = stratify(all, 13); // 13 × 4 piles ≈ 50 fixtures
    eprintln!("oracle sweep on {} stratified-sample fixtures (capped 4-core)", sample.len());

    let ks: [usize; 6] = [96, 128, 160, 192, 224, 256];
    let ds: [f32; 2] = [0.0, 0.3];
    let p: u8 = 6;

    // Cap rayon to 4 threads so user's machine stays responsive
    // ([[feedback-no-long-sweeps-in-workflow]]).
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .expect("thread pool");

    let t0 = std::time::Instant::now();
    let results: Vec<Best> = pool.install(|| {
        sample.par_iter().filter_map(|e| {
        let orig = corpus.join(&e.name);
        let tiny = tiny_dir.join(&e.name);
        if !orig.exists() || !tiny.exists() { return None; }
        let img = ImageReader::open(&orig).ok()?.with_guessed_format().ok()?.decode().ok()?;
        let rgba = img.to_rgba8();
        let (wi, he) = (rgba.width(), rgba.height());
        let raw = rgba.into_raw();
        let tiny_size = std::fs::metadata(&tiny).ok()?.len();
        let reference = Image::open(&orig).ok()?;
        let tiny_dssim = dssim_of_path(&reference, &tiny).ok()?;
        let cap = (tiny_size as f64 * 0.80) as u64;

        let mut best: Option<(usize, f32, u64, f64)> = None;
        for &k in &ks {
            for &d in &ds {
                let bytes = quantize(&raw, wi, he, k, d, p);
                let sz = bytes.len() as u64;
                let ds_v = match dssim_of(&reference, &bytes) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let pass = sz <= cap && ds_v <= tiny_dssim;
                if pass {
                    match best {
                        None => best = Some((k, d, sz, ds_v)),
                        Some((_,_,bs,_)) if sz < bs => best = Some((k, d, sz, ds_v)),
                        _ => {}
                    }
                }
            }
        }
        let pass = best.is_some();
        let (bk, bd, bsz, bds) = best.unwrap_or((0, 0.0, 0, 0.0));
        Some(Best {
            name: e.name.clone(), pile: e.pile.clone(),
            tiny_size, tiny_dssim,
            best_k: bk, best_d: bd, best_size: bsz, best_dssim: bds,
            pass,
        })
        }).collect()
    });
    let dt = t0.elapsed();
    eprintln!("done in {:.1}s ({:.2}s/fixture wall, {} ops total)",
        dt.as_secs_f64(), dt.as_secs_f64()/results.len() as f64,
        results.len() * ks.len() * ds.len());

    println!("fixture\tpile\ttiny_size\ttiny_dssim\tbest_K\tbest_d\tbest_size\tbest_dssim\tratio\tpass");
    for r in &results {
        if r.pass {
            println!("{}\t{}\t{}\t{:.6}\t{}\t{:.1}\t{}\t{:.6}\t{:.4}\tY",
                r.name, r.pile, r.tiny_size, r.tiny_dssim, r.best_k, r.best_d,
                r.best_size, r.best_dssim, r.best_size as f64 / r.tiny_size as f64);
        } else {
            println!("{}\t{}\t{}\t{:.6}\t-\t-\t-\t-\t-\tN", r.name, r.pile, r.tiny_size, r.tiny_dssim);
        }
    }

    // Summary by pile
    use std::collections::BTreeMap;
    let mut by_pile: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    for r in &results {
        let e = by_pile.entry(r.pile.clone()).or_insert((0, 0));
        e.0 += 1;
        if r.pass { e.1 += 1; }
    }
    let total = results.len() as u32;
    let pass = results.iter().filter(|r| r.pass).count() as u32;

    // K-winners histogram for PASS rows
    let mut k_hist: BTreeMap<usize, u32> = BTreeMap::new();
    let mut d_hist: BTreeMap<String, u32> = BTreeMap::new();
    for r in &results {
        if r.pass {
            *k_hist.entry(r.best_k).or_insert(0) += 1;
            *d_hist.entry(format!("{:.1}", r.best_d)).or_insert(0) += 1;
        }
    }

    eprintln!();
    eprintln!("=== Cycle 107 oracle (K×d×p=6) on 100-stratified-sample ===");
    eprintln!("ORACLE PASS = {}/{} ({:.1}%)", pass, total, 100.0 * pass as f64 / total as f64);
    eprintln!();
    eprintln!("Per-pile oracle PASS:");
    for (pile, (n, pass)) in &by_pile {
        eprintln!("  {:<6} n={:>3} oracle_pass={:>3} ({:>5.1}%)", pile, n, pass, 100.0 * *pass as f64 / *n as f64);
    }
    eprintln!();
    eprintln!("Oracle K histogram (winners only): {:?}", k_hist);
    eprintln!("Oracle d histogram (winners only): {:?}", d_hist);

    Ok(())
}
