//! Cycle 106 R4 — emit per-fixture winner PNGs to
//! `assets/png-bench/nupic-corpus-500-c106-r4/` based on `pile_a_winners.tsv`.
//!
//! Also tries a zopfli postpass on the 4 "size edge" fixtures (DSSIM-feasible
//! floor 0.80-0.82×) to see if a heavier filter chain pushes them into PASS.

use std::path::{Path, PathBuf};

use image::ImageReader;
use oxipng::{indexset, Deflaters, RowFilter};

use nupic_core::metrics;
use nupic_core::Image;
use nupic_quantize::{quantize_indexed_png, QuantizeOpts};

struct Winner {
    name: String,
    k: usize,
    d: f32,
}

struct Edge {
    name: String,
    floor_k: usize,
    floor_d: f32,
}

fn read_winners(p: &Path) -> anyhow::Result<Vec<Winner>> {
    let mut out = Vec::new();
    let txt = std::fs::read_to_string(p)?;
    for line in txt.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 3 {
            continue;
        }
        out.push(Winner {
            name: cols[0].to_string(),
            k: cols[1].parse()?,
            d: cols[2].parse()?,
        });
    }
    Ok(out)
}

fn quantize(raw: &[u8], w: u32, h: u32, k: usize, d: f32, p: u8) -> Vec<u8> {
    let mut opts = QuantizeOpts::default();
    opts.n_colors = k;
    opts.dither_strength = d;
    opts.oxipng_preset = p;
    opts.strip_metadata = true;
    quantize_indexed_png(raw, w, h, opts).expect("quantize")
}

fn zopfli_refine(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut opts = oxipng::Options::from_preset(6);
    opts.deflate = Deflaters::Zopfli {
        iterations: std::num::NonZeroU8::new(30).unwrap(),
    };
    opts.filter = indexset! {
        RowFilter::None, RowFilter::Sub, RowFilter::Up, RowFilter::Average, RowFilter::Paeth,
        RowFilter::MinSum, RowFilter::Entropy, RowFilter::Bigrams, RowFilter::BigEnt,
    };
    Ok(oxipng::optimize_from_memory(bytes, &opts)?)
}

fn dssim_of(reference: &Image, png_bytes: &[u8]) -> anyhow::Result<f64> {
    let d = Image::decode(png_bytes)?;
    Ok(metrics::dssim(reference, &d)?)
}

fn dssim_of_path(reference: &Image, p: &Path) -> anyhow::Result<f64> {
    let d = Image::open(p)?;
    Ok(metrics::dssim(reference, &d)?)
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();
    let winners_tsv = root.join("assets/png-bench/cycle106-r4/pile_a_winners.tsv");
    let corpus = root.join("assets/png-bench/corpus-500");
    let tiny_dir = root.join("assets/png-bench/tinypng-corpus-500");
    let out_dir = root.join("assets/png-bench/nupic-corpus-500-c106-r4");
    std::fs::create_dir_all(&out_dir)?;

    let winners = read_winners(&winners_tsv)?;
    eprintln!("emit: {} winners", winners.len());

    for w in &winners {
        let src = corpus.join(&w.name);
        let dst = out_dir.join(&w.name);
        let img = ImageReader::open(&src)?.with_guessed_format()?.decode()?;
        let rgba = img.to_rgba8();
        let (wi, he) = (rgba.width(), rgba.height());
        let raw = rgba.into_raw();
        let bytes = quantize(&raw, wi, he, w.k, w.d, 6);
        std::fs::write(&dst, &bytes)?;
        println!("WROTE {} ({} B) K={} d={:.1}", w.name, bytes.len(), w.k, w.d);
    }

    // Edge-case zopfli probe: 4 "DSSIM-feasible but size>0.80×" fixtures.
    let edges: [Edge; 4] = [
        Edge { name: "n24_sun.png".into(), floor_k: 224, floor_d: 0.6 },
        Edge { name: "p295_3840x2560.png".into(), floor_k: 192, floor_d: 0.6 },
        Edge { name: "p283_3840x2560.png".into(), floor_k: 64, floor_d: 0.0 },
        Edge { name: "n36_comet.png".into(), floor_k: 64, floor_d: 0.3 },
    ];
    eprintln!("\n=== zopfli refine probe on 4 size-edge fixtures ===");
    println!("\nfixture\ttiny_size\ttiny_dssim\tfloor_K_d\tplain_B\tplain_DSSIM\tplain_ratio\tzopfli_B\tzopfli_DSSIM\tzopfli_ratio\tpass_plain\tpass_zopfli");
    for e in &edges {
        let src = corpus.join(&e.name);
        let tiny = tiny_dir.join(&e.name);
        if !src.exists() || !tiny.exists() {
            eprintln!("MISS {}", e.name);
            continue;
        }
        let img = ImageReader::open(&src)?.with_guessed_format()?.decode()?;
        let rgba = img.to_rgba8();
        let (wi, he) = (rgba.width(), rgba.height());
        let raw = rgba.into_raw();
        let tiny_size = std::fs::metadata(&tiny)?.len();
        let reference = Image::open(&src)?;
        let tiny_dssim = dssim_of_path(&reference, &tiny)?;
        let cap = (tiny_size as f64 * 0.80) as u64;

        let plain = quantize(&raw, wi, he, e.floor_k, e.floor_d, 6);
        let plain_dssim = dssim_of(&reference, &plain)?;
        let plain_size = plain.len() as u64;
        let zopfli = zopfli_refine(&plain)?;
        let zopfli_dssim = dssim_of(&reference, &zopfli)?;
        let zopfli_size = zopfli.len() as u64;

        let plain_pass = plain_size <= cap && plain_dssim <= tiny_dssim;
        let zopfli_pass = zopfli_size <= cap && zopfli_dssim <= tiny_dssim;

        println!(
            "{}\t{}\t{:.6}\tK={} d={:.1}\t{}\t{:.6}\t{:.4}\t{}\t{:.6}\t{:.4}\t{}\t{}",
            e.name,
            tiny_size,
            tiny_dssim,
            e.floor_k,
            e.floor_d,
            plain_size,
            plain_dssim,
            plain_size as f64 / tiny_size as f64,
            zopfli_size,
            zopfli_dssim,
            zopfli_size as f64 / tiny_size as f64,
            if plain_pass { "Y" } else { "N" },
            if zopfli_pass { "Y" } else { "N" },
        );

        if zopfli_pass {
            std::fs::write(out_dir.join(&e.name), &zopfli)?;
            eprintln!("WROTE (zopfli rescue) {}", e.name);
        }
    }
    Ok(())
}
