//! Cycle 102 attempt 2 — re-oxipng with preset=6 (libdeflate compression=12)
//! on production output. The default production preset is 3 (libdeflate
//! compression=8); bumping to 6 brings in slower-but-better deflate plus more
//! filter trials. The goal: see how many KB this gets across baseline-7,
//! with focus on the 4 sub-gate fixtures (01, 03, 06, 07).
//!
//! No production source change. SSIM is preserved (lossless re-encoding).

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use oxipng::{indexset, RowFilter};

const BASELINE_7: &[&str] = &[
    "01-png-transparency-demo", "02-pluto-transparent", "03-wikipedia-logo",
    "04-photo-portrait", "05-photo-mountain", "06-photo-landscape", "07-photo-product",
];

fn ssim(orig: &PathBuf, c: &PathBuf, nupic: &PathBuf) -> f64 {
    let out = Command::new(nupic).args(["compare", "-m", "ssimulacra2"]).arg(orig).arg(c).output().expect("nupic");
    String::from_utf8_lossy(&out.stdout).lines().find_map(|l| {
        l.strip_prefix("SSIMULACRA2: ").and_then(|v| v.split_whitespace().next()).and_then(|n| n.parse::<f64>().ok())
    }).unwrap_or(f64::NAN)
}

fn try_preset(bytes: &[u8], preset: u8) -> (Vec<u8>, f64) {
    let mut opts = oxipng::Options::from_preset(preset);
    // Make sure all filter options on for preset 6 (we already verified Brute path)
    opts.filter = indexset! {
        RowFilter::None, RowFilter::Sub, RowFilter::Up, RowFilter::Average,
        RowFilter::Paeth, RowFilter::MinSum, RowFilter::Entropy,
        RowFilter::Bigrams, RowFilter::BigEnt,
    };
    let t = Instant::now();
    let out = oxipng::optimize_from_memory(bytes, &opts).expect("oxipng");
    (out, t.elapsed().as_secs_f64() * 1000.0)
}

fn try_zopfli(bytes: &[u8]) -> (Vec<u8>, f64) {
    // Use a stand-alone oxipng config with zopfli deflater
    let mut opts = oxipng::Options::from_preset(6);
    opts.deflate = oxipng::Deflaters::Zopfli { iterations: std::num::NonZeroU8::new(15).unwrap() };
    opts.filter = indexset! {
        RowFilter::None, RowFilter::Sub, RowFilter::Up, RowFilter::Average,
        RowFilter::Paeth, RowFilter::MinSum, RowFilter::Entropy,
        RowFilter::Bigrams, RowFilter::BigEnt,
    };
    let t = Instant::now();
    let out = oxipng::optimize_from_memory(bytes, &opts).expect("oxipng");
    (out, t.elapsed().as_secs_f64() * 1000.0)
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();
    let nupic = root.join("target/release/nupic");

    println!("Cycle 102 attempt 2 — re-oxipng preset=6 + zopfli on production output");
    println!("Gate: size ≤ 0.80× tinypng AND SSIM ≥ tinypng AND perf max");
    println!();

    let mut sum_v0 = 0i64; let mut sum_v6 = 0i64; let mut sum_vz = 0i64; let mut sum_t = 0i64;
    let mut pass_v0 = 0; let mut pass_v6 = 0; let mut pass_vz = 0;
    let mut total_wall_v6 = 0.0; let mut total_wall_vz = 0.0;

    for f in BASELINE_7 {
        let orig = root.join("assets/png-bench/inputs").join(format!("{}.png", f));
        let tiny = root.join("assets/png-bench/tinypng-web").join(format!("{}.png", f));
        let v126 = PathBuf::from("/tmp/nupic-v126-bench").join(format!("{}.png", f));
        let v126_bytes = std::fs::read(&v126)?;
        let tiny_size = std::fs::metadata(&tiny)?.len() as i64;
        let v0_size = v126_bytes.len() as i64;

        let (v6, v6_wall) = try_preset(&v126_bytes, 6);
        let v6_size = v6.len() as i64;
        let (vz, vz_wall) = try_zopfli(&v126_bytes);
        let vz_size = vz.len() as i64;

        let v6_path = PathBuf::from("/tmp/nupic-v126-bench").join(format!("{}-v6.png", f));
        let vz_path = PathBuf::from("/tmp/nupic-v126-bench").join(format!("{}-vz.png", f));
        std::fs::write(&v6_path, &v6)?;
        std::fs::write(&vz_path, &vz)?;

        let v0_ssim = ssim(&orig, &v126, &nupic);
        let v6_ssim = ssim(&orig, &v6_path, &nupic);
        let vz_ssim = ssim(&orig, &vz_path, &nupic);
        let t_ssim = ssim(&orig, &tiny, &nupic);

        let cap = tiny_size * 80 / 100;
        let p0 = v0_size <= cap && v0_ssim >= t_ssim;
        let p6 = v6_size <= cap && v6_ssim >= t_ssim;
        let pz = vz_size <= cap && vz_ssim >= t_ssim;
        if p0 { pass_v0 += 1; } if p6 { pass_v6 += 1; } if pz { pass_vz += 1; }
        sum_v0 += v0_size; sum_v6 += v6_size; sum_vz += vz_size; sum_t += tiny_size;
        total_wall_v6 += v6_wall; total_wall_vz += vz_wall;

        let saved_v6 = (v0_size - v6_size) as f64 / 1024.0;
        let saved_vz = (v0_size - vz_size) as f64 / 1024.0;
        println!("[{:<26}] v0={:>7}B v6={:>7}B (-{:>4.1}KB) vz={:>7}B (-{:>4.1}KB) cap={:>7}B | gate v0/v6/vz={}/{}/{} | wall v6={:.0} vz={:.0}ms",
                 f, v0_size, v6_size, saved_v6, vz_size, saved_vz, cap,
                 mark(p0), mark(p6), mark(pz), v6_wall, vz_wall);
    }

    println!();
    println!("=== Aggregate ===");
    let cap_total = sum_t * 80 / 100;
    println!("  v0 production : {:>8} B  ratio {:.3}x  pass {}/7   wall (n/a, from prod)",
             sum_v0, sum_v0 as f64 / sum_t as f64, pass_v0);
    println!("  v6 preset=6   : {:>8} B  ratio {:.3}x  pass {}/7   wall total {:.0}ms (mean {:.0}ms/fix)",
             sum_v6, sum_v6 as f64 / sum_t as f64, pass_v6, total_wall_v6, total_wall_v6/7.0);
    println!("  vz zopfli(15) : {:>8} B  ratio {:.3}x  pass {}/7   wall total {:.0}ms (mean {:.0}ms/fix)",
             sum_vz, sum_vz as f64 / sum_t as f64, pass_vz, total_wall_vz, total_wall_vz/7.0);
    println!("  tiny / cap    : {:>8} B  / {:>8} B (0.80x)", sum_t, cap_total);
    println!();

    let best_pass = pass_v0.max(pass_v6).max(pass_vz);
    if best_pass == 7 {
        println!(">>> GREEN — attempt 2 clears 7/7. Production: bump preset (or attach zopfli) for baseline-7 tier.");
    } else if best_pass > pass_v0 {
        println!(">>> YELLOW — {}/7 cleared, +{} fixtures vs production. Need additional attack for the rest.",
                 best_pass, best_pass - pass_v0);
    } else {
        println!(">>> RED — preset/zopfli bumps give no gate progress. Need palette-side change.");
    }

    Ok(())
}

fn mark(b: bool) -> &'static str { if b { "✓" } else { "✗" } }
