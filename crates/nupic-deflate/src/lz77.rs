//! Greedy LZ77 hash chain (phase 1.0.1) + per-block format chooser
//! (phase 1.0.2: best of stored / static Huffman / dynamic Huffman).
//!
//! Single-block output. The encoder collects an LZ77 token stream
//! once, then picks the smallest of three valid DEFLATE encodings:
//!
//! * BTYPE=00 stored — raw passthrough, ~1.0005× input.
//! * BTYPE=01 static Huffman — RFC 1951 §3.2.6 fixed code, no header
//!   overhead beyond 3 bits.
//! * BTYPE=10 dynamic Huffman — frequency-tuned canonical Huffman
//!   tree (length-limited 15) emitted in the RFC 1951 §3.2.7 header.
//!
//! Hash chain follows zlib's classic design: 15-bit hash from the
//! first 3 bytes of the lookahead; `hash_head[hash]` points to the
//! most-recent occurrence of that prefix in the window;
//! `hash_prev[i]` chains backward. Match search walks the chain up
//! to `MAX_CHAIN` steps looking for the longest match within the 32
//! KiB sliding window.

use nupic_bits::BitWriter;

use crate::deflate_stored;
use crate::huffman::{CL_CODE_ORDER, canonical_codes, limited_lengths, rle_code_lengths};
use crate::tables::{
    DIST_CODES, DIST_SYM_SMALL, LENGTH_SYM, LIT_LEN_CODES, dist_sym_large,
};

const WIN_SIZE: usize = 32_768;
const HASH_BITS: usize = 15;
const HASH_SIZE: usize = 1 << HASH_BITS;
const HASH_MASK: u32 = (HASH_SIZE - 1) as u32;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
/// Greedy-mode chain depth — phase 1.0.1 `Level::Fast` parameter
/// (matches zlib level 1).
const GREEDY_CHAIN: usize = 32;
/// Lazy-mode chain depth — phase 1.1 `Level::Best` parameter
/// (matches zlib level 6).
const LAZY_CHAIN: usize = 128;
/// Lazy-mode "good enough" threshold — phase 1.1. If the current
/// match is at least this long, commit immediately without trying the
/// next position (matches zlib level 6 `max_lazy = 16`).
const LAZY_MAX: usize = 16;
const NIL: u32 = u32::MAX;

const NUM_LIT_LEN: usize = 286;
const NUM_DIST: usize = 30;
const NUM_CL: usize = 19;
const MAX_LIT_LEN_BITS: u8 = 15;
const MAX_DIST_BITS: u8 = 15;
const MAX_CL_BITS: u8 = 7;

#[derive(Clone, Copy)]
enum Token {
    Literal(u8),
    Match { length: u16, distance: u16 },
}

/// Encode `data` as a single static-Huffman DEFLATE block. Uses
/// greedy LZ77 (phase 1.0.1 semantics).
pub fn deflate_static(data: &[u8]) -> Vec<u8> {
    let tokens = collect_tokens_greedy(data, GREEDY_CHAIN);
    let mut w = BitWriter::with_capacity(data.len() / 2 + 16);
    emit_static_block(&mut w, &tokens, true);
    w.into_bytes()
}

