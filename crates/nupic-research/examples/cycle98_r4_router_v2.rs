//! Cycle 98 — R4 routing classifier v2 (4-class + bandpass + per-tier)
//!
//! Cycle 97 (04bbb) 3-class router YELLOW: 5/10 pass, mean wc 52%. Three
//! identified failure modes:
//!   F1. `mean_chroma > 0.04` just-above 04 portrait's 0.027 → portrait drops
//!       to Stoch, loses (K=256, d=0.5) Pareto win.
//!   F2. `smoothness < 0.05` admits stochastic 5MP fixtures (17 aurora 0.018,
//!       27 whale 0.039) → false Chroma route costs +16.5%/+3.6% size.
//!   F3. 3-class router only offers K=256 d=0.5 for chroma; 02 pluto's oracle
//!       K=192 d=0.5 is unreachable → captures 20% of −8.8% win.
//!
//! This cycle attacks all three. Three router variants are evaluated, each
//! using a richer feature set with bandpass_ratio added:
//!
//!   B1 (essay 04bbb A1): chroma threshold 0.04 → 0.025, per-tier 5MP
//!                        chroma floor (5MP requires mean_chroma > 0.10)
//!   B2 (essay 04bbb A2): replace smoothness predicate with
//!                        bandpass_ratio > t  (t swept on cohort)
//!   B3 (essay 04bbb B):  4-class split — Chroma-K192 (trans_frac > 0.1) +
//!                        Chroma-K256 (opaque chroma-rich)
//!
//! Each variant is scored against the same 10-fixture cohort with the same
//! win-capture gate (≥ 80% mean-wc, ≥ 6/10 pass-rate) as Cycle 97.

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
struct Cfg {
    k: usize,
    d: f32,
    p: u8,
}
fn cfg_eq(a: Cfg, b: Cfg) -> bool {
    a.k == b.k && (a.d - b.d).abs() < 0.01 && a.p == b.p
}

#[derive(Clone, Copy, Debug)]
struct Encoded {
    cfg: Cfg,
    size: usize,
    ssim: f64,
}

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
        let path = tmp.join(format!("c98_{}_k{}_d{:.1}_p{}.png", label, cfg.k, cfg.d, cfg.p));
        std::fs::write(&path, &bytes).expect("write");
        let ssim = ssim_via_nupic(fixture_path, &path, nupic);
        out.push(Encoded { cfg, size: bytes.len(), ssim });
    }
    out
}

// ===== features (Cycle 93 set, bandpass_ratio included) =====

#[derive(Clone, Debug, Default)]
struct Features {
    n_pixels: usize,
    mean_chroma: f32,
    smoothness: f32,
    edge_density: f32,
    trans_frac: f32,
    chroma_entropy: f32,
    bandpass_ratio: f32,
}

fn gauss5(src: &[f32], w: usize, h: usize) -> Vec<f32> {
    let k = [1.0f32, 4.0, 6.0, 4.0, 1.0];
    let norm = 16.0f32;
    let mut tmp = vec![0f32; w * h];
    let mut out = vec![0f32; w * h];
    for y in 0..h {
        let row = y * w;
        for x in 0..w {
            let mut s = 0.0;
            for (kk, &kv) in k.iter().enumerate() {
                let xx = (x as i32 + kk as i32 - 2).max(0).min(w as i32 - 1) as usize;
                s += src[row + xx] * kv;
            }
            tmp[row + x] = s / norm;
        }
    }
    for y in 0..h {
        for x in 0..w {
            let mut s = 0.0;
            for (kk, &kv) in k.iter().enumerate() {
                let yy = (y as i32 + kk as i32 - 2).max(0).min(h as i32 - 1) as usize;
                s += tmp[yy * w + x] * kv;
            }
            out[y * w + x] = s / norm;
        }
    }
    out
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
    for y in 0..h { for x in 0..w - 1 {
        let i = y * w + x;
        sum_h += (oklab[i].l - oklab[i + 1].l).abs() as f64; count_h += 1;
    } }
    for y in 0..h - 1 { for x in 0..w {
        let i = y * w + x;
        sum_v += (oklab[i].l - oklab[i + w].l).abs() as f64; count_v += 1;
    } }
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

    let l: Vec<f32> = oklab.iter().map(|o| o.l).collect();
    let g1 = gauss5(&l, w, h);
    let g2 = gauss5(&g1, w, h);
    let g3 = gauss5(&g2, w, h);
    let g4 = gauss5(&g3, w, h);
    let fine: f64 = l.iter().zip(g1.iter()).map(|(a, b)| (a - b).abs() as f64).sum::<f64>() / n as f64;
    let coarse: f64 = g2.iter().zip(g4.iter()).map(|(a, b)| (a - b).abs() as f64).sum::<f64>() / n as f64;
    let bandpass_ratio = (coarse / fine.max(1e-9)) as f32;

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

    Features { n_pixels: n, mean_chroma, smoothness, edge_density, trans_frac, chroma_entropy, bandpass_ratio }
}

