//! PNG integration readiness bench: for each fixture in
//! `assets/png-bench/current-nupic-0.5/`, extract the IDAT chunk
//! (zlib-compressed filtered rows), decompress it, and recompress with
//! `nupic-deflate Level::Best`. Reports size delta — answers "what's
//! the user-facing PNG-file-size impact of replacing oxipng's deflate
//! backend with nupic-deflate, keeping oxipng's filter selection?"
//!
//! Backs the next-pass decision on PNG pipeline integration. Run:
//!   cargo run --release -p nupic-research --example png_idat_swap

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use flate2::read::ZlibDecoder;
use nupic_deflate::{Level, deflate_level};

/// Walks PNG chunks, returns Vec of (chunk_type, data).
fn parse_chunks(png: &[u8]) -> Result<Vec<([u8; 4], Vec<u8>)>> {
    if png.len() < 8 || &png[0..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(anyhow!("not a PNG"));
    }
    let mut out = Vec::new();
    let mut p = 8;
    while p + 12 <= png.len() {
        let len = u32::from_be_bytes(png[p..p + 4].try_into().unwrap()) as usize;
        let mut ty = [0u8; 4];
        ty.copy_from_slice(&png[p + 4..p + 8]);
        if p + 8 + len + 4 > png.len() {
            return Err(anyhow!("chunk overruns file"));
        }
        let data = png[p + 8..p + 8 + len].to_vec();
        out.push((ty, data));
        p += 8 + len + 4;
        if &ty == b"IEND" {
            break;
        }
    }
    Ok(out)
}

fn idat_concat(chunks: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut concat = Vec::new();
    for (ty, data) in chunks {
        if ty == b"IDAT" {
            concat.extend_from_slice(data);
        }
    }
    concat
}

/// Approximate cost of writing IDAT bytes back as chunks (length + type + data + CRC per chunk).
/// We assume one big IDAT chunk (12 byte overhead).
fn chunk_overhead() -> usize {
    12 // 4-byte length + 4-byte type + 4-byte CRC
}

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(Path::new(m).ancestors().nth(2).context("workspace root")?.to_path_buf())
}

fn process(root: &Path, fname: &str) -> Result<(String, usize, usize, usize, usize)> {
    let path = root.join("assets/png-bench/current-nupic-0.5").join(fname);
    let png = fs::read(&path).with_context(|| format!("read {path:?}"))?;
    let total_png = png.len();
    let chunks = parse_chunks(&png)?;
    let idat = idat_concat(&chunks);
    let old_idat = idat.len();

    // Decompress the IDAT bytes — it's a zlib stream (CMF/FLG + DEFLATE + Adler).
    let mut decoder = ZlibDecoder::new(idat.as_slice());
    let mut filtered_rows = Vec::new();
    decoder.read_to_end(&mut filtered_rows)?;

    // Recompress with nupic-deflate (zlib wrapper).
    let new_idat = nupic_deflate::zlib_compress(&filtered_rows);
    let new_idat_len = new_idat.len();

    // Theoretical new total = total - old_idat + new_idat (chunk overhead
    // is the same). Reported via sum_old / sum_new in the caller.
    let _ = total_png;
    let _ = chunk_overhead;

    // Bonus: try nupic-deflate at Level::Fast too, for comparison.
    let new_idat_fast = deflate_level(&filtered_rows, Level::Fast);

    Ok((fname.to_string(), total_png, old_idat, new_idat_len, new_idat_fast.len()))
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
    println!(
        "{:<32} {:>10} {:>10} {:>10} {:>10}   {:>7} {:>7}",
        "fixture", "png_total", "old_IDAT", "nupic_B", "nupic_F", "B/old", "F/old"
    );
    println!("{}", "-".repeat(96));

    let (mut sum_old, mut sum_new) = (0usize, 0usize);
    for fname in fixtures {
        match process(&root, fname) {
            Ok((name, total, old, new_b, new_f)) => {
                let r_b = new_b as f64 / old as f64;
                let r_f = new_f as f64 / old as f64;
                println!(
                    "{:<32} {:>10} {:>10} {:>10} {:>10}   {:>6.2}× {:>6.2}×",
                    name, total, old, new_b, new_f, r_b, r_f
                );
                sum_old += old;
                sum_new += new_b;
            }
            Err(e) => println!("{:<32} ERROR: {e}", fname),
        }
    }
    println!("{}", "-".repeat(96));
    println!(
        "{:<32} {:>10} {:>10} {:>10}              {:>6.2}×",
        "TOTAL",
        "",
        sum_old,
        sum_new,
        sum_new as f64 / sum_old.max(1) as f64,
    );
    Ok(())
}
