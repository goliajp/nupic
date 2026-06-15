//! Property + oracle tests for `nupic-bits`. Tests the **contract**
//! of the three primitives:
//!
//! - CRC-32 matches `crc32fast` (which matches zlib bit-exactly) on
//!   random byte sequences
//! - Adler-32 matches the `adler32` crate on random byte sequences
//! - BitReader / BitWriter round-trip every bit width 1..=32 over
//!   randomised streams

use nupic_bits::{BitReader, BitWriter, adler32, adler32_update, crc32, crc32_update};

// --- CRC-32 -----------------------------------------------------------

#[test]
fn crc32_rfc1952_fixtures() {
    assert_eq!(crc32(b""), 0);
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b"a"), 0xE8B7_BE43);
    assert_eq!(crc32(b"abc"), 0x3524_41C2);
}

#[test]
fn crc32_matches_crc32fast_on_random_lengths() {
    let mut s = 0xC0DEu64;
    for len in 0..600usize {
        // simple LCG for deterministic content
        let mut data = Vec::with_capacity(len);
        for _ in 0..len {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            data.push((s >> 32) as u8);
        }
        let ours = crc32(&data);
        let theirs = crc32fast::hash(&data);
        assert_eq!(ours, theirs, "CRC mismatch at len {len}");
    }
}

#[test]
fn crc32_incremental_update_matches_one_shot() {
    let mut s = 0xDEAD_BEEFu64;
    let mut data = Vec::with_capacity(4096);
    for _ in 0..4096 {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        data.push((s >> 32) as u8);
    }
    let one_shot = crc32(&data);

    // Split at multiple points and verify the incremental path agrees.
    for split in [1usize, 7, 64, 1023, 4095] {
        let (head, tail) = data.split_at(split);
        let acc_head = crc32_update(head, !0u32);
        let acc_full = !crc32_update(tail, acc_head);
        assert_eq!(acc_full, one_shot, "split={split}");
    }
}

// --- Adler-32 ---------------------------------------------------------

#[test]
fn adler32_rfc1950_fixtures() {
    assert_eq!(adler32(b""), 1);
    assert_eq!(adler32(b"a"), 0x0062_0062);
    assert_eq!(adler32(b"abc"), 0x024D_0127);
    assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
}

#[test]
fn adler32_matches_adler32_crate_on_random_lengths() {
    let mut s = 0xBEEFu64;
    for len in 0..600usize {
        let mut data = Vec::with_capacity(len);
        for _ in 0..len {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            data.push((s >> 32) as u8);
        }
        let ours = adler32(&data);
        let mut state = adler32::RollingAdler32::new();
        state.update_buffer(&data);
        let theirs = state.hash();
        assert_eq!(ours, theirs, "Adler-32 mismatch at len {len}");
    }
}

#[test]
fn adler32_incremental_update_matches_one_shot() {
    let mut s = 0xFA_CAD_E5u64;
    let mut data = Vec::with_capacity(8192);
    for _ in 0..8192 {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        data.push((s >> 32) as u8);
    }
    let one_shot = adler32(&data);

    for split in [1usize, 100, NMAX_BOUNDARY, 5_553, 8_191] {
        let (head, tail) = data.split_at(split.min(data.len()));
        let acc = adler32_update(head, 1);
        let acc_full = adler32_update(tail, acc);
        assert_eq!(acc_full, one_shot, "split={split}");
    }
}

const NMAX_BOUNDARY: usize = 5_552; // RFC 1950 chunk size

// --- Bit I/O ----------------------------------------------------------

#[test]
fn bit_io_round_trip_every_width() {
    // For each width 1..=32, write a sequence of values, read them back,
    // and assert per-value equality.
    for width in 1u8..=32 {
        let mask: u64 = if width == 32 { 0xFFFF_FFFF } else { (1u64 << width) - 1 };
        let mut values = Vec::new();
        let mut w = BitWriter::new();
        let mut s = 0xCAFEu64;
        let n_values = 257usize; // ensure we cross multiple byte boundaries
        for _ in 0..n_values {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let v = (s & mask) as u32;
            values.push(v);
            w.write_bits(v, width);
        }
        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        for (i, &expected) in values.iter().enumerate() {
            let got = r.read_bits(width).expect("eof");
            assert_eq!(got, expected, "width={width} idx={i}");
        }
    }
}

#[test]
fn bit_writer_aligns_to_byte() {
    let mut w = BitWriter::new();
    w.write_bits(0b1, 1);
    w.write_bits(0b10, 2);
    w.align_to_byte();
    w.write_bits(0xFF, 8);
    let bytes = w.into_bytes();
    assert_eq!(bytes.len(), 2, "expected 2 bytes, got {bytes:?}");
    assert_eq!(bytes[0] & 0b111, 0b101);
    assert_eq!(bytes[1], 0xFF);
}

#[test]
fn bit_reader_returns_err_on_eof() {
    let bytes = [0xAA, 0x55];
    let mut r = BitReader::new(&bytes);
    let _ = r.read_bits(16).unwrap();
    assert!(r.read_bits(1).is_err(), "should be at EOF");
}

#[test]
fn bit_reader_lsb_first() {
    // LSB-first means the first written bit is the LSB of byte 0.
    let mut w = BitWriter::new();
    w.write_bits(0b1, 1);
    w.write_bits(0b1, 1);
    w.write_bits(0b0, 1);
    w.write_bits(0b1, 1);
    let bytes = w.into_bytes();
    assert_eq!(bytes[0] & 0xF, 0b1011, "LSB-first bit layout broken");
}

#[test]
fn bit_writer_length_tracking_basic() {
    let mut w = BitWriter::new();
    assert_eq!(w.bit_len(), 0);
    w.write_bits(0, 5);
    assert_eq!(w.bit_len(), 5);
    w.write_bits(0, 3);
    assert_eq!(w.bit_len(), 8);
    w.write_bits(0, 12);
    assert_eq!(w.bit_len(), 20);
}