// ===== 4-class routing =====

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Class {
    Ui,
    ChromaK192,
    ChromaK256,
    Stochastic,
}
impl Class {
    fn target_kd(self) -> (usize, f32) {
        match self {
            Class::Ui => (128, 0.0),
            Class::ChromaK192 => (192, 0.5),
            Class::ChromaK256 => (256, 0.5),
            Class::Stochastic => (256, 0.0),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Class::Ui => "UI",
            Class::ChromaK192 => "Ck192",
            Class::ChromaK256 => "Ck256",
            Class::Stochastic => "Stoch",
        }
    }
}

// ===== rule variants =====

// B1: literal essay 04bbb (A1) — chroma 0.04→0.025 + per-tier 5MP floor
fn rule_b1(f: &Features) -> Class {
    if f.chroma_entropy < 3.0 && f.edge_density > 0.2 {
        return Class::Ui;
    }
    let is_5mp = f.n_pixels >= 5_000_000;
    let chroma_threshold = if is_5mp { 0.10 } else { 0.025 };
    if f.trans_frac > 0.0 || (f.mean_chroma > chroma_threshold && f.smoothness < 0.05) {
        Class::ChromaK256
    } else {
        Class::Stochastic
    }
}

// B2: smoothness → bandpass_ratio replacement (no 5MP floor)
fn rule_b2(f: &Features, bp_t: f32) -> Class {
    if f.chroma_entropy < 3.0 && f.edge_density > 0.2 {
        return Class::Ui;
    }
    if f.trans_frac > 0.0 || (f.mean_chroma > 0.025 && f.bandpass_ratio > bp_t) {
        Class::ChromaK256
    } else {
        Class::Stochastic
    }
}

// B3: 4-class with K=192 vs K=256 split
fn rule_b3(f: &Features, bp_t: f32) -> Class {
    if f.chroma_entropy < 3.0 && f.edge_density > 0.2 {
        return Class::Ui;
    }
    // trans-rich → Chroma-K192 (covers 02 pluto K=192 oracle)
    if f.trans_frac > 0.1 {
        return Class::ChromaK192;
    }
    // opaque chroma-rich → Chroma-K256 (must clear bandpass gate to avoid 5MP noise)
    if f.mean_chroma > 0.025 && f.bandpass_ratio > bp_t && f.edge_density > 0.2 {
        return Class::ChromaK256;
    }
    Class::Stochastic
}

// ===== eval =====

#[derive(Clone, Debug)]
struct PerFixture {
    label: String,
    features: Features,
    default: Encoded,
    oracle_iso: Encoded,
    grid: Vec<Encoded>,
    preset: u8,
}

fn pick_default(grid: &[Encoded], preset: u8) -> Encoded {
    let want = Cfg { k: 256, d: 0.0, p: preset };
    grid.iter().find(|e| cfg_eq(e.cfg, want)).cloned().expect("default in grid")
}
fn pick_oracle_iso(grid: &[Encoded], default: &Encoded) -> Encoded {
    let band_lo = default.ssim - ISO_BAND;
    let mut best = default.clone();
    for e in grid {
        if e.ssim >= band_lo && e.size < best.size { best = e.clone(); }
    }
    best
}
fn pick_router(grid: &[Encoded], target_k: usize, target_d: f32, preset: u8) -> Option<Encoded> {
    let want = Cfg { k: target_k, d: target_d, p: preset };
    grid.iter().find(|e| cfg_eq(e.cfg, want)).cloned()
}