/// Encode `data` as **multi-block DEFLATE**, picking per-block format
/// (static vs dynamic Huffman) and globally comparing against a
/// whole-call stored fallback. Uses **lazy** LZ77 matching with
/// deeper hash-chain search (phase 1.1 semantics) and split-search
/// over {1, 2, 4, 8} equal-sized token partitions (phase 1.2).
pub fn deflate_best(data: &[u8]) -> Vec<u8> {
    let tokens = collect_tokens_lazy(data, LAZY_CHAIN, LAZY_MAX);

    // Find best block partition. Always at least 1 block (no split).
    let (partition, multi_bits) = best_partition(&tokens);

    // Compare against a whole-call stored fallback.
    let stored_bits = if data.len() <= STORED_MAX_FOR_BEST {
        // Tight upper bound on stored-block size:
        //   3 header bits + ≤ 7 align bits + 4 (LEN+NLEN) + N raw bytes
        Some(16u64 + (data.len() as u64) * 8)
    } else {
        None
    };

    if let Some(sb) = stored_bits
        && sb < multi_bits
    {
        return deflate_stored(data);
    }

    // Emit multi-block. Each block independently picks static vs
    // dynamic and BFINAL is set only on the last block.
    let mut w = BitWriter::with_capacity((multi_bits / 8 + 16) as usize);
    let last_idx = partition.len() - 1;
    for (idx, block_tokens) in partition.iter().enumerate() {
        let bfinal = idx == last_idx;
        let static_bits = static_block_bits(block_tokens);
        let plan = DynamicPlan::build(block_tokens);
        let dynamic_bits = plan.total_bits();
        if dynamic_bits < static_bits {
            emit_dynamic_block(&mut w, block_tokens, &plan, bfinal);
        } else {
            emit_static_block(&mut w, block_tokens, bfinal);
        }
    }
    w.into_bytes()
}

const STORED_MAX_FOR_BEST: usize = 65_535;

/// Try splitting tokens into 1 / 2 / 4 / 8 equal-sized blocks. Return
/// the partition with the smallest total encoded bit count.
///
/// Each block independently picks static vs dynamic Huffman — so the
/// per-block cost is `min(static_bits, dynamic_bits)`.
///
/// Small token streams (< 2 × `MIN_SPLIT_TOKENS`) always stay at one
/// block — header overhead would dominate any gain.
fn best_partition(tokens: &[Token]) -> (Vec<&[Token]>, u64) {
    const MIN_SPLIT_TOKENS: usize = 2048;
    const CANDIDATES: &[usize] = &[1, 2, 4, 8];

    let n = tokens.len();
    let mut best: (Vec<&[Token]>, u64) = (vec![tokens], single_block_cost(tokens));
    if n < MIN_SPLIT_TOKENS * 2 {
        return best;
    }

    for &n_blocks in CANDIDATES {
        if n_blocks <= 1 {
            continue;
        }
        if n / n_blocks < MIN_SPLIT_TOKENS {
            continue;
        }
        let partition = split_equal(tokens, n_blocks);
        let cost: u64 = partition.iter().map(|b| single_block_cost(b)).sum();
        if cost < best.1 {
            best = (partition, cost);
        }
    }
    best
}

fn single_block_cost(tokens: &[Token]) -> u64 {
    let s = static_block_bits(tokens);
    let plan = DynamicPlan::build(tokens);
    let d = plan.total_bits();
    s.min(d)
}

fn split_equal(tokens: &[Token], n_blocks: usize) -> Vec<&[Token]> {
    let n = tokens.len();
    let block_size = n / n_blocks;
    let mut out: Vec<&[Token]> = Vec::with_capacity(n_blocks);
    for i in 0..n_blocks {
        let start = i * block_size;
        let end = if i == n_blocks - 1 { n } else { (i + 1) * block_size };
        out.push(&tokens[start..end]);
    }
    out
}

// =====================================================================
// Token collection
// =====================================================================
//
// Two flavours live here:
// * `collect_tokens_greedy` (phase 1.0.1) — take every match as found.
// * `collect_tokens_lazy`   (phase 1.1)   — defer each match by one
//   byte to see whether `i+1` has a strictly longer match. If yes,
//   sacrifice `data[i-1]` as a literal; the longer match at `i` wins.
//   Otherwise commit the deferred match.

