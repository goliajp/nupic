//! Cycle 6 Pass 3 — sweep nupic-deflate iterative cost-DP pass count
//! to test if 5 is the saturation point or if more iters close the
//! 18% gap vs libdeflate on 04-portrait filtered rows.
//!
//! Test by re-running deflate at varying iter counts via a custom
//! collect-iterate path. Since ITER_PASSES is a const, we can't sweep
//! it via runtime arg — instead, run Path A's filtered rows through
//! `nupic_deflate::zlib_compress` at the current default and compare
//! against the previously-measured oxipng/libdeflate ground truth.
//!
//! Quick test:rebuild with `ITER_PASSES = N` for N ∈ {5, 10, 15, 20}
//! between runs; this example only measures the current build's
//! deflate-quality on Path A's filtered rows.
//!
//! Run:
//!   ITER_PASSES=10 cargo run --release -p nupic-research --example iter_passes_sweep

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::Result;
use flate2::read::ZlibDecoder;
use nupic_quantize::{QuantizeOpts, quantize_indexed_png};

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(PathBuf::from(m).ancestors().nth(2).unwrap().to_path_buf())
}

fn extract_idat(png: &[u8]) -> Result<Vec<u8>> {
    let mut p = 8;
    let mut idat = Vec::new();
    while p + 12 <= png.len() {
        let len = u32::from_be_bytes(png[p..p + 4].try_into().unwrap()) as usize;
        let ty = &png[p + 4..p + 8];
        if ty == b"IDAT" {
            idat.extend_from_slice(&png[p + 8..p + 8 + len]);
        }
        p += 8 + len + 4;
        if ty == b"IEND" {
            break;
        }
    }
    Ok(idat)
}

fn ground_truth_filtered_rows(src: &Path) -> Result<Vec<u8>> {
    let img = image::ImageReader::open(src)?.with_guessed_format()?.decode()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let raw = rgba.into_raw();
    let opts = QuantizeOpts {
        n_colors: 256,
        oxipng_preset: 5,
        strip_metadata: true,
        dither_strength: 0.0,
            ..Default::default()
        };
    let path_a_png = quantize_indexed_png(&raw, w, h, opts).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let idat = extract_idat(&path_a_png)?;
    let mut decoder = ZlibDecoder::new(&idat[..]);
    let mut filtered = Vec::new();
    decoder.read_to_end(&mut filtered)?;
    Ok(filtered)
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

    println!("{:<32} {:>10} {:>10}  ratio_nupic/libdeflate", "fixture", "libdeflate", "nupic_Best");
    for f in &fixtures {
        let src = root.join("assets/png-bench/inputs").join(f);
        let path_a_png = {
            let img = image::ImageReader::open(&src)?.with_guessed_format()?.decode()?;
            let rgba = img.to_rgba8();
            let (w, h) = (rgba.width(), rgba.height());
            let raw = rgba.into_raw();
            let opts = QuantizeOpts {
                n_colors: 256,
                oxipng_preset: 5,
                strip_metadata: true,
                dither_strength: 0.0,
            ..Default::default()
        };
            quantize_indexed_png(&raw, w, h, opts).map_err(|e| anyhow::anyhow!("{e:?}"))?
        };
        let idat_a = extract_idat(&path_a_png)?;
        let lib_size = idat_a.len(); // libdeflate near-optimal compressed size

        let filtered = ground_truth_filtered_rows(&src)?;
        let nupic = nupic_deflate::zlib_compress(&filtered);
        let ratio = nupic.len() as f64 / lib_size as f64;
        println!("{:<32} {:>10} {:>10}  {:.4}×", f, lib_size, nupic.len(), ratio);
    }
    Ok(())
}
