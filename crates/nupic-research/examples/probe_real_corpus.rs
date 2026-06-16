//! Probe classify + signals on real-photo extended corpus.

use std::collections::HashSet;
use std::path::PathBuf;
use anyhow::Result;
use image::ImageReader;

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(PathBuf::from(m).ancestors().nth(2).unwrap().to_path_buf())
}

fn signals(raw: &[u8], w: u32) -> (f64, f64, f64, f64, usize) {
    let n_total = raw.len() / 4;
    let mut n_opaque = 0usize;
    for px in raw.chunks_exact(4) {
        if px[3] == 255 { n_opaque += 1; }
    }
    let opq = n_opaque as f64 / n_total as f64;

    let mut runs: u64 = 0; let mut total_runs: u64 = 0;
    let mut prev: [u8; 3] = [0,0,0]; let mut cur_run: u64 = 0;
    for (i, p) in raw.chunks_exact(4).enumerate() {
        let rgb = [p[0],p[1],p[2]];
        if i > 0 && rgb == prev { cur_run += 1; }
        else { if cur_run > 0 { runs += cur_run; total_runs += 1; } cur_run = 1; }
        prev = rgb;
    }
    if cur_run > 0 { runs += cur_run; total_runs += 1; }
    let mr = if total_runs == 0 { 1.0 } else { runs as f64 / total_runs as f64 };

    let mut uniq: HashSet<u32> = HashSet::new();
    let step_u = if n_total > 1_000_000 { 4 } else { 1 };
    for p in raw.chunks_exact(4).step_by(step_u) {
        if p[3] == 255 {
            uniq.insert((p[0] as u32) | ((p[1] as u32)<<8) | ((p[2] as u32)<<16));
            if uniq.len() >= 1_500_000 { break; }
        }
    }

    let w = w as usize;
    let h = n_total / w;
    let target = 500_000;
    let target_rows = target / (w-1).max(1);
    let step = (h / target_rows.max(1)).max(1);
    let mut sum = 0u64; let mut sq = 0u64; let mut cnt = 0u64;
    for y in (0..h).step_by(step) {
        for x in 0..w-1 {
            let i = (y*w+x)*4;
            let l0 = (raw[i] as u32 + raw[i+1] as u32 + raw[i+2] as u32) / 3;
            let l1 = (raw[i+4] as u32 + raw[i+5] as u32 + raw[i+6] as u32) / 3;
            let d = (l0 as i32 - l1 as i32).unsigned_abs() as u64;
            sum += d; sq += d*d; cnt += 1;
        }
    }
    let mean = sum as f64 / cnt as f64;
    let var = (sq as f64 / cnt as f64) - mean * mean;
    (opq, mr, mean, var, uniq.len())
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    for f in [
        "16-earthrise-25mp.png", "17-aurora-5mp.png",
        "18-snowflake-17mp.png", "19-iceberg-3mp.png",
        "20-rainbow-19mp.png",
        "24-melk-abbey-24mp.png", "25-sofia-cathedral-5mp.png",
        "26-angkor-wat-32mp.png", "27-whale-tail-5mp.png",
        "28-orca-14mp.png", "29-sundew-3mp.png",
    ] {
        let p = root.join("assets/png-bench/inputs-ext-real").join(f);
        let img = ImageReader::open(&p)?.with_guessed_format()?.decode()?;
        let r = img.to_rgba8();
        let w = r.width();
        let raw = r.into_raw();
        let (opq, mr, am, av, uniq) = signals(&raw, w);
        let d = nupic_quantize::classify_for_auto_dither(&raw, w);
        let grad = nupic_quantize::is_gradient_candidate(&raw, w);
        println!("{:<28} opq={:.3} mr={:.2} adj_mn={:.2} var={:.1} uniq={:>9} d={:.2} grad={}",
            f, opq, mr, am, av, uniq, d, grad);
    }
    Ok(())
}
