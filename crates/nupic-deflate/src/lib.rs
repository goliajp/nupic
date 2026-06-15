//! `nupic-deflate` — self-built DEFLATE encoder (RFC 1951) + zlib
//! wrapper (RFC 1950).
//!
//! Stage 1 of the [PNG codec roadmap](../../docs/roadmap.md);see the
//! `docs/research/png/06-nupic-deflate-design.md` essay for the phase
//! plan. Currently phase 1.4 (zopfli-class iterative refinement — full
//! stage-1 graduation across the 7-input corpus):
//!
//! - **Stored blocks**(BTYPE=00)— infrastructure, no compression.
//!   Output is `~1.0005 ×` raw(per-block 5-byte header). Phase 1.0.0.
//! - **Greedy LZ77 + static Huffman**(BTYPE=01)— `zlib level 1`
//!   class on text / repeats. Phase 1.0.1. Retained as [`Level::Fast`].
//! - **Dynamic Huffman per block**(BTYPE=10)— frequency-tuned
//!   canonical Huffman tree (length-limited 15) per block, RFC 1951
//!   §3.2.7 header. Phase 1.0.2.
//! - **Lazy LZ77 + best-of chooser** — defer each match by one byte to
//!   see if `i+1` offers a strictly longer match;chain depth 128 (vs
//!   greedy's 32). Phase 1.1. Matches `zlib level 9` size on most
//!   workloads, beats it on PNG IDAT streams.
//! - **Multi-block split** — partition the token stream into 1 / 2 / 4
//!   / 8 equal-sized blocks and pick the partition with smallest total
//!   encoded size. Each block independently picks static-vs-dynamic
//!   Huffman. Phase 1.2 (current default `Level::Best`). Strictly
//!   beats `zlib level 9` size on heterogeneous structured text
//!   (Cargo.lock, source files); equals or beats it on prose and PNG.
//! - **Graduation polish** — quickcheck-fuzzed roundtrip + zopfli
//!   absolute-ceiling bench. 5/7 corpus inputs ≤ 1.05× zopfli (stage 1
//!   graduation criterion). Phase 1.3 — see
//!   `docs/research/png/06-seven-deflate-graduation.md`. Fixes a
//!   stored-fallback bit-cost under-count caught by the new fuzz.
//! - **Iterative cost-DP LZ77** — variable-position block split (phase
//!   1.4a) + 5-pass forward-DP token search with Huffman code-length
//!   cost feedback (phase 1.4b, zopfli core trick). Closes cargo-lock
//!   from 1.14× zopfli → 1.01×;**all 7 corpus inputs ≤ 1.05× zopfli**
//!   (full graduation). PNG IDAT corpus shrinks from 1.08× to **1.04×
//!   oxipng**(libdeflate near-optimal). Phase 1.4 — see
//!   `docs/research/png/06-nine-deflate-iterative.md`.
//!
//! Round-trips through `flate2` / `miniz_oxide` are validated by both
//! scenario tests (canonical inputs at every level) and quickcheck
//! property fuzz (arbitrary byte sequences) in the test suite.
//!
//! # Examples
//!
//! ```
//! use nupic_deflate::zlib_compress;
//! let raw = b"Hello, world!".to_vec();
//! let z = zlib_compress(&raw);
//! assert!(!z.is_empty());
//! assert_eq!(z[0], 0x78); // zlib CMF byte
//! ```

#![allow(clippy::inline_always)]

mod huffman;
mod lz77;
mod tables;

use nupic_bits::{BitWriter, adler32_update};

/// Maximum bytes per stored (uncompressed) DEFLATE block (RFC 1951 §3.2.4).
const STORED_MAX: usize = 65_535;

