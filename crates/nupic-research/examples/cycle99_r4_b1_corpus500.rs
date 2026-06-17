//! Cycle 99 — R4 B1 router validation on corpus-500 sample
//!
//! Cycle 98 GREEN on baseline-7 + 5MP (10 fixtures): B1 router 8/10 pass,
//! mean wc 90%. Essay 04ccc § Risk flagged "B1 calibrated on 10 fixtures;
//! chroma threshold 0.025 sits 0.004 above 06 landscape (0.023); could be
//! too tight on wider corpus." Per [[feedback-full-corpus-before-classifier-
//! ship]], we re-validate on the 20-fixture corpus-500 sample (the Cycle 92
//! ground truth set) before any production wiring proposal.
//!
//! For each of the 20 fixtures:
//!   1. Encode 9-config grid (K∈{128,192,256} × d∈{0,0.3,0.5}, preset=3)
//!   2. Find oracle iso-SSIM (smallest size with SSIM ≥ default −0.5)
//!   3. Compute features
//!   4. Apply B1 router → pick (K, d) target
//!   5. Score win-capture, in-band check
//!
//! Gate (same as Cycle 98):
//!   mean win-capture ≥ 80%  AND  pass-fraction ≥ 60%  → GREEN ship-ready
//!
//! If GREEN: production wiring task lands in research roadmap.
//! If YELLOW: widen safety band, re-validate.
//! If RED: the 10-fixture cohort was unrepresentative; widen and repeat.

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use image::ImageReader;
use rgb::Rgb;

use nupic_color::{srgb_u8_to_oklab, Oklab};
use nupic_quantize::{quantize_indexed_png, QuantizeOpts};

const ISO_BAND: f64 = 0.5;
const WIN_CAPTURE_GATE: f64 = 0.80;
const PASS_FRACTION_GATE: f64 = 0.60;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Cfg { k: usize, d: f32, p: u8 }
fn cfg_eq(a: Cfg, b: Cfg) -> bool { a.k == b.k && (a.d - b.d).abs() < 0.01 && a.p == b.p }

#[derive(Clone, Copy, Debug)]
struct Encoded { cfg: Cfg, size: usize, ssim: f64 }

fn ssim_via_nupic(orig: &PathBuf, cmp_path: &PathBuf, nupic: &PathBuf) -> f64 {
    let out = Command::new(nupic)
        .args(["compare", "-m", "ssimulacra2"])
        .arg(orig).arg(cmp_path).output().expect("nupic compare");
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines().find_map(|l| {
        l.strip_prefix("SSIMULACRA2: ")
            .and_then(|v| v.split_whitespace().next())
            .and_then(|n| n.parse::<f64>().ok())
    }).unwrap_or(f64::NAN)
}

fn encode_one(raw: &[u8], w: u32, h: u32, cfg: Cfg) -> Vec<u8> {
    let mut opts = QuantizeOpts::default();
    opts.n_colors = cfg.k;
    opts.dither_strength = cfg.d;
    opts.oxipng_preset = cfg.p;
    opts.strip_metadata = true;
    quantize_indexed_png(raw, w, h, opts).expect("encode")
}

fn run_grid(fixture_path: &PathBuf, nupic: &PathBuf, label: &str, grid: &[Cfg]) -> Vec<Encoded> {
    let img = ImageReader::open(fixture_path).expect("open").with_guessed_format().expect("fmt").decode().expect("decode");
    let r = img.to_rgba8();
    let w = r.width();
    let h = r.height();
    let raw = r.into_raw();
    let tmp = std::env::temp_dir();
    let mut out = Vec::with_capacity(grid.len());
    for &cfg in grid {
        let bytes = encode_one(&raw, w, h, cfg);
        let path = tmp.join(format!("c99_{}_k{}_d{:.1}_p{}.png", label, cfg.k, cfg.d, cfg.p));
        std::fs::write(&path, &bytes).expect("write");
        let ssim = ssim_via_nupic(fixture_path, &path, nupic);
        out.push(Encoded { cfg, size: bytes.len(), ssim });
    }
    out
}

#[derive(Clone, Debug, Default)]
struct Features {
    n_pixels: usize,
    mean_chroma: f32,
    smoothness: f32,
    edge_density: f32,
    trans_frac: f32,
    chroma_entropy: f32,
}

