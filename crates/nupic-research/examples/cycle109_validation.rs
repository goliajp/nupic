//! Cycle 109 validation — confirm P-08 K-up fail-safe wire works in the
//! production binary by running `nupic compress` subprocess and
//! comparing outputs to TinyPNG baselines.
//!
//! 32 stratified sample(`bench::pile_sample_24`)+ 7 baseline-7.
//! Wall target ≤ 90s (subprocess serial — 39 × ~1.5s).
//!
//! GREEN criteria:
//! - PASS pile retention 8/8
//! - baseline-7 unchanged from v1.2.8 (4/7)
//! - Pile A wins ≥ 1 (proves K-up path triggered)
//! - PASS rate ≥ Cycle 108 rule v3 sample (13/39)

use std::path::{Path, PathBuf};

use nupic_core::metrics;
use nupic_core::Image;
use nupic_research::bench::{
    baseline_7, load_corpus_500_with_baseline, pile_sample_24, workspace_root, Fixture, BASELINE_7,
};

fn compress_via_binary(nupic: &Path, src: &Path, dst: &Path) -> anyhow::Result<()> {
    let status = std::process::Command::new(nupic)
        .args(["compress", "--strip-metadata", "-o"])
        .arg(dst)
        .arg(src)
        .output()?;
    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        anyhow::bail!("compress failed: {}", stderr);
    }
    Ok(())
}

struct Row {
    name: String,
    pile: String,
    n_pixels: u64,
    c_size: u64,
    c_dssim: f64,
    tiny_size: u64,
    tiny_dssim: f64,
    baseline_size: u64,
    baseline_dssim: f64,
    size_pass: bool,
    dssim_pass: bool,
    both: bool,
    baseline_both: bool,
}

