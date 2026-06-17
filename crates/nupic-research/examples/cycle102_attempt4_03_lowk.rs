//! Cycle 102 attempt 4 — 03 wiki K<96 floor probe
//!
//! Attempt 3 showed K=96 saturates 03 wiki at 12253 B (need ≤ 10793 B).
//! Probe lower K to see if the logo content tolerates K < 96 while
//! maintaining SSIM ≥ TinyPNG (which is −63.72 due to alpha-edge floor —
//! so practically any positive SSIM passes).

use std::path::PathBuf;
use std::process::Command;

use image::ImageReader;
use oxipng::{indexset, RowFilter};

use nupic_quantize::{quantize_indexed_png, QuantizeOpts};

fn ssim(orig: &PathBuf, c: &PathBuf, nupic: &PathBuf) -> f64 {
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
    opts.deflate = oxipng::Deflaters::Zopfli { iterations: std::num::NonZeroU8::new(30).unwrap() };
    opts.filter = indexset! {
        RowFilter::None, RowFilter::Sub, RowFilter::Up, RowFilter::Average, RowFilter::Paeth,
        RowFilter::MinSum, RowFilter::Entropy, RowFilter::Bigrams, RowFilter::BigEnt,
    };
    oxipng::optimize_from_memory(bytes, &opts).expect("zopfli")
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();
    let nupic = root.join("target/release/nupic");
    let orig = root.join("assets/png-bench/inputs/03-wikipedia-logo.png");
    let tiny = root.join("assets/png-bench/tinypng-web/03-wikipedia-logo.png");
    let tiny_size = std::fs::metadata(&tiny)?.len() as i64;
    let tiny_ssim = ssim(&orig, &tiny, &nupic);
    let cap = tiny_size * 80 / 100;

    let img = ImageReader::open(&orig)?.with_guessed_format()?.decode()?;
    let r = img.to_rgba8();
    let w = r.width(); let h = r.height();
    let raw = r.into_raw();

    println!("Cycle 102 attempt 4 — 03 wiki K<96 floor probe");
    println!("Target: size ≤ {} B  (tinypng {} B, gate 0.80×)  AND SSIM ≥ {:.2}", cap, tiny_size, tiny_ssim);
    println!();
    let tmp_dir = std::env::temp_dir();
    let ks = [4usize, 8, 12, 16, 24, 32, 48, 64, 80, 96];
    let ds = [0.0f32, 0.3, 0.6];
    let mut best: Option<(usize, f32, u8, usize, f64)> = None;
    let mut best_zopfli: Option<(usize, f32, u8, usize, f64)> = None;
    for &k in &ks {
        for &d in &ds {
            let q = quantize(&raw, w, h, k, d, 6);
            let path = tmp_dir.join(format!("c102a4_k{}_d{:.1}.png", k, d));
            std::fs::write(&path, &q)?;
            let s = ssim(&orig, &path, &nupic);
            let z = oxipng_zopfli(&q);
            let zpath = tmp_dir.join(format!("c102a4_k{}_d{:.1}_z.png", k, d));
            std::fs::write(&zpath, &z)?;
            let zs = ssim(&orig, &zpath, &nupic);
            let pass = (q.len() as i64) <= cap && s >= tiny_ssim;
            let zpass = (z.len() as i64) <= cap && zs >= tiny_ssim;
            println!("  K={:>3} d={:.1}  q={:>5}B SSIM {:>6.2} {}   zopfli={:>5}B SSIM {:>6.2} {}",
                     k, d, q.len(), s, mark(pass), z.len(), zs, mark(zpass));
            if pass {
                let cand = (k, d, 6, q.len(), s);
                match best {
                    None => best = Some(cand),
                    Some((_,_,_,bs,_)) if q.len() < bs => best = Some(cand),
                    _ => {}
                }
            }
            if zpass {
                let cand = (k, d, 6, z.len(), zs);
                match best_zopfli {
                    None => best_zopfli = Some(cand),
                    Some((_,_,_,bs,_)) if z.len() < bs => best_zopfli = Some(cand),
                    _ => {}
                }
            }
        }
    }
    println!();
    match best {
        Some((k, d, p, sz, s)) => println!("Best gate-passing (no zopfli): K={} d={:.1} p={}  {}B  SSIM {:.2}  (-{:.1}% vs cap)",
                                            k, d, p, sz, s, (1.0 - sz as f64/cap as f64)*100.0),
        None => println!("No gate-passing without zopfli."),
    }
    match best_zopfli {
        Some((k, d, p, sz, s)) => println!("Best gate-passing (with zopfli): K={} d={:.1} p={}  {}B  SSIM {:.2}  (-{:.1}% vs cap)",
                                            k, d, p, sz, s, (1.0 - sz as f64/cap as f64)*100.0),
        None => println!("No gate-passing even with zopfli."),
    }

    Ok(())
}

fn mark(b: bool) -> &'static str { if b { "✓" } else { "✗" } }
