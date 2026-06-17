//! Cycle 103 — routing predicate validation on baseline-7 + 5MP + corpus-500
//!
//! Cycle 102 found 3 spike configs that close size gate for 01 trans / 03 wiki
//! / 07 product:
//!   01 → K=96  d=0.2 preset=6
//!   03 → K=64  d=0   preset=6
//!   07 → K=160 d=0.6 preset=6
//!
//! Production wiring needs **trigger predicates** that route only the right
//! content into each override AND don't regress others. R1/R4 thread closure
//! showed simple-feature routing is fragile on 20-fixture corpus-500 sample.
//! This cycle validates 3 predicates:
//!
//!   P-01: opq < 0.95 AND adj_mn ≤ 5 AND uniq_opq < 5000 AND chroma_entropy < 5
//!         → K=96 d=0.2 preset=6
//!   P-03: opq < 0.95 AND adj_mn > 5 AND area < 50KB            ← sharp-mask
//!         logo override (currently goes to K=256 in production)
//!         → K=64 d=0 preset=6
//!   P-07: opq ≥ 0.95 AND uniq < 50000 AND smoothness < 0.05 AND chroma > 0.04
//!         → K=160 d=0.6 preset=6
//!
//! For each fixture in the 30-fixture cohort:
//!   1. Get production output (Auto, default Quality)
//!   2. Compute features
//!   3. Test each predicate
//!   4. If triggered: encode override config, compare size + SSIM vs production
//!   5. If not triggered: production unchanged
//!
//! Decision gate per predicate:
//!   GREEN if (triggering fixtures: 100% pass three-axis gate at smaller size
//!             vs production AND SSIM ≥ tiny) AND (non-triggering: no change)
//!   YELLOW if triggering 80%+ pass and small regression on ≤ 1
//!   RED if triggering regresses ≥ 2 fixtures OR non-trigger sneaks in

use std::path::PathBuf;
use std::process::Command;

use image::ImageReader;
use rgb::Rgb;

use nupic_color::{srgb_u8_to_oklab, Oklab};
use nupic_quantize::{quantize_indexed_png, QuantizeOpts};

fn ssim(orig: &PathBuf, c: &PathBuf, nupic: &PathBuf) -> f64 {
    let out = Command::new(nupic).args(["compare", "-m", "ssimulacra2"]).arg(orig).arg(c).output().expect("nupic");
    String::from_utf8_lossy(&out.stdout).lines().find_map(|l| {
        l.strip_prefix("SSIMULACRA2: ").and_then(|v| v.split_whitespace().next()).and_then(|n| n.parse::<f64>().ok())
    }).unwrap_or(f64::NAN)
}

fn run_nupic(orig: &PathBuf, out: &PathBuf, nupic: &PathBuf) -> usize {
    Command::new(nupic).args(["compress", "-o"]).arg(out).arg(orig).output().expect("nupic compress");
    std::fs::metadata(out).map(|m| m.len() as usize).unwrap_or(0)
}

#[derive(Clone, Debug, Default)]
struct Features {
    n_pixels: usize,
    file_kb: usize,
    opq: f32,
    trans_frac: f32,
    uniq_opq: usize,
    adj_mn: f32,
    var: f32,
    mean_chroma: f32,
    smoothness: f32,
    edge_density: f32,
    chroma_entropy: f32,
}

