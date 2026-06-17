//! Cycle 93 — R1 classifier richer features + re-fit on 30 ground-truth
//!
//! Cycle 92 RED'd the 4-rule classifier (12/20 = 60%) on corpus-500 sample.
//! Hard FPs all live in the chroma=0.04-0.07 band and 4 simple features
//! cannot separate them. This cycle adds 3 richer features:
//!
//!   bandpass_coarse_ratio:
//!     mean(|G2 − G4|) / max(mean(|G0 − G1|), ε)
//!     — ratio of mid-scale to fine-scale OKLab L bandpass energy. High value
//!       = content lives in mid-scale frequencies (where R1's b-weight is tuned);
//!       low value = content is dominated by fine-scale noise (R1-hostile).
//!
//!   chroma_entropy:
//!     Shannon entropy over 16-bin 2D histogram of (OKLab a, b) — measures
//!     how broadly chroma is distributed. R1 helps on wide-gamut content,
//!     hurts on narrow-palette photos (mars/aurora).
//!
//!   edge_chroma_corr:
//!     Pearson correlation between per-pixel sqrt(a²+b²) and per-pixel
//!     gradient magnitude. High = chroma coincides with edges (R1-friendly,
//!     b-weight amplifies these); near-zero = chroma scattered across smooth
//!     regions (R1 weighting has no traction).
//!
//! Ground truth = Cycle 91a's 10 fixtures + Cycle 92's 20 fixtures = 30 total.
//! All ΔSSIMs are hardcoded from prior bench outputs.

use std::path::PathBuf;

use image::ImageReader;
use rgb::Rgb;

use nupic_color::{Oklab, srgb_u8_to_oklab};

struct Truth {
    rel: &'static str,
    label: &'static str,
    d_ssim: f64,
}

const GROUND_TRUTH: &[Truth] = &[
    // Cycle 91a's 10 (baseline-7 + 3 × 5MP)
    Truth { rel: "inputs/01-png-transparency-demo.png", label: "01_trans", d_ssim: 35.97 },
    Truth { rel: "inputs/02-pluto-transparent.png",     label: "02_pluto", d_ssim:  6.02 },
    Truth { rel: "inputs/03-wikipedia-logo.png",        label: "03_wiki",  d_ssim: -0.01 },
    Truth { rel: "inputs/04-photo-portrait.png",        label: "04_portrait", d_ssim:  1.22 },
    Truth { rel: "inputs/05-photo-mountain.png",        label: "05_mountain", d_ssim: -4.35 },
    Truth { rel: "inputs/06-photo-landscape.png",       label: "06_landscape", d_ssim: -0.41 },
    Truth { rel: "inputs/07-photo-product.png",         label: "07_product", d_ssim: -0.45 },
    Truth { rel: "inputs-ext-real/17-aurora-5mp.png",   label: "17_aurora", d_ssim: -2.17 },
    Truth { rel: "inputs-ext-real/25-sofia-cathedral-5mp.png", label: "25_sofia", d_ssim: 5.19 },
    Truth { rel: "inputs-ext-real/27-whale-tail-5mp.png", label: "27_whale", d_ssim: 1.66 },
    // Cycle 92's 20 (corpus-500 sample) — ΔSSIM from 04ww essay table
    Truth { rel: "corpus-500/mi0.png",               label: "mi0", d_ssim: 0.00 },
    Truth { rel: "corpus-500/n29_astronaut.png",     label: "n29_astronaut", d_ssim: -2.28 },
    Truth { rel: "corpus-500/p11_480x320.png",       label: "p11", d_ssim: -0.05 },
    Truth { rel: "corpus-500/p32_480x320.png",       label: "p32", d_ssim: 4.14 },
    Truth { rel: "corpus-500/p409_sm_300x320.png",   label: "p409", d_ssim: 0.47 },
    Truth { rel: "corpus-500/p426_sm_460x380.png",   label: "p426", d_ssim: -0.67 },
    Truth { rel: "corpus-500/p449_sm_300x320.png",   label: "p449", d_ssim: 0.57 },
    Truth { rel: "corpus-500/p66_1024x768.png",      label: "p66", d_ssim: -3.39 },
    Truth { rel: "corpus-500/p7_480x320.png",        label: "p7", d_ssim: -0.85 },
    Truth { rel: "corpus-500/s042_stripes_p8.png",   label: "s042", d_ssim: 0.00 },
    Truth { rel: "corpus-500/n01_mars.png",          label: "n01_mars", d_ssim: -2.60 },
    Truth { rel: "corpus-500/n31_rover.png",         label: "n31_rover", d_ssim: -1.28 },
    Truth { rel: "corpus-500/p119_1024x768.png",     label: "p119", d_ssim: -0.26 },
    Truth { rel: "corpus-500/p38_480x320.png",       label: "p38", d_ssim: -0.05 },
    Truth { rel: "corpus-500/p430_sm_380x380.png",   label: "p430", d_ssim: 0.09 },
    Truth { rel: "corpus-500/p56_480x320.png",       label: "p56", d_ssim: -0.48 },
    Truth { rel: "corpus-500/p84_1024x768.png",      label: "p84", d_ssim: -8.68 },
    Truth { rel: "corpus-500/s006_gradient_1306x1113.png", label: "s006", d_ssim: 0.00 },
    Truth { rel: "corpus-500/s040_stripes_p2.png",   label: "s040", d_ssim: 0.00 },
    Truth { rel: "corpus-500/s059_solid.png",        label: "s059", d_ssim: 0.00 },
];

