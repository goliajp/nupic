//! Cycle 12 — profile 05-mountain quantize ceiling.
//! What's bottlenecking SSIM at 76.82?
//! - Are all 256 palette slots used (palette-budget bound)?
//! - How many Lloyd's iters before EPS auto-stop?
//! - What's pre-Lloyd's SSIM vs post-Lloyd's?

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use image::ImageReader;
use nupic_quantize::{QuantizeOpts, quantize_indexed_png};

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

fn count_unique_indices(indices: &[u8]) -> usize {
    let mut seen = [false; 256];
    for &i in indices {
        seen[i as usize] = true;
    }
    seen.iter().filter(|&&x| x).count()
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let src = root.join("assets/png-bench/inputs/05-photo-mountain.png");
    let img = ImageReader::open(&src)?.with_guessed_format()?.decode()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let raw = rgba.into_raw();

    // Pre-Lloyd: imagequant only (refine_iters = 0)
    let qi_no_lloyd = nupic_quantize::quantize_with_dither(&raw, w, h, 256, 0, 0.7)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let palette_no_lloyd = count_unique_indices(&qi_no_lloyd.indices);
    let palette_size_decl = qi_no_lloyd.palette_srgb.len();

    // After Lloyd's at DEFAULT_REFINE_ITERS (100)
    let qi_100 = nupic_quantize::quantize_with_dither(&raw, w, h, 256, 100, 0.7)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let palette_100 = count_unique_indices(&qi_100.indices);
    let pal_decl_100 = qi_100.palette_srgb.len();

    // After Lloyd's at 200, 300 to test convergence
    let qi_200 = nupic_quantize::quantize_with_dither(&raw, w, h, 256, 200, 0.7)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let pal_decl_200 = qi_200.palette_srgb.len();

    let qi_500 = nupic_quantize::quantize_with_dither(&raw, w, h, 256, 500, 0.7)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let pal_decl_500 = qi_500.palette_srgb.len();

    let tmpdir = std::env::temp_dir().join("cycle12-05");
    std::fs::create_dir_all(&tmpdir)?;

    println!("05-photo-mountain.png ({}x{} = {} pixels)", w, h, w*h);
    println!();
    println!("Quantize profile (dither=0.7):");
    println!("{:<24} {:>10} {:>16} {:>12}", "stage", "pal_decl", "pal_uniq_indices", "indices_len");
    println!("{:<24} {:>10} {:>16} {:>12}", "iq_only_(iter=0)", palette_size_decl, palette_no_lloyd, qi_no_lloyd.indices.len());
    println!("{:<24} {:>10} {:>16} {:>12}", "iter=100_(default)", pal_decl_100, palette_100, qi_100.indices.len());
    println!("{:<24} {:>10} {:>16}", "iter=200", pal_decl_200, "—");
    println!("{:<24} {:>10} {:>16}", "iter=500", pal_decl_500, "—");

    // Compare SSIM at different iter caps via full pipeline
    println!();
    println!("Full pipeline SSIM (dither=0.7, all opts):");
    for iters in [0usize, 50, 100, 200, 500] {
        let opts = QuantizeOpts {
            n_colors: 256,
            oxipng_preset: 5,
            strip_metadata: true,
            dither_strength: 0.7,
            ..Default::default()
        };
        let _ = iters; // QuantizeOpts uses DEFAULT_REFINE_ITERS internally; skip iter variations here
        let png = quantize_indexed_png(&raw, w, h, opts).map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let out = tmpdir.join("iter-default.png");
        std::fs::write(&out, &png)?;
        let s = ssimulacra2(&src, &out);
        println!("  default (100) iters: {} bytes SSIM={}", png.len(), s);
        break; // just one measurement; iters within QuantizeOpts is fixed
    }
    Ok(())
}
