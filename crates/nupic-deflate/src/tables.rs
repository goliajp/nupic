//! RFC 1951 fixed Huffman + length / distance encoding tables.
//!
//! All codes are stored **bit-reversed** (LSB-first packing into the
//! output byte stream). The original RFC convention is MSB-first
//! within a code; reversing once at compile time lets the encoder
//! pass the value straight through `BitWriter::write_bits` which is
//! LSB-first.

/// Static Huffman codes for literal/length symbols 0..287 (RFC 1951
/// §3.2.6). Each entry: (reversed_code_bits, bit_length).
pub const LIT_LEN_CODES: [(u32, u8); 288] = build_lit_len_codes();

/// Static Huffman codes for distance symbols 0..31 (RFC 1951 §3.2.6).
/// Distance tree is all 5-bit codes, with codes 30 / 31 reserved/illegal.
pub const DIST_CODES: [(u32, u8); 32] = build_dist_codes();

/// For a given match length (3..=258), the corresponding length symbol
/// (257..=285), the base length, and the number of extra bits.
/// Indexed by length - 3.
pub const LENGTH_SYM: [(u16, u8, u8); 256] = build_length_sym();

/// For a given match distance (1..=32_768), maps to a distance symbol
/// (0..=29) + base + extra bits. We use a small lookup for distances 1..256
/// (array index = distance - 1) and a formula for distances > 256.
pub const DIST_SYM_SMALL: [(u8, u16, u8); 256] = build_dist_sym_small();

// =====================================================================
// const fn helpers
// =====================================================================

const fn reverse_bits(mut v: u32, n_bits: u8) -> u32 {
    let mut r = 0u32;
    let mut i = 0;
    while i < n_bits {
        r = (r << 1) | (v & 1);
        v >>= 1;
        i += 1;
    }
    r
}

const fn build_lit_len_codes() -> [(u32, u8); 288] {
    let mut out = [(0u32, 0u8); 288];
    let mut i = 0;
    // 0..=143: 8-bit codes 0b00110000 .. 0b10111111
    while i <= 143 {
        let code = (0b0011_0000u32) + i as u32;
        out[i] = (reverse_bits(code, 8), 8);
        i += 1;
    }
    // 144..=255: 9-bit codes 0b110010000 .. 0b111111111
    while i <= 255 {
        let code = (0b1_1001_0000u32) + (i - 144) as u32;
        out[i] = (reverse_bits(code, 9), 9);
        i += 1;
    }
    // 256..=279: 7-bit codes 0b0000000 .. 0b0010111
    while i <= 279 {
        let code = (i - 256) as u32;
        out[i] = (reverse_bits(code, 7), 7);
        i += 1;
    }
    // 280..=287: 8-bit codes 0b11000000 .. 0b11000111
    while i <= 287 {
        let code = (0b1100_0000u32) + (i - 280) as u32;
        out[i] = (reverse_bits(code, 8), 8);
        i += 1;
    }
    out
}

const fn build_dist_codes() -> [(u32, u8); 32] {
    let mut out = [(0u32, 0u8); 32];
    let mut i = 0;
    while i < 32 {
        out[i] = (reverse_bits(i as u32, 5), 5);
        i += 1;
    }
    out
}

// length-symbol table per RFC 1951 §3.2.5
const fn build_length_sym() -> [(u16, u8, u8); 256] {
    // For length L (3..=258), table index = L - 3.
    let mut out = [(0u16, 0u8, 0u8); 256];
    // Per RFC: symbols 257..=285 cover lengths 3..=258 with extra-bit
    // splits. We hand-roll the cumulative table.
    //   sym 257: L=3,   extra=0
    //   sym 258: L=4,   extra=0
    //   ...
    //   sym 264: L=10,  extra=0
    //   sym 265: L=11..=12, extra=1
    //   sym 266: L=13..=14, extra=1
    //   sym 267: L=15..=16, extra=1
    //   sym 268: L=17..=18, extra=1
    //   sym 269: L=19..=22, extra=2
    //   sym 270: L=23..=26, extra=2
    //   sym 271: L=27..=30, extra=2
    //   sym 272: L=31..=34, extra=2
    //   sym 273: L=35..=42, extra=3
    //   sym 274: L=43..=50, extra=3
    //   sym 275: L=51..=58, extra=3
    //   sym 276: L=59..=66, extra=3
    //   sym 277: L=67..=82, extra=4
    //   sym 278: L=83..=98, extra=4
    //   sym 279: L=99..=114, extra=4
    //   sym 280: L=115..=130, extra=4
    //   sym 281: L=131..=162, extra=5
    //   sym 282: L=163..=194, extra=5
    //   sym 283: L=195..=226, extra=5
    //   sym 284: L=227..=257, extra=5
    //   sym 285: L=258, extra=0
    let buckets: &[(u16, u8, u16, u16)] = &[
        // (sym, extra_bits, base_length, count_in_bucket)
        (257, 0,  3,  1), (258, 0,  4,  1), (259, 0,  5,  1), (260, 0,  6,  1),
        (261, 0,  7,  1), (262, 0,  8,  1), (263, 0,  9,  1), (264, 0, 10,  1),
        (265, 1, 11,  2), (266, 1, 13,  2), (267, 1, 15,  2), (268, 1, 17,  2),
        (269, 2, 19,  4), (270, 2, 23,  4), (271, 2, 27,  4), (272, 2, 31,  4),
        (273, 3, 35,  8), (274, 3, 43,  8), (275, 3, 51,  8), (276, 3, 59,  8),
        (277, 4, 67, 16), (278, 4, 83, 16), (279, 4, 99, 16), (280, 4, 115, 16),
        (281, 5, 131, 32), (282, 5, 163, 32), (283, 5, 195, 32), (284, 5, 227, 31),
        (285, 0, 258, 1),
    ];
    let mut i = 0;
    while i < buckets.len() {
        let (sym, extra, base, count) = buckets[i];
        let mut k = 0u16;
        while k < count {
            let length = base + k;
            // length 3..=258 → index length - 3 ∈ 0..=255
            let idx = (length - 3) as usize;
            out[idx] = (sym, extra, base as u8);
            // base as u8 truncates for L > 255; but we recompute the
            // base via length - (length - base) using base separately
            // in the encoder. Storing the low 8 bits of base is just
            // for ergonomics; the encoder ignores it.
            let _ = base; // silence "unused" in some compilers
            k += 1;
        }
        i += 1;
    }
    out
}

