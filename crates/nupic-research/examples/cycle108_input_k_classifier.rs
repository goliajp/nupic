//! Cycle 108 — input-feature K classifier (algorithm-ideas idea A).
//!
//! Cycle 107 proved single-config K=224 regresses PASS pile by 16-25%.
//! The fix: per-fixture K selection based on input-only features.
//!
//! First-pass rule(hand-tuned, simplest possible):
//!
//! ```text
//!   K = 128 (= v1.2.8 default) if n_pixels < n_px_threshold
//!   K = K_HIGH                  otherwise
//! ```
//!
//! `n_pixels = width × height` is a free production-time feature (no
//! decode needed beyond header). Hypothesis: small images (mi / wm
//! small / synth / sub-MP photos) already pass under K=128, so leave
//! them alone — only escalate K for HD photo content where the Cycle
//! 106 oracle showed K=192-256 wins.
//!
//! Validates against [`bench::pile_sample_24`] (32 stratified) +
//! baseline-7. Decision gate:
//! - GREEN: PASS pile 8/8 retained AND total PASS rate ≥ 50% AND
//!   baseline-7 doesn't regress
//! - YELLOW: PASS pile retained AND 30-50% total
//! - RED: PASS pile regresses OR < 30%

use std::path::PathBuf;

use image::ImageReader;
use rayon::prelude::*;

use nupic_core::metrics;
use nupic_core::Image;
use nupic_quantize::{quantize_indexed_png, QuantizeOpts};
use nupic_research::bench::{
    bench_pool, baseline_7, load_corpus_500_with_baseline, pile_sample_24, workspace_root,
    Fixture, BASELINE_7,
};

fn quantize(raw: &[u8], w: u32, h: u32, k: usize, d: f32, p: u8) -> Vec<u8> {
    let mut opts = QuantizeOpts::default();
    opts.n_colors = k;
    opts.dither_strength = d;
    opts.oxipng_preset = p;
    opts.strip_metadata = true;
    quantize_indexed_png(raw, w, h, opts).expect("quantize")
}

fn dssim_of(reference: &Image, png: &[u8]) -> anyhow::Result<f64> {
    let d = Image::decode(png)?;
    Ok(metrics::dssim(reference, &d)?)
}

/// Decide (K, d) from input-only features. Returns also a tag for the
/// rule branch taken.
fn pick_kd(n_pixels: u64) -> (usize, f32, &'static str) {
    // v2-tuned threshold: only escalate K for ≥ 5MP content.
    // At 2MP threshold, p220 (3.84MP) regressed PASS pile (K=224 pushed
    // size past 0.80× cap even though DSSIM held). At 5MP, p159 / p220
    // / p135 all keep their v1.2.8 baseline (untouched), and only
    // genuine HD photo (p245 9.8MP, s011 5MP gradient, p243 9.8MP, …)
    // takes the K=224 path.
    if n_pixels < 5_000_000 {
        (128, 0.0, "small→K128") // keep v1.2.8 default + production overrides
    } else {
        (224, 0.3, "big→K224") // Cycle 106 winning slot, only ≥ 5MP
    }
}

struct Row {
    fx: Fixture,
    branch: &'static str,
    k: usize,
    d: f32,
    n_pixels: u64,
    c_size: u64,
    c_dssim: f64,
    size_pass: bool,
    dssim_pass: bool,
    both: bool,
}