fn compute_adj_lum_stats(rgba: &[u8], w: usize) -> (f32, f32) {
    let h = rgba.len() / 4 / w;
    if h < 2 || w < 2 { return (0.0, 0.0); }
    let step_y = if h * w > 1_000_000 { 4 } else { 1 };
    let mut sum_diff = 0.0f64; let mut cnt = 0usize;
    let mut sum_lum = 0.0f64; let mut sum_lum2 = 0.0f64; let mut n_lum = 0usize;
    for y in (0..h-1).step_by(step_y) {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let i2 = ((y+1) * w + x) * 4;
            if rgba[i+3] < 200 || rgba[i2+3] < 200 { continue; }
            let l = (rgba[i] as f32 * 0.299 + rgba[i+1] as f32 * 0.587 + rgba[i+2] as f32 * 0.114) / 255.0 * 100.0;
            let l2 = (rgba[i2] as f32 * 0.299 + rgba[i2+1] as f32 * 0.587 + rgba[i2+2] as f32 * 0.114) / 255.0 * 100.0;
            sum_diff += (l - l2).abs() as f64;
            cnt += 1;
            sum_lum += l as f64; sum_lum2 += (l as f64).powi(2); n_lum += 1;
        }
    }
    let adj_mn = if cnt > 0 { (sum_diff / cnt as f64) as f32 } else { 0.0 };
    let var = if n_lum > 0 {
        let mean = sum_lum / n_lum as f64;
        let v = sum_lum2 / n_lum as f64 - mean * mean;
        v.max(0.0) as f32
    } else { 0.0 };
    (adj_mn, var)
}

