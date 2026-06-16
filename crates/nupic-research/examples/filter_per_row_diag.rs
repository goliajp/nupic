//! Cycle 10 — diagnose what per-row filter min-SAD picks on 04
//! compared to oxipng's per-row choice. Determine if our heuristic
//! is making different choices.

use std::path::PathBuf;
use anyhow::Result;
use image::ImageReader;
use nupic_png::{FilterType, filter_image};
use nupic_quantize::quantize;

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(PathBuf::from(m).ancestors().nth(2).unwrap().to_path_buf())
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    for fname in ["04-photo-portrait.png", "05-photo-mountain.png", "06-photo-landscape.png", "07-photo-product.png"] {
        let src = root.join("assets/png-bench/inputs").join(fname);
        let img = ImageReader::open(&src)?.with_guessed_format()?.decode()?;
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        let raw = rgba.into_raw();
        let qi = quantize(&raw, w, h, 256).map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let filtered = filter_image(w, h, &qi.indices);
        let bpr = 1 + w as usize;
        let n_rows = h as usize;
        let mut hist = [0u32; 5];
        for r in 0..n_rows {
            let ft = filtered[r * bpr];
            if ft < 5 { hist[ft as usize] += 1; }
        }
        let names = ["None", "Sub", "Up", "Avg", "Paeth"];
        let total: u32 = hist.iter().sum();
        print!("{:<32} per-row min-SAD: ", fname);
        for (n, c) in names.iter().zip(hist.iter()) {
            print!("{}={} ({:.1}%) ", n, c, *c as f64 / total as f64 * 100.0);
        }
        let _ = FilterType::None;
        println!();
    }
    Ok(())
}
