//! Cycle 105 — P-03 sharp-mask logo override validation (production-aligned adj_mn)
//!
//! Cycle 103's spike used a vertical-luma-weighted adj_mn formula that
//! disagreed with production's horizontal-mean-rgb formula (3.6 vs 8.20
//! on 03 wiki). P-03 never triggered as a result. This cycle:
//!
//! 1. Replicates production's `compute_adj_lum_diff_stats` exactly
//!    (horizontal adjacent, (R+G+B)/3, 500_000-sample row sub-sample).
//! 2. Reruns 30-fixture cohort and prints per-fixture (opq, adj_mn,
//!    n_pixels, input_file_kb, uniq_opq).
//! 3. Tests three candidate P-03 predicates:
//!      V_input:  opq<0.95 ∧ adj_mn>5 ∧ input_file_kb < 50
//!      V_npix:   opq<0.95 ∧ adj_mn>5 ∧ n_pixels < 100_000
//!      V_uniq:   opq<0.95 ∧ adj_mn>5 ∧ uniq_opq_capped < 500
//!    (V_npix / V_uniq are production-computable; V_input is the memo
//!    plan's reference target.)
//! 4. For each predicate variant + triggering fixtures: encode
//!    K=64 d=0 preset=6, compare size + SSIM vs production and TinyPNG.
//! 5. Decision gate per variant.

use std::path::PathBuf;
use std::process::Command;

use image::ImageReader;

use nupic_quantize::{quantize_indexed_png, QuantizeOpts};

fn ssim(orig: &PathBuf, c: &PathBuf, nupic: &PathBuf) -> f64 {
    let out = Command::new(nupic)
        .args(["compare", "-m", "ssimulacra2"])
        .arg(orig)
        .arg(c)
        .output()
        .expect("nupic");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| {
            l.strip_prefix("SSIMULACRA2: ")
                .and_then(|v| v.split_whitespace().next())
                .and_then(|n| n.parse::<f64>().ok())
        })
        .unwrap_or(f64::NAN)
}

fn run_nupic(orig: &PathBuf, out: &PathBuf, nupic: &PathBuf) -> usize {
    Command::new(nupic)
        .args(["compress", "-o"])
        .arg(out)
        .arg(orig)
        .output()
        .expect("nupic compress");
    std::fs::metadata(out).map(|m| m.len() as usize).unwrap_or(0)
}

/// EXACT copy of production's `compute_adj_lum_diff_stats`
/// (crates/nupic-quantize/src/lib.rs:1434). Horizontal adjacent pixels,
/// luma = (R+G+B)/3 integer div, 500K-sample row sub-sampling.
fn prod_adj_mn(src_rgba: &[u8], width: usize) -> (f64, f64) {
    let n_total = src_rgba.len() / 4;
    let w = width.max(2);
    let h = n_total / w;
    let target = 500_000;
    let target_rows = target / (w - 1).max(1);
    let step = (h / target_rows.max(1)).max(1);
    let mut sum_diff: u64 = 0;
    let mut sum_sq: u64 = 0;
    let mut count: u64 = 0;
    for y in (0..h).step_by(step) {
        for x in 0..w.saturating_sub(1) {
            let i = (y * w + x) * 4;
            if i + 7 >= src_rgba.len() {
                break;
            }
            let l0 = (src_rgba[i] as u32 + src_rgba[i + 1] as u32 + src_rgba[i + 2] as u32) / 3;
            let l1 = (src_rgba[i + 4] as u32 + src_rgba[i + 5] as u32 + src_rgba[i + 6] as u32) / 3;
            let d = (l0 as i32 - l1 as i32).unsigned_abs() as u64;
            sum_diff += d;
            sum_sq += d * d;
            count += 1;
        }
    }
    if count == 0 {
        return (0.0, 0.0);
    }
    let mean = sum_diff as f64 / count as f64;
    let var = (sum_sq as f64 / count as f64) - mean * mean;
    (mean, var)
}

#[derive(Clone, Debug, Default)]
struct Feats {
    n_pixels: usize,
    input_kb: usize,
    opq: f32,
    adj_mn: f64,
    uniq_opq_capped: usize,
}

fn feats(rgba: &[u8], w: usize, input_size: usize) -> Feats {
    let n = rgba.len() / 4;
    let n_opq = rgba.chunks_exact(4).filter(|p| p[3] == 255).count();
    let opq = n_opq as f32 / n as f32;
    let mut uniq = std::collections::HashSet::with_capacity(6_000);
    let step_u = if n > 1_000_000 { 4 } else { 1 };
    for p in rgba.chunks_exact(4).step_by(step_u) {
        if p[3] != 255 {
            continue;
        }
        let key = (p[0] as u32) | ((p[1] as u32) << 8) | ((p[2] as u32) << 16);
        uniq.insert(key);
        if uniq.len() >= 5_000 {
            break;
        }
    }
    let (adj_mn, _) = prod_adj_mn(rgba, w);
    Feats {
        n_pixels: n,
        input_kb: input_size / 1024,
        opq,
        adj_mn,
        uniq_opq_capped: uniq.len(),
    }
}

