//! Cycle 85 — R2 α-expansion graph-cut spike on 04 portrait, n=192
//!
//! Replaces the ICM step (per-pixel greedy, traps in local min) with
//! Boykov-Veksler-Zabih α-expansion (256-label graph cut, 2-approximate).
//! Pairwise smoothness = Potts (λ if labels differ, 0 otherwise).
//! Max-flow = scaled-integer Dinic on a per-α-rebuilt graph.
//!
//! Decision gate on 04 portrait (vs Cycle 71 anneal baseline 86.19):
//!   ΔSSIM ≥ +2.0  → green: R2 paper path
//!   ΔSSIM <  +1.0 → red:   switch to R1 (M-weighted Lloyd)
//!   +1.0..+2.0    → yellow: write essay, decide
//!
//! Pairwise reduction (Kolmogorov-Zabih, no aux nodes needed for Potts):
//!   case A  l_p = l_q = α          : no edges
//!   case B  one of l_p,l_q = α     : unary λ on "stay" cap of the non-α node
//!   case D  l_p = l_q ≠ α          : n-link p↔q cap λ each direction
//!   case E  l_p ≠ l_q, both ≠ α    : unary λ/2 on s→p, λ/2 on s→q, n-link λ/2 each direction
//!
//! Env knobs:
//!   ALPHA_EXP_LAMBDAS = "1e-4,5e-5,2e-5"  Potts λ schedule (one outer iter per value);
//!                                          default matches ICM Cycle 71 anneal.
//!   ALPHA_EXP_NCOL    = 192                palette size

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use image::ImageReader;
use rgb::Rgb;

use nupic_color::{Oklab, srgb_u8_to_oklab, oklab_to_srgb_u8};
use nupic_quantize::{
    apply_palette_rgba, encode_indexed_png_with_alpha, refine_palette_kmeans,
    train_palette_rgba,
};

const CAP_SCALE: f64 = 1.0e7;

#[inline]
fn sqr_dist(a: Oklab, b: Oklab) -> f32 {
    let dl = a.l - b.l;
    let da = a.a - b.a;
    let db = a.b - b.b;
    dl * dl + da * da + db * db
}

#[inline]
fn to_cap(c: f64) -> i64 {
    (c * CAP_SCALE).round() as i64
}

// ---------- Dinic max-flow on integer caps ----------
struct Dinic {
    head: Vec<i32>,
    nxt: Vec<i32>,
    to: Vec<u32>,
    cap: Vec<i64>,
    level: Vec<i32>,
    iter: Vec<i32>,
}

impl Dinic {
    fn with_capacity(n_nodes: usize, est_edges: usize) -> Self {
        Self {
            head: vec![-1; n_nodes],
            nxt: Vec::with_capacity(est_edges * 2),
            to: Vec::with_capacity(est_edges * 2),
            cap: Vec::with_capacity(est_edges * 2),
            level: vec![-1; n_nodes],
            iter: vec![-1; n_nodes],
        }
    }

    fn add_edge(&mut self, u: usize, v: usize, c: i64) {
        let e = self.to.len() as i32;
        self.to.push(v as u32);
        self.cap.push(c);
        self.nxt.push(self.head[u]);
        self.head[u] = e;

        self.to.push(u as u32);
        self.cap.push(0);
        self.nxt.push(self.head[v]);
        self.head[v] = e + 1;
    }

    fn bfs(&mut self, s: usize, t: usize) -> bool {
        for x in self.level.iter_mut() {
            *x = -1;
        }
        self.level[s] = 0;
        let mut q: std::collections::VecDeque<u32> = std::collections::VecDeque::new();
        q.push_back(s as u32);
        while let Some(u) = q.pop_front() {
            let mut e = self.head[u as usize];
            while e >= 0 {
                let ei = e as usize;
                let v = self.to[ei];
                if self.cap[ei] > 0 && self.level[v as usize] < 0 {
                    self.level[v as usize] = self.level[u as usize] + 1;
                    q.push_back(v);
                }
                e = self.nxt[ei];
            }
        }
        self.level[t] >= 0
    }

