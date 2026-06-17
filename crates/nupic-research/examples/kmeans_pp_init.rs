//! Cycle 88 — R8 k-means++ init replacement spike (perf engineering)
//!
//! Current `train_palette_rgba` uses imagequant median-cut for init,
//! then `refine_palette_kmeans` (subsampled Lloyd) for refinement.
//! k-means++ picks better-spread initial centroids — hypothesis is that
//! refine converges in fewer iters, recovering wall time.
//!
//! Spike: [A] imagequant init / [B] k-means++ init, both fed into the
//! same `refine_palette_kmeans(100 iters)`. Apples-to-apples refine
//! timing. Bench on baseline-7-mid (04/05/06/07) + 3 × 5MP+ from
//! inputs-ext-real (17 aurora, 25 sofia, 27 whale).
//!
//! Gate per roadmap R8: 5MP perf −15–20 ms, SSIM 持平/微升 → ship.

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use image::ImageReader;
use rgb::Rgb;

use nupic_color::{Oklab, oklab_to_srgb_u8, srgb_u8_to_oklab};
use nupic_quantize::{
    apply_palette_rgba, classify_for_palette_size_with_importance,
    encode_indexed_png_with_alpha, refine_palette_kmeans, refine_palette_kmeans_importance,
    train_palette_rgba,
};

// ---------- Deterministic LCG RNG ----------
struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_add(0xdeadbeef))
    }
    fn next_f32(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as u32) as f32 / (u32::MAX as f32)
    }
}