fn collect_tokens_greedy(data: &[u8], max_chain: usize) -> Vec<Token> {
    let n = data.len();
    let mut tokens: Vec<Token> = Vec::with_capacity(n / 2 + 1);
    if n < MIN_MATCH {
        for &b in data {
            tokens.push(Token::Literal(b));
        }
        return tokens;
    }
    let mut hash_head = vec![NIL; HASH_SIZE];
    let mut hash_prev = vec![NIL; WIN_SIZE];
    let mut i = 0usize;
    while i < n {
        let (best_len, best_dist) = if i + MIN_MATCH <= n {
            let h = hash3(&data[i..]);
            let head = hash_head[h];
            find_longest_match(data, i, head, &hash_prev, max_chain)
        } else {
            (0, 0)
        };

        if best_len >= MIN_MATCH {
            tokens.push(Token::Match {
                length: best_len as u16,
                distance: best_dist as u16,
            });
            let mut k = 0;
            while k < best_len && i + k + MIN_MATCH <= n {
                insert_hash(&mut hash_head, &mut hash_prev, data, i + k);
                k += 1;
            }
            i += best_len;
        } else {
            tokens.push(Token::Literal(data[i]));
            if i + MIN_MATCH <= n {
                insert_hash(&mut hash_head, &mut hash_prev, data, i);
            }
            i += 1;
        }
    }
    tokens
}

fn collect_tokens_lazy(data: &[u8], max_chain: usize, lazy_threshold: usize) -> Vec<Token> {
    let n = data.len();
    let mut tokens: Vec<Token> = Vec::with_capacity(n / 2 + 1);
    if n < MIN_MATCH {
        for &b in data {
            tokens.push(Token::Literal(b));
        }
        return tokens;
    }
    let mut hash_head = vec![NIL; HASH_SIZE];
    let mut hash_prev = vec![NIL; WIN_SIZE];

    // Invariant: before evaluating position `i`, every position in
    // `0..i` has had its hash inserted (when it had MIN_MATCH lookahead).
    //
    // `prev_len` carries the match found at `i-1` that we are deferring;
    // 0 means no deferred match.
    let mut prev_len = 0usize;
    let mut prev_dist = 0usize;
    let mut i = 0usize;

    while i < n {
        // Find longest match at `i`.
        let (cur_len, cur_dist) = if i + MIN_MATCH <= n {
            let h = hash3(&data[i..]);
            let head = hash_head[h];
            find_longest_match(data, i, head, &hash_prev, max_chain)
        } else {
            (0, 0)
        };

        // Insert hash for `i` so future searches see it.
        if i + MIN_MATCH <= n {
            insert_hash(&mut hash_head, &mut hash_prev, data, i);
        }

        if prev_len >= MIN_MATCH && cur_len <= prev_len {
            // Commit the deferred match at i-1.
            tokens.push(Token::Match {
                length: prev_len as u16,
                distance: prev_dist as u16,
            });
            // Match covers positions i-1 .. i-1+prev_len-1. Hashes already
            // inserted at i-1 (when found) and i (just now). Need to add
            // hashes for i+1 .. i+prev_len-2.
            let end = i - 1 + prev_len;
            let mut k = i + 1;
            while k < end {
                if k + MIN_MATCH <= n {
                    insert_hash(&mut hash_head, &mut hash_prev, data, k);
                }
                k += 1;
            }
            i = end;
            prev_len = 0;
        } else if prev_len >= MIN_MATCH {
            // `cur_len > prev_len`: lazy paid off, sacrifice data[i-1] as
            // literal and defer (or commit) the better match at `i`.
            tokens.push(Token::Literal(data[i - 1]));
            if cur_len >= lazy_threshold {
                // commit immediately — no further deferral
                tokens.push(Token::Match {
                    length: cur_len as u16,
                    distance: cur_dist as u16,
                });
                let end = i + cur_len;
                let mut k = i + 1;
                while k < end {
                    if k + MIN_MATCH <= n {
                        insert_hash(&mut hash_head, &mut hash_prev, data, k);
                    }
                    k += 1;
                }
                i = end;
                prev_len = 0;
            } else {
                prev_len = cur_len;
                prev_dist = cur_dist;
                i += 1;
            }
        } else {
            // No deferred match. Decide on `cur`.
            if cur_len >= lazy_threshold {
                // commit immediately
                tokens.push(Token::Match {
                    length: cur_len as u16,
                    distance: cur_dist as u16,
                });
                let end = i + cur_len;
                let mut k = i + 1;
                while k < end {
                    if k + MIN_MATCH <= n {
                        insert_hash(&mut hash_head, &mut hash_prev, data, k);
                    }
                    k += 1;
                }
                i = end;
            } else if cur_len >= MIN_MATCH {
                prev_len = cur_len;
                prev_dist = cur_dist;
                i += 1;
            } else {
                tokens.push(Token::Literal(data[i]));
                i += 1;
            }
        }
    }

    // Flush trailing deferred match (no lookahead left to compare against).
    if prev_len >= MIN_MATCH {
        tokens.push(Token::Match {
            length: prev_len as u16,
            distance: prev_dist as u16,
        });
    }

    tokens
}

