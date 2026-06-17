//! Cycle 101 — R10 oxipng filter prediction probe (paper §7 perf engineering)
//!
//! With the R4 routing thread closed at Cycle 100, we pivot to the next
//! paper-track item per [[research-roadmap-1-2-x]] § P4 R10:
//!
//!   "Learned filter prediction" — given an image, predict which oxipng
//!   RowFilter is optimal so oxipng can skip trying the others. Expected
//!   gain: 5MP −50-80ms (oxipng runs ~1 of 5 filters instead of 4-7).
//!
//! This first spike measures **per-fixture optimal filter distribution**
//! and **wall-time delta** vs the production preset. We don't train a
//! classifier yet — that's Cycle 102 conditional on this spike showing
//! the perf headroom exists.
//!
//! Cohort: baseline-7 (preset=3 → 4 filters tried by default) +
//!         5MP {17, 25, 27} (preset=0 → 1-2 filters tried by default).
//!
//! Method per fixture:
//!   1. Encode through nupic-quantize once → indexed PNG bytes (already
//!      oxipng-optimized with production preset).
//!   2. For each of 9 RowFilter variants {None, Sub, Up, Average, Paeth,
//!      MinSum, Entropy, Bigrams, BigEnt}, re-run oxipng on those bytes
//!      with `filter = { F }` only — measures size + wall time as if
//!      that filter alone had been picked.
//!   3. Also re-run oxipng with the production preset's default filter
//!      set (preset=3 default = {None, Sub, Entropy, Bigrams}; preset=0
//!      default ≈ {None, Sub, Bigrams}) for baseline comparison.
//!   4. Identify best-single-filter (smallest IDAT) per fixture.
//!
//! Decision gate:
//!   if best-single-filter wall-time is ≤ 50% of default-preset wall-time
//!   AND best-single-filter size is ≤ 0.5% larger than default-preset
//!   size → R10 has paper-track perf headroom, write Cycle 102 classifier
//!   spike (5-feature linear classifier → predicted filter).
//!
//!   else → log result, decide pivot (R3 VQ-VAE / R6 multi-tile).

use std::path::PathBuf;
use std::time::Instant;

use image::ImageReader;

use nupic_quantize::{quantize_indexed_png, QuantizeOpts};
use oxipng::{indexset, RowFilter};

#[derive(Clone, Copy, Debug, PartialEq)]
struct Cfg { k: usize, d: f32, p: u8 }

fn encode_nupic(raw: &[u8], w: u32, h: u32, cfg: Cfg) -> Vec<u8> {
    let mut opts = QuantizeOpts::default();
    opts.n_colors = cfg.k;
    opts.dither_strength = cfg.d;
    opts.oxipng_preset = cfg.p;
    opts.strip_metadata = true;
    quantize_indexed_png(raw, w, h, opts).expect("encode")
}

fn oxipng_with_filter(bytes: &[u8], filter: RowFilter, preset: u8) -> (Vec<u8>, f64) {
    // Build options from preset (so deflate config matches production) but
    // restrict the filter set to a single filter.
    let mut opts = oxipng::Options::from_preset(preset);
    opts.filter = indexset! { filter };
    let t = Instant::now();
    let out = oxipng::optimize_from_memory(bytes, &opts).expect("oxipng");
    (out, t.elapsed().as_secs_f64() * 1000.0)
}

fn oxipng_preset_default(bytes: &[u8], preset: u8) -> (Vec<u8>, f64) {
    let opts = oxipng::Options::from_preset(preset);
    let t = Instant::now();
    let out = oxipng::optimize_from_memory(bytes, &opts).expect("oxipng");
    (out, t.elapsed().as_secs_f64() * 1000.0)
}

const FILTERS: &[(RowFilter, &str)] = &[
    (RowFilter::None,    "None"),
    (RowFilter::Sub,     "Sub"),
    (RowFilter::Up,      "Up"),
    (RowFilter::Average, "Average"),
    (RowFilter::Paeth,   "Paeth"),
    (RowFilter::MinSum,  "MinSum"),
    (RowFilter::Entropy, "Entropy"),
    (RowFilter::Bigrams, "Bigrams"),
    (RowFilter::BigEnt,  "BigEnt"),
];

