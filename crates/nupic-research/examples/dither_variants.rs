//! Stone E dither variants research — bench Floyd-Steinberg
//! (vanilla raster), serpentine-raster FS, Sierra-3 (7-neighbor), and
//! Sierra-Lite (4-neighbor different weights) on the 7-fixture corpus
//! + 2 dogfood. Each variant × strength {0.25, 0.5} per fixture.
//!
//! Goal: see if any variant gives strict improvement over vanilla FS
//! (better SSIM at same size, or smaller size at same SSIM) — would
//! be a "free" quality bump if available.
//!
//! Run:
//!   cargo run --release -p nupic-research --example dither_variants

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use image::ImageReader;
use nupic_color::{Oklab, oklab_to_srgb_u8, srgb_u8_to_oklab};
use nupic_quantize::{encode_indexed_png_with_alpha, refine_palette_kmeans, train_palette_rgba};
use rgb::Rgb;

#[derive(Clone, Copy, Debug)]
enum DitherVariant {
    FsVanilla,
    FsSerpentine,
    Sierra3,
    SierraLite,
}

const VARIANTS: [(DitherVariant, &str); 4] = [
    (DitherVariant::FsVanilla, "FS"),
    (DitherVariant::FsSerpentine, "FS-snake"),
    (DitherVariant::Sierra3, "Sierra-3"),
    (DitherVariant::SierraLite, "Sierra-Lite"),
];

/// Diffuse residual to neighbours per variant. Coordinates relative to
/// current pixel (dx, dy); weights sum to 1.0.
///
/// In serpentine mode, dx is flipped for right-to-left rows so the
/// pattern moves consistently with raster direction.
fn diffuse(
    variant: DitherVariant,
    pixels: &mut [(f32, f32, f32, f32)],
    x: usize, y: usize, w: usize, h: usize,
    rtl: bool,
    err: (f32, f32, f32, f32),
) {
    let (taps, total): (&[(i32, i32, u32)], u32) = match variant {
        DitherVariant::FsVanilla | DitherVariant::FsSerpentine => (
            &[(1, 0, 7), (-1, 1, 3), (0, 1, 5), (1, 1, 1)],
            16,
        ),
        DitherVariant::Sierra3 => (
            // Sierra-3 (Sierra filter, /32 weights):
            //        X 5 3
            //  2 4 5 4 2
            //    2 3 2
            &[
                (1, 0, 5), (2, 0, 3),
                (-2, 1, 2), (-1, 1, 4), (0, 1, 5), (1, 1, 4), (2, 1, 2),
                (-1, 2, 2), (0, 2, 3), (1, 2, 2),
            ],
            32,
        ),
        DitherVariant::SierraLite => (
            // Sierra Lite (/4):
            //   X 2
            // 1 1
            &[(1, 0, 2), (-1, 1, 1), (0, 1, 1)],
            4,
        ),
    };
    let flip = rtl;
    for &(dx, dy, num) in taps {
        let real_dx = if flip { -dx } else { dx };
        let nx = x as i64 + real_dx as i64;
        let ny = y + dy as usize;
        if nx < 0 || nx >= w as i64 || ny >= h {
            continue;
        }
        let target_idx = ny * w + nx as usize;
        let weight = num as f32 / total as f32;
        pixels[target_idx].0 += err.0 * weight;
        pixels[target_idx].1 += err.1 * weight;
        pixels[target_idx].2 += err.2 * weight;
        pixels[target_idx].3 += err.3 * weight;
    }
}

