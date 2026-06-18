//! Cycle 107 — single-config (K=224 d=0.3 p=6) routing test on a
//! stratified sample (~80 fixtures), rayon-parallel.
//!
//! Wall-clock target: ≤ 2 min. Sample stratified by pile classification:
//! 10 PASS + 20 PileA + 20 PileB + 20 PileC = ~70 fixtures. Diagnostic
//! enough to gate-test single-config routing without 30-min full-corpus
//! sweep ([[feedback-no-long-sweeps-in-workflow]]).

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
struct Entry {
    name: String,
    pile: String,
}

fn read_classification(p: &Path) -> anyhow::Result<Vec<Entry>> {
    let txt = std::fs::read_to_string(p)?;
    let mut out = Vec::new();
    for (i, line) in txt.lines().enumerate() {
        if i == 0 || line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 3 {
            continue;
        }
        out.push(Entry {
            name: cols[0].to_string(),
            pile: cols[2].to_string(),
        });
    }
    Ok(out)
}

/// Deterministic stratified sample: take `per_pile` from each pile by
/// stride sampling (index 0, n/per_pile, 2n/per_pile, …).
fn stratify(entries: Vec<Entry>, per_pile: usize) -> Vec<Entry> {
    use std::collections::BTreeMap;
    let mut by_pile: BTreeMap<String, Vec<Entry>> = BTreeMap::new();
    for e in entries {
        by_pile.entry(e.pile.clone()).or_default().push(e);
    }
    let mut out = Vec::new();
    for (_p, rows) in by_pile {
        let take = per_pile.min(rows.len());
        if take == rows.len() {
            out.extend(rows);
        } else {
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
struct Row {
    name: String,
    pile: String,
    c_size: u64,
    c_dssim: f64,
    tiny_size: u64,
    tiny_dssim: f64,
    size_pass: bool,
    dssim_pass: bool,
    both_pass: bool,
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();
    let class_tsv = root.join("assets/png-bench/cycle107/pile_classification.tsv");
    let corpus = root.join("assets/png-bench/corpus-500");
    let tiny_dir = root.join("assets/png-bench/tinypng-corpus-500");

    let all = read_classification(&class_tsv)?;
    let sample = stratify(all, 25); // 25 × 4 piles = up to 100, but pile sizes cap PASS=106 PileA=307 PileB=40 PileC=53
    eprintln!("processing {} stratified-sample fixtures", sample.len());

    let k: usize = 224;
    let d: f32 = 0.3;
    let p: u8 = 6;

    let t0 = std::time::Instant::now();
    let rows: Vec<Row> = sample
        .par_iter()
        .filter_map(|e| {
            let orig = corpus.join(&e.name);
            let tiny = tiny_dir.join(&e.name);
            if !orig.exists() || !tiny.exists() {
                eprintln!("MISS {}", e.name);
                return None;
            }
            let img = ImageReader::open(&orig).ok()?.with_guessed_format().ok()?.decode().ok()?;
            let rgba = img.to_rgba8();
            let (wi, he) = (rgba.width(), rgba.height());
            let raw = rgba.into_raw();
            let tiny_size = std::fs::metadata(&tiny).ok()?.len();
            let reference = Image::open(&orig).ok()?;
            let tiny_dssim = dssim_of_path(&reference, &tiny).ok()?;
            let bytes = quantize(&raw, wi, he, k, d, p);
            let c_size = bytes.len() as u64;
            let c_dssim = dssim_of(&reference, &bytes).ok()?;
            let cap = (tiny_size as f64 * 0.80) as u64;
            let size_pass = c_size <= cap;
            let dssim_pass = c_dssim <= tiny_dssim;
            let both_pass = size_pass && dssim_pass;
            Some(Row {
                name: e.name.clone(),
                pile: e.pile.clone(),
                c_size,
                c_dssim,
                tiny_size,
                tiny_dssim,
                size_pass,
                dssim_pass,
                both_pass,
            })
        })
        .collect();
    let dt = t0.elapsed();
    eprintln!("done in {:.1}s ({:.1}s/fixture wall)", dt.as_secs_f64(), dt.as_secs_f64() / rows.len() as f64);

    println!("fixture\tpile\tc107_size\tc107_dssim\ttiny_size\ttiny_dssim\tsize_ratio\tdssim_delta\tsize_pass\tdssim_pass\tboth_pass");
    for r in &rows {
        println!(
            "{}\t{}\t{}\t{:.6}\t{}\t{:.6}\t{:.4}\t{:+.6}\t{}\t{}\t{}",
            r.name,
            r.pile,
            r.c_size,
            r.c_dssim,
            r.tiny_size,
            r.tiny_dssim,
            r.c_size as f64 / r.tiny_size as f64,
            r.c_dssim - r.tiny_dssim,
            if r.size_pass { "Y" } else { "N" },
            if r.dssim_pass { "Y" } else { "N" },
            if r.both_pass { "Y" } else { "N" },
        );
    }

    // Summary by pile.
    use std::collections::BTreeMap;
    let mut by_pile: BTreeMap<String, (u32, u32, u32, u32)> = BTreeMap::new(); // (n, pass, size_pass, dssim_pass)
    for r in &rows {
        let e = by_pile.entry(r.pile.clone()).or_insert((0, 0, 0, 0));
        e.0 += 1;
        if r.both_pass { e.1 += 1; }
        if r.size_pass { e.2 += 1; }
        if r.dssim_pass { e.3 += 1; }
    }

    eprintln!();
    eprintln!("=== Cycle 107 single-config (K=224 d=0.3 p=6) stratified-sample summary ===");
    let total = rows.len() as u32;
    let pass = rows.iter().filter(|r| r.both_pass).count() as u32;
    let size_pass = rows.iter().filter(|r| r.size_pass).count() as u32;
    let dssim_pass = rows.iter().filter(|r| r.dssim_pass).count() as u32;
    eprintln!("PASS both  = {}/{} ({:.1}%)", pass, total, 100.0 * pass as f64 / total as f64);
    eprintln!("size pass  = {}/{} ({:.1}%)", size_pass, total, 100.0 * size_pass as f64 / total as f64);
    eprintln!("dssim pass = {}/{} ({:.1}%)", dssim_pass, total, 100.0 * dssim_pass as f64 / total as f64);
    eprintln!();
    eprintln!("Per-pile:");
    for (pile, (n, pass, sp, dp)) in &by_pile {
        eprintln!(
            "  {:<6} n={:>3} pass={:>3} ({:>5.1}%)   size={:>3} ({:>5.1}%)   dssim={:>3} ({:>5.1}%)",
            pile, n, pass, 100.0 * *pass as f64 / *n as f64,
            sp, 100.0 * *sp as f64 / *n as f64,
            dp, 100.0 * *dp as f64 / *n as f64
        );
    }

    Ok(())
}
