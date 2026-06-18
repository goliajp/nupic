//! Cycle 106 R4 — Rate-Distortion routing spike over Pile A (31 fixtures).
//!
//! Pile A = corpus-500 entries where v1.2.8 wastes bytes relative to TinyPNG
//! while DSSIM is already ~0 (nu_d ≈ 0, size > 1.3× tiny). Hypothesis: a
//! per-fixture (K, dither, preset) selector can trade a small DSSIM rise
//! (still ≤ tiny_dssim) for a large size drop, lifting cohort PASS rate.
//!
//! Grid: K ∈ {64, 96, 128, 192} × dither ∈ {0.0, 0.3, 0.6} × preset ∈ {3, 6}.
//! Per fixture: pick the lowest-size config that still satisfies
//!   size ≤ 0.80 × tiny_size  AND  dssim ≤ tiny_dssim.
//!
//! DSSIM is computed in-process via `nupic_core::metrics::dssim` (fast path —
//! no subprocess to the `nupic compare` CLI).

use std::path::{Path, PathBuf};

use image::ImageReader;

use nupic_core::metrics;
use nupic_core::Image;
use nupic_quantize::{quantize_indexed_png, QuantizeOpts};

struct Fixture {
    name: String,
    tiny_size: u64,
}

fn read_pile_a(tsv: &Path) -> anyhow::Result<Vec<Fixture>> {
    let text = std::fs::read_to_string(tsv)?;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if i == 0 || line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 7 {
            continue;
        }
        out.push(Fixture {
            name: cols[0].to_string(),
            tiny_size: cols[3].parse::<u64>().unwrap_or(0),
        });
    }
    Ok(out)
}

fn dssim_of(reference: &Image, png_bytes: &[u8]) -> anyhow::Result<f64> {
    let distorted = Image::decode(png_bytes)?;
    Ok(metrics::dssim(reference, &distorted)?)
}

fn dssim_of_path(reference: &Image, path: &Path) -> anyhow::Result<f64> {
    let distorted = Image::open(path)?;
    Ok(metrics::dssim(reference, &distorted)?)
}

