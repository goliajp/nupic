//! Cycle 116 — WebP transcoder probe for R6 cohort.
//!
//! Cycle 110-114 confirmed 6 fixtures un-rescuable inside the
//! single-palette PNG container (lossless 1.36-1.95× tiny, R6 hybrid
//! strict DSSIM 0/6, .nupic small images palette-floor blocked).
//! Cycle 116 tests WebP lossy as the production-realizable rescue
//! path: nupic already supports `nupic compress -f webp -q N`.
//!
//! Sweep quality q ∈ {75, 80, 85, 90, 95}. Pick best q per fixture
//! satisfying `size ≤ 0.80× tiny ∧ DSSIM ≤ tiny_dssim`.
//!
//! If ≥ 3/6 PASS, P-09 production wiring: route the R6-like cohort
//! (input-feature trigger TBD Cycle 117) to WebP encoder. Ship v1.2.10.

use std::path::PathBuf;
use std::process::Command;

use rayon::prelude::*;

use nupic_core::metrics;
use nupic_core::Image;
use nupic_research::bench::{bench_pool, workspace_root};

fn compress_webp(nupic: &PathBuf, src: &PathBuf, dst: &PathBuf, q: u8) -> anyhow::Result<()> {
    let status = Command::new(nupic)
        .args(["compress", "-f", "webp", "--strip-metadata", "-q"])
        .arg(q.to_string())
        .arg("-o")
        .arg(dst)
        .arg(src)
        .output()?;
    if !status.status.success() {
        anyhow::bail!("webp compress failed: {}", String::from_utf8_lossy(&status.stderr));
    }
    Ok(())
}

struct Out {
    fixture: String,
    q: u8,
    tiny_size: u64,
    tiny_dssim: f64,
    webp_size: u64,
    webp_dssim: f64,
    size_ratio: f64,
    size_pass: bool,
    dssim_pass: bool,
    both: bool,
}

fn main() -> anyhow::Result<()> {
    let root = workspace_root();
    let corpus = root.join("assets/png-bench/corpus-500");
    let tiny_dir = root.join("assets/png-bench/tinypng-corpus-500");
    let nupic_bin = root.join("target/release/nupic");
    let tmp_dir = std::env::temp_dir().join("c116-webp");
    std::fs::create_dir_all(&tmp_dir)?;

    let fixtures: &[(&str, f64)] = &[
        ("p115_1024x768.png", 0.001970),
        ("p125_1920x1080.png", 0.009766),
        ("p167_1920x1080.png", 0.000880),
        ("p175_1920x1080.png", 0.001966),
        ("p214_2400x1600.png", 0.002845),
        ("p274_3840x2560.png", 0.003084),
    ];
    let qs: &[u8] = &[75, 80, 85, 90, 95];

    let pool = bench_pool()?;
    let t0 = std::time::Instant::now();
    let mut jobs: Vec<(&str, f64, u8)> = Vec::new();
    for &(name, tdss) in fixtures {
        for &q in qs {
            jobs.push((name, tdss, q));
        }
    }

    let results: Vec<Out> = pool.install(|| {
        jobs.par_iter().filter_map(|(name, tdss, q)| {
            let orig = corpus.join(name);
            let tiny = tiny_dir.join(name);
            let dst = tmp_dir.join(format!("{}_q{}.webp", name, q));
            let reference = Image::open(&orig).ok()?;
            let tiny_size = std::fs::metadata(&tiny).ok()?.len();
            compress_webp(&nupic_bin, &orig, &dst, *q).ok()?;
            let webp_size = std::fs::metadata(&dst).ok()?.len();
            let webp_img = Image::open(&dst).ok()?;
            let webp_dssim = metrics::dssim(&reference, &webp_img).ok()?;
            let size_cap = (tiny_size as f64 * 0.80) as u64;
            let size_pass = webp_size <= size_cap;
            let dssim_pass = webp_dssim <= *tdss;
            let size_ratio = webp_size as f64 / tiny_size as f64;
            Some(Out {
                fixture: name.to_string(), q: *q,
                tiny_size, tiny_dssim: *tdss,
                webp_size, webp_dssim,
                size_ratio, size_pass, dssim_pass,
                both: size_pass && dssim_pass,
            })
        }).collect()
    });
    let dt = t0.elapsed();
    eprintln!("wall = {:.1}s ({} jobs)", dt.as_secs_f64(), results.len());

    println!("fixture\tq\ttiny_KB\ttiny_dssim\twebp_KB\twebp_dssim\tratio\tsize_pass\tdssim_pass\tboth");
    for r in &results {
        println!("{}\t{}\t{:.1}\t{:.6}\t{:.1}\t{:.6}\t{:.4}\t{}\t{}\t{}",
            r.fixture, r.q,
            r.tiny_size as f64 / 1024.0, r.tiny_dssim,
            r.webp_size as f64 / 1024.0, r.webp_dssim,
            r.size_ratio,
            if r.size_pass {"Y"} else {"N"},
            if r.dssim_pass {"Y"} else {"N"},
            if r.both {"Y"} else {"N"},
        );
    }

    // Best q per fixture (smallest size satisfying both gates).
    use std::collections::BTreeMap;
    let mut best: BTreeMap<String, Option<(u8, u64, f64, f64)>> = BTreeMap::new();
    for r in &results {
        let e = best.entry(r.fixture.clone()).or_insert(None);
        if r.both {
            match e {
                None => *e = Some((r.q, r.webp_size, r.webp_dssim, r.size_ratio)),
                Some((_, bs, _, _)) if r.webp_size < *bs => {
                    *e = Some((r.q, r.webp_size, r.webp_dssim, r.size_ratio));
                }
                _ => {}
            }
        }
    }
    let pass_count = best.values().filter(|v| v.is_some()).count();
    let total = best.len();
    eprintln!();
    eprintln!("=== Cycle 116 WebP for R6 cohort ===");
    eprintln!("PASS both: {}/{}", pass_count, total);
    eprintln!();
    eprintln!("Best q per fixture:");
    for (fx, v) in &best {
        match v {
            Some((q, sz, ds, r)) => {
                eprintln!("  {:<28} q={} size={:.1}KB ratio={:.3}× DSSIM={:.6}", fx, q, *sz as f64/1024.0, r, ds);
            }
            None => eprintln!("  {:<28} (no q satisfies both)", fx),
        }
    }
    eprintln!();
    let verdict = if pass_count >= 3 {
        "GREEN — WebP rescue viable, P-09 wire candidate for v1.2.10"
    } else if pass_count >= 1 {
        "YELLOW — partial rescue, tune q range or accept partial coverage"
    } else {
        "RED — WebP also fails strict DSSIM gate"
    };
    eprintln!("→ {}", verdict);

    Ok(())
}