fn compute_features(raw_rgba: &[u8], w: usize, h: usize) -> Features {
    let n = w * h;
    let mut alpha_count_lt = 0usize;
    let oklab: Vec<Oklab> = raw_rgba.chunks_exact(4).map(|p| {
        if p[3] < 255 { alpha_count_lt += 1; }
        srgb_u8_to_oklab(Rgb { r: p[0], g: p[1], b: p[2] })
    }).collect();
    let trans_frac = alpha_count_lt as f32 / n as f32;
    let sum_chroma: f64 = oklab.iter().map(|o| (o.a * o.a + o.b * o.b).sqrt() as f64).sum();
    let mean_chroma = (sum_chroma / n as f64) as f32;
    let mut sum_h = 0.0f64; let mut count_h = 0usize;
    let mut sum_v = 0.0f64; let mut count_v = 0usize;
    for y in 0..h { for x in 0..w.saturating_sub(1) {
        let i = y * w + x;
        sum_h += (oklab[i].l - oklab[i + 1].l).abs() as f64; count_h += 1;
    } }
    if h >= 1 {
        for y in 0..h.saturating_sub(1) { for x in 0..w {
            let i = y * w + x;
            sum_v += (oklab[i].l - oklab[i + w].l).abs() as f64; count_v += 1;
        } }
    }
    let smoothness = ((sum_h / count_h.max(1) as f64) + (sum_v / count_v.max(1) as f64)) as f32;
    let mut edge_count = 0usize; let mut edge_total = 0usize;
    if w >= 3 && h >= 3 {
        for y in 1..h - 1 { for x in 1..w - 1 {
            let i = y * w + x;
            let gx = oklab[i + 1].l - oklab[i - 1].l;
            let gy = oklab[i + w].l - oklab[i - w].l;
            let mag = (gx * gx + gy * gy).sqrt();
            if mag > 0.05 { edge_count += 1; }
            edge_total += 1;
        } }
    }
    let edge_density = edge_count as f32 / edge_total.max(1) as f32;
    let bins = 16usize;
    let mut hist = vec![0u32; bins * bins];
    let mut a_min = f32::INFINITY; let mut a_max = f32::NEG_INFINITY;
    let mut b_min = f32::INFINITY; let mut b_max = f32::NEG_INFINITY;
    for o in &oklab {
        if o.a < a_min { a_min = o.a; } if o.a > a_max { a_max = o.a; }
        if o.b < b_min { b_min = o.b; } if o.b > b_max { b_max = o.b; }
    }
    let a_span = (a_max - a_min).max(1e-6);
    let b_span = (b_max - b_min).max(1e-6);
    for o in &oklab {
        let ai = (((o.a - a_min) / a_span) * bins as f32).floor() as i32;
        let bi = (((o.b - b_min) / b_span) * bins as f32).floor() as i32;
        let ai = ai.max(0).min(bins as i32 - 1) as usize;
        let bi = bi.max(0).min(bins as i32 - 1) as usize;
        hist[ai * bins + bi] += 1;
    }
    let total = n as f64;
    let mut entropy = 0.0f64;
    for &c in hist.iter() { if c > 0 { let p = c as f64 / total; entropy -= p * p.log2(); } }
    let chroma_entropy = entropy as f32;
    Features { n_pixels: n, mean_chroma, smoothness, edge_density, trans_frac, chroma_entropy }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Class { Ui, Chroma, Stoch }
impl Class {
    fn target_kd(self) -> (usize, f32) {
        match self {
            Class::Ui => (128, 0.0),
            Class::Chroma => (256, 0.5),
            Class::Stoch => (256, 0.0),
        }
    }
    fn name(self) -> &'static str {
        match self { Class::Ui => "UI", Class::Chroma => "Chrm", Class::Stoch => "Stoch" }
    }
}

// B1 router (Cycle 98 GREEN spec)
fn rule_b1(f: &Features) -> Class {
    if f.chroma_entropy < 3.0 && f.edge_density > 0.2 {
        return Class::Ui;
    }
    let is_5mp = f.n_pixels >= 5_000_000;
    let chroma_threshold = if is_5mp { 0.10 } else { 0.025 };
    if f.trans_frac > 0.0 || (f.mean_chroma > chroma_threshold && f.smoothness < 0.05) {
        Class::Chroma
    } else {
        Class::Stoch
    }
}

