//! Cycle 97 — R4 K/dither 3-class routing classifier (paper §5 framework)
//!
//! Cycle 96 R4 R-D grid YELLOW: median −0.3% / mean −3.0% iso-SSIM size, with
//! per-fixture wins distributed bimodally (02 −8.8%, 03 −11.0%, 04 +0.02 SSIM
//! at −0.3%, 05/06/01 default-on-front). Essay flagged "easier routing problem
//! than R1 because (a) classes visually obvious from features, (b) misrouting
//! cost bounded."
//!
//! This cycle spikes the 3-class hand router proposed at the end of 04aaa:
//!   - UI/logo class:       chroma_entropy < 3.0 AND edge_density > 0.2
//!                          → route (K=128, d=0)
//!   - Chroma-rich class:   trans_frac > 0 OR (mean_chroma > 0.04 AND
//!                          smoothness < 0.05)
//!                          → route (K=256, d=0.5)
//!   - Stochastic-noise:    all others
//!                          → route (K=256, d=0)  (production default)
//!
//! Validation cohort: 10 fixtures = baseline-7 + 5MP {17 aurora, 25 sofia,
//! 27 whale}. For each fixture we encode the relevant (K, d) grid at the
//! tier-appropriate oxipng preset (3 for baseline-7, 0 for 5MP per Cycle 79),
//! compute the per-fixture iso-SSIM Pareto-best (oracle), the router-picked
//! config, and the production default.
//!
//! Decision gate (per essay 04aaa § "Cycle 97 next-up"):
//!   route-picked config achieves ≥ 80% of full-grid Pareto-optimal iso-SSIM
//!   size win on ≥ 6/10 fixtures → GREEN, candidate for `Quality::Auto-R4`.
//!
//! Iso-SSIM band = SSIM ≥ default_SSIM − 0.5. Win capture =
//!   (default_size − router_size) / max(default_size − oracle_size, 1)
//! when oracle_size < default_size; when default is on the Pareto front the
//! fixture passes iff router agrees with default (no win to capture).

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use image::ImageReader;
use rgb::Rgb;

use nupic_color::{srgb_u8_to_oklab, Oklab};
use nupic_quantize::{quantize_indexed_png, QuantizeOpts};

const ISO_BAND: f64 = 0.5;
const WIN_CAPTURE_GATE: f64 = 0.80;
const PASS_FRACTION_GATE: f64 = 0.60; // 6/10 fixtures

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
        .arg(orig)
        .arg(cmp_path)
        .output()
        .expect("nupic compare");
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines()
        .find_map(|l| {
            l.strip_prefix("SSIMULACRA2: ")
                .and_then(|v| v.split_whitespace().next())
                .and_then(|n| n.parse::<f64>().ok())
        })
        .unwrap_or(f64::NAN)
}

fn encode_one(raw: &[u8], w: u32, h: u32, cfg: Cfg) -> Vec<u8> {
    let mut opts = QuantizeOpts::default();
    opts.n_colors = cfg.k;
    opts.dither_strength = cfg.d;
    opts.oxipng_preset = cfg.p;
    opts.strip_metadata = true;
    quantize_indexed_png(raw, w, h, opts).expect("encode")
}

fn run_grid(
    fixture_path: &PathBuf,
    nupic: &PathBuf,
    label: &str,
    grid: &[Cfg],
) -> Vec<Encoded> {
    let img = ImageReader::open(fixture_path)
        .expect("open")
        .with_guessed_format()
        .expect("fmt")
        .decode()
        .expect("decode");
    let r = img.to_rgba8();
    let w = r.width();
    let h = r.height();
    let raw = r.into_raw();
    let tmp = std::env::temp_dir();
    let mut out = Vec::with_capacity(grid.len());
    for &cfg in grid {
        let bytes = encode_one(&raw, w, h, cfg);
        let path = tmp.join(format!(
            "c97_{}_k{}_d{:.1}_p{}.png",
            label, cfg.k, cfg.d, cfg.p
        ));
        std::fs::write(&path, &bytes).expect("write");
        let ssim = ssim_via_nupic(fixture_path, &path, nupic);
        out.push(Encoded {
            cfg,
            size: bytes.len(),
            ssim,
        });
    }
    out
}

// ===== feature extraction (compatible with Cycle 93) =====

#[derive(Clone, Debug, Default)]
struct Features {
    mean_chroma: f32,
    smoothness: f32,
    edge_density: f32,
    trans_frac: f32,
    chroma_entropy: f32,
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
    let oklab: Vec<Oklab> = raw_rgba
        .chunks_exact(4)
        .map(|p| {
            if p[3] < 255 {
                alpha_count_lt += 1;
            }
            srgb_u8_to_oklab(Rgb {
                r: p[0],
                g: p[1],
                b: p[2],
            })
        })
        .collect();
    let trans_frac = alpha_count_lt as f32 / n as f32;

