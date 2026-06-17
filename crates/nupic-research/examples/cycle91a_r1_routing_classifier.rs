//! Cycle 91a — R1 routing classifier spike (paper §6 routing analysis)
//!
//! Cycle 90 confirmed R1 (M-weighted Lloyd) cannot ship blanket: 5/10 fixtures
//! went RED (quality dip or noise). The fix per roadmap is a content-feature
//! classifier that flips R1 ON only on R1-friendly content.
//!
//! Ground truth from Cycle 90 (ΔSSIM column, R1+R8+R9 combined vs baseline):
//!   R1-FRIENDLY (ΔSSIM ≥ +0.5):  01 trans (+35.97), 02 pluto (+6.02),
//!                                 04 portrait (+1.22), 25 sofia (+5.19),
//!                                 27 whale (+1.66)
//!   R1-HOSTILE  (ΔSSIM < +0.5):  03 wiki (-0.01), 05 mountain (-4.35),
//!                                 06 landscape (-0.41), 07 product (-0.45),
//!                                 17 aurora (-2.17)
//!
//! Features computed per-fixture (cheap — < 1 ms even on 5MP):
//!   1. mean_chroma     = mean of sqrt(a² + b²) over OKLab pixels
//!   2. smoothness      = mean |L_i - L_{i+w}| + mean |L_i - L_{i+1}| (vertical+horizontal adjacent)
//!   3. edge_density    = fraction of pixels with grad-mag > 0.05 OKLab L units
//!   4. trans_frac      = fraction of pixels with alpha < 255 (R1 trivially wins on transparent)
//!
//! Decision-gate approach (paper §6 narrative): try 1-feature threshold; if
//! perfect-separation impossible, escalate to 2-feature linear or rule-based.

use std::path::PathBuf;

use image::ImageReader;
use rgb::Rgb;

use nupic_color::{Oklab, srgb_u8_to_oklab};

struct GroundTruth {
    rel: &'static str,
    label: &'static str,
    d_ssim: f64,
}

const FIXTURES: &[GroundTruth] = &[
    GroundTruth { rel: "inputs/01-png-transparency-demo.png", label: "01 trans",   d_ssim: 35.97 },
    GroundTruth { rel: "inputs/02-pluto-transparent.png",      label: "02 pluto",   d_ssim:  6.02 },
    GroundTruth { rel: "inputs/03-wikipedia-logo.png",         label: "03 wiki",    d_ssim: -0.01 },
    GroundTruth { rel: "inputs/04-photo-portrait.png",         label: "04 portrait",d_ssim:  1.22 },
    GroundTruth { rel: "inputs/05-photo-mountain.png",         label: "05 mountain",d_ssim: -4.35 },
    GroundTruth { rel: "inputs/06-photo-landscape.png",        label: "06 landscape",d_ssim: -0.41 },
    GroundTruth { rel: "inputs/07-photo-product.png",          label: "07 product", d_ssim: -0.45 },
    GroundTruth { rel: "inputs-ext-real/17-aurora-5mp.png",    label: "17 aurora",  d_ssim: -2.17 },
    GroundTruth { rel: "inputs-ext-real/25-sofia-cathedral-5mp.png", label: "25 sofia", d_ssim: 5.19 },
    GroundTruth { rel: "inputs-ext-real/27-whale-tail-5mp.png", label: "27 whale",  d_ssim: 1.66 },
];

const FRIEND_GATE: f64 = 0.5; // ΔSSIM threshold for R1-friendly

#[derive(Clone, Debug)]
struct Features {
    mean_chroma: f32,
    smoothness: f32,
    edge_density: f32,
    trans_frac: f32,
}

fn compute_features(raw_rgba: &[u8], w: usize, h: usize) -> Features {
    let n = w * h;
    let mut alpha_count_lt = 0usize;
    let oklab: Vec<Oklab> = raw_rgba.chunks_exact(4).map(|p| {
        if p[3] < 255 { alpha_count_lt += 1; }
        srgb_u8_to_oklab(Rgb { r: p[0], g: p[1], b: p[2] })
    }).collect();
    let trans_frac = alpha_count_lt as f32 / n as f32;

    // mean chroma
    let sum_chroma: f64 = oklab.iter().map(|o| (o.a * o.a + o.b * o.b).sqrt() as f64).sum();
    let mean_chroma = (sum_chroma / n as f64) as f32;

    // smoothness: mean |L_i - L_{i+1}| (horiz) + mean |L_i - L_{i+w}| (vert)
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
    for y in 0..h-1 {
        for x in 0..w {
            let i = y * w + x;
            sum_v += (oklab[i].l - oklab[i + w].l).abs() as f64;
            count_v += 1;
        }
    }
    let smoothness = ((sum_h / count_h.max(1) as f64) + (sum_v / count_v.max(1) as f64)) as f32;

    // edge density: |∇L| > 0.05 OKLab L units
    let mut edge_count = 0usize;
    let mut edge_total = 0usize;
    for y in 1..h-1 {
        for x in 1..w-1 {
            let i = y * w + x;
            let gx = oklab[i + 1].l - oklab[i - 1].l;
            let gy = oklab[i + w].l - oklab[i - w].l;
            let mag = (gx * gx + gy * gy).sqrt();
            if mag > 0.05 { edge_count += 1; }
            edge_total += 1;
        }
    }
    let edge_density = edge_count as f32 / edge_total.max(1) as f32;

    Features { mean_chroma, smoothness, edge_density, trans_frac }
}

