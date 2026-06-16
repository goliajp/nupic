//! Stone E candidate — Floyd-Steinberg light dither attack on
//! 04-portrait residual SSIM gap (Stone D plateau at 83.06 vs TinyPNG
//! 85.86). Sweeps FS dither strength {0.0, 0.25, 0.5, 0.75, 1.0} on
//! the full corpus to find the sweet spot where size hit is acceptable
//! and SSIM gain dominates.
//!
//! FS dither at strength 1.0 = canonical Floyd-Steinberg (Heckbert 1975):
//! per pixel in raster order, distribute quantization residual to 4
//! neighbors with weights 7/16, 3/16, 5/16, 1/16. Strength < 1.0 scales
//! the residual before diffusion — milder dither, smaller size hit.
//!
//! Run:
//!   cargo run --release -p nupic-research --example stone_e_fs_dither

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use image::ImageReader;
use nupic_color::{Oklab, oklab_to_srgb_u8, srgb_u8_to_oklab};
use nupic_quantize::{encode_indexed_png_with_alpha, refine_palette_kmeans, train_palette_rgba};
use rgb::Rgb;

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(PathBuf::from(m).ancestors().nth(2).unwrap().to_path_buf())
}

fn ssimulacra2(orig: &Path, cmp: &Path) -> f64 {
    let out = Command::new("nupic")
        .args(["compare", "-m", "ssimulacra2"])
        .arg(orig)
        .arg(cmp)
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

/// Apply palette with Floyd-Steinberg light dither in OKLab space.
/// `strength` ∈ [0, 1]: 0 = no dither (= Stone D),1 = full FS.
fn apply_with_fs_dither(
    src_rgba: &[u8],
    w: u32,
    h: u32,
    palette_oklab: &[Oklab],
    palette_alpha: &[u8],
    strength: f32,
) -> (Vec<u8>, Vec<Rgb<u8>>) {
    let width = w as usize;
    let height = h as usize;
    let n_pixels = width * height;
    let k = palette_oklab.len();
    const ALPHA_WEIGHT: f32 = 2.0;
    const ALPHA_SCALE: f32 = ALPHA_WEIGHT / 255.0;

    // Pre-convert all pixels to OKLab + alpha for in-place diffusion.
    let mut pixels: Vec<(f32, f32, f32, f32)> = Vec::with_capacity(n_pixels);
    for px in src_rgba.chunks_exact(4) {
        let p = srgb_u8_to_oklab(Rgb { r: px[0], g: px[1], b: px[2] });
        let pa_scaled = px[3] as f32 * ALPHA_SCALE;
        pixels.push((p.l, p.a, p.b, pa_scaled));
    }

    let mut indices = vec![0u8; n_pixels];
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let (l, a, b, pa) = pixels[idx];

            // Find best palette entry (OKLab + alpha argmin).
            let mut best_j = 0usize;
            let mut best_d2 = f32::INFINITY;
            for j in 0..k {
                let pj = palette_oklab[j];
                let pa_j = palette_alpha[j] as f32 * ALPHA_SCALE;
                let dl = l - pj.l;
                let da = a - pj.a;
                let db = b - pj.b;
                let dpa = pa - pa_j;
                let d2 = dl.mul_add(
                    dl,
                    da.mul_add(da, db.mul_add(db, dpa * dpa)),
                );
                if d2 < best_d2 {
                    best_d2 = d2;
                    best_j = j;
                }
            }
            indices[idx] = best_j as u8;

            if strength > 0.0 {
                let pj = palette_oklab[best_j];
                let pa_j = palette_alpha[best_j] as f32 * ALPHA_SCALE;
                let err_l = (l - pj.l) * strength;
                let err_a = (a - pj.a) * strength;
                let err_b = (b - pj.b) * strength;
                let err_pa = (pa - pa_j) * strength;
                // Floyd-Steinberg weights: right=7/16, below-left=3/16,
                // below=5/16, below-right=1/16
                let diffuse = |px: &mut (f32, f32, f32, f32), w: f32| {
                    px.0 += err_l * w;
                    px.1 += err_a * w;
                    px.2 += err_b * w;
                    px.3 += err_pa * w;
                };
                if x + 1 < width {
                    diffuse(&mut pixels[idx + 1], 7.0 / 16.0);
                }
                if y + 1 < height {
                    if x > 0 {
                        diffuse(&mut pixels[(y + 1) * width + x - 1], 3.0 / 16.0);
                    }
                    diffuse(&mut pixels[(y + 1) * width + x], 5.0 / 16.0);
                    if x + 1 < width {
                        diffuse(&mut pixels[(y + 1) * width + x + 1], 1.0 / 16.0);
                    }
                }
            }
        }
    }
    let palette_srgb: Vec<Rgb<u8>> = palette_oklab.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
    (indices, palette_srgb)
}

fn process_variant_f(src_path: &PathBuf, strength: f32) -> Result<Vec<u8>> {
    let img = ImageReader::open(src_path)?.with_guessed_format()?.decode()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let raw = rgba.into_raw();
    let (palette_oklab, palette_alpha) =
        train_palette_rgba(&raw, w, h, 256).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let (palette_oklab, palette_alpha) = refine_palette_kmeans(
        &raw, w, h, &palette_oklab, &palette_alpha, 20,
    );
    let (idx, pal_srgb) = apply_with_fs_dither(
        &raw, w, h, &palette_oklab, &palette_alpha, strength,
    );
    let trns = if palette_alpha.iter().all(|&a| a == 255) {
        None
    } else {
        Some(palette_alpha.as_slice())
    };
    let png = encode_indexed_png_with_alpha(w, h, &idx, &pal_srgb, trns)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    Ok(png)
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

    let strengths = [0.0f32, 0.25, 0.5, 0.75, 1.0];
    let tmpdir = std::env::temp_dir().join("nupic-stone-e");
    std::fs::create_dir_all(&tmpdir)?;

    for &strength in &strengths {
        println!("\n========== Stone E FS dither strength = {:.2} ==========", strength);
        println!("{:<32} {:>10} {:>8}", "fixture", "size", "SSIM");
        let mut sum_size = 0usize;
        let mut ssim_sum = 0.0;
        for f in &fixtures {
            let src = root.join("assets/png-bench/inputs").join(f);
            let png = process_variant_f(&src, strength)?;
            let out_path = tmpdir.join(format!("s{:.2}-{f}", strength));
            std::fs::write(&out_path, &png)?;
            let ssim = ssimulacra2(&src, &out_path);
            println!("{:<32} {:>10} {:>8.2}", f, png.len(), ssim);
            sum_size += png.len();
            ssim_sum += ssim;
        }
        println!(
            "{:<32} {:>10} {:>8.2}",
            "TOTAL/AVG", sum_size, ssim_sum / fixtures.len() as f64
        );
    }
    Ok(())
}