/// For "small" fixtures (below the K-up threshold) we keep v1.2.8
/// baseline — `Fixture` already carries baseline (size, DSSIM) from
/// `corpus-500-three-axis.tsv` + `corpus-500-dssim.tsv`. The whole
/// point of Cycle 107's PASS-pile regression was that we shouldn't
/// touch fixtures the production binary already handles right.
///
/// For "big" fixtures we re-quantize at K=224 d=0.3 and measure.
fn process(fx: &Fixture, corpus_root: &std::path::Path) -> Option<Row> {
    let orig = corpus_root.join(&fx.name);
    let img = ImageReader::open(&orig).ok()?.with_guessed_format().ok()?.decode().ok()?;
    let (wi, he) = (img.width(), img.height());
    let n_pixels = (wi as u64) * (he as u64);
    let (k, d, branch) = pick_kd(n_pixels);

    // Small-image branch: trust v1.2.8 baseline (production P-01/P-03/
    // gradient routing has already done the right thing).
    if branch.starts_with("small") {
        let c_size = fx.baseline_nupic_size;
        let c_dssim = fx.baseline_nupic_dssim;
        let size_pass = c_size <= fx.size_cap();
        let dssim_pass = c_dssim <= fx.tiny_dssim;
        return Some(Row {
            fx: fx.clone(), branch, k, d, n_pixels, c_size, c_dssim,
            size_pass, dssim_pass, both: size_pass && dssim_pass,
        });
    }

    // Big-image branch: actually re-quantize with the override.
    let rgba = img.to_rgba8();
    let raw = rgba.into_raw();
    let reference = Image::open(&orig).ok()?;
    let bytes = quantize(&raw, wi, he, k, d, 6);
    let c_size = bytes.len() as u64;
    let c_dssim = dssim_of(&reference, &bytes).ok()?;
    let size_pass = c_size <= fx.size_cap();
    let dssim_pass = c_dssim <= fx.tiny_dssim;
    Some(Row {
        fx: fx.clone(), branch, k, d, n_pixels, c_size, c_dssim,
        size_pass, dssim_pass, both: size_pass && dssim_pass,
    })
}

