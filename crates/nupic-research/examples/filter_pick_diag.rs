//! Cycle 6 Pass 2 — diagnose which filter BestOf picks per fixture,
//! and compare with all-candidate Level::Best ranking ground truth.
//! Reveals if BestOf's Level::Fast ranking proxy mispredicts.
//!
//! For each fixture × strategy {None / Sub / Up / Avg / Paeth /
//! min-SAD}, compute (filter_pass + Level::Best deflate size). Then:
//! - Which candidate truly wins (smallest Level::Best size)?
//! - Which candidate does BestOf pick (via Level::Fast proxy)?
//! - Size cost of proxy mispredict.
//!
//! Run:
//!   cargo run --release -p nupic-research --example filter_pick_diag

use std::path::PathBuf;

use anyhow::Result;
use image::ImageReader;
use nupic_deflate::{Level, deflate_level};
use nupic_png::FilterType;
use nupic_quantize::{quantize};

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(PathBuf::from(m).ancestors().nth(2).unwrap().to_path_buf())
}

const STRATEGY_LABELS: &[(&str, fn(u32, u32, &[u8]) -> Vec<u8>)] = &[
    ("None", filter_none),
    ("Sub", filter_sub),
    ("Up", filter_up),
    ("Avg", filter_avg),
    ("Paeth", filter_paeth),
    ("min-SAD", filter_min_sad),
];

fn filter_none(w: u32, h: u32, indices: &[u8]) -> Vec<u8> {
    nupic_png::filter_image_single(w, h, indices, FilterType::None)
}
fn filter_sub(w: u32, h: u32, indices: &[u8]) -> Vec<u8> {
    nupic_png::filter_image_single(w, h, indices, FilterType::Sub)
}
fn filter_up(w: u32, h: u32, indices: &[u8]) -> Vec<u8> {
    nupic_png::filter_image_single(w, h, indices, FilterType::Up)
}
fn filter_avg(w: u32, h: u32, indices: &[u8]) -> Vec<u8> {
    nupic_png::filter_image_single(w, h, indices, FilterType::Average)
}
fn filter_paeth(w: u32, h: u32, indices: &[u8]) -> Vec<u8> {
    nupic_png::filter_image_single(w, h, indices, FilterType::Paeth)
}
fn filter_min_sad(w: u32, h: u32, indices: &[u8]) -> Vec<u8> {
    nupic_png::filter_image(w, h, indices)
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let fixtures = [
        "01-png-transparency-demo.png",
        "02-pluto-transparent.png",
        "03-wikipedia-logo.png",
        "04-photo-portrait.png",
        "05-photo-mountain.png",
        "06-photo-landscape.png",
        "07-photo-product.png",
    ];

    for f in &fixtures {
        let src = root.join("assets/png-bench/inputs").join(f);
        let img = ImageReader::open(&src)?.with_guessed_format()?.decode()?;
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        let raw = rgba.into_raw();
        let qi = quantize(&raw, w, h, 256).map_err(|e| anyhow::anyhow!("{e:?}"))?;

        println!("\n=== {} ===", f);
        println!("{:<10} {:>10} {:>10}  rank_by_Best  rank_by_Fast", "strategy", "Fast_size", "Best_size");

        let mut results: Vec<(&str, usize, usize)> = Vec::new();
        for (label, filter_fn) in STRATEGY_LABELS {
            let filtered = filter_fn(w, h, &qi.indices);
            let fast_size = deflate_level(&filtered, Level::Fast).len();
            let best_size = deflate_level(&filtered, Level::Best).len();
            results.push((label, fast_size, best_size));
            println!("{:<10} {:>10} {:>10}", label, fast_size, best_size);
        }

        // Determine winner by Best (ground truth) vs by Fast (BestOf's proxy)
        let best_winner = results.iter().min_by_key(|r| r.2).unwrap();
        let fast_winner = results.iter().min_by_key(|r| r.1).unwrap();
        println!("  GT_winner_by_Best: {} ({})", best_winner.0, best_winner.2);
        println!("  BestOf_pick_by_Fast: {} ({} → Best={})", fast_winner.0, fast_winner.1, fast_winner.2);
        let cost = (fast_winner.2 as i64) - (best_winner.2 as i64);
        println!("  proxy mispredict cost: {:+} bytes", cost);
    }
    Ok(())
}
