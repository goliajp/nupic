//! Cycle 102 — production 07 product squeeze to break −20% TinyPNG gate
//!
//! Goal: v1.2.6 production 07 product is 289 KB vs TinyPNG 358 KB = 0.807×.
//! New three-axis gate is size ≤ 0.80× tiny (i.e. ≤ 286 KB) AND SSIM ≥ tiny
//! AND perf max. Current state: SSIM ✓ (82.79 vs 80.32), wall fine, **size
//! 2.3 KB above gate**.
//!
//! Strategy: take production binary's Auto output as baseline (no change to
//! production source) and try post-hoc oxipng squeeze variants. If a variant
//! also passes the same gate on the other 6 baseline-7 fixtures, propose a
//! production wiring task. Otherwise, document attempt + try a different
//! attack vector.
//!
//! Variants tested per fixture:
//!   V0  production Auto output (baseline)
//!   V1  re-oxipng with forced Entropy filter only (Cycle 101 found Entropy
//!       dominates for opaque baseline-7)
//!   V2  re-oxipng with forced BigEnt + extra brute
//!   V3  re-oxipng with --interlace none + Brute + strip
//!
//! Per cycle gate check on baseline-7 (all 7 fixtures):
//!   * size ≤ 0.80 × tiny
//!   * v126_SSIM unchanged (we don't touch pixels; only re-DEFLATE/filter)
//!     so SSIM gate is preserved by construction.
//!   * wall delta reported as informational.

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use oxipng::{indexset, RowFilter};

const BASELINE_7: &[&str] = &[
    "01-png-transparency-demo",
    "02-pluto-transparent",
    "03-wikipedia-logo",
    "04-photo-portrait",
    "05-photo-mountain",
    "06-photo-landscape",
    "07-photo-product",
];

fn ssim_via_nupic(orig: &PathBuf, cmp_path: &PathBuf, nupic: &PathBuf) -> f64 {
    let out = Command::new(nupic).args(["compare", "-m", "ssimulacra2"]).arg(orig).arg(cmp_path).output().expect("nupic");
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines().find_map(|l| {
        l.strip_prefix("SSIMULACRA2: ").and_then(|v| v.split_whitespace().next()).and_then(|n| n.parse::<f64>().ok())
    }).unwrap_or(f64::NAN)
}

fn oxipng_squeeze(bytes: &[u8], filter_set: oxipng::IndexSet<RowFilter>, preset: u8) -> (Vec<u8>, f64) {
    let mut opts = oxipng::Options::from_preset(preset);
    opts.filter = filter_set;
    let t = Instant::now();
    let out = oxipng::optimize_from_memory(bytes, &opts).expect("oxipng");
    (out, t.elapsed().as_secs_f64() * 1000.0)
}

