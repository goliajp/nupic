//! Cycle 102 attempt 3 — palette (K, dither, preset) sweep on 4 sub-gate fixtures
//!
//! Attempts 1/2 RED: deflate/filter side exhausted. Now sweep palette-side
//! to find per-fixture overrides that pass the gate (size ≤ 0.80× tiny AND
//! SSIM ≥ tiny). Fixtures attacked: 01-trans, 03-wiki, 06-landscape,
//! 07-product (the 4 currently sub-gate per Cycle 102 sanity).
//!
//! Sweep: K ∈ {96, 128, 144, 160, 192, 208, 224, 240, 256}
//!        dither ∈ {0.0, 0.2, 0.4, 0.6}
//!        preset ∈ {3, 6}
//! Per fixture: report (best config), best size, SSIM, gate verdict.
//! Also re-oxipng with zopfli on the best (smallest gate-passing) config to
//! see whether zopfli closes the remaining gap when the palette change doesn't.
//!
//! Does NOT touch production source. Spike is measurement only.

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use image::ImageReader;
use oxipng::{indexset, RowFilter};

use nupic_quantize::{quantize_indexed_png, QuantizeOpts};

const SUB_GATE: &[&str] = &[
    "01-png-transparency-demo",
    "03-wikipedia-logo",
    "06-photo-landscape",
    "07-photo-product",
];

fn ssim_via_nupic(orig: &PathBuf, c: &PathBuf, nupic: &PathBuf) -> f64 {
    let out = Command::new(nupic).args(["compare", "-m", "ssimulacra2"]).arg(orig).arg(c).output().expect("nupic");
    String::from_utf8_lossy(&out.stdout).lines().find_map(|l| {
        l.strip_prefix("SSIMULACRA2: ").and_then(|v| v.split_whitespace().next()).and_then(|n| n.parse::<f64>().ok())
    }).unwrap_or(f64::NAN)
}

fn quantize(raw: &[u8], w: u32, h: u32, k: usize, d: f32, p: u8) -> Vec<u8> {
    let mut opts = QuantizeOpts::default();
    opts.n_colors = k; opts.dither_strength = d; opts.oxipng_preset = p; opts.strip_metadata = true;
    quantize_indexed_png(raw, w, h, opts).expect("quantize")
}

fn oxipng_zopfli(bytes: &[u8]) -> Vec<u8> {
    let mut opts = oxipng::Options::from_preset(6);
    opts.deflate = oxipng::Deflaters::Zopfli { iterations: std::num::NonZeroU8::new(15).unwrap() };
    opts.filter = indexset! {
        RowFilter::None, RowFilter::Sub, RowFilter::Up, RowFilter::Average, RowFilter::Paeth,
        RowFilter::MinSum, RowFilter::Entropy, RowFilter::Bigrams, RowFilter::BigEnt,
    };
    oxipng::optimize_from_memory(bytes, &opts).expect("zopfli")
}

#[derive(Clone, Copy, Debug)]
struct Candidate {
    k: usize, d: f32, p: u8,
    size: usize, ssim: f64,
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();
    let nupic = root.join("target/release/nupic");

    println!("Cycle 102 attempt 3 — palette (K, d, preset) sweep on 4 sub-gate fixtures");
    println!("Gate: size ≤ 0.80× tinypng AND SSIM ≥ tinypng");
    println!();

    let ks = [96usize, 128, 144, 160, 192, 208, 224, 240, 256];
    let ds = [0.0f32, 0.2, 0.4, 0.6];
    let ps = [3u8, 6];