fn pick_default(grid: &[Encoded], preset: u8) -> Encoded {
    let want = Cfg { k: 256, d: 0.0, p: preset };
    grid.iter().find(|e| cfg_eq(e.cfg, want)).cloned().expect("default")
}
fn pick_oracle_iso(grid: &[Encoded], default: &Encoded) -> Encoded {
    let band_lo = default.ssim - ISO_BAND;
    let mut best = default.clone();
    for e in grid { if e.ssim >= band_lo && e.size < best.size { best = e.clone(); } }
    best
}
fn pick_router(grid: &[Encoded], k: usize, d: f32, p: u8) -> Encoded {
    let want = Cfg { k, d, p };
    grid.iter().find(|e| cfg_eq(e.cfg, want)).cloned().expect("router in grid")
}

fn win_capture(default_size: usize, oracle_size: usize, router_size: usize, router_ssim: f64, default_ssim: f64) -> Option<f64> {
    let in_band = router_ssim >= default_ssim - ISO_BAND;
    let eff = if in_band { router_size } else { default_size };
    let avail = default_size as i64 - oracle_size as i64;
    if avail <= 0 { Some(if eff <= default_size { 1.0 } else { 0.0 }) }
    else { Some((default_size as i64 - eff as i64) as f64 / avail as f64) }
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();
    let nupic = root.join("target/release/nupic");
    let preset: u8 = 3;

    let grid_cfg: Vec<Cfg> = {
        let mut v = Vec::new();
        for &k in &[128usize, 192, 256] { for &d in &[0.0f32, 0.3, 0.5] { v.push(Cfg { k, d, p: preset }); } }
        v
    };

    // Cycle 92 ground-truth 20 corpus-500 fixtures
    let fixtures: &[&str] = &[
        "mi0.png",
        "n29_astronaut.png",
        "p11_480x320.png",
        "p32_480x320.png",
        "p409_sm_300x320.png",
        "p426_sm_460x380.png",
        "p449_sm_300x320.png",
        "p66_1024x768.png",
        "p7_480x320.png",
        "s042_stripes_p8.png",
        "n01_mars.png",
        "n31_rover.png",
        "p119_1024x768.png",
        "p38_480x320.png",
        "p430_sm_380x380.png",
        "p56_480x320.png",
        "p84_1024x768.png",
        "s006_gradient_1306x1113.png",
        "s040_stripes_p2.png",
        "s059_solid.png",
    ];

    println!("Cycle 99 — R4 B1 router corpus-500 validation");
    println!("  cohort: {} fixtures from Cycle 92 ground truth", fixtures.len());
    println!("  grid: {} configs at preset={}", grid_cfg.len(), preset);
    println!("  total encodes: {}", grid_cfg.len() * fixtures.len());
    println!();

    let t_total = Instant::now();
    let mut results: Vec<(String, Features, Encoded, Encoded, Encoded, Class, Option<f64>, f64)> = Vec::new();

    for rel in fixtures {
        let path = root.join("assets/png-bench/corpus-500").join(rel);
        if !path.exists() {
            println!("MISSING: {}", path.display());
            continue;
        }
        let lbl = rel.trim_end_matches(".png").to_string();
        let img = match ImageReader::open(&path).and_then(|r| r.with_guessed_format()) {
            Ok(r) => match r.decode() { Ok(i) => i, Err(_) => { println!("DECODE-FAIL: {}", lbl); continue; } },
            Err(_) => { println!("OPEN-FAIL: {}", lbl); continue; }
        };
        let r = img.to_rgba8();
        let w = r.width() as usize; let h = r.height() as usize;
        let raw = r.into_raw();
        let features = compute_features(&raw, w, h);
        let t0 = Instant::now();
        let grid = run_grid(&path, &nupic, &lbl, &grid_cfg);
        let t = t0.elapsed().as_secs_f64();
        let default = pick_default(&grid, preset);
        let oracle = pick_oracle_iso(&grid, &default);
        let cls = rule_b1(&features);
        let (kt, dt) = cls.target_kd();
        let router = pick_router(&grid, kt, dt, preset);
        let in_band = router.ssim >= default.ssim - ISO_BAND;
        let eff_size = if in_band { router.size } else { default.size };
        let router_delta = (eff_size as f64 / default.size as f64 - 1.0) * 100.0;
        let wc = win_capture(default.size, oracle.size, router.size, router.ssim, default.ssim);
        println!("[{:<26}] {:>4.1}s | def {} B / {:>6.2} | oracle K{} d{:.1} ({:+.2}%) | {} -> K{} d{:.1} ({:+.2}%)  wc={}",
                 lbl, t, default.size, default.ssim,
                 oracle.cfg.k, oracle.cfg.d,
                 (oracle.size as f64 / default.size as f64 - 1.0) * 100.0,
                 cls.name(), router.cfg.k, router.cfg.d, router_delta,
                 wc.map(|w| format!("{:.0}%", w * 100.0)).unwrap_or_else(|| "-".into()));
        results.push((lbl, features, default, oracle, router, cls, wc, router_delta));
    }

    println!();
    println!("Grid + SSIM total: {:.1}s", t_total.elapsed().as_secs_f64());
    println!();

    // Feature dump
    println!("=== Features + router class ===");
    println!("{:<26} {:>7} {:>8} {:>7} {:>7} {:>8} {:>6}",
             "fixture", "chroma", "smooth", "edge", "trans", "entropy", "class");
    for (lbl, f, _, _, _, c, _, _) in &results {
        println!("{:<26} {:>7.3} {:>8.4} {:>7.3} {:>7.3} {:>8.3} {:>6}",
                 lbl, f.mean_chroma, f.smoothness, f.edge_density, f.trans_frac, f.chroma_entropy, c.name());
    }
    println!();

    // Aggregate
    let n = results.len();
    let mut pass_count = 0usize;
    let mut sum_wc = 0.0f64; let mut n_wc = 0usize;
    let mut sum_router_delta = 0.0f64;
    let mut sum_oracle_delta = 0.0f64;
    println!("=== Win-capture table ===");
    println!("{:<26} {:>9} {:>10} {:>8} {:>6}", "fixture", "oracle Δ%", "router Δ%", "wc", "pass?");
    for (lbl, _, default, oracle, _, _, wc, router_delta) in &results {
        let oracle_delta = (oracle.size as f64 / default.size as f64 - 1.0) * 100.0;
        sum_oracle_delta += oracle_delta;
        sum_router_delta += router_delta;
        let pass = wc.map(|w| w >= WIN_CAPTURE_GATE).unwrap_or(false);
        if pass { pass_count += 1; }
        if let Some(w) = wc { sum_wc += w; n_wc += 1; }
        println!("{:<26} {:>+9.2} {:>+10.2} {:>7} {:>6}",
                 lbl, oracle_delta, router_delta,
                 wc.map(|w| format!("{:.0}%", w * 100.0)).unwrap_or_else(|| "-".into()),
                 if pass { "PASS" } else { "fail" });
    }
    let pass_frac = pass_count as f64 / n as f64;
    let mean_wc = if n_wc > 0 { sum_wc / n_wc as f64 } else { 0.0 };
    let mean_oracle = sum_oracle_delta / n as f64;
    let mean_router = sum_router_delta / n as f64;
    println!();
    println!("Aggregate ({} fixtures):", n);
    println!("  pass:                 {}/{}  ({:.0}%)", pass_count, n, pass_frac * 100.0);
    println!("  mean win-capture:     {:.0}%", mean_wc * 100.0);
    println!("  mean oracle Δsize:    {:+.2}% (theoretical ceiling)", mean_oracle);
    println!("  mean router Δsize:    {:+.2}% (router actually gets)", mean_router);
    println!("  router/oracle ratio:  {:.0}%",
             if mean_oracle.abs() > 1e-6 { mean_router / mean_oracle * 100.0 } else { 0.0 });
    println!();

    if pass_frac >= PASS_FRACTION_GATE && mean_wc >= WIN_CAPTURE_GATE {
        println!(">>> GREEN — B1 router clears corpus-500 sample gate. SHIP-READY. Production wiring task can land in research roadmap.");
    } else if pass_frac >= 0.5 && mean_wc >= 0.6 {
        println!(">>> YELLOW — close to gate; consider safety-band widening per Cycle 98 § Risk.");
    } else {
        println!(">>> RED — B1 fails corpus-500. The 10-fixture baseline cohort was unrepresentative. Pause ship; widen cohort + repeat.");
    }

    Ok(())
}
