//! Cycle 6 Pass 1 — decompose the Path B (--use-nupic-png) vs Path A
//! (oxipng) size gap per fixture into (filter selection) and (deflate
//! compression) components. Output: per-fixture size matrix showing
//! which dimension dominates the residual gap, informing whether
//! Cycle 6 should attack filter selection, deflate quality, or both.
//!
//! Methodology for each fixture (running through full nupic-quantize
//! Stone D pipeline so palette + indices are constant):
//!
//! - **Path A**: existing `quantize_indexed_png` → png crate raw → oxipng
//! - **Path B**: `quantize` → `nupic-png` (BestOf filter + adaptive Fast/Best)
//! - **A_filter + nupic_deflate**: take Path A output, extract filtered
//!   rows via zlib decode, recompress with nupic_deflate Level::Best
//! - **B_filter + libdeflate**: take Path B output, extract filtered
//!   rows, recompress via flate2 (miniz_oxide is the cement libdeflate-
//!   equivalent we have available)
//!
//! Gap A → B = (filter-only gap) + (deflate-only gap) + cross-term
//!
//! Run:
//!   cargo run --release -p nupic-research --example default_flip_gap

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use flate2::read::ZlibDecoder;
use image::ImageReader;
use nupic_png::{IndexedImage, encode_indexed_png};
use nupic_quantize::{QuantizeOpts, quantize, quantize_indexed_png};
use rgb::Rgb;

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(PathBuf::from(m).ancestors().nth(2).unwrap().to_path_buf())
}

/// Walk PNG chunks, concatenate IDAT data.
fn extract_idat(png: &[u8]) -> Result<Vec<u8>> {
    if &png[0..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(anyhow::anyhow!("not a PNG"));
    }
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

fn decode_zlib(data: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = ZlibDecoder::new(data);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).context("zlib decode")?;
    Ok(out)
}

fn recompress_with_nupic_deflate(filtered: &[u8]) -> Vec<u8> {
    nupic_deflate::zlib_compress(filtered)
}

fn recompress_with_flate2(filtered: &[u8]) -> Vec<u8> {
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::new(9));
    enc.write_all(filtered).unwrap();
    enc.finish().unwrap()
}

fn process(src_path: &Path) -> Result<(String, usize, usize, usize, usize, usize, usize)> {
    let img = ImageReader::open(src_path)?.with_guessed_format()?.decode()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let raw = rgba.into_raw();

    // Path A: production path (quantize_indexed_png includes Stone D
    // refinement + oxipng).
    let path_a_opts = QuantizeOpts {
        n_colors: 256,
        oxipng_preset: 5,
        strip_metadata: true,
        dither_strength: 0.0,
            ..Default::default()
        };
    let path_a = quantize_indexed_png(&raw, w, h, path_a_opts)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let size_a = path_a.len();
    let idat_a = extract_idat(&path_a)?;
    let filtered_a = decode_zlib(&idat_a)?;

    // Path B: nupic-quantize + nupic-png.
    let qi = quantize(&raw, w, h, 256).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let trns = if qi.palette_alpha.iter().all(|&a| a == 255) {
        None
    } else {
        Some(qi.palette_alpha)
    };
    let png_img = IndexedImage {
        width: w,
        height: h,
        palette: qi.palette_srgb,
        indices: qi.indices,
        trns,
    };
    let path_b = encode_indexed_png(&png_img);
    let size_b = path_b.len();
    let idat_b = extract_idat(&path_b)?;
    let filtered_b = decode_zlib(&idat_b)?;

    // Cross-products: same filtered_rows, different deflate.
    let af_nd = recompress_with_nupic_deflate(&filtered_a);
    let bf_lib = recompress_with_flate2(&filtered_b);
    let af_size = af_nd.len();
    let bf_size = bf_lib.len();

    let fname = src_path.file_name().unwrap().to_string_lossy().into_owned();
    Ok((fname, size_a, size_b, filtered_a.len(), filtered_b.len(), af_size, bf_size))
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let inputs: Vec<PathBuf> = vec![
        root.join("assets/png-bench/inputs/01-png-transparency-demo.png"),
        root.join("assets/png-bench/inputs/02-pluto-transparent.png"),
        root.join("assets/png-bench/inputs/03-wikipedia-logo.png"),
        root.join("assets/png-bench/inputs/04-photo-portrait.png"),
        root.join("assets/png-bench/inputs/05-photo-mountain.png"),
        root.join("assets/png-bench/inputs/06-photo-landscape.png"),
        root.join("assets/png-bench/inputs/07-photo-product.png"),
    ];

    println!(
        "{:<32} {:>9} {:>9}   {:>9} {:>9}   {:>9} {:>9}",
        "fixture",
        "A_total", "B_total",
        "A_idat", "B_idat",
        "Afilt+ND", "Bfilt+lib"
    );
    println!("{}", "-".repeat(120));

    for src in &inputs {
        let (name, size_a, size_b, filt_a_decoded, filt_b_decoded, af_nd, bf_lib) = process(src)?;
        let _ = (filt_a_decoded, filt_b_decoded);
        println!(
            "{:<32} {:>9} {:>9}   {:>9} {:>9}   {:>9} {:>9}",
            name, size_a, size_b,
            // IDAT sizes (post-deflate)
            extract_idat_size(src, size_a, &name, &name),
            extract_idat_size(src, size_b, &name, &name),
            af_nd, bf_lib
        );
        // Gap decomposition:
        // - filter_gap = (size_b - bf_lib) — Path B filter run through libdeflate, vs Path B current
        // - deflate_gap = (af_nd - size_a) — Path A filter run through nupic-deflate, vs Path A current
        let filter_gap = (size_b as i64) - (bf_lib as i64);
        let deflate_gap = (af_nd as i64) - (size_a as i64);
        let total_gap = (size_b as i64) - (size_a as i64);
        println!(
            "  ▸ B-vs-A gap: {:+} bytes (filter contribution ~{:+}, deflate contribution ~{:+})",
            total_gap, filter_gap, deflate_gap
        );
    }
    Ok(())
}

fn extract_idat_size(_src: &Path, _full: usize, _name: &str, _name2: &str) -> usize {
    // helper if needed; for now just returns 0 since we have the totals
    0
}