    let t_total = Instant::now();
    for f in SUB_GATE {
        let orig = root.join("assets/png-bench/inputs").join(format!("{}.png", f));
        let tiny = root.join("assets/png-bench/tinypng-web").join(format!("{}.png", f));
        let tiny_size = std::fs::metadata(&tiny)?.len() as i64;
        let tiny_ssim = ssim_via_nupic(&orig, &tiny, &nupic);
        let cap = tiny_size * 80 / 100;

        let img = ImageReader::open(&orig)?.with_guessed_format()?.decode()?;
        let r = img.to_rgba8();
        let w = r.width(); let h = r.height();
        let raw = r.into_raw();

        let tmp_dir = std::env::temp_dir();
        let mut all = Vec::new();
        let t_fix = Instant::now();
        for &k in &ks { for &d in &ds { for &p in &ps {
            let bytes = quantize(&raw, w, h, k, d, p);
            let path = tmp_dir.join(format!("c102a3_{}_k{}_d{:.1}_p{}.png", f, k, d, p));
            std::fs::write(&path, &bytes)?;
            let s = ssim_via_nupic(&orig, &path, &nupic);
            all.push(Candidate { k, d, p, size: bytes.len(), ssim: s });
        }}}
        let t_fix_s = t_fix.elapsed().as_secs_f64();

        // Filter to gate-passing
        let mut passing: Vec<&Candidate> = all.iter().filter(|c| (c.size as i64) <= cap && c.ssim >= tiny_ssim).collect();
        passing.sort_by_key(|c| c.size);

        // Best near-gate: smallest gate-passing
        // Then also report smallest size with SSIM ≥ tiny (no size cap) and smallest size period
        let mut by_ssim_only: Vec<&Candidate> = all.iter().filter(|c| c.ssim >= tiny_ssim).collect();
        by_ssim_only.sort_by_key(|c| c.size);
        let smallest_with_q = by_ssim_only.first().copied();

        println!("[{}]  tiny={}B  cap={}B (-20%)  tiny_SSIM={:.2}  ({} cfg in {:.1}s)",
                 f, tiny_size, cap, tiny_ssim, ks.len() * ds.len() * ps.len(), t_fix_s);

        match passing.first() {
            Some(best) => {
                println!("  ✓ GATE PASS: K={} d={:.1} p={}  size={}B (-{:.1}% vs cap)  SSIM={:.2}",
                         best.k, best.d, best.p, best.size,
                         (1.0 - best.size as f64 / cap as f64) * 100.0, best.ssim);
                // Try zopfli on the best gate-passing config
                let img2 = ImageReader::open(&orig)?.with_guessed_format()?.decode()?;
                let r2 = img2.to_rgba8();
                let w2 = r2.width(); let h2 = r2.height();
                let raw2 = r2.into_raw();
                let q = quantize(&raw2, w2, h2, best.k, best.d, best.p);
                let z = oxipng_zopfli(&q);
                let z_path = tmp_dir.join(format!("c102a3_{}_best_zopfli.png", f));
                std::fs::write(&z_path, &z)?;
                let z_ssim = ssim_via_nupic(&orig, &z_path, &nupic);
                let z_pass = (z.len() as i64) <= cap && z_ssim >= tiny_ssim;
                println!("  + zopfli: size={}B  SSIM={:.2}  {}",
                         z.len(), z_ssim, if z_pass { "✓" } else { "✗" });
            }
            None => {
                println!("  ✗ NO GATE-PASS in sweep.");
                if let Some(best_q) = smallest_with_q {
                    println!("    smallest with SSIM≥tiny: K={} d={:.1} p={}  size={}B (over cap by {}B)  SSIM={:.2}",
                             best_q.k, best_q.d, best_q.p, best_q.size,
                             best_q.size as i64 - cap, best_q.ssim);
                    // Try zopfli on this candidate
                    let img2 = ImageReader::open(&orig)?.with_guessed_format()?.decode()?;
                    let r2 = img2.to_rgba8();
                    let w2 = r2.width(); let h2 = r2.height();
                    let raw2 = r2.into_raw();
                    let q = quantize(&raw2, w2, h2, best_q.k, best_q.d, best_q.p);
                    let z = oxipng_zopfli(&q);
                    let z_path = tmp_dir.join(format!("c102a3_{}_smqssim_zopfli.png", f));
                    std::fs::write(&z_path, &z)?;
                    let z_ssim = ssim_via_nupic(&orig, &z_path, &nupic);
                    let z_pass = (z.len() as i64) <= cap && z_ssim >= tiny_ssim;
                    println!("    + zopfli: size={}B (over cap by {}B)  SSIM={:.2}  {}",
                             z.len(), z.len() as i64 - cap, z_ssim, if z_pass { "✓ GATE PASS!" } else { "✗" });
                }
                // Show top 3 smallest, regardless of SSIM
                let mut by_size: Vec<&Candidate> = all.iter().collect();
                by_size.sort_by_key(|c| c.size);
                for (i, c) in by_size.iter().take(3).enumerate() {
                    println!("    top{}: K={} d={:.1} p={}  size={}B  SSIM={:.2}  (need SSIM≥{:.2})",
                             i+1, c.k, c.d, c.p, c.size, c.ssim, tiny_ssim);
                }
            }
        }
        println!();
    }
    println!("Total wall: {:.1}s", t_total.elapsed().as_secs_f64());

    Ok(())
}
