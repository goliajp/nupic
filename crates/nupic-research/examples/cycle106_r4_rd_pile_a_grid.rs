//! Cycle 106 R4 — Pile A FULL GRID dump (diagnostic, follow-up to
//! `cycle106_r4_rd_pile_a`).
//!
//! First pass showed 10/31 pass with K∈{64,96,128,192}×d∈{0,.3,.6}×p∈{3,6},
//! but the "best" report for failures only picked min-size — hiding the
//! DSSIM-feasible Pareto floor. Also wins clustered at K=192 — try K=256.
//!
//! This run emits per-(fixture, K, d, p) row to grid TSV so we can read the
//! Pareto curve directly, plus a best-passing summary TSV.

use std::path::{Path, PathBuf};

use image::ImageReader;

use nupic_core::metrics;
use nupic_core::Image;
use nupic_quantize::{quantize_indexed_png, QuantizeOpts};

fn read_pile_a(tsv: &Path) -> anyhow::Result<Vec<String>> {
    let text = std::fs::read_to_string(tsv)?;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if i == 0 || line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.is_empty() {
            continue;
        }
        out.push(cols[0].to_string());
    }
    Ok(out)
}

fn dssim_of(reference: &Image, png_bytes: &[u8]) -> anyhow::Result<f64> {
    let distorted = Image::decode(png_bytes)?;
    Ok(metrics::dssim(reference, &distorted)?)
}

fn dssim_of_path(reference: &Image, path: &Path) -> anyhow::Result<f64> {
    let distorted = Image::open(path)?;
    Ok(metrics::dssim(reference, &distorted)?)
}

fn quantize(raw: &[u8], w: u32, h: u32, k: usize, d: f32, p: u8) -> Vec<u8> {
    let mut opts = QuantizeOpts::default();
    opts.n_colors = k;
    opts.dither_strength = d;
    opts.oxipng_preset = p;
    opts.strip_metadata = true;
    quantize_indexed_png(raw, w, h, opts).expect("quantize")
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();
    let pile_a = root.join("assets/png-bench/corpus-500-pile-a.tsv");
    let corpus = root.join("assets/png-bench/corpus-500");
    let tiny_dir = root.join("assets/png-bench/tinypng-corpus-500");

    let fixtures = read_pile_a(&pile_a)?;
    eprintln!("Pile A: {} fixtures", fixtures.len());

    let ks: [usize; 7] = [64, 96, 128, 160, 192, 224, 256];
    let ds: [f32; 3] = [0.0, 0.3, 0.6];
    let p: u8 = 6;

    println!("fixture\tw\th\ttiny_size\ttiny_dssim\tK\td\tp\tsize\tdssim\tsize_ratio\tdssim_delta\tsize_pass\tdssim_pass\tboth_pass");

    let mut pass_both_count = 0u32;
    let mut pass_dssim_count = 0u32;
    let mut total_tiny: u64 = 0;
    let mut total_best: u64 = 0;
    let mut win_k_hist = std::collections::BTreeMap::<usize, u32>::new();
    let mut win_d_hist = std::collections::BTreeMap::<String, u32>::new();

    for name in &fixtures {
        let orig_path = corpus.join(name);
        let tiny_path = tiny_dir.join(name);
        if !orig_path.exists() || !tiny_path.exists() {
            eprintln!("MISS {}", name);
            continue;
        }
        let img = ImageReader::open(&orig_path)?.with_guessed_format()?.decode()?;
        let r = img.to_rgba8();
        let w = r.width();
        let h = r.height();
        let raw = r.into_raw();
        let tiny_size = std::fs::metadata(&tiny_path)?.len();
        let reference = Image::open(&orig_path)?;
        let tiny_dssim = dssim_of_path(&reference, &tiny_path)?;
        let cap_size = (tiny_size as f64 * 0.80) as u64;

        let mut best_both: Option<(usize, f32, u64, f64)> = None;
        let mut best_dssim_pass: Option<(usize, f32, u64, f64)> = None;
        for &k in &ks {
            for &d in &ds {
                let bytes = quantize(&raw, w, h, k, d, p);
                let size = bytes.len() as u64;
                let dssim = match dssim_of(&reference, &bytes) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("dssim fail {} K={} d={:.1}: {}", name, k, d, e);
                        continue;
                    }
                };
                let size_pass = size <= cap_size;
                let dssim_pass = dssim <= tiny_dssim;
                let both = size_pass && dssim_pass;
                println!(
                    "{}\t{}\t{}\t{}\t{:.6}\t{}\t{:.1}\t{}\t{}\t{:.6}\t{:.4}\t{:+.6}\t{}\t{}\t{}",
                    name,
                    w,
                    h,
                    tiny_size,
                    tiny_dssim,
                    k,
                    d,
                    p,
                    size,
                    dssim,
                    size as f64 / tiny_size as f64,
                    dssim - tiny_dssim,
                    if size_pass { "Y" } else { "N" },
                    if dssim_pass { "Y" } else { "N" },
                    if both { "Y" } else { "N" },
                );
                if both {
                    match best_both {
                        None => best_both = Some((k, d, size, dssim)),
                        Some((_, _, bs, _)) if size < bs => best_both = Some((k, d, size, dssim)),
                        _ => {}
                    }
                }
                if dssim_pass {
                    match best_dssim_pass {
                        None => best_dssim_pass = Some((k, d, size, dssim)),
                        Some((_, _, bs, _)) if size < bs => {
                            best_dssim_pass = Some((k, d, size, dssim))
                        }
                        _ => {}
                    }
                }
            }
        }
        if let Some((k, d, sz, _)) = best_both {
            pass_both_count += 1;
            total_tiny += tiny_size;
            total_best += sz;
            *win_k_hist.entry(k).or_insert(0) += 1;
            *win_d_hist.entry(format!("{:.1}", d)).or_insert(0) += 1;
        }
        if best_dssim_pass.is_some() {
            pass_dssim_count += 1;
        }
    }

    eprintln!();
    eprintln!("=== Pile A summary (K∈{:?} × d∈{:?} × p={}) ===", ks, ds, p);
    eprintln!(
        "pass_both = {}/{} ({:.1}%)",
        pass_both_count,
        fixtures.len(),
        100.0 * pass_both_count as f64 / fixtures.len() as f64
    );
    eprintln!(
        "pass_dssim_only = {}/{} ({:.1}%)",
        pass_dssim_count,
        fixtures.len(),
        100.0 * pass_dssim_count as f64 / fixtures.len() as f64
    );
    eprintln!("winning K histogram: {:?}", win_k_hist);
    eprintln!("winning d histogram: {:?}", win_d_hist);
    eprintln!(
        "winning cohort: best={} B  tiny={} B  ratio={:.4}x  (over {} fixtures)",
        total_best,
        total_tiny,
        if total_tiny > 0 {
            total_best as f64 / total_tiny as f64
        } else {
            0.0
        },
        pass_both_count
    );

    Ok(())
}