// distance-symbol table per RFC 1951 §3.2.5
const fn build_dist_sym_small() -> [(u8, u16, u8); 256] {
    // Distances 1..=256 → indices 0..=255. For distances > 256 the
    // encoder uses a binary-search / direct formula at runtime.
    //   sym 0: D=1, extra=0     sym 1: D=2, extra=0
    //   sym 2: D=3, extra=0     sym 3: D=4, extra=0
    //   sym 4: D=5..=6, extra=1
    //   sym 5: D=7..=8, extra=1
    //   sym 6: D=9..=12, extra=2
    //   sym 7: D=13..=16, extra=2
    //   sym 8: D=17..=24, extra=3
    //   sym 9: D=25..=32, extra=3
    //   sym 10: D=33..=48, extra=4
    //   sym 11: D=49..=64, extra=4
    //   sym 12: D=65..=96, extra=5
    //   sym 13: D=97..=128, extra=5
    //   sym 14: D=129..=192, extra=6
    //   sym 15: D=193..=256, extra=6
    let buckets: &[(u8, u8, u16, u16)] = &[
        (0, 0,   1,  1), (1, 0,   2,  1), (2, 0,   3,  1), (3, 0,   4,  1),
        (4, 1,   5,  2), (5, 1,   7,  2),
        (6, 2,   9,  4), (7, 2,  13,  4),
        (8, 3,  17,  8), (9, 3,  25,  8),
        (10, 4, 33, 16), (11, 4, 49, 16),
        (12, 5, 65, 32), (13, 5, 97, 32),
        (14, 6, 129, 64), (15, 6, 193, 64),
    ];
    let mut out = [(0u8, 0u16, 0u8); 256];
    let mut i = 0;
    while i < buckets.len() {
        let (sym, extra, base, count) = buckets[i];
        let mut k = 0u16;
        while k < count as u16 {
            let dist = base + k;
            let idx = (dist - 1) as usize;
            out[idx] = (sym, base, extra);
            k += 1;
        }
        i += 1;
    }
    out
}

/// For a distance > 256, compute (sym, base, extra_bits) on the fly.
/// All "large" distance symbols 16..29 each cover a power-of-2 range.
#[inline]
pub fn dist_sym_large(dist: u32) -> (u8, u32, u8) {
    debug_assert!(dist > 256 && dist <= 32_768);
    // The pattern from RFC §3.2.5 distance table for sym ≥ 14:
    //   sym 14..29:  extra = sym/2 - 1, base = doubled each pair
    // We compute via leading-zero count on (dist-1).
    //   For dist in [a, b], the high bit position determines the
    //   group. Implemented by binary trial.
    let buckets: &[(u8, u32, u8)] = &[
        (16, 257,    7), (17, 385,    7),    // 257..=384, 385..=512
        (18, 513,    8), (19, 769,    8),    // 513..=768, 769..=1024
        (20, 1025,   9), (21, 1537,   9),    // 1025..=1536, 1537..=2048
        (22, 2049,  10), (23, 3073,  10),    // 2049..=3072, 3073..=4096
        (24, 4097,  11), (25, 6145,  11),    // 4097..=6144, 6145..=8192
        (26, 8193,  12), (27, 12289, 12),    // 8193..=12288, 12289..=16384
        (28, 16385, 13), (29, 24577, 13),    // 16385..=24576, 24577..=32768
    ];
    let mut last = (0u8, 0u32, 0u8);
    for &(sym, base, extra) in buckets {
        if dist >= base {
            last = (sym, base, extra);
        } else {
            break;
        }
    }
    last
}