fn quantize(raw: &[u8], w: u32, h: u32, k: usize, d: f32, p: u8) -> Vec<u8> {
    let mut opts = QuantizeOpts::default();
    opts.n_colors = k;
    opts.dither_strength = d;
    opts.oxipng_preset = p;
    opts.strip_metadata = true;
    quantize_indexed_png(raw, w, h, opts).expect("quantize")
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();
    let pile_a = root.join("assets/png-bench/corpus-500-pile-a.tsv");
    let corpus = root.join("assets/png-bench/corpus-500");
    let tiny_dir = root.join("assets/png-bench/tinypng-corpus-500");
    let out_dir = root.join("assets/png-bench/nupic-corpus-500-c106-r4");
    std::fs::create_dir_all(&out_dir)?;

    let fixtures = read_pile_a(&pile_a)?;
    eprintln!("Pile A: {} fixtures loaded", fixtures.len());

    let ks: [usize; 4] = [64, 96, 128, 192];
    let ds: [f32; 3] = [0.0, 0.3, 0.6];
    let ps: [u8; 2] = [3, 6];

    println!(
        "fixture\tw\th\tinput_size\ttiny_size\ttiny_dssim\tbest_K\tbest_d\tbest_p\tbest_size\tbest_dssim\tsize_ratio_vs_tiny\tdssim_delta_vs_tiny\tpass_0_80x\tpass_dssim\tpass_both"
    );

    let mut pass_both_count = 0u32;
    let mut pass_size_count = 0u32;
    let mut pass_dssim_count = 0u32;
    let mut total_tiny: u64 = 0;
    let mut total_best: u64 = 0;
    for fx in &fixtures {
        let orig_path = corpus.join(&fx.name);
        let tiny_path = tiny_dir.join(&fx.name);
        if !orig_path.exists() {
            eprintln!("MISS orig {}", orig_path.display());
            continue;
        }
        if !tiny_path.exists() {
            eprintln!("MISS tiny {}", tiny_path.display());
            continue;
        }
        let img = ImageReader::open(&orig_path)?.with_guessed_format()?.decode()?;
        let r = img.to_rgba8();
        let w = r.width();
        let h = r.height();
        let raw = r.into_raw();
        let input_size = std::fs::metadata(&orig_path)?.len();
        let tiny_size = std::fs::metadata(&tiny_path)?.len();
        let reference = Image::open(&orig_path)?;
        let tiny_dssim = dssim_of_path(&reference, &tiny_path)?;
        let cap_size = (tiny_size as f64 * 0.80) as u64;

        let mut best: Option<(usize, f32, u8, u64, f64)> = None;
        let mut min_size_seen: u64 = u64::MAX;
        let mut min_size_cfg: Option<(usize, f32, u8, f64)> = None;
        for &k in &ks {
            for &d in &ds {
                for &p in &ps {
                    let bytes = quantize(&raw, w, h, k, d, p);
                    let size = bytes.len() as u64;
                    let dssim = match dssim_of(&reference, &bytes) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("  dssim fail {} K={} d={:.1} p={}: {}", fx.name, k, d, p, e);
                            continue;
                        }
                    };
                    if size < min_size_seen {
                        min_size_seen = size;
                        min_size_cfg = Some((k, d, p, dssim));
                    }
                    let passes = size <= cap_size && dssim <= tiny_dssim;
                    if passes {
                        match best {
                            None => best = Some((k, d, p, size, dssim)),
                            Some((_, _, _, bs, _)) if size < bs => {
                                best = Some((k, d, p, size, dssim));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        // If nothing passes both gates, still report the smallest config so
        // we can see the floor.
        let (k, d, p, size, dssim, pass_size, pass_dssim, pass_both) = match best {
            Some((k, d, p, sz, ds_v)) => (k, d, p, sz, ds_v, true, true, true),
            None => {
                let (k, d, p, ds_v) = min_size_cfg.unwrap();
                let sz = min_size_seen;
                let pass_s = sz <= cap_size;
                let pass_d = ds_v <= tiny_dssim;
                (k, d, p, sz, ds_v, pass_s, pass_d, false)
            }
        };
        if pass_size {
            pass_size_count += 1;
        }
        if pass_dssim {
            pass_dssim_count += 1;
        }
        if pass_both {
            pass_both_count += 1;
        }
        total_tiny += tiny_size;
        total_best += size;

        let size_ratio = size as f64 / tiny_size as f64;
        let dssim_delta = dssim - tiny_dssim;
        println!(
            "{}\t{}\t{}\t{}\t{}\t{:.6}\t{}\t{:.1}\t{}\t{}\t{:.6}\t{:.4}\t{:+.6}\t{}\t{}\t{}",
            fx.name,
            w,
            h,
            input_size,
            tiny_size,
            tiny_dssim,
            k,
            d,
            p,
            size,
            dssim,
            size_ratio,
            dssim_delta,
            if pass_size { "Y" } else { "N" },
            if pass_dssim { "Y" } else { "N" },
            if pass_both { "Y" } else { "N" },
        );

        if pass_both {
            let dst = out_dir.join(&fx.name);
            let bytes = quantize(&raw, w, h, k, d, p);
            std::fs::write(&dst, &bytes)?;
        }
    }

    let cohort_ratio = if total_tiny > 0 {
        total_best as f64 / total_tiny as f64
    } else {
        0.0
    };
    eprintln!();
    eprintln!("=== Pile A summary ===");
    eprintln!("fixtures = {}", fixtures.len());
    eprintln!(
        "pass_both = {}/{} ({:.1}%)",
        pass_both_count,
        fixtures.len(),
        100.0 * pass_both_count as f64 / fixtures.len() as f64
    );
    eprintln!(
        "pass_size_only = {}/{} ({:.1}%)",
        pass_size_count,
        fixtures.len(),
        100.0 * pass_size_count as f64 / fixtures.len() as f64
    );
    eprintln!(
        "pass_dssim_only = {}/{} ({:.1}%)",
        pass_dssim_count,
        fixtures.len(),
        100.0 * pass_dssim_count as f64 / fixtures.len() as f64
    );
    eprintln!(
        "cohort total: best={} B  tiny={} B  ratio={:.4}x",
        total_best, total_tiny, cohort_ratio
    );

    Ok(())
}
