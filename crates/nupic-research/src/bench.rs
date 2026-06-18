//! Bench helpers for cycle spike binaries.
//!
//! Solves two recurring pain points across cycle 106-107+:
//!
//! 1. **No re-computation of baseline data.** TinyPNG size and DSSIM are
//!    fixed per fixture; the corpus-500 baseline TSVs already hold them.
//!    [`Fixture::load_corpus_500_with_baseline`] merges
//!    `corpus-500-three-axis.tsv` + `corpus-500-dssim.tsv` +
//!    `cycle107/pile_classification.tsv` into ready-to-iterate fixtures,
//!    halving DSSIM work per spike (no more `dssim_of_path(tiny)` in the
//!    inner loop).
//!
//! 2. **Rayon thread-pool default crushes the machine.** `par_iter`
//!    grabs all logical cores; on M2 with 13 cores the user's UI freezes
//!    during multi-minute sweeps. [`bench_pool`] returns a 4-thread pool
//!    by default (override with `NUPIC_BENCH_THREADS` env var).
//!
//! Sample sizes default to **31 fixtures**(baseline-7 + 8 + 8 + 8 per
//! pile) so that a single-config sweep finishes in ~30-60s and matches
//! the Cycle 102-105 SSIM-era iteration cadence. See
//! [[feedback-no-long-sweeps-in-workflow]] in `memory/`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rayon::ThreadPool;

/// One fixture with all v1.2.8 baseline data pre-loaded — no DSSIM /
/// PNG decode required to consume it.
#[derive(Clone, Debug)]
pub struct Fixture {
    pub name: String,
    pub family: String,
    pub pile: String,
    pub input_size: u64,
    pub baseline_nupic_size: u64,
    pub tiny_size: u64,
    pub baseline_nupic_dssim: f64,
    pub tiny_dssim: f64,
}

impl Fixture {
    /// `size_pass`(against TinyPNG 0.80× cap).
    pub fn size_cap(&self) -> u64 {
        (self.tiny_size as f64 * 0.80) as u64
    }

    /// Two-axis PASS test: caller passes the spike's (size, dssim).
    pub fn passes(&self, size: u64, dssim: f64) -> bool {
        size <= self.size_cap() && dssim <= self.tiny_dssim
    }
}

fn classify_family(fname: &str) -> &'static str {
    if fname.starts_with("mi") {
        return "mi";
    }
    if fname.starts_with("wm") {
        return "wm";
    }
    if fname.chars().next() == Some('n') && fname[1..].chars().next().map_or(false, |c| c.is_ascii_digit()) {
        return "n";
    }
    if fname.chars().next() == Some('p') && fname[1..].chars().next().map_or(false, |c| c.is_ascii_digit()) {
        return "p";
    }
    if fname.starts_with('s') {
        return "s";
    }
    "?"
}

/// Load all corpus-500 fixtures with v1.2.8 baseline data pre-merged.
///
/// Reads (from `root/assets/png-bench/`):
/// - `corpus-500-three-axis.tsv` — (input_size, nupic_v128_size, tiny_size)
/// - `corpus-500-dssim.tsv` — (nupic_v128_dssim, tiny_dssim)
/// - `cycle107/pile_classification.tsv` — pile = PASS / PileA / PileB / PileC
pub fn load_corpus_500_with_baseline(root: &Path) -> anyhow::Result<Vec<Fixture>> {
    let size_tsv = root.join("assets/png-bench/corpus-500-three-axis.tsv");
    let dss_tsv = root.join("assets/png-bench/corpus-500-dssim.tsv");
    let pile_tsv = root.join("assets/png-bench/cycle107/pile_classification.tsv");

    let mut sizes: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
    let txt = std::fs::read_to_string(&size_tsv)?;
    for (i, line) in txt.lines().enumerate() {
        if i == 0 || line.trim().is_empty() {
            continue;
        }
        let c: Vec<&str> = line.split('\t').collect();
        if c.len() < 5 {
            continue;
        }
        sizes.insert(
            c[0].to_string(),
            (c[1].parse().unwrap_or(0), c[2].parse().unwrap_or(0), c[3].parse().unwrap_or(0)),
        );
    }

    let mut dssims: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    let txt = std::fs::read_to_string(&dss_tsv)?;
    for (i, line) in txt.lines().enumerate() {
        if i == 0 || line.trim().is_empty() {
            continue;
        }
        let c: Vec<&str> = line.split('\t').collect();
        if c.len() < 3 {
            continue;
        }
        dssims.insert(
            c[0].to_string(),
            (c[1].parse().unwrap_or(0.0), c[2].parse().unwrap_or(0.0)),
        );
    }

    let mut piles: BTreeMap<String, String> = BTreeMap::new();
    if pile_tsv.exists() {
        let txt = std::fs::read_to_string(&pile_tsv)?;
        for (i, line) in txt.lines().enumerate() {
            if i == 0 || line.trim().is_empty() {
                continue;
            }
            let c: Vec<&str> = line.split('\t').collect();
            if c.len() < 3 {
                continue;
            }
            piles.insert(c[0].to_string(), c[2].to_string());
        }
    }

    let mut out = Vec::new();
    for (name, (input_size, baseline_nupic_size, tiny_size)) in &sizes {
        let (baseline_nupic_dssim, tiny_dssim) = match dssims.get(name) {
            Some(v) => *v,
            None => continue,
        };
        let pile = piles.get(name).cloned().unwrap_or_else(|| "?".to_string());
        out.push(Fixture {
            name: name.clone(),
            family: classify_family(name).to_string(),
            pile,
            input_size: *input_size,
            baseline_nupic_size: *baseline_nupic_size,
            tiny_size: *tiny_size,
            baseline_nupic_dssim,
            tiny_dssim,
        });
    }
    Ok(out)
}

