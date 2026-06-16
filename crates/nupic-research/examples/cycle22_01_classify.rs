//! Cycle 22 — diagnose 01-transparency-demo classifier.
//! Dither sweep shows d=0.5 gives +11.5 SSIM (-46→-35), but auto picks
//! d=0.0. Why? Check classifier branches.

use std::path::PathBuf;
use anyhow::Result;
use image::ImageReader;

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(PathBuf::from(m).ancestors().nth(2).unwrap().to_path_buf())
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let path = root.join("assets/png-bench/inputs/01-png-transparency-demo.png");
    let img = ImageReader::open(&path)?.with_guessed_format()?.decode()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let raw = rgba.into_raw();

    let mut n_opaque = 0usize;
    let mut n_total = 0usize;
    let mut alphas = std::collections::HashMap::<u8, u32>::new();
    for px in raw.chunks_exact(4) {
        n_total += 1;
        if px[3] == 255 { n_opaque += 1; }
        *alphas.entry(px[3]).or_insert(0) += 1;
    }
    println!("01-transparency-demo ({}x{} = {} pixels):", w, h, n_total);
    let opaque_ratio = n_opaque as f64 / n_total as f64;
    println!("  opaque_ratio: {:.4}  ({}/{} px with alpha=255)", opaque_ratio, n_opaque, n_total);
    println!("  distinct alpha values: {}", alphas.len());
    let mut sorted: Vec<_> = alphas.into_iter().collect();
    sorted.sort_by_key(|&(_, c)| std::cmp::Reverse(c));
    for (a, c) in sorted.iter().take(8) {
        println!("    alpha={:3}: {} px ({:.1}%)", a, c, *c as f64 / n_total as f64 * 100.0);
    }
    println!();
    println!("Classifier branches:");
    println!("  n_total < 200_000?  {}", n_total < 200_000);
    println!("  opaque_ratio < 0.50? {}  (tier-1 transparency-dominant)", opaque_ratio < 0.50);
    println!("  opaque_ratio < 0.95? {}  (tier-2 partial-transparent)", opaque_ratio < 0.95);
    let d = nupic_quantize::classify_for_auto_dither(&raw, w);
    println!("  current classify d = {:.2}", d);
    Ok(())
}
