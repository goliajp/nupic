//! Cycle 11 — test signals to differentiate tier-4 photos that want
//! d=0.5 (e.g. 04-portrait) from those that want d=0.7 (e.g. 05/06/07).
//! Compute mean adjacent-pixel luminance diff per fixture; see if it
//! correlates with optimal-strength preference from Cycle 9 sweep.

use std::path::PathBuf;
use anyhow::Result;
use image::ImageReader;

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(PathBuf::from(m).ancestors().nth(2).unwrap().to_path_buf())
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let fixtures: &[(&str, f64)] = &[
        // (fname, best_d from Cycle 9 sweep)
        ("04-photo-portrait.png", 0.5),
        ("05-photo-mountain.png", 0.75),
        ("06-photo-landscape.png", 0.7),
        ("07-photo-product.png", 0.7),
    ];

    println!("{:<32} {:>14} {:>14} {:>14} {:>10}",
        "fixture", "mean_adj_diff", "var_adj_diff", "uniq_per_row", "best_d");

    for (fname, best_d) in fixtures {
        let src = root.join("assets/png-bench/inputs").join(fname);
        let img = ImageReader::open(&src)?.with_guessed_format()?.decode()?;
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width() as usize, rgba.height() as usize);
        let raw = rgba.into_raw();

        // Mean absolute luminance difference between horizontally adjacent pixels
        let mut sum_diff: u64 = 0;
        let mut count: u64 = 0;
        let mut diff_sq_sum: u64 = 0;
        for y in 0..h {
            for x in 0..w-1 {
                let i0 = (y * w + x) * 4;
                let i1 = (y * w + x + 1) * 4;
                // Luminance approx = (R + G + B) / 3
                let l0 = (raw[i0] as u32 + raw[i0+1] as u32 + raw[i0+2] as u32) / 3;
                let l1 = (raw[i1] as u32 + raw[i1+1] as u32 + raw[i1+2] as u32) / 3;
                let diff = (l0 as i32 - l1 as i32).unsigned_abs() as u64;
                sum_diff += diff;
                diff_sq_sum += diff * diff;
                count += 1;
            }
        }
        let mean_diff = sum_diff as f64 / count as f64;
        let mean_sq = diff_sq_sum as f64 / count as f64;
        let var_diff = mean_sq - mean_diff * mean_diff;

        // Unique colors per row, averaged
        let mut sum_uniq = 0u64;
        for y in 0..h {
            let mut seen = std::collections::HashSet::new();
            for x in 0..w {
                let i = (y * w + x) * 4;
                let key = u32::from(raw[i]) | (u32::from(raw[i+1])<<8) | (u32::from(raw[i+2])<<16);
                seen.insert(key);
            }
            sum_uniq += seen.len() as u64;
        }
        let uniq_per_row = sum_uniq as f64 / h as f64;

        println!("{:<32} {:>14.3} {:>14.3} {:>14.3} {:>10}",
            fname, mean_diff, var_diff, uniq_per_row, best_d);
    }
    Ok(())
}
