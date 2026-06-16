//! Stone D convergence diagnostic — track per-fixture refine_palette_kmeans
//! movement trajectory to inform adaptive cap + EPS tuning. Output:
//! iter-by-iter max_move + mean_move + actual_iter_to_eps for each
//! fixture. Used to validate Pass 3 (adaptive iter shipping).
//!
//! Run:
//!   cargo run --release -p nupic-research --example iter_convergence

use std::path::PathBuf;

use anyhow::Result;
use image::ImageReader;
use nupic_color::{Oklab, srgb_u8_to_oklab};
use nupic_quantize::train_palette_rgba;
use rgb::Rgb;

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(PathBuf::from(m).ancestors().nth(2).unwrap().to_path_buf())
}

/// Returns Vec of (iter_index, max_move, mean_move) per refine step.
fn track_kmeans_convergence(
    src_rgba: &[u8],
    palette_oklab: &[Oklab],
    palette_alpha: &[u8],
    max_iters: usize,
) -> Vec<(usize, f32, f32)> {
    let n_pixels = src_rgba.len() / 4;
    let k = palette_oklab.len();
    const ALPHA_WEIGHT: f32 = 2.0;
    const ALPHA_SCALE: f32 = ALPHA_WEIGHT / 255.0;

    let mut palette = palette_oklab.to_vec();
    let mut alpha = palette_alpha.to_vec();
    let mut trajectory = Vec::new();

    for iter in 0..max_iters {
        let mut assigned: Vec<usize> = vec![0; n_pixels];
        for (i, px) in src_rgba.chunks_exact(4).enumerate() {
            let p = srgb_u8_to_oklab(Rgb { r: px[0], g: px[1], b: px[2] });
            let pa = px[3];
            let mut best_j = 0;
            let mut best_d2 = f32::INFINITY;
            for j in 0..k {
                let pj = palette[j];
                let dl = p.l - pj.l;
                let da = p.a - pj.a;
                let db = p.b - pj.b;
                let d_alpha = (pa as i32 - alpha[j] as i32) as f32 * ALPHA_SCALE;
                let d2 = dl.mul_add(dl, da.mul_add(da, db.mul_add(db, d_alpha * d_alpha)));
                if d2 < best_d2 {
                    best_d2 = d2;
                    best_j = j;
                }
            }
            assigned[i] = best_j;
        }
        let mut sum_l = vec![0.0f64; k];
        let mut sum_a = vec![0.0f64; k];
        let mut sum_b = vec![0.0f64; k];
        let mut sum_alpha = vec![0u64; k];
        let mut count = vec![0u64; k];
        for (i, px) in src_rgba.chunks_exact(4).enumerate() {
            let p = srgb_u8_to_oklab(Rgb { r: px[0], g: px[1], b: px[2] });
            let j = assigned[i];
            sum_l[j] += p.l as f64;
            sum_a[j] += p.a as f64;
            sum_b[j] += p.b as f64;
            sum_alpha[j] += px[3] as u64;
            count[j] += 1;
        }
        let mut max_move = 0.0f32;
        let mut total_move = 0.0f64;
        let mut n_moved = 0u64;
        for j in 0..k {
            if count[j] == 0 {
                continue;
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
            let d_alpha = (new_alpha as i32 - alpha[j] as i32) as f32 * ALPHA_SCALE;
            let move_sq = dl.mul_add(dl, da.mul_add(da, db.mul_add(db, d_alpha * d_alpha)));
            if move_sq > max_move {
                max_move = move_sq;
            }
            total_move += move_sq as f64;
            n_moved += 1;
            palette[j] = Oklab { l: new_l, a: new_a, b: new_b };
            alpha[j] = new_alpha;
        }
        let mean_move = if n_moved == 0 { 0.0 } else { (total_move / n_moved as f64) as f32 };
        trajectory.push((iter + 1, max_move.sqrt(), mean_move.sqrt()));
        if max_move < 0.0005f32 * 0.0005f32 {
            break;
        }
    }
    trajectory
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let fixtures = [
        "01-png-transparency-demo.png",
        "02-pluto-transparent.png",
        "03-wikipedia-logo.png",
        "04-photo-portrait.png",
        "05-photo-mountain.png",
        "06-photo-landscape.png",
        "07-photo-product.png",
    ];

    for f in &fixtures {
        let path = root.join("assets/png-bench/inputs").join(f);
        let img = ImageReader::open(&path)?.with_guessed_format()?.decode()?;
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        let raw = rgba.into_raw();
        let (pal_oklab, pal_alpha) =
            train_palette_rgba(&raw, w, h, 256).map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let traj = track_kmeans_convergence(&raw, &pal_oklab, &pal_alpha, 100);
        println!("\n=== {f} ===");
        println!("{:>5}  {:>12}  {:>12}", "iter", "max_move", "mean_move");
        for (it, mx, mn) in traj.iter().take(50).chain(traj.iter().skip(50)) {
            // Print every iter (limited above 50 for readability)
            if *it <= 30 || *it % 5 == 0 {
                println!("{:>5}  {:>12.6}  {:>12.6}", it, mx, mn);
            }
        }
        println!("converged at iter {} (max_move {:.6})", traj.last().unwrap().0, traj.last().unwrap().1);
    }
    Ok(())
}