fn encode_override(rgba: &[u8], w: u32, h: u32, k: usize, d: f32, p: u8) -> Vec<u8> {
    let mut opts = QuantizeOpts::default();
    opts.n_colors = k;
    opts.dither_strength = d;
    opts.oxipng_preset = p;
    opts.strip_metadata = true;
    quantize_indexed_png(rgba, w, h, opts).expect("quantize")
}

#[derive(Copy, Clone)]
enum Variant {
    VInput,
    VNpix,
    VUniq,
}

fn trigger(v: Variant, f: &Feats) -> bool {
    if f.opq >= 0.95 || f.adj_mn <= 5.0 {
        return false;
    }
    match v {
        Variant::VInput => f.input_kb < 50,
        Variant::VNpix => f.n_pixels < 100_000,
        Variant::VUniq => f.uniq_opq_capped < 500,
    }
}

fn vname(v: Variant) -> &'static str {
    match v {
        Variant::VInput => "V_input(file_kb<50)",
        Variant::VNpix => "V_npix(n_pix<100K)",
        Variant::VUniq => "V_uniq(uniq<500)",
    }
}

fn mark(b: bool) -> &'static str {
    if b {
        "✓"
    } else {
        "✗"
    }
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();
    let nupic = root.join("target/release/nupic");

    let fixtures: Vec<(&str, &str, Option<&str>)> = vec![
        ("inputs/01-png-transparency-demo.png", "01_trans", Some("tinypng-web/01-png-transparency-demo.png")),
        ("inputs/02-pluto-transparent.png", "02_pluto", Some("tinypng-web/02-pluto-transparent.png")),
        ("inputs/03-wikipedia-logo.png", "03_wiki", Some("tinypng-web/03-wikipedia-logo.png")),
        ("inputs/04-photo-portrait.png", "04_portr", Some("tinypng-web/04-photo-portrait.png")),
        ("inputs/05-photo-mountain.png", "05_mtn", Some("tinypng-web/05-photo-mountain.png")),
        ("inputs/06-photo-landscape.png", "06_land", Some("tinypng-web/06-photo-landscape.png")),
        ("inputs/07-photo-product.png", "07_prod", Some("tinypng-web/07-photo-product.png")),
        ("inputs-ext-real/17-aurora-5mp.png", "17_aur", None),
        ("inputs-ext-real/25-sofia-cathedral-5mp.png", "25_sof", None),
        ("inputs-ext-real/27-whale-tail-5mp.png", "27_whl", None),
        ("corpus-500/mi0.png", "mi0", None),
        ("corpus-500/n29_astronaut.png", "n29_astro", None),
        ("corpus-500/p11_480x320.png", "p11", None),
        ("corpus-500/p32_480x320.png", "p32", None),
        ("corpus-500/p409_sm_300x320.png", "p409", None),
        ("corpus-500/p426_sm_460x380.png", "p426", None),
        ("corpus-500/p449_sm_300x320.png", "p449", None),
        ("corpus-500/p66_1024x768.png", "p66", None),
        ("corpus-500/p7_480x320.png", "p7", None),
        ("corpus-500/s042_stripes_p8.png", "s042", None),
        ("corpus-500/n01_mars.png", "n01_mars", None),
        ("corpus-500/n31_rover.png", "n31_rover", None),
        ("corpus-500/p119_1024x768.png", "p119", None),
        ("corpus-500/p38_480x320.png", "p38", None),
        ("corpus-500/p430_sm_380x380.png", "p430", None),
        ("corpus-500/p56_480x320.png", "p56", None),
        ("corpus-500/p84_1024x768.png", "p84", None),
        ("corpus-500/s006_gradient_1306x1113.png", "s006", None),
        ("corpus-500/s040_stripes_p2.png", "s040", None),
        ("corpus-500/s059_solid.png", "s059", None),
    ];

    let tmp = std::env::temp_dir();

    // Phase 1: features per fixture
    println!("=== Cycle 105 — P-03 validation (production-aligned adj_mn) ===");
    println!();
    println!(
        "{:<12} {:>6} {:>6} {:>5} {:>7} {:>7} {:>5}",
        "fixture", "in_KB", "n_pix", "opq", "adj_mn", "uniq", "tier"
    );
    let mut records: Vec<(String, Option<&str>, Feats, Vec<u8>, u32, u32)> = Vec::new();
    for (rel, lbl, tiny_rel) in &fixtures {
        let path = root.join("assets/png-bench").join(rel);
        if !path.exists() {
            println!("MISSING {}", lbl);
            continue;
        }
        let input_size = std::fs::metadata(&path).map(|m| m.len() as usize).unwrap_or(0);
        let img = match ImageReader::open(&path).and_then(|r| r.with_guessed_format()) {
            Ok(r) => match r.decode() {
                Ok(i) => i,
                Err(_) => {
                    println!("decode fail: {}", lbl);
                    continue;
                }
            },
            Err(_) => {
                println!("open fail: {}", lbl);
                continue;
            }
        };
        let r = img.to_rgba8();
        let (w, h) = (r.width(), r.height());
        let raw = r.into_raw();
        let f = feats(&raw, w as usize, input_size);
        let tier = if f.opq < 0.95 {
            if f.adj_mn > 5.0 {
                "sharp"
            } else {
                "trans"
            }
        } else {
            "opq"
        };
        println!(
            "{:<12} {:>6} {:>6} {:>5.2} {:>7.2} {:>7} {:>5}",
            lbl, f.input_kb, f.n_pixels, f.opq, f.adj_mn, f.uniq_opq_capped, tier
        );
        records.push((lbl.to_string(), *tiny_rel, f, raw, w, h));
    }

    // Phase 2: for each predicate variant, list triggers, encode K=64 d=0 p=6, verdict.
    for &v in &[Variant::VInput, Variant::VNpix, Variant::VUniq] {
        println!();
        println!("--- Variant: {} ---", vname(v));
        let triggers: Vec<_> = records.iter().filter(|(_, _, f, _, _, _)| trigger(v, f)).collect();
        if triggers.is_empty() {
            println!("  (no fixtures trigger)");
            continue;
        }
        println!(
            "  {:<12} {:>5} {:>9} {:>9} {:>9} {:>8} {:>8} {:>8} {:>8} {:>6} {:>6}",
            "fixture", "input", "tiny_B", "cap_B", "prod_B", "ovrd_B", "Δvprod", "Δvcap", "pSSIM", "oSSIM", "tSSIM"
        );
        let mut all_pass = true;
        let mut any_loss = false;
        for (lbl, tiny_rel, _, raw, w, h) in &triggers {
            let input_path = root.join("assets/png-bench").join({
                // find the matching fixture path from fixtures list by lbl
                let mut p = String::new();
                for (rel, l, _) in &fixtures {
                    if l == lbl {
                        p = rel.to_string();
                        break;
                    }
                }
                p
            });
            let prod_path = tmp.join(format!("c105_{}_prod.png", lbl));
            let prod_size = run_nupic(&input_path, &prod_path, &nupic);
            let prod_ssim = ssim(&input_path, &prod_path, &nupic);

            let ovrd_bytes = encode_override(raw, *w, *h, 64, 0.0, 6);
            let ovrd_path = tmp.join(format!("c105_{}_p03.png", lbl));
            std::fs::write(&ovrd_path, &ovrd_bytes)?;
            let ovrd_ssim = ssim(&input_path, &ovrd_path, &nupic);

            let (tiny_b, tiny_ssim, cap) = if let Some(t) = tiny_rel {
                let tp = root.join("assets/png-bench").join(t);
                if tp.exists() {
                    let tb = std::fs::metadata(&tp).map(|m| m.len() as i64).unwrap_or(0);
                    (tb, ssim(&input_path, &tp, &nupic), tb * 80 / 100)
                } else {
                    (0i64, f64::NAN, 0i64)
                }
            } else {
                (0i64, f64::NAN, 0i64)
            };
            let dv_prod = ovrd_bytes.len() as i64 - prod_size as i64;
            let dv_cap = ovrd_bytes.len() as i64 - cap;
            let size_pass = cap == 0 || ovrd_bytes.len() as i64 <= cap;
            let ssim_pass = if tiny_ssim.is_nan() {
                ovrd_ssim >= prod_ssim - 0.5
            } else {
                ovrd_ssim >= tiny_ssim
            };
            let smaller_than_prod = ovrd_bytes.len() < prod_size;
            let win = size_pass && ssim_pass && smaller_than_prod;
            if !win {
                all_pass = false;
            }
            if !smaller_than_prod || !ssim_pass {
                any_loss = true;
            }
            println!(
                "  {:<12} {:>5} {:>9} {:>9} {:>9} {:>8} {:>+8} {:>+8} {:>8.2} {:>6.2} {:>6.2} {} {} {}",
                lbl,
                vname(v).split_once('(').unwrap_or((vname(v), "")).0,
                tiny_b,
                cap,
                prod_size,
                ovrd_bytes.len(),
                dv_prod,
                dv_cap,
                prod_ssim,
                ovrd_ssim,
                tiny_ssim,
                mark(size_pass),
                mark(ssim_pass),
                mark(smaller_than_prod)
            );
        }
        println!(
            "  Verdict: {} ({} fixtures triggered, {})",
            if all_pass {
                "✅ GREEN"
            } else if any_loss {
                "❌ RED"
            } else {
                "⚠️ YELLOW"
            },
            triggers.len(),
            if all_pass {
                "all pass cap+SSIM+smaller-than-prod"
            } else {
                "see per-fixture marks"
            }
        );
    }

    Ok(())
}
