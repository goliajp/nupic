//! Cycle 107 — single-config routing test over full corpus-500.
//!
//! Cycle 106 winning slot K=224 d=0.3 p=6 captured 7/23 Pile A winners
//! (35% of the Pile A wins) as a single-config. This run tests whether
//! that **same single config**, applied to ALL 506 corpus-500 fixtures,
//! could function as the v1.2.9 production-default routing — and what
//! PASS rate (size ≤ 0.80× tiny ∧ DSSIM ≤ tiny_dssim) it achieves.
//!
//! This is the simplest possible "production wiring" test: no per-image
//! features, no decision tree, no feature classifier — just push K=224
//! d=0.3 p=6 everywhere and see.
//!
//! Output TSV with per-fixture: (fixture, pile, c107_size, c107_dssim,
//! tiny_size, tiny_dssim, ratio, dssim_delta, size_pass, dssim_pass,
//! both_pass).

use std::path::{Path, PathBuf};

use image::ImageReader;

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

fn read_classification(p: &Path) -> anyhow::Result<Vec<(String, String)>> {
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
        out.push((cols[0].to_string(), cols[2].to_string()));
    }
    Ok(out)
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

    let fixtures = read_classification(&class_tsv)?;
    eprintln!("processing {} fixtures with K=224 d=0.3 p=6", fixtures.len());

    let k: usize = 224;
    let d: f32 = 0.3;
    let p: u8 = 6;

    println!("fixture\tpile\tc107_size\tc107_dssim\ttiny_size\ttiny_dssim\tsize_ratio\tdssim_delta\tsize_pass\tdssim_pass\tboth_pass\tbaseline_pile");

    let mut new_pass = 0u32;
    let mut new_size_pass = 0u32;
    let mut new_dssim_pass = 0u32;
    let mut total_c107: u64 = 0;
    let mut total_tiny: u64 = 0;
    let mut by_pile = std::collections::BTreeMap::<String, (u32, u32)>::new(); // (n, n_pass)

    for (i, (name, pile)) in fixtures.iter().enumerate() {
        let orig = corpus.join(name);
        let tiny = tiny_dir.join(name);
        if !orig.exists() || !tiny.exists() {
            eprintln!("MISS {}", name);
            continue;
        }
        if i % 50 == 0 {
            eprintln!("  [{}/{}] {}", i, fixtures.len(), name);
        }
        let img = ImageReader::open(&orig)?.with_guessed_format()?.decode()?;
        let rgba = img.to_rgba8();
        let (wi, he) = (rgba.width(), rgba.height());
        let raw = rgba.into_raw();
        let tiny_size = std::fs::metadata(&tiny)?.len();
        let reference = Image::open(&orig)?;
        let tiny_dssim = dssim_of_path(&reference, &tiny)?;
        let cap = (tiny_size as f64 * 0.80) as u64;

        let bytes = quantize(&raw, wi, he, k, d, p);
        let c_size = bytes.len() as u64;
        let c_dssim = match dssim_of(&reference, &bytes) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("  dssim fail {}: {}", name, e);
                continue;
            }
        };

        let s_ok = c_size <= cap;
        let q_ok = c_dssim <= tiny_dssim;
        let both = s_ok && q_ok;

        if s_ok {
            new_size_pass += 1;
        }
        if q_ok {
            new_dssim_pass += 1;
        }
        if both {
            new_pass += 1;
        }
        total_c107 += c_size;
        total_tiny += tiny_size;
        let entry = by_pile.entry(pile.clone()).or_insert((0, 0));
        entry.0 += 1;
        if both {
            entry.1 += 1;
        }

        println!(
            "{}\t{}\t{}\t{:.6}\t{}\t{:.6}\t{:.4}\t{:+.6}\t{}\t{}\t{}\t{}",
            name,
            pile,
            c_size,
            c_dssim,
            tiny_size,
            tiny_dssim,
            c_size as f64 / tiny_size as f64,
            c_dssim - tiny_dssim,
            if s_ok { "Y" } else { "N" },
            if q_ok { "Y" } else { "N" },
            if both { "Y" } else { "N" },
            pile,
        );
    }

    eprintln!();
    eprintln!("=== Cycle 107 single-config (K=224 d=0.3 p=6) full-corpus summary ===");
    eprintln!(
        "PASS both     = {}/{} ({:.1}%)",
        new_pass,
        fixtures.len(),
        100.0 * new_pass as f64 / fixtures.len() as f64
    );
    eprintln!(
        "size pass     = {}/{} ({:.1}%)",
        new_size_pass,
        fixtures.len(),
        100.0 * new_size_pass as f64 / fixtures.len() as f64
    );
    eprintln!(
        "dssim pass    = {}/{} ({:.1}%)",
        new_dssim_pass,
        fixtures.len(),
        100.0 * new_dssim_pass as f64 / fixtures.len() as f64
    );
    eprintln!(
        "cohort total  : c107={} B  tiny={} B  ratio={:.4}x",
        total_c107,
        total_tiny,
        if total_tiny > 0 {
            total_c107 as f64 / total_tiny as f64
        } else {
            0.0
        },
    );
    eprintln!();
    eprintln!("Per-pile PASS:");
    for (pile, (n, pass)) in &by_pile {
        eprintln!(
            "  {:<6} {:>4} / {:>4} ({:.1}%)",
            pile,
            pass,
            n,
            100.0 * *pass as f64 / *n as f64
        );
    }

    Ok(())
}