    let sum_chroma: f64 = oklab
        .iter()
        .map(|o| (o.a * o.a + o.b * o.b).sqrt() as f64)
        .sum();
    let mean_chroma = (sum_chroma / n as f64) as f32;

    let mut sum_h = 0.0f64;
    let mut sum_v = 0.0f64;
    let mut count_h = 0usize;
    let mut count_v = 0usize;
    for y in 0..h {
        for x in 0..w - 1 {
            let i = y * w + x;
            sum_h += (oklab[i].l - oklab[i + 1].l).abs() as f64;
            count_h += 1;
        }
    }
    for y in 0..h - 1 {
        for x in 0..w {
            let i = y * w + x;
            sum_v += (oklab[i].l - oklab[i + w].l).abs() as f64;
            count_v += 1;
        }
    }
    let smoothness =
        ((sum_h / count_h.max(1) as f64) + (sum_v / count_v.max(1) as f64)) as f32;

    let mut edge_count = 0usize;
    let mut edge_total = 0usize;
    if w >= 3 && h >= 3 {
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let i = y * w + x;
                let gx = oklab[i + 1].l - oklab[i - 1].l;
                let gy = oklab[i + w].l - oklab[i - w].l;
                let mag = (gx * gx + gy * gy).sqrt();
                if mag > 0.05 {
                    edge_count += 1;
                }
                edge_total += 1;
            }
        }
    }
    let edge_density = edge_count as f32 / edge_total.max(1) as f32;

    let l: Vec<f32> = oklab.iter().map(|o| o.l).collect();
    let _ = gauss5(&l, w, h); // kept for parity with C93 (not used downstream)

    let bins = 16usize;
    let mut hist = vec![0u32; bins * bins];
    let mut a_min = f32::INFINITY;
    let mut a_max = f32::NEG_INFINITY;
    let mut b_min = f32::INFINITY;
    let mut b_max = f32::NEG_INFINITY;
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
    let chroma_entropy = entropy as f32;

    Features {
        mean_chroma,
        smoothness,
        edge_density,
        trans_frac,
        chroma_entropy,
    }
}

// ===== 3-class router =====

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Class {
    Ui,
    ChromaRich,
    Stochastic,
}

impl Class {
    fn target_kd(self) -> (usize, f32) {
        match self {
            Class::Ui => (128, 0.0),
            Class::ChromaRich => (256, 0.5),
            Class::Stochastic => (256, 0.0),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Class::Ui => "UI",
            Class::ChromaRich => "Chroma",
            Class::Stochastic => "Stoch",
        }
    }
}

// Rule A: the literal essay candidate.
fn rule_a(f: &Features) -> Class {
    if f.chroma_entropy < 3.0 && f.edge_density > 0.2 {
        Class::Ui
    } else if f.trans_frac > 0.0 || (f.mean_chroma > 0.04 && f.smoothness < 0.05) {
        Class::ChromaRich
    } else {
        Class::Stochastic
    }
}

// ===== eval helpers =====

#[derive(Clone, Debug)]
struct PerFixture {
    label: String,
    default: Encoded,
    oracle_iso: Encoded,
    grid: Vec<Encoded>,
    features: Features,
    class_router: Class,
    router_pick: Encoded,
    win_capture: Option<f64>, // None when no win is available
}

fn pick_default<'a>(grid: &'a [Encoded], preset: u8) -> &'a Encoded {
    let want = Cfg { k: 256, d: 0.0, p: preset };
    grid.iter()
        .find(|e| cfg_eq(e.cfg, want))
        .expect("default config in grid")
}

fn pick_oracle_iso<'a>(grid: &'a [Encoded], default: &Encoded) -> Encoded {
    let band_lo = default.ssim - ISO_BAND;
    let mut best = default.clone();
    for e in grid {
        if e.ssim >= band_lo && e.size < best.size {
            best = e.clone();
        }
    }
    best
}

fn pick_router_in_grid<'a>(grid: &'a [Encoded], target_k: usize, target_d: f32, preset: u8) -> Encoded {
    let want = Cfg { k: target_k, d: target_d, p: preset };
    grid.iter()
        .find(|e| cfg_eq(e.cfg, want))
        .cloned()
        .expect("router target must be in grid")
}

