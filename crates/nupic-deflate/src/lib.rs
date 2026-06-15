//! `nupic-deflate` — self-built DEFLATE encoder (RFC 1951) + zlib
//! wrapper (RFC 1950).
//!
//! Stage 1 of the [PNG codec roadmap](../../docs/roadmap.md);see the
//! `docs/research/png/06-nupic-deflate-design.md` essay for the phase
//! plan. Currently phase 1.0:
//!
//! - **Stored blocks**(BTYPE=00)— infrastructure only, no compression.
//!   Output is `~1.0005 ×` the raw input(per-block 5-byte header).
//! - LZ77 + static Huffman(phase 1.0.1)— planned follow-up
//!   sub-essay 06-bis-ter
//!
//! Public surface targets the eventual full encoder; today only
//! [`deflate`] and [`zlib_compress`] are productive. Round-trips through
//! `flate2` / `miniz_oxide` / `zlib` are validated in the test suite.
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

use nupic_bits::{BitWriter, adler32_update};

/// Maximum bytes per stored (uncompressed) DEFLATE block (RFC 1951 §3.2.4).
const STORED_MAX: usize = 65_535;

/// Encode `data` as a RFC 1951 DEFLATE bitstream (no zlib wrapper).
///
/// Phase 1.0 implementation: one or more **stored blocks**. Output is
/// approximately `1.0005 × len(data) + 5` bytes (5-byte block header
/// every 65 535 bytes of payload).
#[must_use]
pub fn deflate(data: &[u8]) -> Vec<u8> {
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
            // BitWriter is byte-aligned now — push directly.
            let bytes_so_far = w.bit_len() / 8;
            let _ = bytes_so_far; // (silence: used only as a sanity check below)
            // Append via repeated 8-bit writes (BitWriter doesn't expose a
            // direct slice extension; phase 1.1 will add one).
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
    // FLG = (FCHECK | FDICT=0 | FLEVEL=0 (fastest))
    //   FCHECK chosen so (CMF*256 + FLG) % 31 == 0.
    // (0x78 * 256 + FLG) % 31 == 0  →  FLG = (31 - (0x78 * 256) % 31) % 31
    //   0x78 * 256 = 30720; 30720 % 31 = 12; FLG = 19 = 0x13
    // The "FLEVEL=0" choice is the convention for "fastest". flate2's
    // miniz_oxide accepts any valid FLG.
    const FLG: u8 = 0x01; // FLEVEL=0, no dict, FCHECK adjusted

    // Recompute FCHECK lazily to be robust if we change FLEVEL later.
    let cmf = CMF as u16;
    let mut flg = FLG as u16;
    let header = cmf * 256 + flg;
    if header % 31 != 0 {
        let need = (31 - (header % 31)) % 31;
        flg |= need;
    }

    let deflated = deflate(data);
    let adler = adler32_update(data, 1);

    let mut out = Vec::with_capacity(deflated.len() + 6);
    out.push(CMF);
    out.push(flg as u8);
    out.extend_from_slice(&deflated);
    out.extend_from_slice(&adler.to_be_bytes());
    out
}