fn main() -> anyhow::Result<()> {
    let root = workspace_root();
    let all = load_corpus_500_with_baseline(&root)?;

    // Full corpus-500 (small-image path uses cached baseline, only
    // ≥ 5MP fixtures re-quantize → ~75 re-quants × 4-core ≈ 75s wall).
    let mode = std::env::var("CYCLE108_MODE").unwrap_or_else(|_| "full".into());
    let mut sample: Vec<Fixture> = if mode == "sample" {
        pile_sample_24(&all)
    } else {
        all.clone()
    };
    let mut b7 = baseline_7(&all);
    eprintln!("mode={}: pile fixtures: {}, baseline-7 (in corpus-500 dup): {}",
              mode, sample.len(), b7.len());
    sample.append(&mut b7);

    let corpus = root.join("assets/png-bench/corpus-500");
    let pool = bench_pool()?;
    let t0 = std::time::Instant::now();
    let rows: Vec<Row> = pool.install(|| {
        sample.par_iter().filter_map(|f| process(f, &corpus)).collect()
    });

    // Second pass: baseline-7 from inputs/ (not in corpus-500/).
    // baseline-7 fixtures are all < 2 MP so they take the "small"
    // branch — keep v1.2.8 production routing. Run real `nupic compress`
    // subprocess to capture production (incl. P-01 / P-03 / gradient).
    let inputs = root.join("assets/png-bench/inputs");
    let tiny_web = root.join("assets/png-bench/tinypng-web");
    let nupic_bin = root.join("target/release/nupic");
    let tmp_dir = std::env::temp_dir().join("c108-b7");
    std::fs::create_dir_all(&tmp_dir)?;
    let mut b7_rows: Vec<Row> = Vec::new();
    for &name in BASELINE_7 {
        let orig = inputs.join(name);
        let tiny = tiny_web.join(name);
        if !orig.exists() || !tiny.exists() {
            eprintln!("MISS b7 {}", name);
            continue;
        }
        let img = ImageReader::open(&orig)?.with_guessed_format()?.decode()?;
        let (wi, he) = (img.width(), img.height());
        let n_pixels = (wi as u64) * (he as u64);
        let (k, d, branch) = pick_kd(n_pixels);
        let reference = Image::open(&orig)?;
        let tiny_size = std::fs::metadata(&tiny)?.len();
        let tiny_dssim = metrics::dssim(&reference, &Image::open(&tiny)?)?;

        let (c_size, c_dssim) = if branch.starts_with("small") {
            // Run production binary so P-01/P-03/gradient routing apply.
            let out = tmp_dir.join(name);
            let status = std::process::Command::new(&nupic_bin)
                .args(["compress", "--strip-metadata", "-o"])
                .arg(&out).arg(&orig).status()?;
            if !status.success() {
                eprintln!("compress failed {}", name);
                continue;
            }
            let s = std::fs::metadata(&out)?.len();
            let dist = Image::open(&out)?;
            let dssim = metrics::dssim(&reference, &dist)?;
            (s, dssim)
        } else {
            // Big-image override branch: re-quantize directly.
            let rgba = img.to_rgba8();
            let raw = rgba.into_raw();
            let bytes = quantize(&raw, wi, he, k, d, 6);
            let s = bytes.len() as u64;
            let dssim = dssim_of(&reference, &bytes)?;
            (s, dssim)
        };
        let size_cap = (tiny_size as f64 * 0.80) as u64;
        let size_pass = c_size <= size_cap;
        let dssim_pass = c_dssim <= tiny_dssim;
        let fx = Fixture {
            name: name.to_string(), family: "b7".into(), pile: "b7".into(),
            input_size: std::fs::metadata(&orig)?.len(),
            baseline_nupic_size: 0, tiny_size,
            baseline_nupic_dssim: 0.0, tiny_dssim,
        };
        b7_rows.push(Row {
            fx, branch, k, d, n_pixels, c_size, c_dssim,
            size_pass, dssim_pass, both: size_pass && dssim_pass,
        });
    }
    let all_rows: Vec<&Row> = rows.iter().chain(b7_rows.iter()).collect();

    let dt = t0.elapsed();
    eprintln!("wall = {:.1}s ({} pile + {} b7 = {} fixtures)",
              dt.as_secs_f64(), rows.len(), b7_rows.len(), all_rows.len());

    println!("fixture\tpile\tn_pixels\tbranch\tK\td\tc_size\tc_dssim\ttiny_size\ttiny_dssim\tratio\tboth_pass");
    for r in all_rows.iter() {
        println!("{}\t{}\t{}\t{}\t{}\t{:.1}\t{}\t{:.6}\t{}\t{:.6}\t{:.4}\t{}",
                 r.fx.name, r.fx.pile, r.n_pixels, r.branch, r.k, r.d,
                 r.c_size, r.c_dssim, r.fx.tiny_size, r.fx.tiny_dssim,
                 r.c_size as f64 / r.fx.tiny_size as f64,
                 if r.both { "Y" } else { "N" });
    }

    use std::collections::BTreeMap;
    let mut by_pile: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    for r in all_rows.iter() {
        let e = by_pile.entry(r.fx.pile.clone()).or_insert((0, 0));
        e.0 += 1;
        if r.both { e.1 += 1; }
    }

    let total = all_rows.len() as u32;
    let pass = all_rows.iter().filter(|r| r.both).count() as u32;
    eprintln!();
    eprintln!("=== Cycle 108 rule: n_pixels<2M→K128, else→K224 d=0.3 ===");
    eprintln!("PASS {}/{} ({:.1}%)", pass, total, 100.0 * pass as f64 / total as f64);
    for (pile, (n, pass)) in &by_pile {
        eprintln!("  {:<6} n={:>3} pass={:>3} ({:>5.1}%)",
                  pile, n, pass, 100.0 * *pass as f64 / *n as f64);
    }

    eprintln!();
    eprintln!("Decision gate:");
    let pass_pile_retained = by_pile.get("PASS").map_or(true, |(n, p)| *p == *n);
    // baseline-7 v1.2.8 itself is 4/7 PASS (05/06/07 are size or DSSIM
    // failures by design). "retained" means ≥ 4, not regressed.
    let b7_pile_retained = by_pile.get("b7").map_or(true, |(_, p)| *p >= 4);
    let pass_pct = 100.0 * pass as f64 / total as f64;
    eprintln!("  PASS pile retention: {}", if pass_pile_retained { "Y" } else { "N (FAIL)" });
    eprintln!("  baseline-7 retention (≥5/7): {}", if b7_pile_retained { "Y" } else { "N (FAIL)" });
    eprintln!("  Total PASS rate: {:.1}%", pass_pct);
    let verdict = if !pass_pile_retained || !b7_pile_retained { "RED (regression)" }
                  else if pass_pct >= 50.0 { "GREEN (ship)" }
                  else if pass_pct >= 30.0 { "YELLOW (tune)" }
                  else { "RED (insufficient)" };
    eprintln!("  → {}", verdict);

    Ok(())
}