#[derive(Clone, Debug)]
struct FilterResult {
    name: &'static str,
    size: usize,
    wall_ms: f64,
}

#[derive(Clone, Debug)]
struct PerFx {
    label: String,
    n_pixels: usize,
    preset: u8,
    default_size: usize,
    default_wall: f64,
    per_filter: Vec<FilterResult>,
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();

    let fixtures: &[(&str, &str, u8)] = &[
        ("inputs/01-png-transparency-demo.png", "01_trans",     3),
        ("inputs/02-pluto-transparent.png",     "02_pluto",     3),
        ("inputs/03-wikipedia-logo.png",        "03_wiki",      3),
        ("inputs/04-photo-portrait.png",        "04_portrait",  3),
        ("inputs/05-photo-mountain.png",        "05_mountain",  3),
        ("inputs/06-photo-landscape.png",       "06_landscape", 3),
        ("inputs/07-photo-product.png",         "07_product",   3),
        ("inputs-ext-real/17-aurora-5mp.png",          "17_aurora",  0),
        ("inputs-ext-real/25-sofia-cathedral-5mp.png", "25_sofia",   0),
        ("inputs-ext-real/27-whale-tail-5mp.png",      "27_whale",   0),
    ];

    println!("Cycle 101 — R10 oxipng filter prediction probe");
    println!("  cohort: 10 fixtures (baseline-7 preset=3 + 3 × 5MP preset=0)");
    println!("  filters tested per fixture: {} single-filter RowFilters", FILTERS.len());
    println!();

    let mut per_fx: Vec<PerFx> = Vec::new();
    let t_total = Instant::now();

    for (rel, lbl, preset) in fixtures {
        let path = root.join("assets/png-bench").join(rel);
        if !path.exists() { println!("MISSING {}", lbl); continue; }
        let img = ImageReader::open(&path)?.with_guessed_format()?.decode()?;
        let r = img.to_rgba8();
        let w = r.width(); let h = r.height();
        let n_pixels = (w as usize) * (h as usize);
        let raw = r.into_raw();

        // Default-config nupic-quantize output (K=256 d=0 at tier preset)
        let cfg = Cfg { k: 256, d: 0.0, p: *preset };
        let nupic_bytes = encode_nupic(&raw, w, h, cfg);

        // Default oxipng (preset's full filter set) on top → baseline
        let (default_out, default_wall) = oxipng_preset_default(&nupic_bytes, *preset);
        let default_size = default_out.len();

        // Per-filter forced re-encode
        let mut per_filter = Vec::with_capacity(FILTERS.len());
        for &(f, name) in FILTERS {
            let (out, wall) = oxipng_with_filter(&nupic_bytes, f, *preset);
            per_filter.push(FilterResult { name, size: out.len(), wall_ms: wall });
        }

        // Print row
        let best_idx = per_filter.iter().enumerate().min_by_key(|(_, r)| r.size).map(|(i, _)| i).unwrap_or(0);
        let best = &per_filter[best_idx];
        let bs_pct = (best.size as f64 / default_size as f64 - 1.0) * 100.0;
        println!("[{:<14}] preset={} | default {:>7} B / {:>6.1} ms | best {:>7} B {:>+6.2}% / {:>6.1} ms = {:.0}% wall | filter={}",
                 lbl, preset, default_size, default_wall,
                 best.size, bs_pct, best.wall_ms,
                 best.wall_ms / default_wall * 100.0, best.name);

        per_fx.push(PerFx {
            label: lbl.to_string(),
            n_pixels,
            preset: *preset,
            default_size,
            default_wall,
            per_filter,
        });
    }

    println!();
    println!("Total wall: {:.1}s", t_total.elapsed().as_secs_f64());
    println!();