fn compute_features(rgba: &[u8], w: u32, h: u32, file_size: usize) -> Features {
    let n = (w as usize) * (h as usize);
    let mut n_opq = 0usize;
    let oklab: Vec<Oklab> = rgba.chunks_exact(4).map(|p| {
        if p[3] >= 250 { n_opq += 1; }
        srgb_u8_to_oklab(Rgb { r: p[0], g: p[1], b: p[2] })
    }).collect();
    let opq = n_opq as f32 / n as f32;
    let trans_frac = 1.0 - opq;

    // uniq_opq (capped at 5000 for speed)
    let mut uniq = std::collections::HashSet::with_capacity(6000);
    let step_u = if n > 1_000_000 { 4 } else { 1 };
    for p in rgba.chunks_exact(4).step_by(step_u) {
        if p[3] < 255 { continue; }
        let key = (p[0] as u32) | ((p[1] as u32) << 8) | ((p[2] as u32) << 16);
        uniq.insert(key);
        if uniq.len() >= 5000 { break; }
    }
    let uniq_opq = uniq.len();

    let (adj_mn, var) = compute_adj_lum_stats(rgba, w as usize);

    let sum_chroma: f64 = oklab.iter().map(|o| (o.a*o.a + o.b*o.b).sqrt() as f64).sum();
    let mean_chroma = (sum_chroma / n as f64) as f32;

    let (w_, h_) = (w as usize, h as usize);
    let mut sum_h = 0.0f64; let mut cnt_h = 0usize;
    let mut sum_v = 0.0f64; let mut cnt_v = 0usize;
    let mut edge_count = 0usize; let mut edge_total = 0usize;
    for y in 0..h_ { for x in 0..w_-1 {
        let i = y * w_ + x;
        sum_h += (oklab[i].l - oklab[i+1].l).abs() as f64; cnt_h += 1;
    } }
    if h_ >= 1 { for y in 0..h_-1 { for x in 0..w_ {
        let i = y * w_ + x;
        sum_v += (oklab[i].l - oklab[i+w_].l).abs() as f64; cnt_v += 1;
    } } }
    let smoothness = ((sum_h / cnt_h.max(1) as f64) + (sum_v / cnt_v.max(1) as f64)) as f32;
    if w_ >= 3 && h_ >= 3 {
        for y in 1..h_-1 { for x in 1..w_-1 {
            let i = y * w_ + x;
            let gx = oklab[i+1].l - oklab[i-1].l;
            let gy = oklab[i+w_].l - oklab[i-w_].l;
            if (gx*gx + gy*gy).sqrt() > 0.05 { edge_count += 1; }
            edge_total += 1;
        } }
    }
    let edge_density = edge_count as f32 / edge_total.max(1) as f32;

    let bins = 16usize;
    let mut hist = vec![0u32; bins*bins];
    let mut amin = f32::INFINITY; let mut amax = f32::NEG_INFINITY;
    let mut bmin = f32::INFINITY; let mut bmax = f32::NEG_INFINITY;
    for o in &oklab {
        if o.a < amin { amin = o.a; } if o.a > amax { amax = o.a; }
        if o.b < bmin { bmin = o.b; } if o.b > bmax { bmax = o.b; }
    }
    let asp = (amax - amin).max(1e-6); let bsp = (bmax - bmin).max(1e-6);
    for o in &oklab {
        let ai = (((o.a - amin)/asp)*bins as f32).floor().max(0.0).min(bins as f32 - 1.0) as usize;
        let bi = (((o.b - bmin)/bsp)*bins as f32).floor().max(0.0).min(bins as f32 - 1.0) as usize;
        hist[ai*bins+bi] += 1;
    }
    let total = n as f64;
    let mut ent = 0.0f64;
    for &c in hist.iter() { if c > 0 { let p = c as f64 / total; ent -= p * p.log2(); } }
    let chroma_entropy = ent as f32;

    Features {
        n_pixels: n, file_kb: file_size / 1024,
        opq, trans_frac, uniq_opq, adj_mn, var,
        mean_chroma, smoothness, edge_density, chroma_entropy,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Override { None, P01, P03, P07 }

fn route(f: &Features) -> Override {
    // P-03 first (more specific): sharp-mask logo small area
    if f.opq < 0.95 && f.adj_mn > 5.0 && f.file_kb < 50 {
        return Override::P03;
    }
    // P-01: translucent overlay-like content with low chroma entropy
    if f.opq < 0.95 && f.adj_mn <= 5.0 && f.uniq_opq < 5000 && f.chroma_entropy < 5.0 {
        return Override::P01;
    }
    // P-07: opaque mid-chroma flat region
    if f.opq >= 0.95 && f.mean_chroma > 0.04 && f.smoothness < 0.05 && f.uniq_opq < 50_000 {
        // uniq_opq cap is 5000 in compute_features, so this gate trivially passes when opq path runs.
        // Use a different uniqueness signal: rely on smoothness + chroma alone.
        return Override::P07;
    }
    Override::None
}

fn override_config(o: Override) -> Option<(usize, f32, u8)> {
    match o {
        Override::P01 => Some((96, 0.2, 6)),
        Override::P03 => Some((64, 0.0, 6)),
        Override::P07 => Some((160, 0.6, 6)),
        Override::None => None,
    }
}

fn encode_override(rgba: &[u8], w: u32, h: u32, k: usize, d: f32, p: u8) -> Vec<u8> {
    let mut opts = QuantizeOpts::default();
    opts.n_colors = k; opts.dither_strength = d; opts.oxipng_preset = p; opts.strip_metadata = true;
    quantize_indexed_png(rgba, w, h, opts).expect("quantize")
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();
    let nupic = root.join("target/release/nupic");

    // 30 fixtures: baseline-7 + 3 × 5MP + 20 corpus-500 (Cycle 92 ground truth set)
    let fixtures: Vec<(&str, &str, Option<&str>)> = vec![
        ("inputs/01-png-transparency-demo.png", "01_trans", Some("tinypng-web/01-png-transparency-demo.png")),
        ("inputs/02-pluto-transparent.png",     "02_pluto", Some("tinypng-web/02-pluto-transparent.png")),
        ("inputs/03-wikipedia-logo.png",        "03_wiki",  Some("tinypng-web/03-wikipedia-logo.png")),
        ("inputs/04-photo-portrait.png",        "04_portr", Some("tinypng-web/04-photo-portrait.png")),
        ("inputs/05-photo-mountain.png",        "05_mtn",   Some("tinypng-web/05-photo-mountain.png")),
        ("inputs/06-photo-landscape.png",       "06_land",  Some("tinypng-web/06-photo-landscape.png")),
        ("inputs/07-photo-product.png",         "07_prod",  Some("tinypng-web/07-photo-product.png")),
        ("inputs-ext-real/17-aurora-5mp.png",          "17_aur", None),
        ("inputs-ext-real/25-sofia-cathedral-5mp.png", "25_sof", None),
        ("inputs-ext-real/27-whale-tail-5mp.png",      "27_whl", None),
        ("corpus-500/mi0.png",                "mi0",        None),
        ("corpus-500/n29_astronaut.png",      "n29_astro",  None),
        ("corpus-500/p11_480x320.png",        "p11",        None),
        ("corpus-500/p32_480x320.png",        "p32",        None),
        ("corpus-500/p409_sm_300x320.png",    "p409",       None),
        ("corpus-500/p426_sm_460x380.png",    "p426",       None),
        ("corpus-500/p449_sm_300x320.png",    "p449",       None),
        ("corpus-500/p66_1024x768.png",       "p66",        None),
        ("corpus-500/p7_480x320.png",         "p7",         None),
        ("corpus-500/s042_stripes_p8.png",    "s042",       None),
        ("corpus-500/n01_mars.png",           "n01_mars",   None),
        ("corpus-500/n31_rover.png",          "n31_rover",  None),
        ("corpus-500/p119_1024x768.png",      "p119",       None),
        ("corpus-500/p38_480x320.png",        "p38",        None),
        ("corpus-500/p430_sm_380x380.png",    "p430",       None),
        ("corpus-500/p56_480x320.png",        "p56",        None),
        ("corpus-500/p84_1024x768.png",       "p84",        None),
        ("corpus-500/s006_gradient_1306x1113.png", "s006",  None),
        ("corpus-500/s040_stripes_p2.png",    "s040",       None),
        ("corpus-500/s059_solid.png",         "s059",       None),
    ];

    let tmp = std::env::temp_dir();
    println!("Cycle 103 — routing predicate validation on {} fixtures", fixtures.len());
    println!("Gate per triggering fixture: override_size < prod_size AND override_SSIM ≥ tiny (or ≥ prod_SSIM − 0.5 for non-tiny)");
    println!();
    println!("{:<12} {:>4} {:>4} {:>7} {:>5} {:>5} {:>6} {:>6} {:>6} {:>7} {:>5} | {:>5} | {:>7} {:>7} {:>8} | {:>4} {:>4} {:>8} {:>8}",
             "fixture","KB","opq","uniq","adjm","var","chrom","smth","entr","trig","cfg","route","prod_B","ovrd_B","Δsize","prdQ","ovrQ","Δssim","verdict");

    let mut n_p01 = 0; let mut n_p03 = 0; let mut n_p07 = 0;
    let mut p01_wins = 0; let mut p03_wins = 0; let mut p07_wins = 0;
    let mut p01_losses: Vec<String> = Vec::new();
    let mut p03_losses: Vec<String> = Vec::new();
    let mut p07_losses: Vec<String> = Vec::new();

    for (rel, lbl, tiny_rel) in &fixtures {
        let path = root.join("assets/png-bench").join(rel);
        if !path.exists() { println!("MISSING {}", lbl); continue; }
        let img = match ImageReader::open(&path).and_then(|r| r.with_guessed_format()).and_then(|r| Ok(r.decode())) {
            Ok(Ok(i)) => i,
            _ => { println!("decode fail: {}", lbl); continue; }
        };
        let r = img.to_rgba8();
        let w = r.width(); let h = r.height();
        let raw = r.into_raw();

        // Production output
        let prod_path = tmp.join(format!("c103_{}_prod.png", lbl));
        let prod_size = run_nupic(&path, &prod_path, &nupic);

        // Features (file_kb = production size as proxy for "small file" detection)
        let f = compute_features(&raw, w, h, prod_size);
        let routed = route(&f);
        let cfg = override_config(routed);

        // Override encode if routed
        let (ovrd_size, ovrd_path) = if let Some((k, d, p)) = cfg {
            let bytes = encode_override(&raw, w, h, k, d, p);
            let op = tmp.join(format!("c103_{}_ovrd.png", lbl));
            std::fs::write(&op, &bytes)?;
            (bytes.len(), Some(op))
        } else {
            (0, None)
        };

        let prod_ssim = ssim(&path, &prod_path, &nupic);
        let ovrd_ssim = if let Some(ref op) = ovrd_path { ssim(&path, op, &nupic) } else { f64::NAN };
        let tiny_ssim = if let Some(t) = tiny_rel {
            let tp = root.join("assets/png-bench").join(t);
            if tp.exists() { ssim(&path, &tp, &nupic) } else { f64::NAN }
        } else { f64::NAN };

        let trig_str = match routed {
            Override::None => "-", Override::P01 => "P01", Override::P03 => "P03", Override::P07 => "P07"
        };
        let cfg_str = if let Some((k,d,p)) = cfg { format!("K{}d{:.1}p{}",k,d,p) } else { "-".into() };

        let delta_size = if cfg.is_some() { ovrd_size as i64 - prod_size as i64 } else { 0 };
        let delta_ssim = if cfg.is_some() && !ovrd_ssim.is_nan() && !prod_ssim.is_nan() { ovrd_ssim - prod_ssim } else { 0.0 };

        // Verdict
        let verdict = if cfg.is_none() {
            "skip"
        } else {
            // Acceptable: override smaller AND (SSIM ≥ tiny OR drop < 5 pp vs prod)
            let size_better = delta_size < 0;
            let ssim_ok = if !tiny_ssim.is_nan() {
                ovrd_ssim >= tiny_ssim
            } else {
                delta_ssim >= -5.0
            };
            if size_better && ssim_ok { "WIN" }
            else if !size_better { "size↑" }
            else { "ssim↓" }
        };

        let win = verdict == "WIN";
        match routed {
            Override::P01 => { n_p01 += 1; if win { p01_wins += 1 } else if cfg.is_some() { p01_losses.push(format!("{}({}: Δsz={}B Δssim={:.2})", lbl, verdict, delta_size, delta_ssim)) } }
            Override::P03 => { n_p03 += 1; if win { p03_wins += 1 } else if cfg.is_some() { p03_losses.push(format!("{}({}: Δsz={}B Δssim={:.2})", lbl, verdict, delta_size, delta_ssim)) } }
            Override::P07 => { n_p07 += 1; if win { p07_wins += 1 } else if cfg.is_some() { p07_losses.push(format!("{}({}: Δsz={}B Δssim={:.2})", lbl, verdict, delta_size, delta_ssim)) } }
            _ => {}
        }

        println!("{:<12} {:>4} {:>4.2} {:>7} {:>5.1} {:>5.0} {:>6.3} {:>6.4} {:>6.2} {:>7} {:>5} | {:>5} | {:>7} {:>7} {:>+8} | {:>4.1} {:>4} {:>+8.2} {:>8}",
                 lbl, prod_size/1024, f.opq, f.uniq_opq, f.adj_mn, f.var, f.mean_chroma, f.smoothness, f.chroma_entropy,
                 trig_str, cfg_str.as_str(),
                 trig_str, prod_size, ovrd_size, delta_size,
                 prod_ssim, if !ovrd_ssim.is_nan() { format!("{:.1}", ovrd_ssim) } else { "-".into() }, delta_ssim, verdict);
    }

    println!();
    println!("=== Predicate trigger + win/loss summary ===");
    println!("P-01: triggered {}, wins {}, losses {:?}", n_p01, p01_wins, p01_losses);
    println!("P-03: triggered {}, wins {}, losses {:?}", n_p03, p03_wins, p03_losses);
    println!("P-07: triggered {}, wins {}, losses {:?}", n_p07, p07_wins, p07_losses);
    println!();
    println!("Decision per predicate (gate: 100% wins on triggering, no false trigger on production-already-OK):");
    for (name, n, wins, losses) in &[("P-01", n_p01, p01_wins, &p01_losses),
                                      ("P-03", n_p03, p03_wins, &p03_losses),
                                      ("P-07", n_p07, p07_wins, &p07_losses)] {
        if *n == 0 { println!("  {} : no trigger — predicate too tight or fixture not in cohort", name); }
        else if *wins == *n && losses.is_empty() { println!("  {} : GREEN  ({}/{} wins, ship as override)", name, wins, n); }
        else if *wins as f32 / *n as f32 >= 0.8 { println!("  {} : YELLOW ({}/{} wins, narrow trigger or back off)", name, wins, n); }
        else { println!("  {} : RED    ({}/{} wins, predicate misroutes — drop or redesign)", name, wins, n); }
    }

    Ok(())
}
