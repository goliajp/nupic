//! Cycle 21 — probe oxipng Zopfli deflater for size depth on full corpus.
//! Standard libdeflater (preset 5) vs Zopfli (iterations=15).
//! Goal: see if "又小又好" mission would benefit from --effort 10 →
//! zopfli toggle.

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;
use std::num::NonZeroU8;

use anyhow::Result;
use image::ImageReader;

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(PathBuf::from(m).ancestors().nth(2).unwrap().to_path_buf())
}

fn ssimulacra2(orig: &std::path::Path, cmp: &std::path::Path) -> f64 {
    let out = Command::new("nupic")
        .args(["compare", "-m", "ssimulacra2"])
        .arg(orig).arg(cmp).output().expect("nupic compare");
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines().find_map(|l| {
        l.strip_prefix("SSIMULACRA2: ")
            .and_then(|v| v.split_whitespace().next())
            .and_then(|n| n.parse::<f64>().ok())
    }).unwrap_or(0.0)
}

fn encode_pipeline(raw: &[u8], w: u32, h: u32, deflater: oxipng::Deflaters) -> Result<Vec<u8>> {
    let strength = nupic_quantize::classify_for_auto_dither(raw, w);
    let qi = nupic_quantize::quantize_with_dither(raw, w, h, 256,
        nupic_quantize::DEFAULT_REFINE_ITERS, strength)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let trns = if qi.palette_alpha.iter().all(|&a| a == 255) {
        None
    } else {
        Some(qi.palette_alpha.clone())
    };
    let raw_png = nupic_quantize::encode_indexed_png_with_alpha(
        w, h, &qi.indices, &qi.palette_srgb, trns.as_deref(),
    ).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let mut opts = oxipng::Options::from_preset(5);
    opts.deflate = deflater;
    let polished = oxipng::optimize_from_memory(&raw_png, &opts)
        .map_err(|e| anyhow::anyhow!("oxipng: {e:?}"))?;
    Ok(polished)
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
    let tmpdir = std::env::temp_dir().join("cycle21-zopfli");
    std::fs::create_dir_all(&tmpdir)?;

    println!("Cycle 21: oxipng Libdeflater vs Zopfli (iterations=15) on full corpus");
    println!("{:<32} {:>10} {:>9} {:>7}    {:>10} {:>9} {:>7}    {:>+8} {:>+8}",
        "fixture", "lib_size", "lib_SSIM", "lib_ms",
        "zop_size", "zop_SSIM", "zop_ms",
        "Δ size", "Δ SSIM");
    let mut sum_lib = 0i64;
    let mut sum_zop = 0i64;
    for fname in &fixtures {
        let p = root.join("assets/png-bench/inputs").join(fname);
        let img = ImageReader::open(&p)?.with_guessed_format()?.decode()?;
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        let raw = rgba.into_raw();

        let t = Instant::now();
        let lib = encode_pipeline(&raw, w, h, oxipng::Deflaters::Libdeflater { compression: 12 })?;
        let lib_ms = t.elapsed().as_millis();
        let p_lib = tmpdir.join(format!("{}-lib.png", fname));
        std::fs::write(&p_lib, &lib)?;
        let lib_ssim = ssimulacra2(&p, &p_lib);

        let t = Instant::now();
        let zop = encode_pipeline(&raw, w, h, oxipng::Deflaters::Zopfli {
            iterations: NonZeroU8::new(15).unwrap(),
        })?;
        let zop_ms = t.elapsed().as_millis();
        let p_zop = tmpdir.join(format!("{}-zop.png", fname));
        std::fs::write(&p_zop, &zop)?;
        let zop_ssim = ssimulacra2(&p, &p_zop);

        let ds = zop.len() as i64 - lib.len() as i64;
        let dq = zop_ssim - lib_ssim;
        sum_lib += lib.len() as i64;
        sum_zop += zop.len() as i64;
        println!("{:<32} {:>10} {:>9.3} {:>7}    {:>10} {:>9.3} {:>7}    {:>+8} {:>+8.3}",
            fname, lib.len(), lib_ssim, lib_ms, zop.len(), zop_ssim, zop_ms, ds, dq);
    }
    println!();
    println!("Total: lib={} B, zopfli={} B, delta={:+} B ({:+.2}%)",
        sum_lib, sum_zop, sum_zop - sum_lib,
        (sum_zop - sum_lib) as f64 / sum_lib as f64 * 100.0);
    Ok(())
}
