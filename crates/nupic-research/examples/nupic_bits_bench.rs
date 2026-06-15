//! Stage-0 bench:CRC-32 + Adler-32 throughput vs the established
//! Rust references (`crc32fast` / `adler32` crate).
//!
//! Backs `docs/research/png/05-nupic-bits-stage-0.md`. 1 MB random
//! buffer, median of 11 runs.
//!
//! Run:
//!   cargo run --release -p nupic-research --example nupic_bits_bench

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use nupic_bits::{adler32, crc32};

const OUT_DIR: &str = "target/research-out";

fn main() -> Result<()> {
    let root = workspace_root()?;
    let out_dir = root.join(OUT_DIR);
    fs::create_dir_all(&out_dir)?;

    let mut buf = vec![0u8; 1 << 20]; // 1 MiB
    let mut s = 0xC0DEu64;
    for byte in &mut buf {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *byte = (s >> 32) as u8;
    }

    println!("== CRC-32 (1 MiB) ==");
    let nupic_crc = bench(11, || crc32(&buf));
    let cement_crc = bench(11, || crc32fast::hash(&buf));
    println!("  nupic-bits   : {:>7.3} ms / {:>5.2} GB/s, value = 0x{:08X}",
             nupic_crc.0, gbps(buf.len(), nupic_crc.0), nupic_crc.1);
    println!("  crc32fast    : {:>7.3} ms / {:>5.2} GB/s, value = 0x{:08X}",
             cement_crc.0, gbps(buf.len(), cement_crc.0), cement_crc.1);
    println!("  ratio nupic/cement: {:.2}×", nupic_crc.0 / cement_crc.0);
    assert_eq!(nupic_crc.1, cement_crc.1, "CRC values diverge");

    println!("\n== Adler-32 (1 MiB) ==");
    let nupic_adler = bench(11, || adler32(&buf));
    let cement_adler = bench(11, || {
        let mut a = adler32::RollingAdler32::new();
        a.update_buffer(&buf);
        a.hash()
    });
    println!("  nupic-bits   : {:>7.3} ms / {:>5.2} GB/s, value = 0x{:08X}",
             nupic_adler.0, gbps(buf.len(), nupic_adler.0), nupic_adler.1);
    println!("  adler32      : {:>7.3} ms / {:>5.2} GB/s, value = 0x{:08X}",
             cement_adler.0, gbps(buf.len(), cement_adler.0), cement_adler.1);
    println!("  ratio nupic/cement: {:.2}×", nupic_adler.0 / cement_adler.0);
    assert_eq!(nupic_adler.1, cement_adler.1, "Adler values diverge");

    // also write a small csv
    let csv = format!(
        "metric,nupic_ms,cement_ms,nupic_gbps,cement_gbps,nupic_value,cement_value\n\
         crc32,{:.3},{:.3},{:.3},{:.3},{},{}\n\
         adler32,{:.3},{:.3},{:.3},{:.3},{},{}\n",
        nupic_crc.0, cement_crc.0,
        gbps(buf.len(), nupic_crc.0), gbps(buf.len(), cement_crc.0),
        nupic_crc.1, cement_crc.1,
        nupic_adler.0, cement_adler.0,
        gbps(buf.len(), nupic_adler.0), gbps(buf.len(), cement_adler.0),
        nupic_adler.1, cement_adler.1,
    );
    fs::write(out_dir.join("05-nupic-bits-bench.csv"), csv)?;
    println!("\nwrote target/research-out/05-nupic-bits-bench.csv");
    Ok(())
}

fn bench<T: PartialEq + Copy>(runs: usize, mut f: impl FnMut() -> T) -> (f64, T) {
    let mut value = f();
    let mut times = Vec::with_capacity(runs);
    for _ in 0..runs {
        let t0 = Instant::now();
        value = f();
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (times[times.len() / 2], value)
}

fn gbps(bytes: usize, ms: f64) -> f64 {
    (bytes as f64) / (ms / 1000.0) / 1e9
}

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(Path::new(m).ancestors().nth(2).context("workspace root")?.to_path_buf())
}
