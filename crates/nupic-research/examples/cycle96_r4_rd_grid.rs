//! Cycle 96 — R4 rate-distortion grid spike (paper §5 framework)
//!
//! Per-fixture grid over (n_colors × dither_strength × oxipng_preset) to map
//! the achievable (size, SSIM) Pareto front. Compare to the current production
//! default (classifier-picked n, dither=0, preset=auto-tier) — distance to
//! Pareto front quantifies "how much room is left under one-size-fits-all
//! routing."
//!
//! Grid (3 × 3 × 3 = 27 configs per fixture, 7 fixtures = 189 encodes):
//!   K_colors ∈ {128, 192, 256}
//!   dither_strength ∈ {0.0, 0.3, 0.5}
//!   oxipng_preset ∈ {0, 1, 3}
//!
//! Decision gate (per roadmap R4):
//!   median Pareto-front -%size at iso-SSIM (vs default) ≥ 3%  → GREEN ship-config
//!   median 0-3%                                                → YELLOW
//!   median <0% (default IS Pareto-front)                       → RED

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use image::ImageReader;

use nupic_quantize::{quantize_indexed_png, QuantizeOpts};

fn ssim_via_nupic(orig: &PathBuf, cmp_path: &PathBuf, nupic: &PathBuf) -> f64 {
    let out = Command::new(nupic)
        .args(["compare", "-m", "ssimulacra2"])
        .arg(orig).arg(cmp_path)
        .output().expect("nupic compare");
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines().find_map(|l| {
        l.strip_prefix("SSIMULACRA2: ")
            .and_then(|v| v.split_whitespace().next())
            .and_then(|n| n.parse::<f64>().ok())
    }).unwrap_or(f64::NAN)
}

#[derive(Clone, Copy, Debug)]
struct Config {
    n_colors: usize,
    dither: f32,
    preset: u8,
}

#[derive(Clone, Debug)]
struct Result {
    cfg: Config,
    size: usize,
    ssim: f64,
}

fn run_grid_one(fixture_path: &PathBuf, nupic: &PathBuf, label: &str) -> Vec<Result> {
    let img = ImageReader::open(fixture_path).expect("open")
        .with_guessed_format().expect("fmt").decode().expect("decode");
    let r = img.to_rgba8();
    let w = r.width();
    let h = r.height();
    let raw = r.into_raw();
    let tmp = std::env::temp_dir();

    let mut out = Vec::new();
    let ns = [128usize, 192, 256];
    let dithers = [0.0f32, 0.3, 0.5];
    let presets = [0u8, 1, 3];
    for &n in &ns {
        for &d in &dithers {
            for &p in &presets {
                let mut opts = QuantizeOpts::default();
                opts.n_colors = n;
                opts.dither_strength = d;
                opts.oxipng_preset = p;
                opts.strip_metadata = true;
                let bytes = match quantize_indexed_png(&raw, w, h, opts) {
                    Ok(b) => b,
                    Err(e) => {
                        println!("  ERR {} n={} d={} p={}: {:?}", label, n, d, p, e);
                        continue;
                    }
                };
                let path = tmp.join(format!("c96_{}_n{}_d{:.1}_p{}.png", label, n, d, p));
                std::fs::write(&path, &bytes).expect("write");
                let ssim = ssim_via_nupic(fixture_path, &path, nupic);
                out.push(Result {
                    cfg: Config { n_colors: n, dither: d, preset: p },
                    size: bytes.len(),
                    ssim,
                });
            }
        }
    }
    out
}