fn eval_gate(features: &[Features], ground_truths: &[&GroundTruth], gate_fn: impl Fn(&Features) -> bool) -> (usize, usize, Vec<bool>) {
    let mut correct = 0usize;
    let n = features.len();
    let mut decisions = Vec::with_capacity(n);
    for i in 0..n {
        let predicted_friend = gate_fn(&features[i]);
        let actual_friend = ground_truths[i].d_ssim >= FRIEND_GATE;
        if predicted_friend == actual_friend { correct += 1; }
        decisions.push(predicted_friend);
    }
    (correct, n, decisions)
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors().nth(2).unwrap().to_path_buf();

    println!("Cycle 91a — R1 routing classifier spike");
    println!("  goal: predict R1-friendly fixtures (ΔSSIM ≥ +0.5) from content features");
    println!();

    let mut feats: Vec<Features> = Vec::new();
    let mut gts: Vec<&GroundTruth> = Vec::new();

    println!(
        "{:<14} {:>9} {:>10} {:>11} {:>11} {:>11} {:>10}",
        "fixture", "ΔSSIM", "friendly", "chroma", "smooth", "edge_dens", "trans"
    );
    for gt in FIXTURES {
        let path = root.join("assets/png-bench").join(gt.rel);
        let img = ImageReader::open(&path)?.with_guessed_format()?.decode()?;
        let r = img.to_rgba8();
        let w = r.width() as usize;
        let h = r.height() as usize;
        let raw = r.into_raw();
        let f = compute_features(&raw, w, h);
        let friendly = gt.d_ssim >= FRIEND_GATE;
        println!(
            "{:<14} {:>+9.2} {:>10} {:>11.5} {:>11.5} {:>11.4} {:>10.3}",
            gt.label, gt.d_ssim,
            if friendly { "FRIEND" } else { "HOSTILE" },
            f.mean_chroma, f.smoothness, f.edge_density, f.trans_frac
        );
        feats.push(f);
        gts.push(gt);
    }
    println!();

    // --- Sweep single-feature thresholds ---
    let n = feats.len();

    // Sweep chroma
    let mut chromas: Vec<f32> = feats.iter().map(|f| f.mean_chroma).collect();
    chromas.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("--- Single-feature: mean_chroma threshold sweep (predict FRIEND if chroma > t) ---");
    let mut best_chroma_t = 0.0f32;
    let mut best_chroma_acc = 0usize;
    for w_pair in chromas.windows(2) {
        let t = (w_pair[0] + w_pair[1]) / 2.0;
        let (corr, _, _) = eval_gate(&feats, &gts, |f| f.mean_chroma > t);
        if corr > best_chroma_acc {
            best_chroma_acc = corr;
            best_chroma_t = t;
        }
    }
    println!("  best chroma threshold = {:.5}  acc = {}/{}", best_chroma_t, best_chroma_acc, n);

    // Sweep smoothness (predict FRIEND if smoothness > t — high adjacent variation = chroma/edge content)
    let mut smooths: Vec<f32> = feats.iter().map(|f| f.smoothness).collect();
    smooths.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("--- Single-feature: smoothness threshold sweep (predict FRIEND if smoothness > t) ---");
    let mut best_smooth_t = 0.0f32;
    let mut best_smooth_acc = 0usize;
    for w_pair in smooths.windows(2) {
        let t = (w_pair[0] + w_pair[1]) / 2.0;
        let (corr, _, _) = eval_gate(&feats, &gts, |f| f.smoothness > t);
        if corr > best_smooth_acc {
            best_smooth_acc = corr;
            best_smooth_t = t;
        }
    }
    println!("  best smoothness threshold = {:.5}  acc = {}/{}", best_smooth_t, best_smooth_acc, n);

    // Sweep edge density (predict FRIEND if edge_density > t)
    let mut eds: Vec<f32> = feats.iter().map(|f| f.edge_density).collect();
    eds.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("--- Single-feature: edge_density threshold sweep (predict FRIEND if edge_density > t) ---");
    let mut best_ed_t = 0.0f32;
    let mut best_ed_acc = 0usize;
    for w_pair in eds.windows(2) {
        let t = (w_pair[0] + w_pair[1]) / 2.0;
        let (corr, _, _) = eval_gate(&feats, &gts, |f| f.edge_density > t);
        if corr > best_ed_acc {
            best_ed_acc = corr;
            best_ed_t = t;
        }
    }
    println!("  best edge_density threshold = {:.5}  acc = {}/{}", best_ed_t, best_ed_acc, n);

    // Trans-aware shortcut: trans > 0 → friend (01/02 always wins big)
    println!("--- Rule: trans_frac > 0 ⇒ FRIEND ---");
    let (corr_t, _, _) = eval_gate(&feats, &gts, |f| f.trans_frac > 0.0);
    println!("  acc = {}/{}", corr_t, n);

    // 2-feature: trans OR (chroma > t)
    println!();
    println!("--- 2-feature: FRIEND if trans_frac > 0 OR chroma > t ---");
    let mut best_t = 0.0f32;
    let mut best_acc = 0usize;
    let mut best_decisions: Vec<bool> = Vec::new();
    for w_pair in chromas.windows(2) {
        let t = (w_pair[0] + w_pair[1]) / 2.0;
        let (corr, _, decs) = eval_gate(&feats, &gts, |f| f.trans_frac > 0.0 || f.mean_chroma > t);
        if corr > best_acc {
            best_acc = corr;
            best_t = t;
            best_decisions = decs;
        }
    }
    println!("  best chroma_t = {:.5}  acc = {}/{}", best_t, best_acc, n);

    // 2-feature: trans OR (low smoothness ⇒ smooth-only) — invert: predict HOSTILE if smoothness < t & not trans
    // Equivalently: FRIEND if trans>0 OR smoothness > t
    println!("--- 2-feature: FRIEND if trans_frac > 0 OR smoothness > t ---");
    let mut best_t2 = 0.0f32;
    let mut best_acc2 = 0usize;
    let mut best_decisions2: Vec<bool> = Vec::new();
    for w_pair in smooths.windows(2) {
        let t = (w_pair[0] + w_pair[1]) / 2.0;
        let (corr, _, decs) = eval_gate(&feats, &gts, |f| f.trans_frac > 0.0 || f.smoothness > t);
        if corr > best_acc2 {
            best_acc2 = corr;
            best_t2 = t;
            best_decisions2 = decs;
        }
    }
    println!("  best smoothness_t = {:.5}  acc = {}/{}", best_t2, best_acc2, n);

    // 2-feature: trans OR (edge_density > t)
    println!("--- 2-feature: FRIEND if trans_frac > 0 OR edge_density > t ---");
    let mut best_t3 = 0.0f32;
    let mut best_acc3 = 0usize;
    let mut best_decisions3: Vec<bool> = Vec::new();
    for w_pair in eds.windows(2) {
        let t = (w_pair[0] + w_pair[1]) / 2.0;
        let (corr, _, decs) = eval_gate(&feats, &gts, |f| f.trans_frac > 0.0 || f.edge_density > t);
        if corr > best_acc3 {
            best_acc3 = corr;
            best_t3 = t;
            best_decisions3 = decs;
        }
    }
    println!("  best edge_density_t = {:.5}  acc = {}/{}", best_t3, best_acc3, n);

    // 3-rule AND: chroma > t1 AND edge_density < t2  (target 17 aurora's low edge)
    println!("--- 3-rule: FRIEND if trans_frac > 0 OR (chroma > t1 AND edge_density < t2) ---");
    let mut best_acc_3a = 0usize;
    let mut best_t1_3a = 0.0f32;
    let mut best_t2_3a = 0.0f32;
    let mut best_decs_3a: Vec<bool> = Vec::new();
    for w1 in chromas.windows(2) {
        let t1 = (w1[0] + w1[1]) / 2.0;
        for w2 in eds.windows(2) {
            let t2 = (w2[0] + w2[1]) / 2.0;
            let (corr, _, decs) = eval_gate(&feats, &gts,
                |f| f.trans_frac > 0.0 || (f.mean_chroma > t1 && f.edge_density < t2));
            if corr > best_acc_3a {
                best_acc_3a = corr;
                best_t1_3a = t1;
                best_t2_3a = t2;
                best_decs_3a = decs;
            }
        }
    }
    println!("  best chroma_t1={:.5} edge_t2={:.5}  acc = {}/{}", best_t1_3a, best_t2_3a, best_acc_3a, n);

    // 3-rule: chroma > t1 AND smoothness < t2  (target 05 mountain's high smoothness)
    println!("--- 3-rule: FRIEND if trans_frac > 0 OR (chroma > t1 AND smoothness < t2) ---");
    let mut best_acc_3b = 0usize;
    let mut best_t1_3b = 0.0f32;
    let mut best_t2_3b = 0.0f32;
    let mut best_decs_3b: Vec<bool> = Vec::new();
    for w1 in chromas.windows(2) {
        let t1 = (w1[0] + w1[1]) / 2.0;
        for w2 in smooths.windows(2) {
            let t2 = (w2[0] + w2[1]) / 2.0;
            let (corr, _, decs) = eval_gate(&feats, &gts,
                |f| f.trans_frac > 0.0 || (f.mean_chroma > t1 && f.smoothness < t2));
            if corr > best_acc_3b {
                best_acc_3b = corr;
                best_t1_3b = t1;
                best_t2_3b = t2;
                best_decs_3b = decs;
            }
        }
    }
    println!("  best chroma_t1={:.5} smooth_t2={:.5}  acc = {}/{}", best_t1_3b, best_t2_3b, best_acc_3b, n);

    // 4-rule: chroma>t1 AND edge>t2 (target 17 aurora low-edge) AND smoothness<t3 (target 05 stochastic)
    println!("--- 4-rule: FRIEND if trans_frac > 0 OR (chroma>t1 AND edge_density>t2 AND smoothness<t3) ---");
    let mut best_acc_4 = 0usize;
    let mut best_t1_4 = 0.0f32;
    let mut best_t2_4 = 0.0f32;
    let mut best_t3_4 = 0.0f32;
    let mut best_decs_4: Vec<bool> = Vec::new();
    for w1 in chromas.windows(2) {
        let t1 = (w1[0] + w1[1]) / 2.0;
        for w2 in eds.windows(2) {
            let t2 = (w2[0] + w2[1]) / 2.0;
            for w3 in smooths.windows(2) {
                let t3 = (w3[0] + w3[1]) / 2.0;
                let (corr, _, decs) = eval_gate(&feats, &gts,
                    |f| f.trans_frac > 0.0 ||
                        (f.mean_chroma > t1 && f.edge_density > t2 && f.smoothness < t3));
                if corr > best_acc_4 {
                    best_acc_4 = corr;
                    best_t1_4 = t1;
                    best_t2_4 = t2;
                    best_t3_4 = t3;
                    best_decs_4 = decs;
                }
            }
        }
    }
    println!("  best chroma_t1={:.5} edge_t2={:.5} smooth_t3={:.5}  acc = {}/{}",
             best_t1_4, best_t2_4, best_t3_4, best_acc_4, n);

    // Choose the overall best gate
    println!();
    let candidates: Vec<(&str, usize, Vec<bool>, String)> = vec![
        ("1-feat chroma alone", best_chroma_acc, eval_gate(&feats, &gts, |f| f.mean_chroma > best_chroma_t).2,
         format!("chroma > {:.5}", best_chroma_t)),
        ("2-feat trans||chroma", best_acc, best_decisions.clone(), format!("trans > 0 OR chroma > {:.5}", best_t)),
        ("3-rule trans|(chroma&edge<)", best_acc_3a, best_decs_3a.clone(), format!("trans > 0 OR (chroma > {:.5} AND edge < {:.5})", best_t1_3a, best_t2_3a)),
        ("3-rule trans|(chroma&smooth<)", best_acc_3b, best_decs_3b.clone(), format!("trans > 0 OR (chroma > {:.5} AND smooth < {:.5})", best_t1_3b, best_t2_3b)),
        ("4-rule trans|(chroma&edge>&smooth<)", best_acc_4, best_decs_4.clone(),
         format!("trans > 0 OR (chroma > {:.5} AND edge > {:.5} AND smooth < {:.5})", best_t1_4, best_t2_4, best_t3_4)),
    ];
    let best_overall = candidates.iter().max_by_key(|c| c.1).unwrap();
    let best_name = best_overall.0;
    let best_acc_final = best_overall.1;
    let best_decs_final = best_overall.2.clone();
    let best_rule = best_overall.3.clone();
    println!("Best overall: {} ({})  acc = {}/{}", best_name, best_rule, best_acc_final, n);
    println!();
    println!(
        "{:<14} {:>+9} {:>10} {:>10} {:>10}",
        "fixture", "ΔSSIM", "actual", "predicted", "verdict"
    );
    for i in 0..n {
        let actual = gts[i].d_ssim >= FRIEND_GATE;
        let pred = best_decs_final[i];
        let verdict = if actual == pred { "OK" } else if pred { "FP" } else { "FN" };
        println!(
            "{:<14} {:>+9.2} {:>10} {:>10} {:>10}",
            gts[i].label, gts[i].d_ssim,
            if actual { "FRIEND" } else { "HOSTILE" },
            if pred { "FRIEND" } else { "HOSTILE" },
            verdict
        );
    }
    Ok(())
}