    fn dfs(&mut self, u: usize, t: usize, f: i64) -> i64 {
        if u == t {
            return f;
        }
        while self.iter[u] >= 0 {
            let ei = self.iter[u] as usize;
            let v = self.to[ei] as usize;
            if self.cap[ei] > 0 && self.level[v] == self.level[u] + 1 {
                let d = self.dfs(v, t, f.min(self.cap[ei]));
                if d > 0 {
                    self.cap[ei] -= d;
                    self.cap[ei ^ 1] += d;
                    return d;
                }
            }
            self.iter[u] = self.nxt[ei];
        }
        0
    }

    fn max_flow(&mut self, s: usize, t: usize) -> i64 {
        let mut total: i64 = 0;
        while self.bfs(s, t) {
            for u in 0..self.head.len() {
                self.iter[u] = self.head[u];
            }
            loop {
                let f = self.dfs(s, t, i64::MAX / 4);
                if f == 0 {
                    break;
                }
                total += f;
            }
        }
        total
    }

    fn reachable_from_source(&self, s: usize) -> Vec<bool> {
        let n = self.head.len();
        let mut reach = vec![false; n];
        reach[s] = true;
        let mut q: std::collections::VecDeque<u32> = std::collections::VecDeque::new();
        q.push_back(s as u32);
        while let Some(u) = q.pop_front() {
            let mut e = self.head[u as usize];
            while e >= 0 {
                let ei = e as usize;
                let v = self.to[ei];
                if self.cap[ei] > 0 && !reach[v as usize] {
                    reach[v as usize] = true;
                    q.push_back(v);
                }
                e = self.nxt[ei];
            }
        }
        reach
    }
}