// Pareto front: minimise size, maximise SSIM. Result r dominates r' iff
// size(r) <= size(r') AND ssim(r) >= ssim(r') AND not both equal.
fn pareto_front(results: &[Result]) -> Vec<usize> {
    let mut front = Vec::new();
    'outer: for (i, r) in results.iter().enumerate() {
        for (j, r2) in results.iter().enumerate() {
            if i == j { continue; }
            let strictly_better = (r2.size <= r.size && r2.ssim >= r.ssim) &&
                                  (r2.size < r.size || r2.ssim > r.ssim);
            if strictly_better { continue 'outer; }
        }
        front.push(i);
    }
    front
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors().nth(2).unwrap().to_path_buf();
    let nupic = root.join("target/release/nupic");

    let fixtures: &[(&str, &str)] = &[
        ("inputs/01-png-transparency-demo.png", "01_trans"),
        ("inputs/02-pluto-transparent.png",     "02_pluto"),
        ("inputs/03-wikipedia-logo.png",        "03_wiki"),
        ("inputs/04-photo-portrait.png",        "04_portrait"),
        ("inputs/05-photo-mountain.png",        "05_mountain"),
        ("inputs/06-photo-landscape.png",       "06_landscape"),
        ("inputs/07-photo-product.png",         "07_product"),
    ];

    println!("Cycle 96 — R4 rate-distortion grid spike");
    println!("  grid: K ∈ {{128, 192, 256}} × d ∈ {{0.0, 0.3, 0.5}} × preset ∈ {{0, 1, 3}}");
    println!("        27 configs per fixture × 7 fixtures = 189 encodes");
    println!();

    let mut all_fixture_results: Vec<(String, Vec<Result>, Result, Vec<usize>)> = Vec::new();
    let t_total = Instant::now();
    for &(rel, lbl) in fixtures {
        let path = root.join("assets/png-bench").join(rel);
        let t0 = Instant::now();
        let results = run_grid_one(&path, &nupic, lbl);
        let t = t0.elapsed().as_secs_f64();

        // "Default-like": find the production-style config:
        // K=256 (Auto default), dither=0, preset matches the 3-tier rule
        // (for baseline-7 < 2 MP all use preset=3).
        // But we don't have K=256 always — n_colors gets capped by classifier
        // in production. For this grid we report against K=256 as the "Auto
        // default-equivalent" since user can opt-in via QuantizeOpts.
        let default_cfg = Config { n_colors: 256, dither: 0.0, preset: 3 };
        let default_r = results.iter()
            .find(|r| r.cfg.n_colors == default_cfg.n_colors
                   && (r.cfg.dither - default_cfg.dither).abs() < 0.01
                   && r.cfg.preset == default_cfg.preset)
            .cloned()
            .expect("default config must be in the grid");

        let front = pareto_front(&results);
        println!("[{}]  grid {:.1}s   default: K{} d{:.1} p{} → {} B, SSIM {:.2}   |   Pareto front: {} configs",
                 lbl, t,
                 default_r.cfg.n_colors, default_r.cfg.dither, default_r.cfg.preset,
                 default_r.size, default_r.ssim, front.len());
        all_fixture_results.push((lbl.to_string(), results, default_r, front));
    }

    println!();
    println!("Total grid time: {:.1}s", t_total.elapsed().as_secs_f64());
    println!();

    // === Analysis: per-fixture Pareto details + iso-SSIM size delta ===
    println!("=== Per-fixture analysis ===");
    println!("{:<14} {:>10} {:>9} {:>5} {:>5} {:>2} {:>10} {:>9} {:>9} {:>9}",
             "fixture", "def_B", "def_SSIM", "fr_n", "Δ-iso", "B", "best_B", "best_SSIM", "Δsize%", "best_cfg");
    let mut iso_size_pcts: Vec<f64> = Vec::new();
    for (lbl, results, default_r, front) in &all_fixture_results {
        // Best size at iso-SSIM (within 0.5 SSIM of default)
        let iso_band_lo = default_r.ssim - 0.5;
        let mut best_iso_size = default_r.size;
        let mut best_iso_idx: Option<usize> = None;
        for (i, r) in results.iter().enumerate() {
            if r.ssim >= iso_band_lo && r.size < best_iso_size {
                best_iso_size = r.size;
                best_iso_idx = Some(i);
            }
        }
        let (best_b, best_ssim, best_cfg_str) = match best_iso_idx {
            Some(i) => {
                let r = &results[i];
                (r.size, r.ssim, format!("K{} d{:.1} p{}", r.cfg.n_colors, r.cfg.dither, r.cfg.preset))
            }
            None => (default_r.size, default_r.ssim, format!("(default)")),
        };
        let pct = (best_b as f64 / default_r.size as f64 - 1.0) * 100.0;
        iso_size_pcts.push(pct);
        println!("{:<14} {:>10} {:>9.2} {:>5} {:>+5.0} {:>2} {:>10} {:>9.2} {:>+8.2}% {:>9}",
                 lbl, default_r.size, default_r.ssim, front.len(),
                 pct, 0, // Δ-iso column header is "iso-SSIM size%", best_idx hidden
                 best_b, best_ssim, pct, best_cfg_str);
    }
    println!();

    // Aggregate
    let mean_pct: f64 = iso_size_pcts.iter().sum::<f64>() / iso_size_pcts.len() as f64;
    let median_pct = {
        let mut s = iso_size_pcts.clone();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        s[s.len() / 2]
    };
    println!("Iso-SSIM (-0.5 band) size: mean {:+.2}%   median {:+.2}%", mean_pct, median_pct);
    println!();

    if median_pct <= -3.0 {
        println!(">>> GREEN — Pareto sweep finds ≥3% savings at iso-SSIM. R-D config tuning is shippable.");
    } else if median_pct <= 0.0 {
        println!(">>> YELLOW — Pareto savings 0-3%. Borderline ship.");
    } else {
        println!(">>> RED — current default is already on the Pareto front. No R-D win available.");
    }

    // Detail dump: for each fixture, the Pareto front sorted by SSIM
    println!();
    println!("=== Pareto front per fixture (sorted by SSIM ascending) ===");
    for (lbl, results, default_r, front) in &all_fixture_results {
        println!("[{}] default {} B / SSIM {:.2}", lbl, default_r.size, default_r.ssim);
        let mut front_results: Vec<&Result> = front.iter().map(|&i| &results[i]).collect();
        front_results.sort_by(|a, b| a.ssim.partial_cmp(&b.ssim).unwrap());
        for r in front_results {
            let marker = if r.cfg.n_colors == default_r.cfg.n_colors
                && (r.cfg.dither - default_r.cfg.dither).abs() < 0.01
                && r.cfg.preset == default_r.cfg.preset { " <-- default" } else { "" };
            println!("    K{:>3} d{:.1} p{}  {:>8} B  SSIM {:>6.2}{}",
                     r.cfg.n_colors, r.cfg.dither, r.cfg.preset, r.size, r.ssim, marker);
        }
    }

    Ok(())
}
