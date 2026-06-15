//! `nupic-bits` — stage-0 stone for the self-built PNG / DEFLATE
//! codec roadmap.
//!
//! Three primitives:
//!
//! - [`crc32`] — IEEE 802.3 polynomial 0xEDB88320, slice-by-8 inner
//!   loop. Bit-exact equal to `zlib` / `crc32fast` on every RFC 1952
//!   fixture.
//! - [`adler32`] — RFC 1950 Adler-32 with the running-modulo trick.
//!   Bit-exact equal to `adler32` crate / `zlib`.
//! - [`BitReader`] / [`BitWriter`] — LSB-first(DEFLATE convention)
//!   bit-level streams over `Vec<u8>` / `&[u8]`.
//!
//! **Zero runtime dependencies**. Dev-deps `crc32fast` / `adler32`
//! are used as oracles in the test suite.

#![cfg_attr(not(test), no_std)]
#![allow(clippy::inline_always)]

#[cfg(not(test))]
extern crate alloc;
#[cfg(not(test))]
use alloc::vec::Vec;

// =====================================================================
//                              CRC-32
// =====================================================================
//
// IEEE 802.3 polynomial 0xEDB88320 (reflected); identical to the
// `gzip`, `PNG`, `zlib`, `RFC 1952` CRC. We build the 256-entry table
// at compile time, then run slice-by-8 (8 bytes per inner iter, table
// of 8 × 256 entries) for the hot loop. Tail bytes fall back to the
// 1-byte-at-a-time path.
//
// The slice-by-N table layout is the standard "Sarwate + Brumme"
// trick — each row k holds CRC(0xff..00 at offset k). At lookup time
// the 8 table reads can be done in parallel by the CPU.

/// 256-entry CRC-32 table for the byte-at-a-time path.
pub const CRC32_TABLE: [u32; 256] = build_crc32_table();

const fn build_crc32_table() -> [u32; 256] {
    let mut tbl = [0u32; 256];
    let mut n = 0u32;
    while n < 256 {
        let mut c = n;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB88320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        tbl[n as usize] = c;
        n += 1;
    }
    tbl
}

/// Slice-by-8 table — 8 rows × 256 entries.
pub const CRC32_TABLE_SLICE8: [[u32; 256]; 8] = build_crc32_table_slice8();

const fn build_crc32_table_slice8() -> [[u32; 256]; 8] {
    let base = CRC32_TABLE;
    let mut out = [[0u32; 256]; 8];
    let mut i = 0;
    while i < 256 {
        out[0][i] = base[i];
        i += 1;
    }
    let mut row = 1;
    while row < 8 {
        let mut i = 0;
        while i < 256 {
            let prev = out[row - 1][i];
            out[row][i] = (prev >> 8) ^ base[(prev & 0xff) as usize];
            i += 1;
        }
        row += 1;
    }
    out
}

/// Compute the CRC-32 of `data` starting from the initial value `init`
/// (use `0` to start fresh).
///
/// The standard PNG / gzip CRC is `crc32(data, 0) ^ 0xFFFFFFFF` after
/// flipping bits; we follow zlib's convention and have the caller
/// XOR-with-all-ones on entry and exit (see [`crc32`]).
#[inline]
#[must_use]
pub fn crc32_update(data: &[u8], init: u32) -> u32 {
    let mut crc = init;
    let mut i = 0;
    let n = data.len();
    // slice-by-8 over the bulk
    while i + 8 <= n {
        let b0 = data[i] as u32;
        let b1 = data[i + 1] as u32;
        let b2 = data[i + 2] as u32;
        let b3 = data[i + 3] as u32;
        let one = crc ^ ((b3 << 24) | (b2 << 16) | (b1 << 8) | b0);
        let b4 = data[i + 4] as usize;
        let b5 = data[i + 5] as usize;
        let b6 = data[i + 6] as usize;
        let b7 = data[i + 7] as usize;
        crc = CRC32_TABLE_SLICE8[0][b7]
            ^ CRC32_TABLE_SLICE8[1][b6]
            ^ CRC32_TABLE_SLICE8[2][b5]
            ^ CRC32_TABLE_SLICE8[3][b4]
            ^ CRC32_TABLE_SLICE8[4][((one >> 24) & 0xff) as usize]
            ^ CRC32_TABLE_SLICE8[5][((one >> 16) & 0xff) as usize]
            ^ CRC32_TABLE_SLICE8[6][((one >> 8) & 0xff) as usize]
            ^ CRC32_TABLE_SLICE8[7][(one & 0xff) as usize];
        i += 8;
    }
    // tail
    while i < n {
        crc = (crc >> 8) ^ CRC32_TABLE[((crc ^ data[i] as u32) & 0xff) as usize];
        i += 1;
    }
    crc
}

/// Compute the standard CRC-32 over `data` (PNG / gzip / zlib /
/// RFC 1952 convention).
///
/// ```
/// use nupic_bits::crc32;
/// assert_eq!(crc32(b""), 0);
/// assert_eq!(crc32(b"123456789"), 0xCBF43926);
/// ```
#[inline]
#[must_use]
pub fn crc32(data: &[u8]) -> u32 {
    !crc32_update(data, !0u32)
}

// =====================================================================
//                              Adler-32
// =====================================================================
//
// RFC 1950 Adler-32: a = 1 mod 65521; b runs sum of a's, mod 65521.
// We can defer the modulo for up to NMAX = 5552 bytes (per RFC), then
// take the modulo. Faster than per-byte mod by ~20×.