// ---------- k-means++ init on subsample ----------
fn kmeans_pp_init_oklab(src_oklab: &[Oklab], k: usize, seed: u64) -> Vec<Oklab> {
    let n = src_oklab.len();
    let sample_size = 20_000.min(n);
    let stride = (n / sample_size).max(1);
    let samples: Vec<Oklab> = (0..sample_size)
        .map(|i| src_oklab[i * stride])
        .collect();

    let mut rng = Lcg::new(seed);
    let mut centroids: Vec<Oklab> = Vec::with_capacity(k);
    let first_idx = (rng.next_f32() * sample_size as f32) as usize % sample_size;
    centroids.push(samples[first_idx]);

    let mut min_dists: Vec<f32> = samples
        .iter()
        .map(|p| {
            let c = centroids[0];
            let dl = p.l - c.l;
            let da = p.a - c.a;
            let db = p.b - c.b;
            dl * dl + da * da + db * db
        })
        .collect();

    for _ in 1..k {
        // pick with prob ∝ min_dist²; cumulative scan
        let total: f64 = min_dists.iter().map(|&v| v as f64).sum();
        if total <= 0.0 {
            centroids.push(samples[first_idx]);
            continue;
        }
        let pick = rng.next_f32() as f64 * total;
        let mut cumul = 0.0f64;
        let mut chosen = sample_size - 1;
        for (i, &d) in min_dists.iter().enumerate() {
            cumul += d as f64;
            if cumul >= pick {
                chosen = i;
                break;
            }
        }
        let new_c = samples[chosen];
        centroids.push(new_c);
        for (i, p) in samples.iter().enumerate() {
            let dl = p.l - new_c.l;
            let da = p.a - new_c.a;
            let db = p.b - new_c.b;
            let d = dl * dl + da * da + db * db;
            if d < min_dists[i] {
                min_dists[i] = d;
            }
        }
    }
    centroids
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

fn run_fixture(
    fixture_path: &PathBuf,
    nupic: &PathBuf,
    label: &str,
) -> anyhow::Result<()> {
    let img = ImageReader::open(fixture_path)?
        .with_guessed_format()?
        .decode()?;
    let r = img.to_rgba8();
    let w = r.width();
    let h = r.height();
    let n_pixels = (w as usize) * (h as usize);
    let raw_rgba = r.into_raw();
    let (n_colors, alpha_imp) =
        classify_for_palette_size_with_importance(&raw_rgba, w as usize);

    let tmp = std::env::temp_dir();
    let mut oxi = oxipng::Options::from_preset(3);
    oxi.strip = oxipng::StripChunks::Safe;

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

    // ---------- [A] imagequant init + refine ----------
    let t_a_init0 = Instant::now();
    let (pi_a, ai_a) = train_palette_rgba(&raw_rgba, w, h, n_colors)?;
    let init_time_a = t_a_init0.elapsed().as_secs_f64();
    let t_a_refine0 = Instant::now();
    let (pal_a, alpha_a) = if alpha_imp > 0.0 {
        refine_palette_kmeans_importance(&raw_rgba, w, h, &pi_a, &ai_a, 100, alpha_imp)
    } else {
        refine_palette_kmeans(&raw_rgba, w, h, &pi_a, &ai_a, 100)
    };
    let refine_time_a = t_a_refine0.elapsed().as_secs_f64();
    let (indices_a, ps_a) = apply_palette_rgba(&raw_rgba, w, h, &pal_a, &alpha_a);
    let trns_a = if alpha_a.iter().all(|&a| a == 255) {
        None
    } else {
        Some(alpha_a.as_slice())
    };
    let raw_png_a = encode_indexed_png_with_alpha(w, h, &indices_a, &ps_a, trns_a)?;
    let out_a = oxipng::optimize_from_memory(&raw_png_a, &oxi).unwrap();
    let path_a = tmp.join(format!("c88_{}_iq.png", label.replace(' ', "_")));
    std::fs::write(&path_a, &out_a)?;
    let ssim_a = ssim_via_nupic(fixture_path, &path_a, &nupic);

    // ---------- [B] k-means++ init + refine ----------
    let t_b_init0 = Instant::now();
    let pi_b = kmeans_pp_init_oklab(&src_oklab, n_colors, label.len() as u64 * 31 + 7);
    // Reuse imagequant's alpha array for fair refine path; alpha is independent
    // of palette colour init (Cycle 88 spike's perf delta is in the colour init only).
    let ai_b = ai_a.clone();
    let init_time_b = t_b_init0.elapsed().as_secs_f64();
    let t_b_refine0 = Instant::now();
    let (pal_b, alpha_b) = if alpha_imp > 0.0 {
        refine_palette_kmeans_importance(&raw_rgba, w, h, &pi_b, &ai_b, 100, alpha_imp)
    } else {
        refine_palette_kmeans(&raw_rgba, w, h, &pi_b, &ai_b, 100)
    };
    let refine_time_b = t_b_refine0.elapsed().as_secs_f64();
    let (indices_b, ps_b) = apply_palette_rgba(&raw_rgba, w, h, &pal_b, &alpha_b);
    let trns_b = if alpha_b.iter().all(|&a| a == 255) {
        None
    } else {
        Some(alpha_b.as_slice())
    };
    let raw_png_b = encode_indexed_png_with_alpha(w, h, &indices_b, &ps_b, trns_b)?;
    let out_b = oxipng::optimize_from_memory(&raw_png_b, &oxi).unwrap();
    let path_b = tmp.join(format!("c88_{}_pp.png", label.replace(' ', "_")));
    std::fs::write(&path_b, &out_b)?;
    let ssim_b = ssim_via_nupic(fixture_path, &path_b, &nupic);

    let total_a = init_time_a + refine_time_a;
    let total_b = init_time_b + refine_time_b;
    let d_init_ms = (init_time_b - init_time_a) * 1000.0;
    let d_refine_ms = (refine_time_b - refine_time_a) * 1000.0;
    let d_total_ms = (total_b - total_a) * 1000.0;
    let d_ssim = ssim_b - ssim_a;
    let d_size_pct = (out_b.len() as f64 / out_a.len() as f64 - 1.0) * 100.0;

    println!(
        "{:<22} {:>2}MP n={:>3} | iq init {:>5.0}ms refine {:>5.0}ms total {:>5.0}ms | pp init {:>5.0}ms refine {:>5.0}ms total {:>5.0}ms | Δinit {:>+5.0} Δrefine {:>+5.0} Δtotal {:>+5.0}ms | ΔSSIM {:+5.2} Δsize {:>+5.2}%",
        label,
        n_pixels / 1_000_000,
        n_colors,
        init_time_a * 1000.0,
        refine_time_a * 1000.0,
        total_a * 1000.0,
        init_time_b * 1000.0,
        refine_time_b * 1000.0,
        total_b * 1000.0,
        d_init_ms,
        d_refine_ms,
        d_total_ms,
        d_ssim,
        d_size_pct,
    );
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();
    let nupic = root.join("target/release/nupic");

    let fixtures: &[(&str, &str)] = &[
        ("inputs/04-photo-portrait.png", "04 portrait"),
        ("inputs/05-photo-mountain.png", "05 mountain"),
        ("inputs/06-photo-landscape.png", "06 landscape"),
        ("inputs/07-photo-product.png", "07 product"),
        ("inputs-ext-real/17-aurora-5mp.png", "17 aurora 5.9MP"),
        ("inputs-ext-real/25-sofia-cathedral-5mp.png", "25 sofia 5.5MP"),
        ("inputs-ext-real/27-whale-tail-5mp.png", "27 whale 5.5MP"),
    ];

    println!("Cycle 88 — R8 k-means++ init spike");
    println!("  bench: imagequant median-cut vs kmeans++ init, both → refine_palette_kmeans(100 iters)");
    println!("  gate (per R8 roadmap): 5MP −15-20ms total, SSIM 持平/微升");
    println!();
    for &(rel, lbl) in fixtures {
        let path = root.join("assets/png-bench").join(rel);
        if !path.exists() {
            println!("  (skip {}: not found)", lbl);
            continue;
        }
        run_fixture(&path, &nupic, lbl)?;
    }
    Ok(())
}