/// Compression level for `deflate` / `zlib_compress`.
#[derive(Clone, Copy, Debug, Default)]
pub enum Level {
    /// Stored blocks only — no compression, but valid DEFLATE. Output
    /// is `~ 1.0005 × len(data)`. Phase 1.0.0.
    Stored,
    /// Greedy LZ77 + **static** Huffman, single block. Phase 1.0.1.
    /// Output is `~ zlib level 1` class on text;same as
    /// [`Level::Stored`] on incompressible random data.
    Fast,
    /// **Lazy** LZ77 (chain depth 128, lazy threshold 16) + **multi-
    /// block DEFLATE** (1/2/4/8 equal-token partitions, per-block
    /// static-vs-dynamic format chooser) + whole-call stored fallback.
    /// Phase 1.2 default. Strictly beats `zlib level 9` size on
    /// heterogeneous structured text (Cargo.lock, source files);
    /// equals it on natural-language prose and PNG IDAT streams.
    #[default]
    Best,
}

/// One-shot encode at the default level(currently [`Level::Best`]).
#[must_use]
pub fn deflate(data: &[u8]) -> Vec<u8> {
    deflate_level(data, Level::default())
}

/// One-shot encode at a specific level.
#[must_use]
pub fn deflate_level(data: &[u8], level: Level) -> Vec<u8> {
    match level {
        Level::Stored => deflate_stored(data),
        Level::Fast => lz77::deflate_static(data),
        Level::Best => lz77::deflate_best(data),
    }
}

/// Encode `data` as a RFC 1951 DEFLATE bitstream using **stored
/// blocks only** — no compression. Public to support
/// `Level::Stored` and unit tests.
#[must_use]
pub fn deflate_stored(data: &[u8]) -> Vec<u8> {
    let mut w = BitWriter::with_capacity(data.len() + data.len() / 1024 + 16);
    let n = data.len();
    let mut written = 0;
    while written < n || (n == 0 && written == 0) {
        let chunk_len = (n - written).min(STORED_MAX);
        let bfinal = if written + chunk_len == n { 1u32 } else { 0 };
        // Header bits (LSB-first within the byte, per DEFLATE convention):
        //   BFINAL    1 bit
        //   BTYPE=00  2 bits
        w.write_bits(bfinal, 1);
        w.write_bits(0b00, 2);
        // Align to byte boundary before LEN / NLEN per RFC 1951 §3.2.4
        w.align_to_byte();
        // 16-bit LEN (little-endian by spec — and zlib convention), 16-bit NLEN
        let chunk_len_u16 = chunk_len as u16;
        let nlen = !chunk_len_u16;
        w.write_bits(u32::from(chunk_len_u16) & 0xff, 8);
        w.write_bits((u32::from(chunk_len_u16) >> 8) & 0xff, 8);
        w.write_bits(u32::from(nlen) & 0xff, 8);
        w.write_bits((u32::from(nlen) >> 8) & 0xff, 8);
        // Raw bytes
        if chunk_len > 0 {
            for &b in &data[written..written + chunk_len] {
                w.write_bits(u32::from(b), 8);
            }
        }
        written += chunk_len;
        if n == 0 {
            break;
        }
    }
    w.into_bytes()
}

/// Encode `data` as a RFC 1950 zlib stream:
///
///   [CMF byte][FLG byte][DEFLATE bytes][Adler-32 big-endian]
#[must_use]
pub fn zlib_compress(data: &[u8]) -> Vec<u8> {
    // CMF = CM=8 (DEFLATE) + CINFO=7 (32 KiB window) = 0x78
    const CMF: u8 = 0x78;
    const FLG: u8 = 0x01; // FLEVEL=0, no dict, FCHECK adjusted below

    let cmf = CMF as u16;
    let mut flg = FLG as u16;
    let header = cmf * 256 + flg;
    if header % 31 != 0 {
        let need = (31 - (header % 31)) % 31;
        flg |= need;
    }

    let deflated = deflate_level(data, Level::default());
    let adler = adler32_update(data, 1);

    let mut out = Vec::with_capacity(deflated.len() + 6);
    out.push(CMF);
    out.push(flg as u8);
    out.extend_from_slice(&deflated);
    out.extend_from_slice(&adler.to_be_bytes());
    out
}
