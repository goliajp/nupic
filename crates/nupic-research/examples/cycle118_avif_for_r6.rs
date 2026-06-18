//! Cycle 118 — AVIF transcoder probe for R6 cohort (sibling of Cycle 116 WebP).
//!
//! v1.2.10 ships --photo-rescue-webp; Cycle 118 asks whether AVIF is
//! enough better than WebP to warrant a second transcoder flag.
//!
//! Method:
//! 1. nupic compress -f avif -q N (subprocess) — N ∈ {50, 70, 80, 90}
//! 2. sips -s format png to decode AVIF → PNG (image crate has no AVIF
//!    decoder; sips is on macOS 12+)
//! 3. DSSIM via nupic_core::metrics on decoded PNG vs original
//! 4. Compare to Cycle 116 WebP best per fixture
//!
//! Decision gate (vs WebP best):
//! - AVIF mean size < 0.80× WebP mean size AND DSSIM ≥ WebP → wire
//!   --photo-rescue-avif flag, ship v1.2.11
//! - Marginal → keep WebP as default rescue

use std::path::PathBuf;
use std::process::Command;

use rayon::prelude::*;

use nupic_core::metrics;
use nupic_core::Image;
use nupic_research::bench::{bench_pool, workspace_root};

fn compress_avif(nupic: &PathBuf, src: &PathBuf, dst: &PathBuf, q: u8) -> anyhow::Result<()> {
    let status = Command::new(nupic)
        .args(["compress", "-f", "avif", "--strip-metadata", "-q"])
        .arg(q.to_string())
        .arg("-o").arg(dst).arg(src)
        .output()?;
    if !status.status.success() {
        anyhow::bail!("avif compress failed: {}", String::from_utf8_lossy(&status.stderr));
    }
    Ok(())
}

fn sips_avif_to_png(avif: &PathBuf, png: &PathBuf) -> anyhow::Result<()> {
    let status = Command::new("sips")
        .args(["-s", "format", "png"])
        .arg(avif).arg("--out").arg(png)
        .output()?;
    if !status.status.success() {
        anyhow::bail!("sips failed: {}", String::from_utf8_lossy(&status.stderr));
    }
    Ok(())
}

struct Out {
    fixture: String,
    q: u8,
    tiny_size: u64,
    tiny_dssim: f64,
    avif_size: u64,
    avif_dssim: f64,
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
    let tmp_dir = std::env::temp_dir().join("c118-avif");
    std::fs::create_dir_all(&tmp_dir)?;

    let fixtures: &[(&str, f64)] = &[
        ("p115_1024x768.png", 0.001970),
        ("p125_1920x1080.png", 0.009766),
        ("p167_1920x1080.png", 0.000880),
        ("p175_1920x1080.png", 0.001966),
        ("p214_2400x1600.png", 0.002845),
        ("p274_3840x2560.png", 0.003084),
    ];
    let qs: &[u8] = &[70, 80, 90];

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
            let avif_dst = tmp_dir.join(format!("{}_q{}.avif", name, q));
            let png_dst = tmp_dir.join(format!("{}_q{}.png", name, q));
            let reference = Image::open(&orig).ok()?;
            let tiny_size = std::fs::metadata(&tiny).ok()?.len();
            compress_avif(&nupic_bin, &orig, &avif_dst, *q).ok()?;
            let avif_size = std::fs::metadata(&avif_dst).ok()?.len();
            sips_avif_to_png(&avif_dst, &png_dst).ok()?;
            let avif_img = Image::open(&png_dst).ok()?;
            let avif_dssim = metrics::dssim(&reference, &avif_img).ok()?;
            let size_cap = (tiny_size as f64 * 0.80) as u64;
            let size_pass = avif_size <= size_cap;
            let dssim_pass = avif_dssim <= *tdss;
            let size_ratio = avif_size as f64 / tiny_size as f64;
            Some(Out {
                fixture: name.to_string(), q: *q,
                tiny_size, tiny_dssim: *tdss,
                avif_size, avif_dssim,
                size_ratio, size_pass, dssim_pass,
                both: size_pass && dssim_pass,
            })
        }).collect()
    });
    let dt = t0.elapsed();
    eprintln!("wall = {:.1}s ({} jobs)", dt.as_secs_f64(), results.len());

    println!("fixture\tq\ttiny_KB\ttiny_dssim\tavif_KB\tavif_dssim\tratio\tsize_pass\tdssim_pass\tboth");
    for r in &results {
        println!("{}\t{}\t{:.1}\t{:.6}\t{:.1}\t{:.6}\t{:.4}\t{}\t{}\t{}",
            r.fixture, r.q,
            r.tiny_size as f64 / 1024.0, r.tiny_dssim,
            r.avif_size as f64 / 1024.0, r.avif_dssim,
            r.size_ratio,
            if r.size_pass {"Y"} else {"N"},
            if r.dssim_pass {"Y"} else {"N"},
            if r.both {"Y"} else {"N"},
        );
    }

    use std::collections::BTreeMap;
    let mut best: BTreeMap<String, Option<(u8, u64, f64, f64)>> = BTreeMap::new();
    for r in &results {
        let e = best.entry(r.fixture.clone()).or_insert(None);
        if r.both {
            match e {
                None => *e = Some((r.q, r.avif_size, r.avif_dssim, r.size_ratio)),
                Some((_, bs, _, _)) if r.avif_size < *bs => {
                    *e = Some((r.q, r.avif_size, r.avif_dssim, r.size_ratio));
                }
                _ => {}
            }
        }
    }
    let pass_count = best.values().filter(|v| v.is_some()).count();
    let total = best.len();
    eprintln!();
    eprintln!("=== Cycle 118 AVIF for R6 cohort ===");
    eprintln!("PASS both: {}/{}", pass_count, total);
    for (fx, v) in &best {
        match v {
            Some((q, sz, ds, r)) => eprintln!("  {:<28} q={} size={:.1}KB ratio={:.3}× DSSIM={:.6}",
                fx, q, *sz as f64/1024.0, r, ds),
            None => eprintln!("  {:<28} (no q satisfies both)", fx),
        }
    }
    eprintln!();
    eprintln!("=== AVIF vs WebP (Cycle 116) per fixture ===");
    let webp_data: &[(&str, u8, f64, f64)] = &[
        ("p115_1024x768.png", 75, 17.3, 0.001373),
        ("p125_1920x1080.png", 75, 46.4, 0.007535),
        ("p167_1920x1080.png", 85, 51.3, 0.000699),
        ("p175_1920x1080.png", 75, 37.5, 0.001389),
        ("p214_2400x1600.png", 75, 102.0, 0.001680),
        ("p274_3840x2560.png", 75, 187.9, 0.001463),
    ];
    eprintln!("  {:<28} {:>10} {:>10} {:>12} {:>12}", "fixture", "WebP_KB", "AVIF_KB", "size_diff", "DSSIM_diff");
    for (fx, _wq, wkb, wdss) in webp_data {
        if let Some(Some((aq, asz, adss, _))) = best.get(*fx) {
            let akb = *asz as f64 / 1024.0;
            let size_diff_pct = 100.0 * (akb - wkb) / wkb;
            let dssim_diff = adss - wdss;
            eprintln!("  {:<28} {:>10.1} {:>10.1} {:>+11.1}% {:>+12.6}",
                fx, wkb, akb, size_diff_pct, dssim_diff);
        }
    }
    Ok(())
}