fn win_capture(default_size: usize, oracle_size: usize, router_size: usize, router_ssim: f64, default_ssim: f64) -> Option<f64> {
    // If router is out of iso band → no capture; effectively defaults to default size.
    let in_band = router_ssim >= default_ssim - ISO_BAND;
    let eff_router = if in_band { router_size } else { default_size };
    let available = default_size as i64 - oracle_size as i64;
    if available <= 0 {
        // No win available — router passes iff it picks something at-least-as-good as default
        if eff_router <= default_size { Some(1.0) } else { Some(0.0) }
    } else {
        let captured = default_size as i64 - eff_router as i64;
        Some(captured as f64 / available as f64)
    }
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();
    let nupic = root.join("target/release/nupic");

    // baseline-7 grid: 9 configs at preset=3 (Cycle 96 confirmed preset=3 dominates baseline-7)
    let preset_b7: u8 = 3;
    let baseline7_grid: Vec<Cfg> = {
        let mut v = Vec::new();
        for &k in &[128usize, 192, 256] {
            for &d in &[0.0f32, 0.3, 0.5] {
                v.push(Cfg { k, d, p: preset_b7 });
            }
        }
        v
    };

    // 5MP grid: 6 configs at preset=0 (Cycle 79 3-tier rule)
    let preset_5mp: u8 = 0;
    let mp5_grid: Vec<Cfg> = {
        let mut v = Vec::new();
        for &k in &[128usize, 192, 256] {
            for &d in &[0.0f32, 0.5] {
                v.push(Cfg { k, d, p: preset_5mp });
            }
        }
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

    println!("Cycle 97 — R4 K/dither 3-class routing classifier");
    println!("  baseline-7 grid: K×d ∈ 3×3 at preset=3 ({} configs)", baseline7_grid.len());
    println!("  5MP grid:        K×d ∈ 3×2 at preset=0 ({} configs)", mp5_grid.len());
    println!("  total encodes: {}", baseline7_grid.len() * 7 + mp5_grid.len() * 3);
    println!("  router classes → (K, d): UI→(128,0)  Chroma→(256,0.5)  Stoch→(256,0)");
    println!();

    let mut per_fixture: Vec<PerFixture> = Vec::new();
    let t_total = Instant::now();

    for (rel, lbl, grid_cfg, preset) in &fixtures {
        let path = root.join("assets/png-bench").join(rel);
        if !path.exists() {
            println!("MISSING {}: {}", lbl, path.display());
            continue;
        }

        // features
        let img = ImageReader::open(&path)?.with_guessed_format()?.decode()?;
        let r = img.to_rgba8();
        let w = r.width() as usize;
        let h = r.height() as usize;
        let raw_for_feat = r.clone().into_raw();
        let features = compute_features(&raw_for_feat, w, h);
        drop(raw_for_feat);

        // grid encode
        let t0 = Instant::now();
        let grid = run_grid(&path, &nupic, lbl, grid_cfg);
        let t = t0.elapsed().as_secs_f64();

        let default = pick_default(&grid, *preset).clone();
        let oracle = pick_oracle_iso(&grid, &default);
        let class = rule_a(&features);
        let (k_t, d_t) = class.target_kd();
        let router_pick = pick_router_in_grid(&grid, k_t, d_t, *preset);
        let wc = win_capture(default.size, oracle.size, router_pick.size, router_pick.ssim, default.ssim);

        println!(
            "[{:<14}] {:>4.1}s | def {} B / SSIM {:>6.2} | oracle K{} d{:.1} {} B SSIM {:>6.2} ({:+.2}%) | router={:>6} K{} d{:.1} {} B SSIM {:>6.2} ({:+.2}%)  wc={}",
            lbl, t,
            default.size, default.ssim,
            oracle.cfg.k, oracle.cfg.d, oracle.size, oracle.ssim,
            (oracle.size as f64 / default.size as f64 - 1.0) * 100.0,
            class.name(), router_pick.cfg.k, router_pick.cfg.d, router_pick.size, router_pick.ssim,
            (router_pick.size as f64 / default.size as f64 - 1.0) * 100.0,
            wc.map(|x| format!("{:.0}%", x * 100.0)).unwrap_or_else(|| "-".into()),
        );

        per_fixture.push(PerFixture {
            label: lbl.to_string(),
            default,
            oracle_iso: oracle,
            grid,
            features,
            class_router: class,
            router_pick,
            win_capture: wc,
        });
    }

    println!();
    println!("Grid + SSIM total: {:.1}s", t_total.elapsed().as_secs_f64());
    println!();

    // ===== feature dump =====
    println!("=== Per-fixture features + router class ===");
    println!("{:<14} {:>7} {:>8} {:>7} {:>7} {:>8}  {:>6}",
             "fixture", "chroma", "smooth", "edge", "trans", "entropy", "class");
    for p in &per_fixture {
        println!("{:<14} {:>7.3} {:>8.4} {:>7.3} {:>7.3} {:>8.3}  {:>6}",
                 p.label,
                 p.features.mean_chroma,
                 p.features.smoothness,
                 p.features.edge_density,
                 p.features.trans_frac,
                 p.features.chroma_entropy,
                 p.class_router.name());
    }
    println!();

    // ===== win-capture summary =====
    println!("=== Win-capture vs oracle ===");
    println!("{:<14} {:>10} {:>10} {:>10} {:>10}",
             "fixture", "avail Δ%", "router Δ%", "wc", "pass?");
    let mut pass_count = 0usize;
    let mut sum_wc = 0.0f64;
    let mut n_wc = 0usize;
    let mut sum_router_delta = 0.0f64;
    let mut sum_oracle_delta = 0.0f64;

    for p in &per_fixture {
        let oracle_delta = (p.oracle_iso.size as f64 / p.default.size as f64 - 1.0) * 100.0;
        let router_in_band = p.router_pick.ssim >= p.default.ssim - ISO_BAND;
        let eff_router_size = if router_in_band { p.router_pick.size } else { p.default.size };
        let router_delta = (eff_router_size as f64 / p.default.size as f64 - 1.0) * 100.0;

        sum_router_delta += router_delta;
        sum_oracle_delta += oracle_delta;

        let pass = match p.win_capture {
            Some(wc) => wc >= WIN_CAPTURE_GATE,
            None => false,
        };
        if pass { pass_count += 1; }
        if let Some(wc) = p.win_capture {
            sum_wc += wc;
            n_wc += 1;
        }

        println!("{:<14} {:>+10.2} {:>+10.2} {:>9} {:>10}",
                 p.label,
                 oracle_delta,
                 router_delta,
                 p.win_capture.map(|w| format!("{:.0}%", w * 100.0)).unwrap_or_else(|| "-".into()),
                 if pass { "PASS" } else { "fail" });
    }

    let n = per_fixture.len();
    let pass_frac = pass_count as f64 / n as f64;
    let mean_wc = if n_wc > 0 { sum_wc / n_wc as f64 } else { 0.0 };
    let mean_oracle = sum_oracle_delta / n as f64;
    let mean_router = sum_router_delta / n as f64;

    println!();
    println!("Aggregate:");
    println!("  fixtures passing (wc ≥ {:.0}%):  {}/{}  ({:.0}%)",
             WIN_CAPTURE_GATE * 100.0, pass_count, n, pass_frac * 100.0);
    println!("  mean win-capture:               {:.0}%", mean_wc * 100.0);
    println!("  mean oracle Δsize:              {:+.2}% (theoretical ceiling)", mean_oracle);
    println!("  mean router Δsize:              {:+.2}% (router actually gets)", mean_router);
    println!("  router/oracle ratio:            {:.0}%",
             if mean_oracle.abs() > 1e-6 { mean_router / mean_oracle * 100.0 } else { 0.0 });
    println!();

    if pass_frac >= PASS_FRACTION_GATE && mean_wc >= WIN_CAPTURE_GATE {
        println!(">>> GREEN — 3-class router clears {:.0}% pass + {:.0}% mean-wc gate. Ship candidate for `Quality::Auto-R4`.",
                 PASS_FRACTION_GATE * 100.0, WIN_CAPTURE_GATE * 100.0);
    } else if pass_frac >= PASS_FRACTION_GATE {
        println!(">>> YELLOW-frac — ≥{:.0}% pass but mean-wc {:.0}% < gate. Need router refinement (4-class? threshold tune?).",
                 PASS_FRACTION_GATE * 100.0, mean_wc * 100.0);
    } else if mean_wc >= 0.5 {
        println!(">>> YELLOW-wc — mean-wc {:.0}% promising but fixture pass-rate {:.0}% < gate. Likely class boundaries misaligned.",
                 mean_wc * 100.0, pass_frac * 100.0);
    } else {
        println!(">>> RED — router fails both gates. Hand rule does not capture R-D structure.");
    }

    // ===== quick per-class confusion analysis =====
    println!();
    println!("=== Class confusion (router vs oracle-implied class) ===");
    println!("oracle-implied class = which class's target (K,d) most closely matches oracle_iso.cfg");
    println!("{:<14} {:>8} {:>8} {:>6}", "fixture", "router", "implied", "match");
    let mut confusion = std::collections::HashMap::<(Class, Class), usize>::new();
    for p in &per_fixture {
        let oracle_kd = (p.oracle_iso.cfg.k, p.oracle_iso.cfg.d);
        let implied = if oracle_kd.1 >= 0.4 {
            Class::ChromaRich
        } else if oracle_kd.0 <= 160 {
            Class::Ui
        } else {
            Class::Stochastic
        };
        let m = if p.class_router == implied { "✓" } else { "✗" };
        println!("{:<14} {:>8} {:>8} {:>6}",
                 p.label, p.class_router.name(), implied.name(), m);
        *confusion.entry((p.class_router, implied)).or_insert(0) += 1;
    }

    Ok(())
}