// ---------- α-expansion: one outer pass over all labels ----------
// Convention:  s-side (x_p=1) ⇒ pixel labeled α
//              t-side (x_p=0) ⇒ pixel keeps l_p
// Unary:   cap(s→p) = D_p(l_p)          (paid if p ∈ T = stays)
//          cap(p→t) = D_p(α)            (paid if p ∈ S = switches)
// Pairwise (Potts, aux-free Kolmogorov-Zabih decomposition):
//   A: l_p = l_q = α          → no edges
//   B: one of l_p,l_q = α     → cap(s → non-α-node) += λ
//   D: l_p = l_q ≠ α          → p↔q n-link cap λ each direction
//   E: l_p ≠ l_q, both ≠ α    → cap(s→p) += λ/2, cap(s→q) += λ/2, p↔q n-link cap λ/2 each direction
fn alpha_expansion_pass(
    src_oklab: &[Oklab],
    pairs: &[(u32, u32)],
    palette: &[Oklab],
    indices: &mut [u8],
    lambda: f64,
) -> usize {
    let k = palette.len();
    let n_pixels = src_oklab.len();
    let lam_cap = to_cap(lambda);
    let lam_half_cap = to_cap(lambda * 0.5);
    let mut changed_total = 0usize;

    let pix_node = |p: usize| -> usize { 2 + p };

    for alpha in 0..k {
        let alpha_u8 = alpha as u8;
        let pal_alpha = palette[alpha];

        // upper bound: 2 unary per pixel + up to 4 directed adds per pair
        let n_nodes = 2 + n_pixels;
        let est_edges = 2 * n_pixels + 4 * pairs.len();
        let mut d = Dinic::with_capacity(n_nodes, est_edges);
        let s: usize = 0;
        let t: usize = 1;

        // unary (base)
        let mut n_count_a = 0usize;
        for p in 0..n_pixels {
            let px = src_oklab[p];
            let lp = indices[p] as usize;
            if lp == alpha {
                n_count_a += 1;
            }
            let d_alpha = sqr_dist(px, pal_alpha) as f64;
            let d_stay = sqr_dist(px, palette[lp]) as f64;
            d.add_edge(s, pix_node(p), to_cap(d_stay));
            d.add_edge(pix_node(p), t, to_cap(d_alpha));
        }

        // pairwise — each case adds extra unary/n-link without aux
        let mut c_a = 0usize;
        let mut c_b = 0usize;
        let mut c_d = 0usize;
        let mut c_e = 0usize;
        for &(p_u, q_u) in pairs.iter() {
            let p = p_u as usize;
            let q = q_u as usize;
            let lp = indices[p];
            let lq = indices[q];
            let a_p = lp == alpha_u8;
            let a_q = lq == alpha_u8;

            if a_p && a_q {
                c_a += 1;
                continue;
            }
            if a_p {
                d.add_edge(s, pix_node(q), lam_cap);
                c_b += 1;
                continue;
            }
            if a_q {
                d.add_edge(s, pix_node(p), lam_cap);
                c_b += 1;
                continue;
            }
            if lp == lq {
                d.add_edge(pix_node(p), pix_node(q), lam_cap);
                d.add_edge(pix_node(q), pix_node(p), lam_cap);
                c_d += 1;
                continue;
            }
            // case E
            d.add_edge(s, pix_node(p), lam_half_cap);
            d.add_edge(s, pix_node(q), lam_half_cap);
            d.add_edge(pix_node(p), pix_node(q), lam_half_cap);
            d.add_edge(pix_node(q), pix_node(p), lam_half_cap);
            c_e += 1;
        }

        let _flow = d.max_flow(s, t);
        let reach = d.reachable_from_source(s);

        let mut changed = 0usize;
        for p in 0..n_pixels {
            if reach[pix_node(p)] && indices[p] != alpha_u8 {
                indices[p] = alpha_u8;
                changed += 1;
            }
        }
        changed_total += changed;

        if alpha < 4 || alpha % 32 == 0 || changed > 5000 {
            println!(
                "  α={:3}  n_α={:>6}  A={:>6} B={:>6} D={:>7} E={:>7}  changed={:>6}",
                alpha, n_count_a, c_a, c_b, c_d, c_e, changed
            );
        }
    }
    changed_total
}

// ---------- palette retrain (mean per cluster) — same as speed_sweep.rs ----------
fn palette_retrain(src_oklab: &[Oklab], palette: &mut [Oklab], indices: &[u8]) {
    let k = palette.len();
    let mut sum_l = vec![0f64; k];
    let mut sum_a = vec![0f64; k];
    let mut sum_b = vec![0f64; k];
    let mut count = vec![0u32; k];
    for (px, &idx) in src_oklab.iter().zip(indices.iter()) {
        let j = idx as usize;
        sum_l[j] += px.l as f64;
        sum_a[j] += px.a as f64;
        sum_b[j] += px.b as f64;
        count[j] += 1;
    }
    for j in 0..k {
        if count[j] > 0 {
            let c = count[j] as f64;
            palette[j] = Oklab {
                l: (sum_l[j] / c) as f32,
                a: (sum_a[j] / c) as f32,
                b: (sum_b[j] / c) as f32,
            };
        }
    }
}

// ---------- ICM step (cycle 71 baseline; in-process for clean head-to-head) ----------
fn icm_step(
    src_oklab: &[Oklab],
    w: usize,
    h: usize,
    palette: &[Oklab],
    indices: &mut [u8],
    lambda_sq: f32,
) {
    let k = palette.len();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let px = src_oklab[i];
            let n_up = if y > 0 { indices[i - w] } else { 255 };
            let n_dn = if y + 1 < h { indices[i + w] } else { 255 };
            let n_lf = if x > 0 { indices[i - 1] } else { 255 };
            let n_rt = if x + 1 < w { indices[i + 1] } else { 255 };
            let mut best_j = indices[i];
            let mut best_cost = f32::INFINITY;
            for j in 0..k {
                let pj = palette[j];
                let dl = px.l - pj.l;
                let da = px.a - pj.a;
                let db = px.b - pj.b;
                let data = dl * dl + da * da + db * db;
                let mut sc = 0u32;
                if n_up != j as u8 && n_up != 255 {
                    sc += 1;
                }
                if n_dn != j as u8 && n_dn != 255 {
                    sc += 1;
                }
                if n_lf != j as u8 && n_lf != 255 {
                    sc += 1;
                }
                if n_rt != j as u8 && n_rt != 255 {
                    sc += 1;
                }
                let cost = data + lambda_sq * (sc as f32);
                if cost < best_cost {
                    best_cost = cost;
                    best_j = j as u8;
                }
            }
            indices[i] = best_j;
        }
    }
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

