//! Phase 1.0.2 sanity check: compare nupic-deflate Fast (static
//! Huffman, phase 1.0.1) and Best (best of {stored, static, dynamic},
//! phase 1.0.2) against zlib levels 1 / 6 / 9.
//!
//! Backs `docs/research/png/06-quater-deflate-dynamic.md`. Run:
//!   cargo run --release -p nupic-research --example deflate_compare

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use flate2::Compression;
use flate2::write::DeflateEncoder;
use nupic_deflate::{Level, deflate_level};
use std::io::Write;

fn main() -> Result<()> {
    let root = workspace_root()?;

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
    // English prose payload — exercises dynamic Huffman more than the
    // 45-byte phrase repeat (which has trivially low entropy).
    let lorem = b"Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do \
eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim \
veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo \
consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum \
dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, \
sunt in culpa qui officia deserunt mollit anim id est laborum.";
    inputs.push(("lorem-prose", {
        let mut buf = Vec::with_capacity(lorem.len() * 20);
        for _ in 0..20 { buf.extend_from_slice(lorem); }
        buf
    }));

    println!(
        "{:<22} {:>10}  {:>10} {:>10}  {:>10} {:>10} {:>10}   {:>8} {:>8} {:>8} {:>8}",
        "input", "raw",
        "nupic_F", "nupic_B",
        "zl_1", "zl_6", "zl_9",
        "B/zl1", "B/zl6", "B/zl9", "B/F",
    );
    println!("{}", "-".repeat(124));

    for (name, data) in &inputs {
        let raw_len = data.len();

        let nupic_fast = deflate_level(data, Level::Fast);
        let nupic_best = deflate_level(data, Level::Best);

        let zl1 = compress_zlib(data, 1);
        let zl6 = compress_zlib(data, 6);
        let zl9 = compress_zlib(data, 9);

        let f = nupic_fast.len();
        let b = nupic_best.len();
        let r_z1 = b as f64 / zl1.len() as f64;
        let r_z6 = b as f64 / zl6.len() as f64;
        let r_z9 = b as f64 / zl9.len() as f64;
        let r_bf = b as f64 / f as f64;
        println!(
            "{:<22} {:>10}  {:>10} {:>10}  {:>10} {:>10} {:>10}   {:>7.2}× {:>7.2}× {:>7.2}× {:>7.2}×",
            name, raw_len, f, b, zl1.len(), zl6.len(), zl9.len(), r_z1, r_z6, r_z9, r_bf,
        );
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