fn apply_dither_variant(
    src_rgba: &[u8],
    width: u32, height: u32,
    palette_oklab: &[Oklab], palette_alpha: &[u8],
    strength: f32,
    variant: DitherVariant,
) -> (Vec<u8>, Vec<Rgb<u8>>) {
    let w = width as usize;
    let h = height as usize;
    let n_pixels = w * h;
    let k = palette_oklab.len();
    const ALPHA_WEIGHT: f32 = 2.0;
    const ALPHA_SCALE: f32 = ALPHA_WEIGHT / 255.0;

    let mut pixels: Vec<(f32, f32, f32, f32)> = src_rgba
        .chunks_exact(4)
        .map(|px| {
            let p = srgb_u8_to_oklab(Rgb { r: px[0], g: px[1], b: px[2] });
            (p.l, p.a, p.b, px[3] as f32 * ALPHA_SCALE)
        })
        .collect();

    let mut indices = vec![0u8; n_pixels];
    let serpentine = matches!(variant, DitherVariant::FsSerpentine);

    for y in 0..h {
        let rtl = serpentine && (y % 2 == 1);
        let xs: Box<dyn Iterator<Item = usize>> = if rtl {
            Box::new((0..w).rev())
        } else {
            Box::new(0..w)
        };
        for x in xs {
            let idx = y * w + x;
            let (l, a, b, pa) = pixels[idx];
            let mut best_j = 0usize;
            let mut best_d2 = f32::INFINITY;
            for j in 0..k {
                let pj = palette_oklab[j];
                let pa_j = palette_alpha[j] as f32 * ALPHA_SCALE;
                let dl = l - pj.l;
                let da = a - pj.a;
                let db = b - pj.b;
                let dpa = pa - pa_j;
                let d2 = dl.mul_add(dl, da.mul_add(da, db.mul_add(db, dpa * dpa)));
                if d2 < best_d2 {
                    best_d2 = d2;
                    best_j = j;
                }
            }
            indices[idx] = best_j as u8;

            if strength > 0.0 {
                let pj = palette_oklab[best_j];
                let pa_j = palette_alpha[best_j] as f32 * ALPHA_SCALE;
                let err = (
                    (l - pj.l) * strength,
                    (a - pj.a) * strength,
                    (b - pj.b) * strength,
                    (pa - pa_j) * strength,
                );
                diffuse(variant, &mut pixels, x, y, w, h, rtl, err);
            }
        }
    }
    let palette_srgb: Vec<Rgb<u8>> = palette_oklab.iter().map(|c| oklab_to_srgb_u8(*c)).collect();
    (indices, palette_srgb)
}

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

fn main() -> Result<()> {
    let root = workspace_root()?;
    let fixtures = [
        "04-photo-portrait.png",
        "05-photo-mountain.png",
        "06-photo-landscape.png",
        "07-photo-product.png",
    ];
    let strengths = [0.25_f32, 0.5];
    let tmpdir = std::env::temp_dir().join("nupic-dither-variants");
    std::fs::create_dir_all(&tmpdir)?;

    for &strength in &strengths {
        println!("\n========== strength = {:.2} ==========", strength);
        print!("{:<32}", "fixture");
        for (_, name) in VARIANTS {
            print!(" {:>14} {:>8}", format!("{name}_size"), format!("{name}_S"));
        }
        println!();
        println!("{}", "-".repeat(220));

        for fname in &fixtures {
            let src = root.join("assets/png-bench/inputs").join(fname);
            let img = ImageReader::open(&src)?.with_guessed_format()?.decode()?;
            let rgba = img.to_rgba8();
            let (w, h) = (rgba.width(), rgba.height());
            let raw = rgba.into_raw();
            let (pal_oklab, pal_alpha) =
                train_palette_rgba(&raw, w, h, 256).map_err(|e| anyhow::anyhow!("{e:?}"))?;
            let (pal_oklab, pal_alpha) =
                refine_palette_kmeans(&raw, w, h, &pal_oklab, &pal_alpha, 50);

            print!("{:<32}", fname);
            for (variant, label) in VARIANTS {
                let (idx, pal_srgb) =
                    apply_dither_variant(&raw, w, h, &pal_oklab, &pal_alpha, strength, variant);
                let trns = if pal_alpha.iter().all(|&a| a == 255) {
                    None
                } else {
                    Some(pal_alpha.as_slice())
                };
                let png = encode_indexed_png_with_alpha(w, h, &idx, &pal_srgb, trns)
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                let out_path =
                    tmpdir.join(format!("{label}-s{:.2}-{fname}", strength));
                std::fs::write(&out_path, &png)?;
                let ssim = ssimulacra2(&src, &out_path);
                print!(" {:>14} {:>8.2}", png.len(), ssim);
            }
            println!();
        }
    }
    Ok(())
}