fn run() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();
    let nupic = root.join("target/release/nupic");
    let tmp = std::env::temp_dir();
    let img_path = root.join("assets/png-bench/inputs/04-photo-portrait.png");

    let n_colors: usize = std::env::var("ALPHA_EXP_NCOL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(192);
    let lambda_sched: Vec<f64> = std::env::var("ALPHA_EXP_LAMBDAS")
        .ok()
        .unwrap_or_else(|| "0.0001,0.00005,0.00002".to_string())
        .split(',')
        .filter_map(|s| s.trim().parse::<f64>().ok())
        .collect();

    println!("Cycle 85 — R2 α-expansion spike on 04 portrait");
    println!("  baseline:  Cycle 71 joint anneal → 86.19 SSIMULACRA2");
    println!(
        "  config:    n_colors={}  λ schedule={:?}",
        n_colors, lambda_sched
    );
    println!("  gate:      ≥+2.0 R2 / +1..+2 essay / <+1 R1\n");

    let img = ImageReader::open(&img_path)?.with_guessed_format()?.decode()?;
    let r = img.to_rgba8();
    let w = r.width();
    let h = r.height();
    let raw_rgba = r.into_raw();
    println!("input: {}×{} = {} px", w, h, (w * h));

    // imagequant init
    let (pi, ai) = train_palette_rgba(&raw_rgba, w, h, n_colors)?;
    let (pal_init, alpha) = refine_palette_kmeans(&raw_rgba, w, h, &pi, &ai, 100);
    let (indices_init, ps_init) = apply_palette_rgba(&raw_rgba, w, h, &pal_init, &alpha);
    let trns = if alpha.iter().all(|&a| a == 255) {
        None
    } else {
        Some(alpha.as_slice())
    };
    let src_oklab: Vec<Oklab> = raw_rgba
        .chunks_exact(4)
        .map(|p| {
            srgb_u8_to_oklab(Rgb {
                r: p[0],
                g: p[1],
                b: p[2],
            })
        })
        .collect();

    let mut oxi = oxipng::Options::from_preset(3);
    oxi.strip = oxipng::StripChunks::Safe;

    // [A] init-only
    let raw_png = encode_indexed_png_with_alpha(w, h, &indices_init, &ps_init, trns)?;
    let out_init = oxipng::optimize_from_memory(&raw_png, &oxi).unwrap();
    let init_path = tmp.join("c85_ae_init.png");
    std::fs::write(&init_path, &out_init)?;
    let ssim_init = ssim_via_nupic(&img_path, &init_path, &nupic);
    println!(
        "[A] imagequant init only:    {} KB   SSIM {:.4}",
        out_init.len() / 1024,
        ssim_init
    );

    // [B] ICM (cycle 71 anneal schedule)
    let lambdas_icm = [0.0001f32, 0.00005, 0.00002];
    let mut pal_icm = pal_init.clone();
    let mut idx_icm = indices_init.clone();
    let t0 = Instant::now();
    for &lam in &lambdas_icm {
        icm_step(&src_oklab, w as usize, h as usize, &pal_icm, &mut idx_icm, lam);
        palette_retrain(&src_oklab, &mut pal_icm, &idx_icm);
    }
    let icm_time = t0.elapsed().as_secs_f64();
    let pal_icm_srgb: Vec<Rgb<u8>> = pal_icm.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
    let raw_png = encode_indexed_png_with_alpha(w, h, &idx_icm, &pal_icm_srgb, trns)?;
    let out_icm = oxipng::optimize_from_memory(&raw_png, &oxi).unwrap();
    let icm_path = tmp.join("c85_ae_icm.png");
    std::fs::write(&icm_path, &out_icm)?;
    let ssim_icm = ssim_via_nupic(&img_path, &icm_path, &nupic);
    println!(
        "[B] ICM (Cycle 71 anneal):   {} KB   SSIM {:.4}   ({:.2}s)",
        out_icm.len() / 1024,
        ssim_icm,
        icm_time
    );

    // [C] α-expansion
    let mut pal_ae = pal_init.clone();
    let mut idx_ae = indices_init.clone();

    // precompute 4-conn neighbor pairs once
    let wu = w as usize;
    let hu = h as usize;
    let mut pairs: Vec<(u32, u32)> = Vec::with_capacity(wu * hu * 2);
    for y in 0..hu {
        for x in 0..wu {
            let p = (y * wu + x) as u32;
            if x + 1 < wu {
                pairs.push((p, p + 1));
            }
            if y + 1 < hu {
                pairs.push((p, p + wu as u32));
            }
        }
    }
    println!("\nα-expansion: {} pairs precomputed", pairs.len());

    let t0 = Instant::now();
    for (outer, &lam) in lambda_sched.iter().enumerate() {
        println!(
            "\nouter {}/{}  (λ={:.2e})",
            outer + 1,
            lambda_sched.len(),
            lam
        );
        let changed = alpha_expansion_pass(&src_oklab, &pairs, &pal_ae, &mut idx_ae, lam);
        println!("  outer {} total relabeled: {}", outer + 1, changed);
        palette_retrain(&src_oklab, &mut pal_ae, &idx_ae);
        if changed == 0 {
            println!("  converged (no relabels), stopping early");
            break;
        }
    }
    let ae_time = t0.elapsed().as_secs_f64();
    let pal_ae_srgb: Vec<Rgb<u8>> = pal_ae.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
    let raw_png = encode_indexed_png_with_alpha(w, h, &idx_ae, &pal_ae_srgb, trns)?;
    let out_ae = oxipng::optimize_from_memory(&raw_png, &oxi).unwrap();
    let ae_path = tmp.join("c85_ae_alpha_exp.png");
    std::fs::write(&ae_path, &out_ae)?;
    let ssim_ae = ssim_via_nupic(&img_path, &ae_path, &nupic);
    println!(
        "\n[C] α-expansion ({}× outer): {} KB   SSIM {:.4}   ({:.1}s)",
        lambda_sched.len(),
        out_ae.len() / 1024,
        ssim_ae,
        ae_time
    );

    let dssim_vs_c71 = ssim_ae - 86.19;
    let dssim_vs_icm = ssim_ae - ssim_icm;
    let dssim_vs_init = ssim_ae - ssim_init;
    println!("\n=== Δ summary ===");
    println!("α-expansion vs Cycle-71 (86.19):  {:+.3}", dssim_vs_c71);
    println!("α-expansion vs ICM here:          {:+.3}", dssim_vs_icm);
    println!("α-expansion vs init:              {:+.3}", dssim_vs_init);
    println!();
    if dssim_vs_c71 >= 2.0 {
        println!(">>> GREEN  (ΔSSIM ≥ +2.0): R2 paper path");
    } else if dssim_vs_c71 < 1.0 {
        println!(">>> RED    (ΔSSIM < +1.0): switch to R1");
    } else {
        println!(">>> YELLOW (+1.0..+2.0): write essay, decide");
    }
    Ok(())
}

fn main() {
    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(run)
        .expect("thread spawn");
    handle.join().expect("thread join").expect("run failed");
}
