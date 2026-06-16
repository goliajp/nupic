//! Probe classify signals across original + extended corpus.
//! Goal: find a signal that differentiates 08 (gradient) from 09 (UI).

use std::collections::HashSet;
use std::path::PathBuf;
use anyhow::Result;
use image::ImageReader;

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(PathBuf::from(m).ancestors().nth(2).unwrap().to_path_buf())
}

fn compute_signals(raw: &[u8], w: u32) -> (f64, f64, f64, f64, usize) {
    let mut n_opaque = 0usize;
    let mut n_total = 0usize;
    for px in raw.chunks_exact(4) {
        n_total += 1;
        if px[3] == 255 { n_opaque += 1; }
    }
    let opq = n_opaque as f64 / n_total as f64;

    // mean_run
    let mut runs: u64 = 0; let mut total_runs: u64 = 0;
    let mut prev: [u8; 3] = [0, 0, 0]; let mut cur_run: u64 = 0;
    for (i, p) in raw.chunks_exact(4).enumerate() {
        let rgb = [p[0], p[1], p[2]];
        if i > 0 && rgb == prev { cur_run += 1; }
        else { if cur_run > 0 { runs += cur_run; total_runs += 1; } cur_run = 1; }
        prev = rgb;
    }
    if cur_run > 0 { runs += cur_run; total_runs += 1; }
    let mean_run = if total_runs == 0 { 1.0 } else { runs as f64 / total_runs as f64 };

    // unique RGB colors (opaque only)
    let mut uniq: HashSet<u32> = HashSet::new();
    for p in raw.chunks_exact(4) {
        if p[3] == 255 {
            uniq.insert((p[0] as u32) | ((p[1] as u32) << 8) | ((p[2] as u32) << 16));
        }
    }

    // mean abs adj diff (luminance) + var
    let w = w as usize;
    let h = n_total / w;
    let mut sum: u64 = 0; let mut sum_sq: u64 = 0; let mut cnt: u64 = 0;
    let step = (h / 500.max(1)).max(1);
    for y in (0..h).step_by(step) {
        for x in 0..w-1 {
            let i = (y * w + x) * 4;
            let l0 = (raw[i] as u32 + raw[i+1] as u32 + raw[i+2] as u32) / 3;
            let l1 = (raw[i+4] as u32 + raw[i+5] as u32 + raw[i+6] as u32) / 3;
            let d = (l0 as i32 - l1 as i32).unsigned_abs() as u64;
            sum += d; sum_sq += d * d; cnt += 1;
        }
    }
    let mean = sum as f64 / cnt as f64;
    let var = (sum_sq as f64 / cnt as f64) - mean * mean;
    (opq, mean_run, mean, var, uniq.len())
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let dirs = [
        ("inputs", &["01-png-transparency-demo.png", "02-pluto-transparent.png",
                     "03-wikipedia-logo.png", "04-photo-portrait.png",
                     "05-photo-mountain.png", "06-photo-landscape.png",
                     "07-photo-product.png"][..]),
        ("inputs-ext", &["08-gradient-large.png", "09-ui-checker-text.png",
                         "10-comic-flat.png", "11-photo-noisy.png",
                         "12-tiny-icon.png", "13-very-large-photo.png",
                         "14-soft-transparent.png", "15-mono-text.png"][..]),
    ];

    println!("{:<32} {:>6} {:>8} {:>7} {:>8} {:>9} {:>6}",
        "fixture", "opq", "mean_run", "adj_mn", "adj_var", "uniq", "tier?");
    for (d, fs) in &dirs {
        for f in *fs {
            let path = root.join("assets/png-bench").join(d).join(f);
            let img = ImageReader::open(&path)?.with_guessed_format()?.decode()?;
            let rgba = img.to_rgba8();
            let w = rgba.width();
            let raw = rgba.into_raw();
            let (opq, mr, am, av, uniq) = compute_signals(&raw, w);
            let cur_d = nupic_quantize::classify_for_auto_dither(&raw, w);
            let tier = if cur_d == 0.0 { "T1" }
                else if cur_d == 0.25 { "T2/T3" }  // both produce 0.25 (T2 actually 0.35 post-Cycle-20)
                else if cur_d == 0.35 { "T2" }
                else if cur_d == 0.5 { "T4a" }
                else if cur_d == 0.7 { "T4b" }
                else { "?" };
            let label = format!("{}", f);
            println!("{:<32} {:>6.3} {:>8.2} {:>7.2} {:>8.1} {:>9} {:>6}",
                label, opq, mr, am, av, uniq, tier);
        }
    }
    Ok(())
}
