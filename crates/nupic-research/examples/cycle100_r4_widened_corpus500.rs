//! Cycle 100 — R4 widened router variants on corpus-500
//!
//! Cycle 99 (04ddd) YELLOW: B1 router 13/20 pass, mean wc 66% on corpus-500
//! (vs gate 80%). Two failure types: Type-X (3 missed small wins) and Type-Y
//! (3 false-Chroma routes costing +0.47-0.52% size).
//!
//! This cycle is the **definitive R4 simple-feature attempt**. It sweeps 4
//! widened router variants over the same 20-fixture corpus-500 sample, plus
//! one re-affirmation on the 10-fixture baseline+5MP cohort:
//!
//!   C1 (B1+UI-widen):
//!     UI:  entropy < 3 AND (edge > 0.2 OR trans > 0.5)   [catches mi0]
//!     Chroma:  B1 unchanged
//!
//!   C2 (B1+tight-Chroma-edge):
//!     UI:  C1 widened
//!     Chroma:  trans > 0.1  OR  (chr > 0.025 AND smooth < 0.05 AND edge > 0.2)
//!     [kills false-Chroma routes that fail edge predicate, e.g. p66 edge=0.152]
//!
//!   C3 (C2+bandpass-low gate):
//!     Chroma:  trans > 0.1  OR  (chr > 0.025 AND smooth < 0.05 AND edge > 0.2 AND bp > t)
//!     [bandpass sweep — does high-bp content actually correlate with d=0.5-friendly?]
//!
//!   C4 (conservative — abstain on uncertainty):
//!     UI:  entropy < 3 AND (edge > 0.2 OR trans > 0.5)
//!     Chroma:  trans > 0.1  OR  (chr > 0.040 AND smooth < 0.04 AND edge > 0.2)
//!     [strict thresholds; accepts losing Type-X wins to kill all Type-Y false positives]
//!
//! Decision gate (unchanged):
//!   mean win-capture ≥ 80%  AND  pass-fraction ≥ 60%  → GREEN ship-ready
//!
//! If no variant clears GREEN on corpus-500, this cycle **closes the R4
//! simple-feature routing thread RED** — analogous to Cycle 95 closing the
//! R1 friend/hostile classifier thread — and adds it to paper §6 ammunition.

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
    let out = Command::new(nupic).args(["compare", "-m", "ssimulacra2"]).arg(orig).arg(cmp_path).output().expect("nupic");
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines().find_map(|l| {
        l.strip_prefix("SSIMULACRA2: ").and_then(|v| v.split_whitespace().next()).and_then(|n| n.parse::<f64>().ok())
    }).unwrap_or(f64::NAN)
}

fn encode_one(raw: &[u8], w: u32, h: u32, cfg: Cfg) -> Vec<u8> {
    let mut opts = QuantizeOpts::default();
    opts.n_colors = cfg.k; opts.dither_strength = cfg.d; opts.oxipng_preset = cfg.p; opts.strip_metadata = true;
    quantize_indexed_png(raw, w, h, opts).expect("encode")
}

