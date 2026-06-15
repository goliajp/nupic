//! Phase 1.0.1: greedy LZ77 hash chain + static Huffman block.
//!
//! Single-block output. Format: BFINAL=1 + BTYPE=01 (fixed Huffman)
//! + token stream + EOB symbol 256. Per RFC 1951 §3.2.5 + §3.2.6.
//!
//! Hash chain follows zlib's classic design: 15-bit hash from the
//! first 3 bytes of the lookahead; `hash_head[hash]` points to the
//! most-recent occurrence of that prefix in the window;
//! `hash_prev[i]` chains backward. Match search walks the chain up
//! to `MAX_CHAIN` steps looking for the longest match within the 32
//! KiB sliding window.

use nupic_bits::BitWriter;

use crate::tables::{
    DIST_CODES, DIST_SYM_SMALL, LENGTH_SYM, LIT_LEN_CODES, dist_sym_large,
};

const WIN_SIZE: usize = 32_768;
const HASH_BITS: usize = 15;
const HASH_SIZE: usize = 1 << HASH_BITS;
const HASH_MASK: u32 = (HASH_SIZE - 1) as u32;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
const MAX_CHAIN: usize = 32;
const NIL: u32 = u32::MAX;

/// Encode `data` as a single static-Huffman DEFLATE block. Empty
/// inputs emit an EOB-only block.
pub fn deflate_fast(data: &[u8]) -> Vec<u8> {
    let mut w = BitWriter::with_capacity(data.len() / 2 + 16);

    // BFINAL = 1, BTYPE = 01 (fixed Huffman)
    w.write_bits(1, 1);
    w.write_bits(0b01, 2);

    let n = data.len();
    if n < MIN_MATCH {
        // All literals — chain init is moot.
        for &b in data {
            emit_literal(&mut w, b);
        }
        emit_symbol(&mut w, 256); // EOB
        return w.into_bytes();
    }

    let mut hash_head = vec![NIL; HASH_SIZE];
    let mut hash_prev = vec![NIL; WIN_SIZE];
    let mut i = 0usize;

    while i < n {
        // Try to find a match starting at i.
        let (best_len, best_dist) = if i + MIN_MATCH <= n {
            let h = hash3(&data[i..]);
            let head = hash_head[h];
            let (m_len, m_dist) = find_longest_match(data, i, head, &hash_prev);
            (m_len, m_dist)
        } else {
            (0, 0)
        };

        if best_len >= MIN_MATCH {
            emit_match(&mut w, best_len, best_dist);
            // Insert hashes for all positions in the match into the chain.
            let mut k = 0;
            while k < best_len && i + k + MIN_MATCH <= n {
                insert_hash(&mut hash_head, &mut hash_prev, data, i + k);
                k += 1;
            }
            i += best_len;
        } else {
            emit_literal(&mut w, data[i]);
            if i + MIN_MATCH <= n {
                insert_hash(&mut hash_head, &mut hash_prev, data, i);
            }
            i += 1;
        }
    }

    emit_symbol(&mut w, 256); // EOB
    w.into_bytes()
}

#[inline]
fn hash3(window: &[u8]) -> usize {
    // 3-byte hash — zlib-class: shift + xor mix into HASH_BITS.
    let b0 = window[0] as u32;
    let b1 = window[1] as u32;
    let b2 = window[2] as u32;
    let mixed = (b0 << 10) ^ (b1 << 5) ^ b2;
    (mixed & HASH_MASK) as usize
}

#[inline]
fn insert_hash(
    hash_head: &mut [u32],
    hash_prev: &mut [u32],
    data: &[u8],
    i: usize,
) {
    let h = hash3(&data[i..]);
    let win_idx = (i % WIN_SIZE) as u32;
    hash_prev[win_idx as usize] = hash_head[h];
    hash_head[h] = i as u32;
}