fn oxipng_brute(bytes: &[u8], preset: u8) -> (Vec<u8>, f64) {
    let mut opts = oxipng::Options::from_preset(preset);
    opts.filter = indexset! {
        RowFilter::None, RowFilter::Sub, RowFilter::Up, RowFilter::Average,
        RowFilter::Paeth, RowFilter::MinSum, RowFilter::Entropy,
        RowFilter::Bigrams, RowFilter::BigEnt, RowFilter::Brute,
    };
    let t = Instant::now();
    let out = oxipng::optimize_from_memory(bytes, &opts).expect("oxipng");
    (out, t.elapsed().as_secs_f64() * 1000.0)
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();
    let nupic = root.join("target/release/nupic");

    println!("Cycle 102 — attempt 1: post-hoc oxipng squeeze on production output");
    println!("Gate: size ≤ 0.80× tinypng AND SSIM ≥ tinypng AND perf max");
    println!();
    printf_header();

    let mut sum_v0 = 0i64; let mut sum_v1 = 0i64; let mut sum_v2 = 0i64; let mut sum_v3 = 0i64; let mut sum_t = 0i64;
    let mut pass_v0 = 0; let mut pass_v1 = 0; let mut pass_v2 = 0; let mut pass_v3 = 0;

    for f in BASELINE_7 {
        let orig = root.join("assets/png-bench/inputs").join(format!("{}.png", f));
        let tiny = root.join("assets/png-bench/tinypng-web").join(format!("{}.png", f));
        let v126 = PathBuf::from("/tmp/nupic-v126-bench").join(format!("{}.png", f));
        let v126_bytes = std::fs::read(&v126)?;
        let tiny_size = std::fs::metadata(&tiny)?.len() as i64;
        let v0_size = v126_bytes.len() as i64;

        // V1: forced Entropy
        let (v1, v1_wall) = oxipng_squeeze(&v126_bytes, indexset! { RowFilter::Entropy }, 3);
        let v1_size = v1.len() as i64;
        // V2: BigEnt + Brute
        let (v2, v2_wall) = oxipng_squeeze(&v126_bytes, indexset! { RowFilter::BigEnt, RowFilter::Brute }, 3);
        let v2_size = v2.len() as i64;
        // V3: full brute over all 10 filter types
        let (v3, v3_wall) = oxipng_brute(&v126_bytes, 3);
        let v3_size = v3.len() as i64;

        // Write V3 (best candidate per fixture) for SSIM check (should match V0 SSIM since pixels unchanged)
        let v3_path = PathBuf::from("/tmp/nupic-v126-bench").join(format!("{}-v3.png", f));
        std::fs::write(&v3_path, &v3)?;
        let v0_ssim = ssim_via_nupic(&orig, &v126, &nupic);
        let v3_ssim = ssim_via_nupic(&orig, &v3_path, &nupic);
        let tiny_ssim = ssim_via_nupic(&orig, &tiny, &nupic);

        let cap = tiny_size * 80 / 100;
        let p0 = v0_size <= cap && v0_ssim >= tiny_ssim;
        let p1 = v1_size <= cap && v0_ssim >= tiny_ssim;
        let p2 = v2_size <= cap && v0_ssim >= tiny_ssim;
        let p3 = v3_size <= cap && v3_ssim >= tiny_ssim;
        if p0 { pass_v0 += 1; }
        if p1 { pass_v1 += 1; }
        if p2 { pass_v2 += 1; }
        if p3 { pass_v3 += 1; }

        sum_v0 += v0_size; sum_v1 += v1_size; sum_v2 += v2_size; sum_v3 += v3_size; sum_t += tiny_size;

        let best = v1_size.min(v2_size).min(v3_size).min(v0_size);
        let saved_kb = (v0_size - best) as f64 / 1024.0;
        println!("[{:<26}] v0={:>7}B  v1={:>7}B  v2={:>7}B  v3={:>7}B  tiny={:>7}B  cap={:>7}B | best saves {:>4.1}KB | gate v0/v1/v2/v3 = {}/{}/{}/{} (v0 SSIM {:.2}, v3 SSIM {:.2})  wall v1={:.0} v2={:.0} v3={:.0}ms",
                 f, v0_size, v1_size, v2_size, v3_size, tiny_size, cap, saved_kb,
                 mark(p0), mark(p1), mark(p2), mark(p3), v0_ssim, v3_ssim, v1_wall, v2_wall, v3_wall);
    }

    println!();
    println!("=== Aggregate ===");
    let cap_total = sum_t * 80 / 100;
    println!("baseline-7 totals:");
    println!("  v0 (production)            : {:>8} B  ratio {:.3}x  pass {}/7",
             sum_v0, sum_v0 as f64 / sum_t as f64, pass_v0);
    println!("  v1 (entropy forced)        : {:>8} B  ratio {:.3}x  pass {}/7",
             sum_v1, sum_v1 as f64 / sum_t as f64, pass_v1);
    println!("  v2 (BigEnt + Brute)        : {:>8} B  ratio {:.3}x  pass {}/7",
             sum_v2, sum_v2 as f64 / sum_t as f64, pass_v2);
    println!("  v3 (full Brute)            : {:>8} B  ratio {:.3}x  pass {}/7",
             sum_v3, sum_v3 as f64 / sum_t as f64, pass_v3);
    println!("  tinypng cohort total       : {:>8} B", sum_t);
    println!("  −20% gate cap (0.80×)      : {:>8} B", cap_total);
    println!();

    let best_pass = pass_v0.max(pass_v1).max(pass_v2).max(pass_v3);
    if best_pass == 7 {
        println!(">>> GREEN — variant clears 7/7 size+SSIM gate on baseline-7. Production wiring candidate.");
    } else if best_pass > pass_v0 {
        println!(">>> YELLOW — variant improves over production but {}/7 still sub-gate. Need different attack.", best_pass);
    } else {
        println!(">>> RED — post-hoc oxipng squeeze gives no gate progress. Need new attack vector (palette tuning / R-D config / new algorithm).");
    }

    Ok(())
}

fn mark(b: bool) -> &'static str { if b { "✓" } else { "✗" } }

fn printf_header() {
    println!("V0 = production Auto output;  V1 = re-oxipng Entropy;  V2 = re-oxipng BigEnt+Brute;  V3 = re-oxipng full Brute");
}