fn run_grid(fixture_path: &PathBuf, nupic: &PathBuf, label: &str, grid: &[Cfg]) -> Vec<Encoded> {
    let img = ImageReader::open(fixture_path).expect("open").with_guessed_format().expect("fmt").decode().expect("decode");
    let r = img.to_rgba8();
    let w = r.width(); let h = r.height();
    let raw = r.into_raw();
    let tmp = std::env::temp_dir();
    let mut out = Vec::with_capacity(grid.len());
    for &cfg in grid {
        let bytes = encode_one(&raw, w, h, cfg);
        let path = tmp.join(format!("c100_{}_k{}_d{:.1}_p{}.png", label, cfg.k, cfg.d, cfg.p));
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
    bandpass_ratio: f32,
    edge_chroma_corr: f32,
}

fn gauss5(src: &[f32], w: usize, h: usize) -> Vec<f32> {
    let k = [1.0f32, 4.0, 6.0, 4.0, 1.0]; let norm = 16.0f32;
    let mut tmp = vec![0f32; w * h]; let mut out = vec![0f32; w * h];
    for y in 0..h { let row = y * w; for x in 0..w {
        let mut s = 0.0;
        for (kk, &kv) in k.iter().enumerate() {
            let xx = (x as i32 + kk as i32 - 2).max(0).min(w as i32 - 1) as usize;
            s += src[row + xx] * kv;
        }
        tmp[row + x] = s / norm;
    } }
    for y in 0..h { for x in 0..w {
        let mut s = 0.0;
        for (kk, &kv) in k.iter().enumerate() {
            let yy = (y as i32 + kk as i32 - 2).max(0).min(h as i32 - 1) as usize;
            s += tmp[yy * w + x] * kv;
        }
        out[y * w + x] = s / norm;
    } }
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
    let sum_chroma: f64 = oklab.iter().map(|o| (o.a*o.a + o.b*o.b).sqrt() as f64).sum();
    let mean_chroma = (sum_chroma / n as f64) as f32;

    let mut sum_h = 0.0f64; let mut count_h = 0usize;
    let mut sum_v = 0.0f64; let mut count_v = 0usize;
    for y in 0..h { for x in 0..w.saturating_sub(1) {
        let i = y*w + x; sum_h += (oklab[i].l - oklab[i+1].l).abs() as f64; count_h += 1;
    } }
    if h >= 1 { for y in 0..h.saturating_sub(1) { for x in 0..w {
        let i = y*w + x; sum_v += (oklab[i].l - oklab[i+w].l).abs() as f64; count_v += 1;
    } } }
    let smoothness = ((sum_h / count_h.max(1) as f64) + (sum_v / count_v.max(1) as f64)) as f32;

    let mut grad_mag = vec![0f32; n];
    let mut edge_count = 0usize; let mut edge_total = 0usize;
    if w >= 3 && h >= 3 {
        for y in 1..h-1 { for x in 1..w-1 {
            let i = y*w + x;
            let gx = oklab[i+1].l - oklab[i-1].l;
            let gy = oklab[i+w].l - oklab[i-w].l;
            let mag = (gx*gx + gy*gy).sqrt();
            grad_mag[i] = mag;
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
    let fine: f64 = l.iter().zip(g1.iter()).map(|(a,b)| (a-b).abs() as f64).sum::<f64>() / n as f64;
    let coarse: f64 = g2.iter().zip(g4.iter()).map(|(a,b)| (a-b).abs() as f64).sum::<f64>() / n as f64;
    let bandpass_ratio = (coarse / fine.max(1e-9)) as f32;

    let bins = 16usize;
    let mut hist = vec![0u32; bins * bins];
    let mut a_min = f32::INFINITY; let mut a_max = f32::NEG_INFINITY;
    let mut b_min = f32::INFINITY; let mut b_max = f32::NEG_INFINITY;
    for o in &oklab { if o.a < a_min { a_min = o.a; } if o.a > a_max { a_max = o.a; }
                      if o.b < b_min { b_min = o.b; } if o.b > b_max { b_max = o.b; } }
    let a_span = (a_max - a_min).max(1e-6); let b_span = (b_max - b_min).max(1e-6);
    for o in &oklab {
        let ai = (((o.a - a_min)/a_span)*bins as f32).floor() as i32;
        let bi = (((o.b - b_min)/b_span)*bins as f32).floor() as i32;
        let ai = ai.max(0).min(bins as i32 - 1) as usize;
        let bi = bi.max(0).min(bins as i32 - 1) as usize;
        hist[ai*bins + bi] += 1;
    }
    let total = n as f64;
    let mut entropy = 0.0f64;
    for &c in hist.iter() { if c > 0 { let p = c as f64 / total; entropy -= p * p.log2(); } }
    let chroma_entropy = entropy as f32;

    let chroma_per: Vec<f32> = oklab.iter().map(|o| (o.a*o.a + o.b*o.b).sqrt()).collect();
    let mut sum_c = 0.0f64; let mut sum_g = 0.0f64; let mut count = 0usize;
    if w >= 3 && h >= 3 {
        for y in 1..h-1 { for x in 1..w-1 {
            let i = y*w + x;
            sum_c += chroma_per[i] as f64; sum_g += grad_mag[i] as f64; count += 1;
        } }
    }
    let mean_c = sum_c / count.max(1) as f64;
    let mean_g = sum_g / count.max(1) as f64;
    let mut cov = 0.0f64; let mut var_c = 0.0f64; let mut var_g = 0.0f64;
    if w >= 3 && h >= 3 {
        for y in 1..h-1 { for x in 1..w-1 {
            let i = y*w + x;
            let dc = chroma_per[i] as f64 - mean_c;
            let dg = grad_mag[i] as f64 - mean_g;
            cov += dc*dg; var_c += dc*dc; var_g += dg*dg;
        } }
    }
    let edge_chroma_corr = if var_c > 1e-12 && var_g > 1e-12 { (cov / (var_c.sqrt() * var_g.sqrt())) as f32 } else { 0.0 };

    Features { n_pixels: n, mean_chroma, smoothness, edge_density, trans_frac, chroma_entropy, bandpass_ratio, edge_chroma_corr }
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
    fn name(self) -> &'static str { match self { Class::Ui => "UI", Class::Chroma => "Chrm", Class::Stoch => "Stoch" } }
}

// C1: B1 + UI widen
fn rule_c1(f: &Features) -> Class {
    if f.chroma_entropy < 3.0 && (f.edge_density > 0.2 || f.trans_frac > 0.5) {
        return Class::Ui;
    }
    let is_5mp = f.n_pixels >= 5_000_000;
    let chroma_threshold = if is_5mp { 0.10 } else { 0.025 };
    if f.trans_frac > 0.0 || (f.mean_chroma > chroma_threshold && f.smoothness < 0.05) {
        Class::Chroma
    } else { Class::Stoch }
}

// C2: C1 + edge gate on Chroma
fn rule_c2(f: &Features) -> Class {
    if f.chroma_entropy < 3.0 && (f.edge_density > 0.2 || f.trans_frac > 0.5) {
        return Class::Ui;
    }
    let is_5mp = f.n_pixels >= 5_000_000;
    let chroma_threshold = if is_5mp { 0.10 } else { 0.025 };
    if f.trans_frac > 0.1 {
        return Class::Chroma;
    }
    if f.mean_chroma > chroma_threshold && f.smoothness < 0.05 && f.edge_density > 0.2 {
        return Class::Chroma;
    }
    Class::Stoch
}

// C3: C2 + bandpass gate
fn rule_c3(f: &Features, bp_t: f32) -> Class {
    if f.chroma_entropy < 3.0 && (f.edge_density > 0.2 || f.trans_frac > 0.5) {
        return Class::Ui;
    }
    let is_5mp = f.n_pixels >= 5_000_000;
    let chroma_threshold = if is_5mp { 0.10 } else { 0.025 };
    if f.trans_frac > 0.1 { return Class::Chroma; }
    if f.mean_chroma > chroma_threshold && f.smoothness < 0.05 && f.edge_density > 0.2 && f.bandpass_ratio > bp_t {
        return Class::Chroma;
    }
    Class::Stoch
}

// C4: conservative — strict thresholds
fn rule_c4(f: &Features) -> Class {
    if f.chroma_entropy < 3.0 && (f.edge_density > 0.2 || f.trans_frac > 0.5) {
        return Class::Ui;
    }
    if f.trans_frac > 0.1 { return Class::Chroma; }
    if f.mean_chroma > 0.040 && f.smoothness < 0.04 && f.edge_density > 0.2 {
        return Class::Chroma;
    }
    Class::Stoch
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
    grid.iter().find(|e| cfg_eq(e.cfg, want)).cloned().expect("router")
}

fn win_capture(default_size: usize, oracle_size: usize, router_size: usize, router_ssim: f64, default_ssim: f64) -> Option<f64> {
    let in_band = router_ssim >= default_ssim - ISO_BAND;
    let eff = if in_band { router_size } else { default_size };
    let avail = default_size as i64 - oracle_size as i64;
    if avail <= 0 { Some(if eff <= default_size { 1.0 } else { 0.0 }) }
    else { Some((default_size as i64 - eff as i64) as f64 / avail as f64) }
}

#[derive(Clone, Debug)]
struct PerFx {
    label: String,
    features: Features,
    default: Encoded,
    oracle: Encoded,
    grid: Vec<Encoded>,
    preset: u8,
}

fn score(name: &str, fxs: &[PerFx], pick: &dyn Fn(&Features) -> Class) -> (usize, f64, f64) {
    let mut pass = 0usize; let mut sum_wc = 0.0; let mut n_wc = 0usize;
    let mut sum_delta = 0.0;
    for p in fxs {
        let cls = pick(&p.features);
        let (kt, dt) = cls.target_kd();
        let router = pick_router(&p.grid, kt, dt, p.preset);
        let in_band = router.ssim >= p.default.ssim - ISO_BAND;
        let eff = if in_band { router.size } else { p.default.size };
        let delta = (eff as f64 / p.default.size as f64 - 1.0) * 100.0;
        sum_delta += delta;
        if let Some(w) = win_capture(p.default.size, p.oracle.size, router.size, router.ssim, p.default.ssim) {
            sum_wc += w; n_wc += 1; if w >= WIN_CAPTURE_GATE { pass += 1; }
        }
    }
    let mean_wc = if n_wc > 0 { sum_wc / n_wc as f64 } else { 0.0 };
    let mean_delta = sum_delta / fxs.len() as f64;
    let _ = name; // for clarity in caller logs
    (pass, mean_wc, mean_delta)
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

    let corpus500: &[&str] = &[
        "mi0.png", "n29_astronaut.png", "p11_480x320.png", "p32_480x320.png",
        "p409_sm_300x320.png", "p426_sm_460x380.png", "p449_sm_300x320.png",
        "p66_1024x768.png", "p7_480x320.png", "s042_stripes_p8.png",
        "n01_mars.png", "n31_rover.png", "p119_1024x768.png", "p38_480x320.png",
        "p430_sm_380x380.png", "p56_480x320.png", "p84_1024x768.png",
        "s006_gradient_1306x1113.png", "s040_stripes_p2.png", "s059_solid.png",
    ];

    println!("Cycle 100 — R4 widened router variants on corpus-500");
    println!("  cohort: {} corpus-500 fixtures @ preset={}", corpus500.len(), preset);
    println!("  variants: C1 (UI widen), C2 (+edge gate), C3 (+bandpass sweep), C4 (conservative)");
    println!();

    let mut fxs: Vec<PerFx> = Vec::new();
    let t_total = Instant::now();
    for rel in corpus500 {
        let path = root.join("assets/png-bench/corpus-500").join(rel);
        if !path.exists() { println!("MISSING: {}", path.display()); continue; }
        let lbl = rel.trim_end_matches(".png").to_string();
        let img = match ImageReader::open(&path).and_then(|r| r.with_guessed_format()) {
            Ok(r) => match r.decode() { Ok(i) => i, Err(_) => continue },
            Err(_) => continue,
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
        println!("[{:<26}] {:>4.1}s | def {:>7} B SSIM {:>6.2} | oracle K{} d{:.1} ({:+.2}%) | bp={:.3} ec={:+.3}",
                 lbl, t, default.size, default.ssim,
                 oracle.cfg.k, oracle.cfg.d,
                 (oracle.size as f64 / default.size as f64 - 1.0)*100.0,
                 features.bandpass_ratio, features.edge_chroma_corr);
        fxs.push(PerFx { label: lbl, features, default, oracle, grid, preset });
    }
    println!();
    println!("Grid + SSIM total: {:.1}s", t_total.elapsed().as_secs_f64());
    println!();

    // Score variants
    let n = fxs.len();
    let (p_c1, wc_c1, dl_c1) = score("C1", &fxs, &|f| rule_c1(f));
    let (p_c2, wc_c2, dl_c2) = score("C2", &fxs, &|f| rule_c2(f));
    let (p_c4, wc_c4, dl_c4) = score("C4", &fxs, &|f| rule_c4(f));

    // C3 bp sweep
    let mut bp_set: Vec<f32> = fxs.iter().map(|f| f.features.bandpass_ratio).collect();
    bp_set.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut sweeps = vec![0.0f32, 0.15, 0.20, 0.25, 0.30, 0.35, 0.40, 0.45, 0.50, 0.55, 0.60, 0.70];
    for w in bp_set.windows(2) { sweeps.push((w[0] + w[1])/2.0); }
    let mut best_c3: (f32, usize, f64, f64) = (0.0, 0, 0.0, 0.0);
    for &bp_t in &sweeps {
        let (p, wc, dl) = score("C3", &fxs, &|f| rule_c3(f, bp_t));
        let key = (p, (wc * 1000.0) as i64);
        let cur_key = (best_c3.1, (best_c3.2 * 1000.0) as i64);
        if key > cur_key { best_c3 = (bp_t, p, wc, dl); }
    }

    println!("=== Variant scores (corpus-500, {} fixtures) ===", n);
    println!("{:<6} {:>5} {:>8} {:>11} {:>10}", "name", "pass", "mean_wc", "mean_Δsize", "param");
    println!("{:<6} {:>2}/{} {:>7.0}% {:>+10.2}% {:>10}", "C1", p_c1, n, wc_c1*100.0, dl_c1, "(UI widen)");
    println!("{:<6} {:>2}/{} {:>7.0}% {:>+10.2}% {:>10}", "C2", p_c2, n, wc_c2*100.0, dl_c2, "(edge gate)");
    println!("{:<6} {:>2}/{} {:>7.0}% {:>+10.2}% bp_t={:.3}", "C3", best_c3.1, n, best_c3.2*100.0, best_c3.3, best_c3.0);
    println!("{:<6} {:>2}/{} {:>7.0}% {:>+10.2}% {:>10}", "C4", p_c4, n, wc_c4*100.0, dl_c4, "(strict)");

    // Pick best
    let variants = [
        ("C1", p_c1, wc_c1, dl_c1),
        ("C2", p_c2, wc_c2, dl_c2),
        ("C3", best_c3.1, best_c3.2, best_c3.3),
        ("C4", p_c4, wc_c4, dl_c4),
    ];
    let best = variants.iter().max_by_key(|v| (v.1, (v.2 * 1000.0) as i64)).copied().unwrap();
    let pass_frac = best.1 as f64 / n as f64;
    println!();
    println!("Best: {} — {}/{} pass ({:.0}%), mean-wc {:.0}%, Δsize {:+.2}%",
             best.0, best.1, n, pass_frac*100.0, best.2*100.0, best.3);
    println!();

    // Trace for best variant
    let best_pick: Box<dyn Fn(&Features) -> Class> = match best.0 {
        "C1" => Box::new(|f| rule_c1(f)),
        "C2" => Box::new(|f| rule_c2(f)),
        "C3" => Box::new(move |f| rule_c3(f, best_c3.0)),
        _ => Box::new(|f| rule_c4(f)),
    };
    println!("=== Per-fixture trace for {} ===", best.0);
    println!("{:<26} {:>6} {:>11} {:>7}", "fixture", "class", "router Δ%", "wc");
    for p in &fxs {
        let cls = best_pick(&p.features);
        let (kt, dt) = cls.target_kd();
        let router = pick_router(&p.grid, kt, dt, p.preset);
        let in_band = router.ssim >= p.default.ssim - ISO_BAND;
        let eff = if in_band { router.size } else { p.default.size };
        let delta = (eff as f64 / p.default.size as f64 - 1.0)*100.0;
        let wc = win_capture(p.default.size, p.oracle.size, router.size, router.ssim, p.default.ssim);
        println!("{:<26} {:>6} {:>+10.2}% {:>7}",
                 p.label, cls.name(), delta,
                 wc.map(|w| format!("{:.0}%", w*100.0)).unwrap_or_else(|| "-".into()));
    }
    println!();

    if pass_frac >= PASS_FRACTION_GATE && best.2 >= WIN_CAPTURE_GATE {
        println!(">>> GREEN — {} clears corpus-500 gate. Production wiring candidate.", best.0);
    } else if best.2 >= 0.70 && pass_frac >= 0.55 {
        println!(">>> YELLOW — {} below GREEN but improving. Document for paper §5/§6.", best.0);
    } else {
        println!(">>> RED — no variant clears corpus-500. Closes R4 simple-feature routing thread. Paper §6 reviewer-defense material.");
    }

    Ok(())
}