#[inline]
fn find_longest_match(
    data: &[u8],
    pos: usize,
    head: u32,
    hash_prev: &[u32],
) -> (usize, usize) {
    let mut chain_pos = head;
    let mut best_len = 0usize;
    let mut best_dist = 0usize;
    let n = data.len();
    let max_len_here = (n - pos).min(MAX_MATCH);
    if max_len_here < MIN_MATCH {
        return (0, 0);
    }

    let min_pos = pos.saturating_sub(WIN_SIZE);
    let mut depth = 0;
    while chain_pos != NIL && (chain_pos as usize) >= min_pos && depth < MAX_CHAIN {
        depth += 1;
        let cp = chain_pos as usize;
        // Quick reject: 3-byte head must match
        if data[cp] == data[pos]
            && data[cp + 1] == data[pos + 1]
            && data[cp + 2] == data[pos + 2]
        {
            // count further matching bytes
            let mut k = 3;
            while k < max_len_here && data[cp + k] == data[pos + k] {
                k += 1;
            }
            if k > best_len {
                best_len = k;
                best_dist = pos - cp;
                if k == max_len_here {
                    break;
                }
            }
        }
        // walk chain
        let next = hash_prev[(chain_pos as usize) % WIN_SIZE];
        if next == NIL || next >= chain_pos {
            break;
        }
        chain_pos = next;
    }
    (best_len, best_dist)
}

#[inline]
fn emit_literal(w: &mut BitWriter, byte: u8) {
    emit_symbol(w, byte as u16);
}

#[inline]
fn emit_symbol(w: &mut BitWriter, sym: u16) {
    let (code, bits) = LIT_LEN_CODES[sym as usize];
    w.write_bits(code, bits);
}

#[inline]
fn emit_match(w: &mut BitWriter, length: usize, distance: usize) {
    debug_assert!(length >= MIN_MATCH && length <= MAX_MATCH);
    debug_assert!(distance >= 1 && distance <= WIN_SIZE);
    // length code + extra bits
    let (len_sym, len_extra_bits, len_base_low) = LENGTH_SYM[length - 3];
    let _ = len_base_low; // tables.rs notes ignored field
    emit_symbol(w, len_sym);
    if len_extra_bits > 0 {
        let base = length_base(len_sym);
        let extra = (length - base) as u32;
        w.write_bits(extra, len_extra_bits);
    }
    // distance code + extra bits
    let (dist_sym, dist_base, dist_extra_bits) = if distance <= 256 {
        DIST_SYM_SMALL[distance - 1]
    } else {
        let (sym, base, extra) = dist_sym_large(distance as u32);
        (sym, base as u16, extra)
    };
    let (dcode, dbits) = DIST_CODES[dist_sym as usize];
    w.write_bits(dcode, dbits);
    if dist_extra_bits > 0 {
        let extra = (distance as u32) - (dist_base as u32);
        w.write_bits(extra, dist_extra_bits);
    }
}

/// Recover the base length for a given length symbol (avoids
/// LENGTH_SYM's u8-truncated base field for lengths > 255).
fn length_base(sym: u16) -> usize {
    // 257..=264: base = sym - 254 = 3..=10
    // 265: 11, 266: 13, 267: 15, 268: 17
    // 269: 19, 270: 23, 271: 27, 272: 31
    // 273: 35, 274: 43, 275: 51, 276: 59
    // 277: 67, 278: 83, 279: 99, 280: 115
    // 281: 131, 282: 163, 283: 195, 284: 227
    // 285: 258
    match sym {
        257..=264 => (sym as usize) - 254,
        265 => 11, 266 => 13, 267 => 15, 268 => 17,
        269 => 19, 270 => 23, 271 => 27, 272 => 31,
        273 => 35, 274 => 43, 275 => 51, 276 => 59,
        277 => 67, 278 => 83, 279 => 99, 280 => 115,
        281 => 131, 282 => 163, 283 => 195, 284 => 227,
        285 => 258,
        _ => unreachable!("invalid length sym {sym}"),
    }
}