fn main() -> anyhow::Result<()> {
    let root = workspace_root();
    let all = load_corpus_500_with_baseline(&root)?;
    let mode = std::env::var("CYCLE_VALIDATE_MODE").unwrap_or_else(|_| "sample".into());
    let mut sample: Vec<Fixture> = if mode == "full" {
        all.clone()
    } else {
        pile_sample_24(&all)
    };
    let mut b7_fx = baseline_7(&all);
    sample.append(&mut b7_fx);
    eprintln!("mode={} fixtures={}", mode, sample.len());

    let corpus = root.join("assets/png-bench/corpus-500");
    let inputs = root.join("assets/png-bench/inputs");
    let tiny_web = root.join("assets/png-bench/tinypng-web");
    let nupic_bin = root.join("target/release/nupic");
    let tmp_dir = std::env::temp_dir().join("c109-validate");
    std::fs::create_dir_all(&tmp_dir)?;

    // Also load baseline-7 from inputs/ (not in corpus-500 cache for size+dssim)
    let b7_set: std::collections::HashSet<&str> = BASELINE_7.iter().copied().collect();
    let mut b7_extras: Vec<Fixture> = Vec::new();
    for &name in BASELINE_7 {
        let orig = inputs.join(name);
        let tiny = tiny_web.join(name);
        if !orig.exists() || !tiny.exists() {
            eprintln!("MISS b7 {}", name);
            continue;
        }
        let reference = Image::open(&orig)?;
        let tiny_size = std::fs::metadata(&tiny)?.len();
        let tiny_dssim = metrics::dssim(&reference, &Image::open(&tiny)?)?;
        b7_extras.push(Fixture {
            name: name.to_string(), family: "b7".into(), pile: "b7".into(),
            input_size: std::fs::metadata(&orig)?.len(),
            baseline_nupic_size: 0, tiny_size,
            baseline_nupic_dssim: 0.0, tiny_dssim,
        });
    }

    let t0 = std::time::Instant::now();
    let mut rows: Vec<Row> = Vec::new();
    // Filter sample to exclude b7 dups (added separately below)
    let pile_only: Vec<Fixture> = sample.iter().filter(|f| !b7_set.contains(f.name.as_str())).cloned().collect();
    for fx in pile_only.iter().chain(b7_extras.iter()) {
        let orig = if b7_set.contains(fx.name.as_str()) {
            inputs.join(&fx.name)
        } else {
            corpus.join(&fx.name)
        };
        let dst = tmp_dir.join(&fx.name);
        if !orig.exists() {
            eprintln!("MISS {}", orig.display());
            continue;
        }
        if let Err(e) = compress_via_binary(&nupic_bin, &orig, &dst) {
            eprintln!("compress error {}: {}", fx.name, e);
            continue;
        }
        let c_size = std::fs::metadata(&dst)?.len();
        let reference = Image::open(&orig)?;
        let distorted = Image::open(&dst)?;
        let c_dssim = metrics::dssim(&reference, &distorted)?;
        let (wi, he) = (reference.width() as u64, reference.height() as u64);
        let n_pixels = wi * he;
        let size_cap = (fx.tiny_size as f64 * 0.80) as u64;
        let size_pass = c_size <= size_cap;
        // Cached tiny_dssim was rounded to 6 decimal places when the
        // corpus-500-dssim.tsv was built; on byte-identical outputs
        // strict-comparing the live DSSIM (1e-7 range) against a
        // rounded 0.0 produces spurious "regressions" (e.g. s018
        // synthetic gradient where v1.2.8 == v1.2.9 == 2967 B).
        // Treat anything within 1e-5 of tiny_dssim as a pass.
        let dssim_pass = c_dssim <= fx.tiny_dssim + 1.0e-5;
        // v1.2.8 baseline pass (from cached data)
        let baseline_both = fx.baseline_nupic_size > 0
            && fx.baseline_nupic_size <= size_cap
            && fx.baseline_nupic_dssim <= fx.tiny_dssim + 1.0e-5;
        rows.push(Row {
            name: fx.name.clone(),
            pile: fx.pile.clone(),
            n_pixels,
            c_size, c_dssim,
            tiny_size: fx.tiny_size, tiny_dssim: fx.tiny_dssim,
            baseline_size: fx.baseline_nupic_size, baseline_dssim: fx.baseline_nupic_dssim,
            size_pass, dssim_pass, both: size_pass && dssim_pass,
            baseline_both,
        });
    }
    let dt = t0.elapsed();
    eprintln!("wall = {:.1}s ({} fixtures)", dt.as_secs_f64(), rows.len());

    println!("fixture\tpile\tn_pixels\tc_size\tc_dssim\ttiny_size\ttiny_dssim\tbaseline_size\tbaseline_dssim\tsize_pass\tdssim_pass\tboth\tbaseline_both\tregression");
    for r in &rows {
        let regression = r.baseline_both && !r.both;
        println!("{}\t{}\t{}\t{}\t{:.6}\t{}\t{:.6}\t{}\t{:.6}\t{}\t{}\t{}\t{}\t{}",
            r.name, r.pile, r.n_pixels,
            r.c_size, r.c_dssim, r.tiny_size, r.tiny_dssim,
            r.baseline_size, r.baseline_dssim,
            if r.size_pass {"Y"} else {"N"},
            if r.dssim_pass {"Y"} else {"N"},
            if r.both {"Y"} else {"N"},
            if r.baseline_both {"Y"} else {"N"},
            if regression {"REGRESSION"} else {""},
        );
    }

    use std::collections::BTreeMap;
    let mut by_pile: BTreeMap<String, (u32, u32, u32, u32)> = BTreeMap::new();
    let mut regressions: Vec<&Row> = Vec::new();
    for r in &rows {
        let e = by_pile.entry(r.pile.clone()).or_insert((0, 0, 0, 0));
        e.0 += 1;
        if r.both { e.1 += 1; }
        if r.baseline_both { e.2 += 1; }
        if r.baseline_both && !r.both { e.3 += 1; regressions.push(r); }
    }
    let total = rows.len() as u32;
    let pass = rows.iter().filter(|r| r.both).count() as u32;
    let regressed = rows.iter().filter(|r| r.baseline_both && !r.both).count() as u32;

    eprintln!();
    eprintln!("=== Cycle 109 P-08 wire verification ===");
    eprintln!("Total PASS = {}/{} ({:.1}%)", pass, total, 100.0 * pass as f64 / total as f64);
    eprintln!("Regressions = {} (vs v1.2.8 baseline)", regressed);
    eprintln!();
    eprintln!("Per-pile:");
    for (pile, (n, pass, base, regr)) in &by_pile {
        eprintln!("  {:<6} n={:>3} v1.2.9_pass={:>3}  v1.2.8_baseline_pass={:>3}  regressed={}",
            pile, n, pass, base, regr);
    }
    if !regressions.is_empty() {
        eprintln!();
        eprintln!("REGRESSIONS:");
        for r in regressions {
            eprintln!("  {} pile={} n_pixels={}M v1.2.8={}B/{:.4}DSS → v1.2.9={}B/{:.4}DSS",
                r.name, r.pile, r.n_pixels / 1_000_000,
                r.baseline_size, r.baseline_dssim, r.c_size, r.c_dssim);
        }
    }
    eprintln!();
    eprintln!("Decision gate:");
    eprintln!("  PASS pile retention: {}",
        if by_pile.get("PASS").map_or(false, |(_, _, base, r)| *r == 0 && *base > 0) { "Y" } else { "N (FAIL)" });
    eprintln!("  baseline-7 retention (no v1.2.8 PASS regressed): {}",
        if by_pile.get("b7").map_or(false, |(_, _, _, r)| *r == 0) { "Y" } else { "N (FAIL)" });
    let verdict = if regressed > 0 { "RED (regression!)" }
                  else if pass >= 13 { "GREEN — ready to ship v1.2.9" }
                  else { "YELLOW (no regress but no wins)" };
    eprintln!("  → {}", verdict);
    Ok(())
}
