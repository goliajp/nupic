//! Phase 1.0.1 sanity check: compare nupic-deflate Fast vs zlib
//! level 1 / level 6 / level 9 + libdeflate-class via miniz_oxide.
//!
//! Backs `docs/research/png/06-ter-deflate-lz77.md`. Run:
//!   cargo run --release -p nupic-research --example deflate_compare

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use flate2::Compression;
use flate2::write::DeflateEncoder;
use nupic_deflate::{Level, deflate_level};
use std::io::Write;

fn main() -> Result<()> {
    let root = workspace_root()?;

    // Inputs: a small text payload, a repeats payload, a PNG IDAT-ish
    // payload from our fixtures, and a random payload.
    let mut inputs: Vec<(&str, Vec<u8>)> = Vec::new();
    inputs.push(("repeats-10k", vec![0x42u8; 10_000]));
    inputs.push(("text-9k", {
        let phrase = b"the quick brown fox jumps over the lazy dog. ";
        let mut buf = Vec::with_capacity(phrase.len() * 200);
        for _ in 0..200 { buf.extend_from_slice(phrase); }
        buf
    }));
    inputs.push(("random-8k", {
        let mut s = 0x12345678u64;
        let mut data = Vec::with_capacity(8192);
        for _ in 0..8192 {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            data.push((s >> 32) as u8);
        }
        data
    }));
    let pluto = fs::read(root.join("assets/png-bench/inputs/02-pluto-transparent.png")).ok();
    if let Some(b) = pluto {
        inputs.push(("02-pluto-png-stream", b));
    }

    println!("{:<22} {:>10}  {:>10} {:>10} {:>10} {:>10} {:>10} {:>10}",
             "input", "raw", "nupic_F", "zl_1", "zl_6", "zl_9", "vs_zl_1", "vs_zl_9");
    println!("{}", "-".repeat(96));

    for (name, data) in &inputs {
        let raw_len = data.len();

        let t = Instant::now();
        let nupic_fast = deflate_level(data, Level::Fast);
        let nupic_ms = t.elapsed().as_secs_f64() * 1000.0;

        let zl1 = compress_zlib(data, 1);
        let zl6 = compress_zlib(data, 6);
        let zl9 = compress_zlib(data, 9);

        let nupic = nupic_fast.len();
        let r_zl1 = nupic as f64 / zl1.len() as f64;
        let r_zl9 = nupic as f64 / zl9.len() as f64;
        println!("{:<22} {:>10}  {:>10} {:>10} {:>10} {:>10} {:>9.2}× {:>9.2}×",
                 name, raw_len, nupic, zl1.len(), zl6.len(), zl9.len(),
                 r_zl1, r_zl9);
        let _ = nupic_ms; // (timing minor relative to compression ratio)
    }
    Ok(())
}

fn compress_zlib(data: &[u8], level: u32) -> Vec<u8> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::new(level));
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(Path::new(m).ancestors().nth(2).context("workspace root")?.to_path_buf())
}
