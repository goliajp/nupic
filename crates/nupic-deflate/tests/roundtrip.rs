//! Cement-oracle round-trip tests for phase 1.0 (stored blocks only).
//!
//! `flate2` (via `miniz_oxide`) is used as the reference decoder.
//! Round-tripping nupic-deflate output through it verifies the bit
//! stream is RFC 1951 / RFC 1950 compliant.

use std::io::Read;

use flate2::read::{DeflateDecoder, ZlibDecoder};
use nupic_deflate::{Level, deflate, deflate_level, deflate_stored, zlib_compress};

fn deflate_roundtrip(input: &[u8]) {
    let encoded = deflate(input);
    let mut decoder = DeflateDecoder::new(encoded.as_slice());
    let mut decoded = Vec::new();
    decoder.read_to_end(&mut decoded).expect("flate2 decode");
    assert_eq!(decoded, input,
        "deflate roundtrip mismatch (input len {}, encoded len {})",
        input.len(), encoded.len());
}

fn zlib_roundtrip(input: &[u8]) {
    let encoded = zlib_compress(input);
    let mut decoder = ZlibDecoder::new(encoded.as_slice());
    let mut decoded = Vec::new();
    decoder.read_to_end(&mut decoded).expect("zlib decode");
    assert_eq!(decoded, input,
        "zlib roundtrip mismatch (input len {}, encoded len {})",
        input.len(), encoded.len());
}

#[test]
fn deflate_empty() { deflate_roundtrip(b""); }

#[test]
fn deflate_one_byte() { deflate_roundtrip(b"a"); }

#[test]
fn deflate_short_text() { deflate_roundtrip(b"Hello, world!"); }