const FRIEND_GATE: f64 = 0.5;

#[derive(Clone, Debug, Default)]
struct Features {
    mean_chroma: f32,
    smoothness: f32,
    edge_density: f32,
    trans_frac: f32,
    bandpass_ratio: f32,
    chroma_entropy: f32,
    edge_chroma_corr: f32,
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

    // smoothness (adjacent abs luma diff, H+V)
    let mut sum_h = 0.0f64;
    let mut sum_v = 0.0f64;
    let mut count_h = 0usize;
    let mut count_v = 0usize;
    for y in 0..h {
        for x in 0..w-1 {
            let i = y * w + x;
            sum_h += (oklab[i].l - oklab[i + 1].l).abs() as f64;
            count_h += 1;
        }
    }
    if h >= 1 {
        for y in 0..h-1 {
            for x in 0..w {
                let i = y * w + x;
                sum_v += (oklab[i].l - oklab[i + w].l).abs() as f64;
                count_v += 1;
            }
        }
    }
    let smoothness = ((sum_h / count_h.max(1) as f64) + (sum_v / count_v.max(1) as f64)) as f32;

    // edge density + gradient magnitude per-pixel (re-used below for corr)
    let mut grad_mag = vec![0f32; n];
    let mut edge_count = 0usize;
    let mut edge_total = 0usize;
    if w >= 3 && h >= 3 {
        for y in 1..h-1 {
            for x in 1..w-1 {
                let i = y * w + x;
                let gx = oklab[i + 1].l - oklab[i - 1].l;
                let gy = oklab[i + w].l - oklab[i - w].l;
                let mag = (gx * gx + gy * gy).sqrt();
                grad_mag[i] = mag;
                if mag > 0.05 { edge_count += 1; }
                edge_total += 1;
            }
        }
    }
    let edge_density = edge_count as f32 / edge_total.max(1) as f32;

    // ===== NEW: bandpass_coarse_ratio =====
    let l: Vec<f32> = oklab.iter().map(|o| o.l).collect();
    let g1 = gauss5(&l, w, h);
    let g2 = gauss5(&g1, w, h);
    let g3 = gauss5(&g2, w, h);
    let g4 = gauss5(&g3, w, h);
    let fine: f64 = l.iter().zip(g1.iter()).map(|(a, b)| (a - b).abs() as f64).sum::<f64>() / n as f64;
    let coarse: f64 = g2.iter().zip(g4.iter()).map(|(a, b)| (a - b).abs() as f64).sum::<f64>() / n as f64;
    let bandpass_ratio = (coarse / fine.max(1e-9)) as f32;