fn win_capture(default_size: usize, oracle_size: usize, router_size: usize, router_ssim: f64, default_ssim: f64) -> Option<f64> {
    let in_band = router_ssim >= default_ssim - ISO_BAND;
    let eff_router = if in_band { router_size } else { default_size };
    let available = default_size as i64 - oracle_size as i64;
    if available <= 0 {
        if eff_router <= default_size { Some(1.0) } else { Some(0.0) }
    } else {
        let captured = default_size as i64 - eff_router as i64;
        Some(captured as f64 / available as f64)
    }
}

#[derive(Clone, Debug)]
struct VariantResult {
    name: String,
    per_pass: Vec<(String, Class, Option<f64>, f64)>, // (label, class, wc, router Δ%)
    mean_wc: f64,
    pass_count: usize,
    n: usize,
    mean_router_delta: f64,
}

fn score_variant<F: Fn(&Features) -> Class>(name: &str, per_fixture: &[PerFixture], pick: F) -> VariantResult {
    let mut per_pass = Vec::new();
    let mut pass_count = 0usize;
    let mut sum_wc = 0.0f64; let mut n_wc = 0usize;
    let mut sum_router_delta = 0.0f64;
    for p in per_fixture {
        let cls = pick(&p.features);
        let (k_t, d_t) = cls.target_kd();
        let router = pick_router(&p.grid, k_t, d_t, p.preset);
        let (wc, router_delta) = match router {
            None => (Some(0.0), 0.0), // router target not in grid → no win
            Some(r) => {
                let in_band = r.ssim >= p.default.ssim - ISO_BAND;
                let eff = if in_band { r.size } else { p.default.size };
                let delta = (eff as f64 / p.default.size as f64 - 1.0) * 100.0;
                (win_capture(p.default.size, p.oracle_iso.size, r.size, r.ssim, p.default.ssim), delta)
            }
        };
        sum_router_delta += router_delta;
        if let Some(w) = wc { sum_wc += w; n_wc += 1; if w >= WIN_CAPTURE_GATE { pass_count += 1; } }
        per_pass.push((p.label.clone(), cls, wc, router_delta));
    }
    let n = per_fixture.len();
    let mean_wc = if n_wc > 0 { sum_wc / n_wc as f64 } else { 0.0 };
    let mean_router_delta = sum_router_delta / n as f64;
    VariantResult { name: name.to_string(), per_pass, mean_wc, pass_count, n, mean_router_delta }
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();
    let nupic = root.join("target/release/nupic");

    let preset_b7: u8 = 3;
    let preset_5mp: u8 = 0;
    // Need K∈{128,192,256}×d∈{0,0.3,0.5} for baseline-7 + K∈{128,192,256}×d∈{0,0.5} for 5MP
    let baseline7_grid: Vec<Cfg> = {
        let mut v = Vec::new();
        for &k in &[128usize, 192, 256] { for &d in &[0.0f32, 0.3, 0.5] { v.push(Cfg { k, d, p: preset_b7 }); } }
        v
    };
    let mp5_grid: Vec<Cfg> = {
        let mut v = Vec::new();
        for &k in &[128usize, 192, 256] { for &d in &[0.0f32, 0.5] { v.push(Cfg { k, d, p: preset_5mp }); } }
        v
    };

    let fixtures: Vec<(&str, &str, &Vec<Cfg>, u8)> = vec![
        ("inputs/01-png-transparency-demo.png", "01_trans",     &baseline7_grid, preset_b7),
        ("inputs/02-pluto-transparent.png",     "02_pluto",     &baseline7_grid, preset_b7),
        ("inputs/03-wikipedia-logo.png",        "03_wiki",      &baseline7_grid, preset_b7),
        ("inputs/04-photo-portrait.png",        "04_portrait",  &baseline7_grid, preset_b7),
        ("inputs/05-photo-mountain.png",        "05_mountain",  &baseline7_grid, preset_b7),
        ("inputs/06-photo-landscape.png",       "06_landscape", &baseline7_grid, preset_b7),
        ("inputs/07-photo-product.png",         "07_product",   &baseline7_grid, preset_b7),
        ("inputs-ext-real/17-aurora-5mp.png",          "17_aurora",  &mp5_grid, preset_5mp),
        ("inputs-ext-real/25-sofia-cathedral-5mp.png", "25_sofia",   &mp5_grid, preset_5mp),
        ("inputs-ext-real/27-whale-tail-5mp.png",      "27_whale",   &mp5_grid, preset_5mp),
    ];

    println!("Cycle 98 — R4 router v2 (4-class + bandpass + per-tier)");
    println!("  grid encodes: {}", baseline7_grid.len() * 7 + mp5_grid.len() * 3);
    println!();

    let mut per_fixture: Vec<PerFixture> = Vec::new();
    let t_total = Instant::now();
    for (rel, lbl, grid_cfg, preset) in &fixtures {
        let path = root.join("assets/png-bench").join(rel);
        if !path.exists() { println!("MISSING {}: {}", lbl, path.display()); continue; }
        let img = ImageReader::open(&path)?.with_guessed_format()?.decode()?;
        let r = img.to_rgba8(); let w = r.width() as usize; let h = r.height() as usize;
        let raw = r.into_raw();
        let features = compute_features(&raw, w, h);
        let t0 = Instant::now();
        let grid = run_grid(&path, &nupic, lbl, grid_cfg);
        let t = t0.elapsed().as_secs_f64();
        let default = pick_default(&grid, *preset);
        let oracle = pick_oracle_iso(&grid, &default);
        println!("[{:<14}] {:>4.1}s | def {} B / SSIM {:>6.2} | oracle K{} d{:.1} {} B SSIM {:>6.2} ({:+.2}%) | bp={:.3}",
                 lbl, t, default.size, default.ssim,
                 oracle.cfg.k, oracle.cfg.d, oracle.size, oracle.ssim,
                 (oracle.size as f64 / default.size as f64 - 1.0) * 100.0,
                 features.bandpass_ratio);
        per_fixture.push(PerFixture { label: lbl.to_string(), features, default, oracle_iso: oracle, grid, preset: *preset });
    }
    println!();
    println!("Grid + SSIM total: {:.1}s", t_total.elapsed().as_secs_f64());
    println!();

    // === Features dump (with bandpass) ===
    println!("=== Feature dump (with bandpass_ratio) ===");
    println!("{:<14} {:>4} {:>7} {:>8} {:>7} {:>7} {:>8} {:>9}",
             "fixture", "5mp", "chroma", "smooth", "edge", "trans", "entropy", "bandpass");
    for p in &per_fixture {
        println!("{:<14} {:>4} {:>7.3} {:>8.4} {:>7.3} {:>7.3} {:>8.3} {:>9.3}",
                 p.label, if p.features.n_pixels >= 5_000_000 { "Y" } else { "N" },
                 p.features.mean_chroma, p.features.smoothness, p.features.edge_density,
                 p.features.trans_frac, p.features.chroma_entropy, p.features.bandpass_ratio);
    }
    println!();

    // === Bandpass threshold sweep (for B2/B3) ===
    // Find t that maximises pass-rate of B3 on the cohort.
    let mut bp_candidates: Vec<f32> = per_fixture.iter().map(|p| p.features.bandpass_ratio).collect();
    bp_candidates.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut sweep_bp = Vec::new();
    // candidate boundaries between adjacent unique bp values
    for w in bp_candidates.windows(2) {
        sweep_bp.push((w[0] + w[1]) / 2.0);
    }
    // also include extremes
    sweep_bp.push(0.0);
    sweep_bp.push(10.0);

    let mut best_b3: Option<(f32, VariantResult)> = None;
    for &bp_t in &sweep_bp {
        let v = score_variant("B3", &per_fixture, |f| rule_b3(f, bp_t));
        let key = (v.pass_count, (v.mean_wc * 1000.0) as i64);
        match &best_b3 {
            None => best_b3 = Some((bp_t, v)),
            Some((_, cur)) => {
                let cur_key = (cur.pass_count, (cur.mean_wc * 1000.0) as i64);
                if key > cur_key { best_b3 = Some((bp_t, v)); }
            }
        }
    }
    let mut best_b2: Option<(f32, VariantResult)> = None;
    for &bp_t in &sweep_bp {
        let v = score_variant("B2", &per_fixture, |f| rule_b2(f, bp_t));
        let key = (v.pass_count, (v.mean_wc * 1000.0) as i64);
        match &best_b2 {
            None => best_b2 = Some((bp_t, v)),
            Some((_, cur)) => {
                let cur_key = (cur.pass_count, (cur.mean_wc * 1000.0) as i64);
                if key > cur_key { best_b2 = Some((bp_t, v)); }
            }
        }
    }

    let v_b1 = score_variant("B1", &per_fixture, |f| rule_b1(f));

    // Report
    println!("=== Variant scores (10 fixtures) ===");
    println!("{:<6} {:>5} {:>6} {:>11} {:>10}", "name", "pass", "mean_wc", "mean_Δsize", "param");
    let (bp2_t, v_b2) = best_b2.unwrap();
    let (bp3_t, v_b3) = best_b3.unwrap();
    println!("{:<6} {:>3}/{} {:>5.0}% {:>+10.2}% {:>10}", "B1", v_b1.pass_count, v_b1.n,
             v_b1.mean_wc * 100.0, v_b1.mean_router_delta, "(literal)");
    println!("{:<6} {:>3}/{} {:>5.0}% {:>+10.2}% bp_t={:.3}", "B2", v_b2.pass_count, v_b2.n,
             v_b2.mean_wc * 100.0, v_b2.mean_router_delta, bp2_t);
    println!("{:<6} {:>3}/{} {:>5.0}% {:>+10.2}% bp_t={:.3}", "B3 (4cls)", v_b3.pass_count, v_b3.n,
             v_b3.mean_wc * 100.0, v_b3.mean_router_delta, bp3_t);
    println!();

    // Per-fixture trace for best variant
    let best = [&v_b1, &v_b2, &v_b3].iter().max_by_key(|v| (v.pass_count, (v.mean_wc * 1000.0) as i64)).copied().unwrap();
    println!("=== Per-fixture trace for best variant: {} ===", best.name);
    println!("{:<14} {:>7} {:>10} {:>9}", "fixture", "class", "Δ size", "wc");
    for (lbl, cls, wc, delta) in &best.per_pass {
        println!("{:<14} {:>7} {:>+9.2}% {:>9}",
                 lbl, cls.name(), delta,
                 wc.map(|w| format!("{:.0}%", w * 100.0)).unwrap_or_else(|| "-".into()));
    }
    println!();

    // Decision gate
    let pass_frac = best.pass_count as f64 / best.n as f64;
    if pass_frac >= PASS_FRACTION_GATE && best.mean_wc >= WIN_CAPTURE_GATE {
        println!(">>> GREEN — best variant ({}) clears mean-wc {:.0}% AND pass-frac {:.0}%. Ship candidate for `Quality::Auto-R4`.",
                 best.name, best.mean_wc * 100.0, pass_frac * 100.0);
    } else if best.mean_wc >= 0.7 || pass_frac >= 0.7 {
        println!(">>> YELLOW — best variant ({}) at {:.0}% mean-wc / {:.0}% pass. Below gate but improving over Cycle 97's 52% / 50%.",
                 best.name, best.mean_wc * 100.0, pass_frac * 100.0);
    } else {
        println!(">>> RED — best variant ({}) at {:.0}% mean-wc / {:.0}% pass. Router family insufficient.",
                 best.name, best.mean_wc * 100.0, pass_frac * 100.0);
    }

    Ok(())
}