#[inline]
fn hash3(window: &[u8]) -> usize {
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
    max_chain: usize,
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
    while chain_pos != NIL && (chain_pos as usize) >= min_pos && depth < max_chain {
        depth += 1;
        let cp = chain_pos as usize;
        if data[cp] == data[pos]
            && data[cp + 1] == data[pos + 1]
            && data[cp + 2] == data[pos + 2]
        {
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
        let next = hash_prev[(chain_pos as usize) % WIN_SIZE];
        if next == NIL || next >= chain_pos {
            break;
        }
        chain_pos = next;
    }
    (best_len, best_dist)
}

// =====================================================================
// Token → DEFLATE symbol mapping
// =====================================================================

/// Resolved symbol info for a token. `dist` is `None` for literals.
struct TokenSyms {
    lit_sym: u16,
    len_extra_val: u32,
    len_extra_bits: u8,
    dist: Option<DistSyms>,
}
struct DistSyms {
    sym: u8,
    extra_val: u32,
    extra_bits: u8,
}

#[inline]
fn token_syms(t: Token) -> TokenSyms {
    match t {
        Token::Literal(b) => TokenSyms {
            lit_sym: u16::from(b),
            len_extra_val: 0,
            len_extra_bits: 0,
            dist: None,
        },
        Token::Match { length, distance } => {
            let (len_sym, len_extra_bits, _base_low) = LENGTH_SYM[length as usize - 3];
            let len_base = length_base(len_sym);
            let len_extra_val = (length as u32) - (len_base as u32);
            let (dsym, dbase, dextra) = if distance <= 256 {
                let (s, b, e) = DIST_SYM_SMALL[distance as usize - 1];
                (s, u32::from(b), e)
            } else {
                let (s, b, e) = dist_sym_large(u32::from(distance));
                (s, b, e)
            };
            let dist_extra_val = u32::from(distance) - dbase;
            TokenSyms {
                lit_sym: len_sym,
                len_extra_val,
                len_extra_bits,
                dist: Some(DistSyms {
                    sym: dsym,
                    extra_val: dist_extra_val,
                    extra_bits: dextra,
                }),
            }
        }
    }
}

fn length_base(sym: u16) -> usize {
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

// =====================================================================
// Static-Huffman block (BTYPE=01)
// =====================================================================

fn static_block_bits(tokens: &[Token]) -> u64 {
    let mut bits = 3u64; // BFINAL + BTYPE
    for &t in tokens {
        let syms = token_syms(t);
        bits += u64::from(LIT_LEN_CODES[syms.lit_sym as usize].1);
        bits += u64::from(syms.len_extra_bits);
        if let Some(d) = syms.dist {
            bits += u64::from(DIST_CODES[d.sym as usize].1);
            bits += u64::from(d.extra_bits);
        }
    }
    bits += u64::from(LIT_LEN_CODES[256].1); // EOB
    bits
}

fn emit_static_block(w: &mut BitWriter, tokens: &[Token], bfinal: bool) {
    w.write_bits(if bfinal { 1 } else { 0 }, 1);
    w.write_bits(0b01, 2); // BTYPE = static Huffman
    for &t in tokens {
        let syms = token_syms(t);
        let (lc, lb) = LIT_LEN_CODES[syms.lit_sym as usize];
        w.write_bits(lc, lb);
        if syms.len_extra_bits > 0 {
            w.write_bits(syms.len_extra_val, syms.len_extra_bits);
        }
        if let Some(d) = syms.dist {
            let (dc, db) = DIST_CODES[d.sym as usize];
            w.write_bits(dc, db);
            if d.extra_bits > 0 {
                w.write_bits(d.extra_val, d.extra_bits);
            }
        }
    }
    let (ec, eb) = LIT_LEN_CODES[256];
    w.write_bits(ec, eb);
}

// =====================================================================
// Dynamic-Huffman block (BTYPE=10) — phase 1.0.2
// =====================================================================

/// All the precomputed pieces needed to (a) compute exact dynamic
/// block size for the chooser and (b) actually emit it.
struct DynamicPlan {
    lit_codes: Vec<(u32, u8)>,
    dist_codes: Vec<(u32, u8)>,
    cl_lens: Vec<u8>,
    cl_codes: Vec<(u32, u8)>,
    /// RLE encoding of (lit_lens[..hlit+257] ++ dist_lens[..hdist+1]).
    rle: Vec<(u8, u32, u8)>,
    /// Bits used by tokens body (without EOB; EOB already counted).
    body_bits: u64,
    hlit: u8,
    hdist: u8,
    hclen: u8,
}

impl DynamicPlan {
    fn build(tokens: &[Token]) -> Self {
        // 1. Frequencies.
        let mut lit_freq = [0u32; NUM_LIT_LEN];
        let mut dist_freq = [0u32; NUM_DIST];
        let mut body_bits: u64 = 0;
        for &t in tokens {
            let syms = token_syms(t);
            lit_freq[syms.lit_sym as usize] += 1;
            body_bits += u64::from(syms.len_extra_bits);
            if let Some(d) = syms.dist {
                dist_freq[d.sym as usize] += 1;
                body_bits += u64::from(d.extra_bits);
            }
        }
        lit_freq[256] += 1; // EOB

        // 2. Code lengths.
        let lit_lens = limited_lengths(&lit_freq, MAX_LIT_LEN_BITS);
        let mut dist_lens = limited_lengths(&dist_freq, MAX_DIST_BITS);

        // RFC 1951 §3.2.7: when there are no distance codes used at all,
        // the encoder may transmit a single dist code length of 0
        // (HDIST=0). If exactly one dist symbol is used, our
        // limited_lengths gives it length 1 — that's also fine. No
        // dummy needed for the all-literals case.
        // However, some decoders historically only accept HDIST=0 with
        // a single-zero length when the encoder explicitly signals it.
        // miniz_oxide handles this; we just emit the natural form.

        // 3. HLIT / HDIST (trim trailing-zero lengths past the
        // minimum-required count of 257 / 1).
        let mut last_lit = 256; // EOB is always present, so min index 256
        for i in (257..NUM_LIT_LEN).rev() {
            if lit_lens[i] != 0 {
                last_lit = i;
                break;
            }
        }
        let hlit = (last_lit - 256) as u8; // hlit + 257 = last_lit + 1
        // Note: actual transmit count is hlit + 257 = last_lit + 1.

        let mut last_dist = 0; // always transmit ≥ 1 dist length
        for i in (1..NUM_DIST).rev() {
            if dist_lens[i] != 0 {
                last_dist = i;
                break;
            }
        }
        let hdist = last_dist as u8;
        // Transmit count = hdist + 1.

        // If all dist freqs are zero, ensure dist_lens[0] = 0 (it
        // already is from limited_lengths; harmless to assert).
        if dist_freq.iter().all(|&f| f == 0) {
            dist_lens[0] = 0;
        }

        // 4. RLE-encode the concatenated length array.
        let mut concat: Vec<u8> = Vec::with_capacity((hlit as usize + 257) + (hdist as usize + 1));
        concat.extend_from_slice(&lit_lens[..hlit as usize + 257]);
        concat.extend_from_slice(&dist_lens[..hdist as usize + 1]);
        let rle = rle_code_lengths(&concat);

        // 5. CL-alphabet frequencies + lengths.
        let mut cl_freq = [0u32; NUM_CL];
        for &(sym, _, _) in &rle {
            cl_freq[sym as usize] += 1;
        }
        let cl_lens = limited_lengths(&cl_freq, MAX_CL_BITS);

        // 6. HCLEN — trim trailing zero CL lengths in the transmission
        // order. RFC requires ≥ 4 transmitted, so HCLEN ≥ 0
        // (HCLEN + 4 transmitted entries).
        let mut last_cl = 3;
        for i in (4..NUM_CL).rev() {
            let cl_idx = CL_CODE_ORDER[i];
            if cl_lens[cl_idx] != 0 {
                last_cl = i;
                break;
            }
        }
        let hclen = (last_cl - 3) as u8;

        let cl_codes = canonical_codes(&cl_lens);
        let lit_codes = canonical_codes(&lit_lens);
        let dist_codes = canonical_codes(&dist_lens);

        // body_bits currently holds only extra bits; add Huffman code bits.
        for &t in tokens {
            let syms = token_syms(t);
            body_bits += u64::from(lit_lens[syms.lit_sym as usize]);
            if let Some(d) = syms.dist {
                body_bits += u64::from(dist_lens[d.sym as usize]);
            }
        }
        body_bits += u64::from(lit_lens[256]); // EOB

        DynamicPlan {
            lit_codes,
            dist_codes,
            cl_lens,
            cl_codes,
            rle,
            body_bits,
            hlit,
            hdist,
            hclen,
        }
    }

    fn header_bits(&self) -> u64 {
        // BFINAL(1) + BTYPE(2) + HLIT(5) + HDIST(5) + HCLEN(4)
        let mut bits = 1u64 + 2 + 5 + 5 + 4;
        // (HCLEN + 4) × 3 bits for CL code lengths.
        bits += u64::from(self.hclen + 4) * 3;
        // RLE'd CL symbols (Huffman-coded) + their extra bits.
        for &(sym, _, extra_bits) in &self.rle {
            bits += u64::from(self.cl_lens[sym as usize]);
            bits += u64::from(extra_bits);
        }
        bits
    }

    fn total_bits(&self) -> u64 {
        self.header_bits() + self.body_bits
    }
}

fn emit_dynamic_block(w: &mut BitWriter, tokens: &[Token], plan: &DynamicPlan, bfinal: bool) {
    // Header.
    w.write_bits(if bfinal { 1 } else { 0 }, 1);
    w.write_bits(0b10, 2); // BTYPE = dynamic Huffman
    w.write_bits(u32::from(plan.hlit), 5);
    w.write_bits(u32::from(plan.hdist), 5);
    w.write_bits(u32::from(plan.hclen), 4);
    // CL code lengths in the prescribed order.
    let cl_count = plan.hclen as usize + 4;
    for i in 0..cl_count {
        let cl_idx = CL_CODE_ORDER[i];
        w.write_bits(u32::from(plan.cl_lens[cl_idx]), 3);
    }
    // RLE'd lit/len + dist code lengths.
    for &(sym, extra_val, extra_bits) in &plan.rle {
        let (cc, cb) = plan.cl_codes[sym as usize];
        w.write_bits(cc, cb);
        if extra_bits > 0 {
            w.write_bits(extra_val, extra_bits);
        }
    }
    // Token body.
    for &t in tokens {
        let syms = token_syms(t);
        let (lc, lb) = plan.lit_codes[syms.lit_sym as usize];
        w.write_bits(lc, lb);
        if syms.len_extra_bits > 0 {
            w.write_bits(syms.len_extra_val, syms.len_extra_bits);
        }
        if let Some(d) = syms.dist {
            let (dc, db) = plan.dist_codes[d.sym as usize];
            w.write_bits(dc, db);
            if d.extra_bits > 0 {
                w.write_bits(d.extra_val, d.extra_bits);
            }
        }
    }
    // EOB.
    let (ec, eb) = plan.lit_codes[256];
    w.write_bits(ec, eb);
}