    // ===== NEW: chroma_entropy =====
    let bins = 16usize;
    let mut hist = vec![0u32; bins * bins];
    // Estimate a/b range from data
    let mut a_min = f32::INFINITY; let mut a_max = f32::NEG_INFINITY;
    let mut b_min = f32::INFINITY; let mut b_max = f32::NEG_INFINITY;
    for o in &oklab {
        if o.a < a_min { a_min = o.a; }
        if o.a > a_max { a_max = o.a; }
        if o.b < b_min { b_min = o.b; }
        if o.b > b_max { b_max = o.b; }
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
    for &c in hist.iter() {
        if c > 0 {
            let p = c as f64 / total;
            entropy -= p * p.log2();
        }
    }
    let chroma_entropy = entropy as f32; // max ≈ log2(256) = 8

    // ===== NEW: edge_chroma_corr (Pearson) =====
    let chroma_per_pixel: Vec<f32> = oklab.iter().map(|o| (o.a * o.a + o.b * o.b).sqrt()).collect();
    let mut sum_c = 0.0f64;
    let mut sum_g = 0.0f64;
    let mut count = 0usize;
    if w >= 3 && h >= 3 {
        for y in 1..h-1 {
            for x in 1..w-1 {
                let i = y * w + x;
                sum_c += chroma_per_pixel[i] as f64;
                sum_g += grad_mag[i] as f64;
                count += 1;
            }
        }
    }
    let mean_c = sum_c / count.max(1) as f64;
    let mean_g = sum_g / count.max(1) as f64;
    let mut cov = 0.0f64;
    let mut var_c = 0.0f64;
    let mut var_g = 0.0f64;
    if w >= 3 && h >= 3 {
        for y in 1..h-1 {
            for x in 1..w-1 {
                let i = y * w + x;
                let dc = chroma_per_pixel[i] as f64 - mean_c;
                let dg = grad_mag[i] as f64 - mean_g;
                cov += dc * dg;
                var_c += dc * dc;
                var_g += dg * dg;
            }
        }
    }
    let edge_chroma_corr = if var_c > 1e-12 && var_g > 1e-12 {
        (cov / (var_c.sqrt() * var_g.sqrt())) as f32
    } else { 0.0 };

    Features { mean_chroma, smoothness, edge_density, trans_frac,
               bandpass_ratio, chroma_entropy, edge_chroma_corr }
}

fn eval_rule(feats: &[Features], gts: &[&Truth], rule: impl Fn(&Features) -> bool) -> (usize, usize, usize, Vec<bool>) {
    let mut correct = 0usize;
    let mut fps = 0usize;
    let mut fns = 0usize;
    let mut decs = Vec::with_capacity(feats.len());
    for (f, gt) in feats.iter().zip(gts.iter()) {
        let pred = rule(f);
        let actual = gt.d_ssim >= FRIEND_GATE;
        if pred == actual { correct += 1; }
        if pred && !actual { fps += 1; }
        if !pred && actual { fns += 1; }
        decs.push(pred);
    }
    (correct, fps, fns, decs)
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors().nth(2).unwrap().to_path_buf();

    println!("Cycle 93 — R1 classifier richer features + re-fit on 30 ground truth");
    println!("  FRIEND_GATE = ΔSSIM ≥ {}", FRIEND_GATE);
    println!();

    let mut feats: Vec<Features> = Vec::new();
    let mut gts: Vec<&Truth> = Vec::new();
    println!("{:<18} {:>7} {:>8} {:>7} {:>7} {:>7} {:>7} {:>9} {:>7} {:>7}",
             "fixture", "ΔSSIM", "FRIEND?", "chroma", "smooth", "edge", "trans",
             "bandpass", "entropy", "ec_corr");
    for gt in GROUND_TRUTH {
        let path = root.join("assets/png-bench").join(gt.rel);
        let img = match ImageReader::open(&path).and_then(|r| r.with_guessed_format()) {
            Ok(r) => match r.decode() { Ok(i) => i, Err(_) => continue },
            Err(_) => continue,
        };
        let r = img.to_rgba8();
        let w = r.width() as usize; let h = r.height() as usize;
        let raw = r.into_raw();
        let f = compute_features(&raw, w, h);
        let friendly = gt.d_ssim >= FRIEND_GATE;
        println!("{:<18} {:>+7.2} {:>8} {:>7.3} {:>7.3} {:>7.3} {:>7.3} {:>9.3} {:>7.2} {:>+7.3}",
                 gt.label, gt.d_ssim,
                 if friendly { "FRIEND" } else { "HOSTILE" },
                 f.mean_chroma, f.smoothness, f.edge_density, f.trans_frac,
                 f.bandpass_ratio, f.chroma_entropy, f.edge_chroma_corr);
        feats.push(f);
        gts.push(gt);
    }
    println!();

    let n = feats.len();
    let n_actual_friend = gts.iter().filter(|g| g.d_ssim >= FRIEND_GATE).count();
    println!("Loaded {} fixtures; {} actual FRIEND, {} HOSTILE", n, n_actual_friend, n - n_actual_friend);
    println!();

    // Helper to grid-sweep a single threshold (predict FRIEND if feature > t)
    fn sweep_gt(feats: &[Features], gts: &[&Truth], name: &str, getter: impl Fn(&Features) -> f32) {
        let mut vals: Vec<f32> = feats.iter().map(&getter).collect();
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut best = (0usize, 0.0f32, 0usize, 0usize);
        for w in vals.windows(2) {
            let t = (w[0] + w[1]) / 2.0;
            let (c, fp, fn_, _) = eval_rule(feats, gts, |f| getter(f) > t);
            if c > best.0 { best = (c, t, fp, fn_); }
        }
        println!("  {:<28} best > t   acc {}/{}  t={:.4}  FP={} FN={}",
                 name, best.0, feats.len(), best.1, best.2, best.3);
    }
    fn sweep_lt(feats: &[Features], gts: &[&Truth], name: &str, getter: impl Fn(&Features) -> f32) {
        let mut vals: Vec<f32> = feats.iter().map(&getter).collect();
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut best = (0usize, 0.0f32, 0usize, 0usize);
        for w in vals.windows(2) {
            let t = (w[0] + w[1]) / 2.0;
            let (c, fp, fn_, _) = eval_rule(feats, gts, |f| getter(f) < t);
            if c > best.0 { best = (c, t, fp, fn_); }
        }
        println!("  {:<28} best < t   acc {}/{}  t={:.4}  FP={} FN={}",
                 name, best.0, feats.len(), best.1, best.2, best.3);
    }

    println!("--- Single-feature threshold sweeps ---");
    sweep_gt(&feats, &gts, "mean_chroma",        |f| f.mean_chroma);
    sweep_lt(&feats, &gts, "smoothness",         |f| f.smoothness);
    sweep_gt(&feats, &gts, "edge_density",       |f| f.edge_density);
    sweep_gt(&feats, &gts, "trans_frac",         |f| f.trans_frac);
    sweep_gt(&feats, &gts, "bandpass_ratio",     |f| f.bandpass_ratio);
    sweep_gt(&feats, &gts, "chroma_entropy",     |f| f.chroma_entropy);
    sweep_gt(&feats, &gts, "edge_chroma_corr",   |f| f.edge_chroma_corr);
    println!();

    // Cycle 91a rule (baseline for comparison)
    println!("--- Baseline: Cycle 91a 4-rule ---");
    let (c, fp, fn_, _) = eval_rule(&feats, &gts,
        |f| f.trans_frac > 0.0 || (f.mean_chroma > 0.0166 && f.edge_density > 0.1502 && f.smoothness < 0.0614));
    println!("  trans||(chroma>0.017 & edge>0.150 & smooth<0.061)  acc {}/{}  FP={} FN={}", c, n, fp, fn_);
    println!();

    // Grid-sweep: trans OR (bandpass > t1 AND edge_chroma_corr > t2)
    println!("--- 2-rule with new features: trans OR (bandpass > t1 AND ec_corr > t2) ---");
    let mut br_vals: Vec<f32> = feats.iter().map(|f| f.bandpass_ratio).collect();
    br_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut ec_vals: Vec<f32> = feats.iter().map(|f| f.edge_chroma_corr).collect();
    ec_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut best_a = (0usize, 0.0f32, 0.0f32, 0usize, 0usize);
    for w1 in br_vals.windows(2) {
        let t1 = (w1[0] + w1[1]) / 2.0;
        for w2 in ec_vals.windows(2) {
            let t2 = (w2[0] + w2[1]) / 2.0;
            let (c, fp, fn_, _) = eval_rule(&feats, &gts,
                |f| f.trans_frac > 0.0 || (f.bandpass_ratio > t1 && f.edge_chroma_corr > t2));
            if c > best_a.0 || (c == best_a.0 && fn_ < best_a.4) {
                best_a = (c, t1, t2, fp, fn_);
            }
        }
    }
    println!("  bandpass > {:.4}  ec_corr > {:.4}  acc {}/{}  FP={} FN={}",
             best_a.1, best_a.2, best_a.0, n, best_a.3, best_a.4);

    // Augmented: trans OR (chroma > t1 AND edge > t2 AND smooth < t3 AND bandpass > t4)
    println!();
    println!("--- 5-rule: trans OR (chroma>t1 AND edge>t2 AND smooth<t3 AND bandpass>t4) ---");
    let mut ch_vals: Vec<f32> = feats.iter().map(|f| f.mean_chroma).collect();
    ch_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut ed_vals: Vec<f32> = feats.iter().map(|f| f.edge_density).collect();
    ed_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut sm_vals: Vec<f32> = feats.iter().map(|f| f.smoothness).collect();
    sm_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    // To keep grid size manageable, decimate threshold candidates to every-other value
    let dec = |vs: &[f32]| -> Vec<f32> {
        let mut out = Vec::new();
        let stride = 2;
        for i in (0..vs.len().saturating_sub(1)).step_by(stride) {
            out.push((vs[i] + vs[i + 1]) / 2.0);
        }
        out
    };
    let ch_grid = dec(&ch_vals);
    let ed_grid = dec(&ed_vals);
    let sm_grid = dec(&sm_vals);
    let br_grid = dec(&br_vals);
    let mut best_b = (0usize, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0usize, 0usize);
    for &t1 in &ch_grid {
        for &t2 in &ed_grid {
            for &t3 in &sm_grid {
                for &t4 in &br_grid {
                    let (c, fp, fn_, _) = eval_rule(&feats, &gts,
                        |f| f.trans_frac > 0.0 || (f.mean_chroma > t1 && f.edge_density > t2 && f.smoothness < t3 && f.bandpass_ratio > t4));
                    if c > best_b.0 || (c == best_b.0 && fn_ < best_b.6) {
                        best_b = (c, t1, t2, t3, t4, fp, fn_);
                    }
                }
            }
        }
    }
    println!("  chroma>{:.4} edge>{:.4} smooth<{:.4} bandpass>{:.4}  acc {}/{}  FP={} FN={}",
             best_b.1, best_b.2, best_b.3, best_b.4, best_b.0, n, best_b.5, best_b.6);

    // 6-rule: 5-rule + entropy
    println!();
    println!("--- 6-rule: 5-rule + chroma_entropy > t5 ---");
    let mut en_vals: Vec<f32> = feats.iter().map(|f| f.chroma_entropy).collect();
    en_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let en_grid = dec(&en_vals);
    let mut best_c = (0usize, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0usize, 0usize);
    for &t1 in &ch_grid {
        for &t2 in &ed_grid {
            for &t3 in &sm_grid {
                for &t4 in &br_grid {
                    for &t5 in &en_grid {
                        let (c, fp, fn_, _) = eval_rule(&feats, &gts,
                            |f| f.trans_frac > 0.0 || (
                                f.mean_chroma > t1 && f.edge_density > t2 && f.smoothness < t3
                                && f.bandpass_ratio > t4 && f.chroma_entropy > t5));
                        if c > best_c.0 || (c == best_c.0 && fn_ < best_c.7) {
                            best_c = (c, t1, t2, t3, t4, t5, fp, fn_);
                        }
                    }
                }
            }
        }
    }
    println!("  chroma>{:.4} edge>{:.4} smooth<{:.4} bandpass>{:.4} entropy>{:.4}  acc {}/{}  FP={} FN={}",
             best_c.1, best_c.2, best_c.3, best_c.4, best_c.5, best_c.0, n, best_c.6, best_c.7);

    // Print error rows for the best 6-rule
    println!();
    let best_rule = |f: &Features| -> bool {
        f.trans_frac > 0.0 || (
            f.mean_chroma > best_c.1 && f.edge_density > best_c.2 && f.smoothness < best_c.3
            && f.bandpass_ratio > best_c.4 && f.chroma_entropy > best_c.5
        )
    };
    let (_, _, _, decs) = eval_rule(&feats, &gts, best_rule);
    println!("Error rows for best 6-rule (acc {}/{}, FP={} FN={}):",
             best_c.0, n, best_c.6, best_c.7);
    println!("{:<18} {:>+7} {:>8} {:>9}", "fixture", "ΔSSIM", "actual", "predicted");
    for i in 0..n {
        let pred = decs[i];
        let actual = gts[i].d_ssim >= FRIEND_GATE;
        if pred != actual {
            println!("{:<18} {:>+7.2} {:>8} {:>9}",
                     gts[i].label, gts[i].d_ssim,
                     if actual { "FRIEND" } else { "HOSTILE" },
                     if pred { "FRIEND" } else { "HOSTILE" });
        }
    }

    let acc_pct = 100.0 * best_c.0 as f64 / n as f64;
    println!();
    if acc_pct >= 85.0 && best_c.7 == 0 {
        println!(">>> GREEN — 6-rule clears 85% with 0 FN");
    } else if acc_pct >= 85.0 {
        println!(">>> YELLOW-acc — 6-rule clears 85% but FN > 0");
    } else if acc_pct >= 70.0 {
        println!(">>> YELLOW — improves over Cycle 92 60% but short of 85%");
    } else {
        println!(">>> RED — features still insufficient");
    }

    Ok(())
}