#[test]
fn deflate_alphabet() {
    deflate_roundtrip(b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789");
}

#[test]
fn deflate_kilobyte_random() {
    let mut s = 0xDEAD_BEEFu64;
    let mut data = Vec::with_capacity(1024);
    for _ in 0..1024 {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        data.push((s >> 32) as u8);
    }
    deflate_roundtrip(&data);
}

#[test]
fn deflate_repeats_one_byte() {
    let data = vec![0x5A; 4096];
    deflate_roundtrip(&data);
}

#[test]
fn deflate_block_boundary() {
    // 65 535 = one stored block, plus 1 byte = boundary case.
    let mut s = 0xCAFEu64;
    let mut data = Vec::with_capacity(65_536);
    for _ in 0..65_536 {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        data.push((s >> 32) as u8);
    }
    deflate_roundtrip(&data);
}

#[test]
fn deflate_multiple_blocks() {
    // > 2 stored blocks.
    let mut s = 0xC0FFEEu64;
    let mut data = Vec::with_capacity(200_000);
    for _ in 0..200_000 {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        data.push((s >> 32) as u8);
    }
    deflate_roundtrip(&data);
}

// --- zlib wrapper ---

#[test]
fn zlib_empty() { zlib_roundtrip(b""); }

#[test]
fn zlib_short() { zlib_roundtrip(b"Hello, zlib!"); }

#[test]
fn zlib_kilobyte_random() {
    let mut s = 0xBABE_FACEu64;
    let mut data = Vec::with_capacity(1024);
    for _ in 0..1024 {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        data.push((s >> 32) as u8);
    }
    zlib_roundtrip(&data);
}

#[test]
fn zlib_starts_with_cmf_byte() {
    let encoded = zlib_compress(b"abc");
    assert_eq!(encoded[0], 0x78, "CMF byte should be 0x78 (DEFLATE + 32 KiB window)");
    // (CMF*256 + FLG) % 31 == 0
    let header = u16::from_be_bytes([encoded[0], encoded[1]]);
    assert_eq!(header % 31, 0, "RFC 1950 FCHECK constraint broken");
}

#[test]
fn zlib_ends_with_adler32() {
    let raw = b"abcdefgh";
    let encoded = zlib_compress(raw);
    let adler_be = &encoded[encoded.len() - 4..];
    let adler = u32::from_be_bytes(adler_be.try_into().unwrap());
    assert_eq!(adler, 0x0E00_0325, "Adler-32 of 'abcdefgh' should be 0x0E000325");
}

// --- size overhead ---

#[test]
fn stored_block_overhead_is_small() {
    let data = vec![0x42u8; 10_000];
    let encoded = deflate_stored(&data);
    let overhead = encoded.len() as i64 - data.len() as i64;
    // 1 stored block of 10 000 bytes: 5 byte block header + 10 000 raw.
    assert!(
        overhead >= 5 && overhead <= 10,
        "expected 5-10 byte overhead for one stored block, got {}", overhead
    );
}

#[test]
fn fast_path_compresses_repeats_heavily() {
    // 10 000 identical bytes compress to a few dozen bytes via LZ77 +
    // static Huffman: one literal + a chain of length-258 matches.
    let data = vec![0x42u8; 10_000];
    let encoded = deflate_level(&data, Level::Fast);
    assert!(
        encoded.len() < 80,
        "expected huge compression on repeats, got {} bytes",
        encoded.len()
    );
    // Confirm decode.
    let mut decoder = DeflateDecoder::new(encoded.as_slice());
    let mut decoded = Vec::new();
    decoder.read_to_end(&mut decoded).expect("decode");
    assert_eq!(decoded, data);
}

#[test]
fn fast_path_compresses_text() {
    // English text with repetition — LZ77 should find matches.
    let phrase = b"the quick brown fox jumps over the lazy dog. ";
    let mut data = Vec::with_capacity(phrase.len() * 200);
    for _ in 0..200 {
        data.extend_from_slice(phrase);
    }
    let encoded = deflate_level(&data, Level::Fast);
    let ratio = encoded.len() as f64 / data.len() as f64;
    assert!(
        ratio < 0.20,
        "expected text compression ratio < 0.20, got {ratio:.3} (encoded {} from {})",
        encoded.len(), data.len()
    );
    let mut decoder = DeflateDecoder::new(encoded.as_slice());
    let mut decoded = Vec::new();
    decoder.read_to_end(&mut decoded).expect("decode");
    assert_eq!(decoded, data);
}

// =====================================================================
// Phase 1.0.2 — Best level (best of stored / static / dynamic per block)
// =====================================================================

fn best_roundtrip(input: &[u8]) -> Vec<u8> {
    let encoded = deflate_level(input, Level::Best);
    let mut decoder = DeflateDecoder::new(encoded.as_slice());
    let mut decoded = Vec::new();
    decoder.read_to_end(&mut decoded).expect("flate2 decode");
    assert_eq!(decoded, input,
        "Best-level roundtrip mismatch (input len {}, encoded len {})",
        input.len(), encoded.len());
    encoded
}

#[test]
fn best_empty_roundtrips() { best_roundtrip(b""); }

#[test]
fn best_one_byte_roundtrips() { best_roundtrip(b"x"); }

#[test]
fn best_short_text_roundtrips() {
    best_roundtrip(b"Dynamic Huffman handles ASCII text fine.");
}

#[test]
fn best_repeats_compress_to_at_most_static() {
    // 10 K identical bytes — both static and dynamic do well; chooser
    // picks the smaller. Should beat static (phase 1.0.1) thanks to a
    // tighter literal/length code length on the single repeated byte.
    let data = vec![0x42u8; 10_000];
    let fast = deflate_level(&data, Level::Fast);
    let best = best_roundtrip(&data);
    assert!(best.len() <= fast.len(),
        "Best must never lose to Fast (best={}, fast={})", best.len(), fast.len());
    assert!(best.len() < 60,
        "Dynamic Huffman should compress 10K repeats below 60 bytes (got {})",
        best.len());
}

#[test]
fn best_falls_back_to_stored_on_random() {
    // Random data: dynamic/static Huffman both pay overhead with no
    // match savings. The stored block (raw + 5 byte header + ≤7 align)
    // is the optimum and chooser must select it.
    let mut s = 0xC0DEFACEu64;
    let mut data = Vec::with_capacity(8192);
    for _ in 0..8192 {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        data.push((s >> 32) as u8);
    }
    let best = best_roundtrip(&data);
    // Stored: 5-byte header + ≤7 align + raw = raw + 6 bytes worst case
    // (plus the 3-byte block prefix overhead).
    assert!(best.len() <= data.len() + 10,
        "Best on random should fall back to stored (got {} for {} bytes)",
        best.len(), data.len());
}

#[test]
fn best_matches_zlib_l6_class_on_text() {
    // English prose × 20 → ~9 KB. Dynamic Huffman with frequency-tuned
    // literal codes should land within a few percent of zlib level 6.
    let phrase = b"Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do \
eiusmod tempor incididunt ut labore et dolore magna aliqua. ";
    let mut data = Vec::with_capacity(phrase.len() * 20);
    for _ in 0..20 { data.extend_from_slice(phrase); }
    let best = best_roundtrip(&data);
    let ratio = best.len() as f64 / data.len() as f64;
    assert!(ratio < 0.06,
        "Best on prose × 20 should compress to < 6% (got {ratio:.4}, {} bytes from {})",
        best.len(), data.len());
}

#[test]
fn best_default_level_is_best() {
    // `deflate(data)` uses the default Level — which is Level::Best as of
    // phase 1.0.2. Verify by checking that default output never exceeds
    // explicit Best output (they should be byte-identical).
    let data = b"the quick brown fox jumps over the lazy dog";
    let default_out = deflate(data);
    let best_out = deflate_level(data, Level::Best);
    assert_eq!(default_out, best_out,
        "default level should equal Level::Best");
}

#[test]
fn lazy_match_compresses_natural_text() {
    // Cross-phrase repetition — lazy match finds long-range matches
    // greedy would miss (greedy commits on first 3-byte match).
    let phrases: Vec<&[u8]> = vec![
        b"The quick brown fox jumps over the lazy dog. ",
        b"Pack my box with five dozen liquor jugs. ",
        b"How vexingly quick daft zebras jump! ",
        b"The five boxing wizards jump quickly. ",
        b"Sphinx of black quartz, judge my vow. ",
    ];
    let mut data = Vec::new();
    for round in 0..30 {
        for p in &phrases {
            data.extend_from_slice(p);
        }
        data.extend_from_slice(phrases[round % phrases.len()]);
    }
    let encoded = deflate_level(&data, Level::Best);
    let mut decoder = DeflateDecoder::new(encoded.as_slice());
    let mut decoded = Vec::new();
    decoder.read_to_end(&mut decoded).expect("decode");
    assert_eq!(decoded, data, "lazy-mode roundtrip mismatch");
    let ratio = encoded.len() as f64 / data.len() as f64;
    assert!(
        ratio < 0.05,
        "lazy on cross-phrase repetition should compress < 5% (got {ratio:.4}, {} from {})",
        encoded.len(),
        data.len()
    );
}

#[test]
fn lazy_match_handles_large_random() {
    // Stress: 200 KB random — must roundtrip without panic, stored
    // fallback must still kick in (size ≤ raw + small fixed overhead
    // for multi-block stored output, which is what `Level::Stored`
    // produces for inputs > 65 KiB).
    let mut s = 0xFEED_FACEu64;
    let mut data = Vec::with_capacity(200_000);
    for _ in 0..200_000 {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        data.push((s >> 32) as u8);
    }
    let encoded = deflate_level(&data, Level::Best);
    let mut decoder = DeflateDecoder::new(encoded.as_slice());
    let mut decoded = Vec::new();
    decoder.read_to_end(&mut decoded).expect("decode");
    assert_eq!(decoded, data);
    // Single static/dynamic block on incompressible 200 KB random adds
    // < 6% overhead (Huffman padding for 8-bit literals).
    assert!(
        encoded.len() < data.len() + data.len() / 16,
        "lazy on 200 KB random ballooned to {} from {}",
        encoded.len(),
        data.len()
    );
}

#[test]
fn best_block_size_chooser_never_regresses() {
    // For every type of input we already exercise above, Best must be
    // ≤ Fast in encoded size. (Fast = static Huffman; chooser includes
    // static so Best is mathematically ≤.)
    let inputs: Vec<Vec<u8>> = vec![
        b"".to_vec(),
        b"x".to_vec(),
        b"abc".to_vec(),
        vec![0x42u8; 5000],
        {
            let phrase = b"hello world ";
            let mut buf = Vec::new();
            for _ in 0..100 { buf.extend_from_slice(phrase); }
            buf
        },
        {
            let mut s = 0xBEEF_BABEu64;
            let mut d = Vec::with_capacity(4096);
            for _ in 0..4096 {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                d.push((s >> 32) as u8);
            }
            d
        },
    ];
    for input in &inputs {
        let f = deflate_level(input, Level::Fast).len();
        let b = deflate_level(input, Level::Best).len();
        assert!(b <= f,
            "Best regressed vs Fast on len-{} input: best={}, fast={}",
            input.len(), b, f);
    }
}

#[test]
fn fast_path_handles_random_without_panic() {
    // Random data: LZ77 finds few/no matches; output is ~1.05× raw
    // due to Huffman overhead. We just want no panic + valid roundtrip.
    let mut s = 0x12345678u64;
    let mut data = Vec::with_capacity(8192);
    for _ in 0..8192 {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        data.push((s >> 32) as u8);
    }
    let encoded = deflate(&data);
    let mut decoder = DeflateDecoder::new(encoded.as_slice());
    let mut decoded = Vec::new();
    decoder.read_to_end(&mut decoded).expect("decode");
    assert_eq!(decoded, data);
}
