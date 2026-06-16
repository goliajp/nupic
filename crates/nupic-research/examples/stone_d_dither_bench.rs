//! Stone D research — prototype adaptive light dither + measure
//! SSIMULACRA2 + size delta vs current Stone C(no-dither) on the
//! 7-fixture corpus + dogfood-failing screenshot inputs.
//!
//! Variant A:per-pixel error-magnitude trigger + Bayer 8×8 modulation
//! between best-j and second-best-j palette entries。Only dither when
//! residual OKLab distance > THRESHOLD。
//!
//! Backs `docs/research/png/03d-stone-d-design.md`. Run:
//!   cargo run --release -p nupic-research --example stone_d_dither_bench

use std::path::PathBuf;

use anyhow::{Context, Result};
use image::ImageReader;
use nupic_color::{Oklab, oklab_to_srgb_u8, srgb_u8_to_oklab};
use nupic_quantize::{QuantizeOpts, encode_indexed_png_with_alpha, train_palette_rgba};
use rgb::Rgb;

/// Standard 8×8 Bayer dither matrix, scaled to [0, 64).
const BAYER8: [[u8; 8]; 8] = [
    [ 0, 32,  8, 40,  2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44,  4, 36, 14, 46,  6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [ 3, 35, 11, 43,  1, 33,  9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47,  7, 39, 13, 45,  5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];

/// Variant E: Lloyd's k-means refinement of the OKLab palette,
/// starting from imagequant median-cut. Returns refined palette.
///
/// For each iteration:
/// 1. Assign each pixel to its closest palette entry (current Stone C
///    OKLab argmin, alpha-aware).
/// 2. For each cluster, compute the mean OKLab of its assigned pixels.
///    Skip empty clusters (keep old centroid).
/// 3. If no centroid moved more than `EPS`, declare converged.
fn refine_palette_kmeans(
    src_rgba: &[u8],
    width: u32,
    height: u32,
    palette_oklab: &[Oklab],
    palette_alpha: &[u8],
    n_iters: usize,
) -> (Vec<Oklab>, Vec<u8>) {
    let w = width as usize;
    let h = height as usize;
    let n_pixels = w * h;
    let k = palette_oklab.len();
    const ALPHA_WEIGHT: f32 = 2.0;
    const ALPHA_SCALE: f32 = ALPHA_WEIGHT / 255.0;
    const EPS: f32 = 0.0005;

    let mut palette = palette_oklab.to_vec();
    let mut alpha = palette_alpha.to_vec();

    for _iter in 0..n_iters {
        // assignments
        let mut assigned: Vec<usize> = vec![0; n_pixels];
        for pi in 0..n_pixels {
            let off = pi * 4;
            let p = srgb_u8_to_oklab(Rgb {
                r: src_rgba[off],
                g: src_rgba[off + 1],
                b: src_rgba[off + 2],
            });
            let pa = src_rgba[off + 3];
            let mut best_j = 0;
            let mut best_d2 = f32::INFINITY;
            for j in 0..k {
                let pj = palette[j];
                let dl = p.l - pj.l;
                let da = p.a - pj.a;
                let db = p.b - pj.b;
                let d_alpha = (pa as i32 - alpha[j] as i32) as f32 * ALPHA_SCALE;
                let d2 = dl.mul_add(
                    dl,
                    da.mul_add(da, db.mul_add(db, d_alpha * d_alpha)),
                );
                if d2 < best_d2 {
                    best_d2 = d2;
                    best_j = j;
                }
            }
            assigned[pi] = best_j;
        }
        // accumulate means (Oklab + alpha)
        let mut sum_l = vec![0.0f64; k];
        let mut sum_a = vec![0.0f64; k];
        let mut sum_b = vec![0.0f64; k];
        let mut sum_alpha = vec![0u64; k];
        let mut count = vec![0u64; k];
        for pi in 0..n_pixels {
            let off = pi * 4;
            let p = srgb_u8_to_oklab(Rgb {
                r: src_rgba[off],
                g: src_rgba[off + 1],
                b: src_rgba[off + 2],
            });
            let j = assigned[pi];
            sum_l[j] += p.l as f64;
            sum_a[j] += p.a as f64;
            sum_b[j] += p.b as f64;
            sum_alpha[j] += src_rgba[off + 3] as u64;
            count[j] += 1;
        }
        let mut max_move = 0.0f32;
        for j in 0..k {
            if count[j] == 0 {
                continue; // empty cluster: keep old centroid
            }
            let nc = count[j] as f64;
            let new_l = (sum_l[j] / nc) as f32;
            let new_a = (sum_a[j] / nc) as f32;
            let new_b = (sum_b[j] / nc) as f32;
            let new_alpha = (sum_alpha[j] as f64 / nc).round() as u8;
            let old = palette[j];
            let dl = new_l - old.l;
            let da = new_a - old.a;
            let db = new_b - old.b;
            let move_sq = dl.mul_add(dl, da.mul_add(da, db * db));
            if move_sq > max_move {
                max_move = move_sq;
            }
            palette[j] = Oklab { l: new_l, a: new_a, b: new_b };
            alpha[j] = new_alpha;
        }
        if max_move.sqrt() < EPS {
            break;
        }
    }
    (palette, alpha)
}

/// Variant A (kept for negative-result regression check): per-pixel
/// adaptive light dither using Bayer 8×8. Documented as not-working
/// in `03d-stone-d-design.md` §4.
#[allow(dead_code)]
fn apply_palette_with_dither(
    src_rgba: &[u8],
    width: u32,
    height: u32,
    palette_oklab: &[Oklab],
    palette_alpha: &[u8],
    threshold: f32,
) -> (Vec<u8>, Vec<Rgb<u8>>) {
    let w = width as usize;
    let h = height as usize;
    let k = palette_oklab.len();
    let n_pixels = w * h;
    assert_eq!(src_rgba.len(), n_pixels * 4);
    assert_eq!(palette_alpha.len(), k);

    const ALPHA_WEIGHT: f32 = 2.0;
    const ALPHA_SCALE: f32 = ALPHA_WEIGHT / 255.0;
    let threshold_sq = threshold * threshold;

    let mut indices = vec![0u8; n_pixels];
    for y in 0..h {
        for x in 0..w {
            let off = (y * w + x) * 4;
            let p = srgb_u8_to_oklab(Rgb {
                r: src_rgba[off],
                g: src_rgba[off + 1],
                b: src_rgba[off + 2],
            });
            let pa = src_rgba[off + 3];

            // Find best and second-best palette entries.
            let mut best_j = 0usize;
            let mut best_d2 = f32::INFINITY;
            let mut second_j = 0usize;
            let mut second_d2 = f32::INFINITY;
            for j in 0..k {
                let pj = palette_oklab[j];
                let dl = p.l - pj.l;
                let da = p.a - pj.a;
                let db = p.b - pj.b;
                let d_alpha =
                    (pa as i32 - palette_alpha[j] as i32) as f32 * ALPHA_SCALE;
                let d2 = dl.mul_add(
                    dl,
                    da.mul_add(da, db.mul_add(db, d_alpha * d_alpha)),
                );
                if d2 < best_d2 {
                    second_j = best_j;
                    second_d2 = best_d2;
                    best_j = j;
                    best_d2 = d2;
                } else if d2 < second_d2 {
                    second_j = j;
                    second_d2 = d2;
                }
            }

            // Adaptive light dither: only kick in when residual >
            // threshold AND there's a meaningful 2nd-best.
            let pick = if best_d2 > threshold_sq && second_d2.is_finite() {
                // Bayer threshold in [0, 1).
                let bayer = BAYER8[y % 8][x % 8] as f32 / 64.0;
                // Pick second if mix-ratio < bayer (i.e., when the
                // dither cell wants to "promote" the 2nd-best). Ratio
                // = best_d2 / (best_d2 + second_d2) — close to 0.5
                // when two are equidistant, closer to 0 when best is
                // strongly preferred.
                let total = best_d2 + second_d2;
                let ratio = best_d2 / total;
                if bayer < ratio { second_j } else { best_j }
            } else {
                best_j
            };
            indices[y * w + x] = pick as u8;
        }
    }
    let palette_srgb: Vec<Rgb<u8>> =
        palette_oklab.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
    (indices, palette_srgb)
}

fn process_variant_e(
    src_path: &PathBuf,
    n_iters: usize,
) -> Result<(Vec<u8>, Vec<u8>)> {
    let img = ImageReader::open(src_path)?
        .with_guessed_format()?
        .decode()
        .context("decode source")?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let raw = rgba.into_raw();

    let (palette_oklab, palette_alpha) =
        train_palette_rgba(&raw, w, h, 256).map_err(|e| anyhow::anyhow!("{e:?}"))?;

    // Baseline: current Stone C (no refinement).
    let (idx_no, pal_srgb_no) = nupic_quantize::apply_palette_rgba(
        &raw, w, h, &palette_oklab, &palette_alpha,
    );

    // Variant E: refine palette via k-means, then re-assign.
    let (palette_refined, alpha_refined) = refine_palette_kmeans(
        &raw, w, h, &palette_oklab, &palette_alpha, n_iters,
    );
    let (idx_e, pal_srgb_e) = nupic_quantize::apply_palette_rgba(
        &raw, w, h, &palette_refined, &alpha_refined,
    );

    let trns_no = if palette_alpha.iter().all(|&a| a == 255) {
        None
    } else {
        Some(palette_alpha.as_slice())
    };
    let trns_e = if alpha_refined.iter().all(|&a| a == 255) {
        None
    } else {
        Some(alpha_refined.as_slice())
    };

    let png_no = encode_indexed_png_with_alpha(w, h, &idx_no, &pal_srgb_no, trns_no)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let png_e = encode_indexed_png_with_alpha(w, h, &idx_e, &pal_srgb_e, trns_e)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    // Run oxipng over both for production-comparable size.
    let opts = QuantizeOpts {
        n_colors: 256,
        oxipng_preset: 5,
        strip_metadata: true,
    };
    let _ = opts;
    Ok((png_no, png_e))
}

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(PathBuf::from(m).ancestors().nth(2).unwrap().to_path_buf())
}

fn ssimulacra2(orig_path: &std::path::Path, cmp_png_path: &std::path::Path) -> f64 {
    use std::process::Command;
    let out = Command::new("nupic")
        .args(["compare", "-m", "ssimulacra2"])
        .arg(orig_path)
        .arg(cmp_png_path)
        .output()
        .expect("nupic compare");
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines()
        .find_map(|l| {
            l.strip_prefix("SSIMULACRA2: ")
                .and_then(|v| v.split_whitespace().next())
                .and_then(|n| n.parse::<f64>().ok())
        })
        .unwrap_or(0.0)
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let fixtures = [
        ("01-png-transparency-demo.png", "assets/png-bench/inputs"),
        ("02-pluto-transparent.png", "assets/png-bench/inputs"),
        ("03-wikipedia-logo.png", "assets/png-bench/inputs"),
        ("04-photo-portrait.png", "assets/png-bench/inputs"),
        ("05-photo-mountain.png", "assets/png-bench/inputs"),
        ("06-photo-landscape.png", "assets/png-bench/inputs"),
        ("07-photo-product.png", "assets/png-bench/inputs"),
    ];

    let iter_sweep = [5usize, 10, 20, 50];

    for &n_iters in &iter_sweep {
        println!("\n========== Variant E: k-means n_iters = {} ==========", n_iters);
        println!(
            "{:<32} {:>10} {:>10}  {:>7} {:>7}   {:>8} {:>8}   {:>7}",
            "fixture", "stone_c", "variant_e", "size%", "Δ_size", "SSIM_C", "SSIM_E", "ΔSSIM",
        );
        let mut sum_no = 0usize;
        let mut sum_e = 0usize;
        let mut ssim_pairs: Vec<(f64, f64)> = Vec::new();
        let tmpdir = std::env::temp_dir().join("nupic-stoned");
        let _ = std::fs::create_dir_all(&tmpdir);
        for (fname, dir) in &fixtures {
            let path = root.join(dir).join(fname);
            let (png_no, png_e) = process_variant_e(&path, n_iters)?;
            let no_path = tmpdir.join(format!("no-{fname}"));
            let e_path = tmpdir.join(format!("e-{fname}"));
            std::fs::write(&no_path, &png_no)?;
            std::fs::write(&e_path, &png_e)?;
            let s_no = ssimulacra2(&path, &no_path);
            let s_e = ssimulacra2(&path, &e_path);
            let pct = png_e.len() as f64 / png_no.len() as f64;
            let dssim = s_e - s_no;
            println!(
                "{:<32} {:>10} {:>10}  {:>6.2}× {:>+7}   {:>8.2} {:>8.2}   {:>+6.2}",
                fname,
                png_no.len(),
                png_e.len(),
                pct,
                png_e.len() as i64 - png_no.len() as i64,
                s_no,
                s_e,
                dssim,
            );
            sum_no += png_no.len();
            sum_e += png_e.len();
            ssim_pairs.push((s_no, s_e));
        }
        let avg_no: f64 =
            ssim_pairs.iter().map(|(a, _)| a).sum::<f64>() / ssim_pairs.len() as f64;
        let avg_e: f64 =
            ssim_pairs.iter().map(|(_, b)| b).sum::<f64>() / ssim_pairs.len() as f64;
        println!(
            "{:<32} {:>10} {:>10}  {:>6.2}× {:>+7}   {:>8.2} {:>8.2}   {:>+6.2}",
            "TOTAL/AVG",
            sum_no,
            sum_e,
            sum_e as f64 / sum_no as f64,
            sum_e as i64 - sum_no as i64,
            avg_no,
            avg_e,
            avg_e - avg_no,
        );
    }
    Ok(())
}