const MOD_ADLER: u32 = 65_521;
const NMAX: usize = 5_552;

/// Update Adler-32 with `data` starting from `init`. Use `1` for a
/// fresh sum (per RFC 1950).
#[inline]
#[must_use]
pub fn adler32_update(data: &[u8], init: u32) -> u32 {
    let mut a = init & 0xFFFF;
    let mut b = init >> 16;
    let mut data = data;
    while !data.is_empty() {
        let chunk_len = data.len().min(NMAX);
        let (chunk, rest) = data.split_at(chunk_len);
        for &byte in chunk {
            a = a.wrapping_add(byte as u32);
            b = b.wrapping_add(a);
        }
        a %= MOD_ADLER;
        b %= MOD_ADLER;
        data = rest;
    }
    (b << 16) | a
}

/// Compute Adler-32 over `data` per RFC 1950.
///
/// ```
/// use nupic_bits::adler32;
/// assert_eq!(adler32(b""), 1);
/// assert_eq!(adler32(b"abcdefgh"), 0x0E000325);
/// ```
#[inline]
#[must_use]
pub fn adler32(data: &[u8]) -> u32 {
    adler32_update(data, 1)
}

// =====================================================================
//                       Bit I/O (LSB-first, DEFLATE)
// =====================================================================
//
// DEFLATE packs bits LSB-first within a byte: bit 0 of the first byte
// is the first bit written. We expose `BitReader<'a>` and `BitWriter`
// which both operate on a byte buffer and a bit cursor.

/// LSB-first bit reader over a byte slice. Cursor advances on each
/// `read_bits` call; `Err(())` on EOF (sentinel — caller can map to
/// a richer type).
pub struct BitReader<'a> {
    data: &'a [u8],
    /// Bit offset from the start of `data` (i.e. cursor in bits).
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    #[inline]
    #[must_use]
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, bit_pos: 0 }
    }

    /// Read `n` (1..=32) bits LSB-first into a `u32`. `Err(())` if
    /// past end of buffer.
    #[inline]
    pub fn read_bits(&mut self, n: u8) -> Result<u32, ()> {
        debug_assert!(n >= 1 && n <= 32);
        let n = n as usize;
        let bit_end = self.bit_pos + n;
        let byte_end = (bit_end + 7) / 8;
        if byte_end > self.data.len() {
            return Err(());
        }
        let mut value = 0u32;
        let mut filled = 0;
        while filled < n {
            let byte_idx = (self.bit_pos + filled) / 8;
            let bit_off = (self.bit_pos + filled) % 8;
            let take = (n - filled).min(8 - bit_off);
            // mask via u16 so `take == 8` doesn't overflow `1u8 << 8`.
            let mask = ((1u16 << take) - 1) as u8;
            let chunk = (self.data[byte_idx] >> bit_off) & mask;
            value |= (chunk as u32) << filled;
            filled += take;
        }
        self.bit_pos += n;
        Ok(value)
    }

    /// Skip bits forward without reading.
    #[inline]
    pub fn skip_bits(&mut self, n: usize) -> Result<(), ()> {
        if self.bit_pos + n > self.data.len() * 8 {
            return Err(());
        }
        self.bit_pos += n;
        Ok(())
    }

    /// Cursor in bits.
    #[inline]
    #[must_use]
    pub fn bit_position(&self) -> usize {
        self.bit_pos
    }

    /// Total bits in the underlying buffer.
    #[inline]
    #[must_use]
    pub fn bit_len(&self) -> usize {
        self.data.len() * 8
    }
}

/// LSB-first bit writer over a growable buffer.
pub struct BitWriter {
    buf: Vec<u8>,
    /// Bit offset *within the current last byte* (0..8). When 0, the
    /// last byte is complete and the next bit starts a fresh byte.
    bit_off: u8,
}

impl Default for BitWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl BitWriter {
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            bit_off: 0,
        }
    }

    #[inline]
    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            buf: Vec::with_capacity(cap),
            bit_off: 0,
        }
    }

    /// Write `n` (1..=32) bits LSB-first from `value`.
    #[inline]
    pub fn write_bits(&mut self, value: u32, n: u8) {
        debug_assert!(n >= 1 && n <= 32);
        let mut remaining = n as usize;
        let mut shift = 0;
        while remaining > 0 {
            if self.bit_off == 0 {
                self.buf.push(0);
            }
            let last = self.buf.last_mut().expect("just pushed");
            let take = remaining.min(8 - self.bit_off as usize);
            let chunk = ((value >> shift) & ((1u32 << take) - 1)) as u8;
            *last |= chunk << self.bit_off;
            self.bit_off = (self.bit_off + take as u8) & 7;
            shift += take;
            remaining -= take;
        }
    }

    /// Pad the current byte with zero bits so the next write starts on
    /// a byte boundary.
    #[inline]
    pub fn align_to_byte(&mut self) {
        self.bit_off = 0;
    }

    /// Consume self, returning the byte buffer.
    #[inline]
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    /// Borrow the byte buffer (e.g. for inspection mid-stream).
    #[inline]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    /// Total bits written so far.
    #[inline]
    #[must_use]
    pub fn bit_len(&self) -> usize {
        if self.buf.is_empty() {
            0
        } else {
            (self.buf.len() - 1) * 8 + self.bit_off as usize
                + if self.bit_off == 0 { 8 } else { 0 }
        }
    }
}
