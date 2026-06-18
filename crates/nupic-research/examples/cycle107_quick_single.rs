//! Cycle 107 — quick single-config bench using `bench` helpers.
//!
//! 24-fixture stratified sample(8 per pile) + 4-core capped rayon +
//! pre-loaded TinyPNG baselines from `corpus-500-dssim.tsv` (no
//! redundant DSSIM on tinypng outputs). Target wall ≤ 30 s.

use std::path::PathBuf;

use image::ImageReader;
use rayon::prelude::*;

use nupic_core::metrics;
use nupic_core::Image;
use nupic_quantize::{quantize_indexed_png, QuantizeOpts};
use nupic_research::bench::{
    bench_pool, load_corpus_500_with_baseline, pile_sample_24, workspace_root, Fixture,
};

fn quantize(raw: &[u8], w: u32, h: u32, k: usize, d: f32, p: u8) -> Vec<u8> {
    let mut opts = QuantizeOpts::default();
    opts.n_colors = k;
    opts.dither_strength = d;
    opts.oxipng_preset = p;
    opts.strip_metadata = true;
    quantize_indexed_png(raw, w, h, opts).expect("quantize")
}

fn dssim_of(reference: &Image, png: &[u8]) -> anyhow::Result<f64> {
    let d = Image::decode(png)?;
    Ok(metrics::dssim(reference, &d)?)
}

struct Out {
    fx: Fixture,
    c_size: u64,
    c_dssim: f64,
    size_pass: bool,
    dssim_pass: bool,
    both: bool,
}

fn main() -> anyhow::Result<()> {
    let root = workspace_root();
    let all = load_corpus_500_with_baseline(&root)?;
    let sample = pile_sample_24(&all);
    eprintln!("sample: {} fixtures (pile stratified, 8 each)", sample.len());

    let k: usize = 224;
    let d: f32 = 0.3;
    let p: u8 = 6;

    let corpus = root.join("assets/png-bench/corpus-500");
    let pool = bench_pool()?;
    let t0 = std::time::Instant::now();

    let rows: Vec<Out> = pool.install(|| {
        sample
            .par_iter()
            .filter_map(|fx| {
                let orig = corpus.join(&fx.name);
                let img = ImageReader::open(&orig).ok()?.with_guessed_format().ok()?.decode().ok()?;
                let rgba = img.to_rgba8();
                let (wi, he) = (rgba.width(), rgba.height());
                let raw = rgba.into_raw();
                let reference = Image::open(&orig).ok()?;
                let bytes = quantize(&raw, wi, he, k, d, p);
                let c_size = bytes.len() as u64;
                let c_dssim = dssim_of(&reference, &bytes).ok()?;
                let size_pass = c_size <= fx.size_cap();
                let dssim_pass = c_dssim <= fx.tiny_dssim;
                let both = size_pass && dssim_pass;
                Some(Out {
                    fx: fx.clone(),
                    c_size,
                    c_dssim,
                    size_pass,
                    dssim_pass,
                    both,
                })
            })
            .collect()
    });
    let dt = t0.elapsed();
    eprintln!(
        "wall = {:.1}s ({:.2}s/fixture, {} cores via bench_pool)",
        dt.as_secs_f64(),
        dt.as_secs_f64() / rows.len() as f64,
        std::env::var("NUPIC_BENCH_THREADS").unwrap_or_else(|_| "4".into()),
    );

    println!("fixture\tpile\tc_size\tc_dssim\ttiny_size\ttiny_dssim\tratio\tdss_delta\tsize_pass\tdssim_pass\tboth");
    for r in &rows {
        println!(
            "{}\t{}\t{}\t{:.6}\t{}\t{:.6}\t{:.4}\t{:+.6}\t{}\t{}\t{}",
            r.fx.name,
            r.fx.pile,
            r.c_size,
            r.c_dssim,
            r.fx.tiny_size,
            r.fx.tiny_dssim,
            r.c_size as f64 / r.fx.tiny_size as f64,
            r.c_dssim - r.fx.tiny_dssim,
            if r.size_pass { "Y" } else { "N" },
            if r.dssim_pass { "Y" } else { "N" },
            if r.both { "Y" } else { "N" },
        );
    }

    use std::collections::BTreeMap;
    let mut by_pile: BTreeMap<String, (u32, u32, u32, u32)> = BTreeMap::new();
    for r in &rows {
        let e = by_pile.entry(r.fx.pile.clone()).or_insert((0, 0, 0, 0));
        e.0 += 1;
        if r.both { e.1 += 1; }
        if r.size_pass { e.2 += 1; }
        if r.dssim_pass { e.3 += 1; }
    }
    let n = rows.len() as u32;
    let pass = rows.iter().filter(|r| r.both).count() as u32;
    eprintln!();
    eprintln!("=== K={} d={:.1} p={} on {}-pile-stratified sample ===", k, d, p, n);
    eprintln!("PASS {}/{} ({:.1}%)", pass, n, 100.0 * pass as f64 / n as f64);
    for (pile, (n, pass, sp, dp)) in &by_pile {
        eprintln!(
            "  {:<6} n={:>3} pass={:>3} ({:>5.1}%)  size={:>3}  dssim={:>3}",
            pile,
            n,
            pass,
            100.0 * *pass as f64 / *n as f64,
            sp,
            dp
        );
    }

    Ok(())
}