    // === Per-filter table per fixture ===
    println!("=== Per-filter size + wall (each filter normalized: size%vs default, wall%vs default) ===");
    print!("{:<14}", "fixture");
    for &(_, name) in FILTERS { print!(" {:>9}", name); }
    println!(" | {:>10} {:>10}", "def_size", "def_wall");
    for p in &per_fx {
        print!("{:<14}", p.label);
        for f in &p.per_filter {
            let size_pct = (f.size as f64 / p.default_size as f64 - 1.0) * 100.0;
            let wall_pct = f.wall_ms / p.default_wall * 100.0;
            print!(" {:>4.0}%/{:>3.0}%", size_pct, wall_pct);
        }
        println!(" | {:>10} {:>9.1}ms", p.default_size, p.default_wall);
    }
    println!();

    // === Best-filter distribution ===
    println!("=== Best-single-filter per fixture ===");
    println!("{:<14} {:>10} {:>5} {:>9}", "fixture", "best", "Δ%", "Δwall%");
    let mut filter_counts = std::collections::HashMap::<&str, usize>::new();
    let mut sum_size_delta = 0.0;
    let mut sum_wall_delta = 0.0;
    for p in &per_fx {
        let best = p.per_filter.iter().min_by_key(|r| r.size).unwrap();
        let size_delta = (best.size as f64 / p.default_size as f64 - 1.0) * 100.0;
        let wall_pct = best.wall_ms / p.default_wall * 100.0;
        sum_size_delta += size_delta;
        sum_wall_delta += wall_pct;
        *filter_counts.entry(best.name).or_insert(0) += 1;
        println!("{:<14} {:>10} {:>+4.2}% {:>8.0}%", p.label, best.name, size_delta, wall_pct);
    }
    let n = per_fx.len();
    let mean_size_delta = sum_size_delta / n as f64;
    let mean_wall_pct = sum_wall_delta / n as f64;
    println!();
    println!("Aggregate (best-filter vs default-preset):");
    println!("  mean size delta:  {:+.2}%  (gate ≤ +0.5%)", mean_size_delta);
    println!("  mean wall pct:    {:.0}%  (gate ≤ 50%)", mean_wall_pct);
    println!("  filter distribution: {:?}", filter_counts);
    println!();

    // === Per-tier breakdown ===
    let baseline7: Vec<&PerFx> = per_fx.iter().filter(|p| p.preset == 3).collect();
    let mp5: Vec<&PerFx> = per_fx.iter().filter(|p| p.preset == 0).collect();
    let tier_stats = |group: &[&PerFx]| -> (f64, f64, usize) {
        let mut sds = 0.0; let mut sws = 0.0;
        for p in group {
            let best = p.per_filter.iter().min_by_key(|r| r.size).unwrap();
            sds += (best.size as f64 / p.default_size as f64 - 1.0) * 100.0;
            sws += best.wall_ms / p.default_wall * 100.0;
        }
        let n = group.len();
        (sds / n as f64, sws / n as f64, n)
    };
    let (b7_sd, b7_sw, b7_n) = tier_stats(&baseline7);
    let (m5_sd, m5_sw, m5_n) = tier_stats(&mp5);
    println!("Tier breakdown:");
    println!("  baseline-7 (preset=3, {} fix): mean Δsize {:+.2}%, mean wall {:.0}%", b7_n, b7_sd, b7_sw);
    println!("  5MP        (preset=0, {} fix): mean Δsize {:+.2}%, mean wall {:.0}%", m5_n, m5_sd, m5_sw);
    println!();

    // === Decision gate ===
    let size_ok = mean_size_delta <= 0.5;
    let wall_ok = mean_wall_pct <= 50.0;
    if size_ok && wall_ok {
        println!(">>> GREEN — best-filter wall {:.0}% (≤50%) and size {:+.2}% (≤+0.5%). R10 paper-track perf headroom confirmed. Cycle 102 → 5-feature filter classifier.",
                 mean_wall_pct, mean_size_delta);
    } else if wall_ok || size_ok {
        println!(">>> YELLOW — partial headroom (size ok {}, wall ok {}). R10 worth pursuing on 5MP only.",
                 size_ok, wall_ok);
    } else {
        println!(">>> RED — best-filter wall {:.0}% / size {:+.2}% — production preset is already near-optimal. R10 perf budget tight; pivot to R3/R6.",
                 mean_wall_pct, mean_size_delta);
    }

    Ok(())
}