/// Stratified sample by pile: `per_pile` items from each of
/// {PASS, PileA, PileB, PileC} using deterministic stride-sampling.
/// Returns ≤ 4 * per_pile fixtures.
pub fn stratified_by_pile(fixtures: &[Fixture], per_pile: usize) -> Vec<Fixture> {
    let mut by_pile: BTreeMap<String, Vec<Fixture>> = BTreeMap::new();
    for f in fixtures {
        by_pile.entry(f.pile.clone()).or_default().push(f.clone());
    }
    let mut out = Vec::new();
    for (_p, mut rows) in by_pile {
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        let take = per_pile.min(rows.len());
        if take == rows.len() {
            out.extend(rows);
        } else {
            let stride = rows.len() as f64 / take as f64;
            for i in 0..take {
                let idx = (i as f64 * stride) as usize;
                out.push(rows[idx].clone());
            }
        }
    }
    out
}

/// The 7 fixtures used as the production sanity cohort.
pub const BASELINE_7: &[&str] = &[
    "01-png-transparency-demo.png",
    "02-pluto-transparent.png",
    "03-wikipedia-logo.png",
    "04-photo-portrait.png",
    "05-photo-mountain.png",
    "06-photo-landscape.png",
    "07-photo-product.png",
];

/// Filter to just the baseline-7. Returns empty if not present in corpus.
pub fn baseline_7(fixtures: &[Fixture]) -> Vec<Fixture> {
    let set: std::collections::HashSet<&str> = BASELINE_7.iter().copied().collect();
    fixtures.iter().filter(|f| set.contains(f.name.as_str())).cloned().collect()
}

/// 31-fixture spike default: baseline-7 + 8 PASS + 8 PileA + 8 PileB
/// (PileC only 53 so 8 fits). baseline-7 sits in `inputs/` not in
/// corpus-500 so requires a different load path; this returns the 24
/// pile sample only and the caller adds baseline-7 separately.
pub fn pile_sample_24(fixtures: &[Fixture]) -> Vec<Fixture> {
    let mut out = Vec::new();
    for pile in &["PASS", "PileA", "PileB", "PileC"] {
        let mut rows: Vec<Fixture> = fixtures
            .iter()
            .filter(|f| f.pile == *pile)
            .cloned()
            .collect();
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        let take = 8.min(rows.len());
        if take == rows.len() {
            out.extend(rows);
        } else {
            let stride = rows.len() as f64 / take as f64;
            for i in 0..take {
                let idx = (i as f64 * stride) as usize;
                out.push(rows[idx].clone());
            }
        }
    }
    out
}

/// Rayon thread pool capped to a safe core count.
///
/// Reads `NUPIC_BENCH_THREADS` env var, default 4 — leaves the user's
/// machine responsive during multi-minute sweeps. M2 has 13 cores; full
/// `par_iter` saturates them and freezes the UI.
pub fn bench_pool() -> anyhow::Result<ThreadPool> {
    let n = std::env::var("NUPIC_BENCH_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(4);
    Ok(rayon::ThreadPoolBuilder::new().num_threads(n).build()?)
}

/// Workspace root (resolve from `CARGO_MANIFEST_DIR` of the calling crate).
pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf()
}
