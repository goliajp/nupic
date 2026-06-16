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
/// Iterative-refinement chain depth — phase 1.4b. Deeper than lazy
/// because each DP iteration costs O(N · chain) once, not per query,
/// so search depth pays off without per-token blowup.
const ITER_CHAIN: usize = 512;
/// Phase 1.4b iteration count. 1 = lazy LZ77 only (no refinement);
/// 2+ = additional cost-DP passes using the previous pass's Huffman
/// code lengths as the per-token cost model. Zopfli default is 15;
/// 5 captures most of the win at a fraction of the wall-clock.
const ITER_PASSES: usize = 5;
/// Inputs smaller than this skip iterative refinement (DP overhead
/// not justified when single-block static beats dynamic anyway).
const ITER_MIN_INPUT: usize = 1024;
const NIL: u32 = u32::MAX;

// RFC 1951 §3.2.6 static Huffman code lengths — used as the iteration-0
// cost model in `collect_tokens_iterative` (matches the zopfli
// initialisation convention).
const STATIC_LIT_LENS: [u8; NUM_LIT_LEN] = {
    let mut a = [0u8; NUM_LIT_LEN];
    let mut i = 0;
    while i < 144 { a[i] = 8; i += 1; }
    while i < 256 { a[i] = 9; i += 1; }
    while i < 280 { a[i] = 7; i += 1; }
    while i < 286 { a[i] = 8; i += 1; }
    a
};
const STATIC_DIST_LENS: [u8; NUM_DIST] = [5u8; NUM_DIST];

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
    let mut tokens = collect_tokens_iterative(data, ITER_PASSES);

    // Phase 1.5: per-block iterative refinement — re-run cost-DP within
    // each block using that block's own Huffman code-lengths as the
    // cost model. Cross-block LZ77 matches are preserved via a pre-
    // seeded hash chain. **Cost-checked**: keep the refinement only if
    // it strictly reduces total encoded bits — guards against the
    // occasional regression where per-block Huffman fit converges
    // differently and ends up larger than global (observed on
    // cargo-lock, −0.7% size loss pre-check vs +0.2% on PNG IDAT
    // corpus).
    {
        let initial_bits;
        let should_refine;
        {
            let (initial_partition, ib) = best_partition(&tokens);
            initial_bits = ib;
            should_refine = initial_partition.len() > 1 && data.len() >= ITER_MIN_INPUT;
            if should_refine {
                // Build owned snapshot of partition boundaries to avoid
                // borrowing `tokens` across the refinement call.
                let owned_partition: Vec<Vec<Token>> =
                    initial_partition.iter().map(|s| s.to_vec()).collect();
                drop(initial_partition);
                let owned_refs: Vec<&[Token]> =
                    owned_partition.iter().map(|v| v.as_slice()).collect();
                let refined = refine_tokens_per_block(data, &tokens, &owned_refs);
                let (_rp, refined_bits) = best_partition(&refined);
                if refined_bits < initial_bits {
                    tokens = refined;
                }
            }
        }
    }

    // Final partition + emission.
    let (partition, multi_bits) = best_partition(&tokens);

    // Compare against a whole-call stored fallback. Exact bit count
    // when `deflate_stored` starts at an empty BitWriter:
    //   BFINAL+BTYPE (3) + align-to-byte (5, since pos=3 → 8) +
    //   LEN+NLEN (32) + N*8 raw = 40 + 8N bits.
    // Earlier code used `16 + 8N` which under-counted the header by
    // 24 bits, making the chooser pick stored on tiny inputs (e.g.
    // a 7-byte fuzz finding: stored is 12 bytes, static is 10).
    let stored_bits = if data.len() <= STORED_MAX_FOR_BEST {
        Some(40u64 + (data.len() as u64) * 8)
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

/// Find the best block partition of `tokens`. Tries:
///
/// 1. Equal-sized splits with N ∈ {1, 2, 4, 8} (phase 1.2 baseline)
/// 2. **Variable-position greedy bisection** (phase 1.4a) — recursively
///    tries 7 evenly-spaced candidate split positions per block and
///    accepts the split if the combined cost drops below the
///    no-split baseline.
///
/// Returns the partition with the smallest total encoded bit count
/// across both strategies. Each block independently picks static vs
/// dynamic Huffman, so the per-block cost is
/// `min(static_bits, dynamic_bits)`.
///
/// Small token streams (< 2 × `MIN_SPLIT_TOKENS`) always stay at one
/// block — header overhead would dominate any gain.
fn best_partition(tokens: &[Token]) -> (Vec<&[Token]>, u64) {
    const CANDIDATES: &[usize] = &[1, 2, 4, 8];

    let n = tokens.len();
    let single_cost = single_block_cost(tokens);
    let mut best: (Vec<&[Token]>, u64) = (vec![tokens], single_cost);
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

    // Phase 1.4a: variable-position greedy bisection.
    let mut variable_partition: Vec<&[Token]> = Vec::new();
    variable_split_recursive(tokens, 0, n, single_cost, &mut variable_partition);
    if !variable_partition.is_empty() {
        let cost: u64 = variable_partition.iter().map(|b| single_block_cost(b)).sum();
        if cost < best.1 {
            best = (variable_partition, cost);
        }
    }

    best
}

/// Greedy bisection: try 7 evenly-spaced split positions inside
/// `tokens[start..end]`; if any split reduces the no-split cost,
/// commit and recurse on each half. Blocks below `MIN_SPLIT_TOKENS`
/// are not split further.
fn variable_split_recursive<'a>(
    tokens: &'a [Token],
    start: usize,
    end: usize,
    baseline_cost: u64,
    out: &mut Vec<&'a [Token]>,
) {
    const N_CANDIDATES: usize = 7;
    let n = end - start;
    if n < 2 * MIN_SPLIT_TOKENS {
        out.push(&tokens[start..end]);
        return;
    }

    let mut best_split: Option<(usize, u64, u64)> = None;
    let mut best_total = baseline_cost;
    for i in 1..=N_CANDIDATES {
        let s = start + n * i / (N_CANDIDATES + 1);
        if s - start < MIN_SPLIT_TOKENS || end - s < MIN_SPLIT_TOKENS {
            continue;
        }
        let left = single_block_cost(&tokens[start..s]);
        let right = single_block_cost(&tokens[s..end]);
        let total = left + right;
        if total < best_total {
            best_total = total;
            best_split = Some((s, left, right));
        }
    }

    match best_split {
        Some((s, left, right)) => {
            variable_split_recursive(tokens, start, s, left, out);
            variable_split_recursive(tokens, s, end, right, out);
        }
        None => out.push(&tokens[start..end]),
    }
}

const MIN_SPLIT_TOKENS: usize = 2048;

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
// Phase 1.4b — iterative LZ77 with Huffman-cost feedback
// =====================================================================
//
// Zopfli's core trick: re-run LZ77 multiple times, each pass using the
// previous pass's per-symbol Huffman code lengths as a per-token cost
// model. The match selection then minimises *output bit cost*, not
// match length. After a few iterations the cost model stabilises and
// the tokens converge to (near-)optimal.
//
// We implement a dynamic-programming forward search: `cost[i]` =
// minimum bits to encode `data[0..i]`, with transitions `i → i+1`
// (literal) and `i → i+len` (match of (len, dist)). Reconstruction
// walks the `best[]` parent array backward.
//
// Pass 0 uses RFC 1951 static Huffman lengths as the cost model
// (same convention as zopfli's first iteration). Passes 1..N build
// the cost model from the previous pass's token frequencies.

/// Compute the bit cost of emitting a (length, distance) match under
/// the provided Huffman code-length tables.
#[inline]
fn cost_of_match(length: u16, distance: u16, lit_lens: &[u8], dist_lens: &[u8]) -> u32 {
    let (len_sym, len_extra_bits, _) = LENGTH_SYM[length as usize - 3];
    let (dist_sym, _, dist_extra_bits) = if distance <= 256 {
        let (s, b, e) = DIST_SYM_SMALL[distance as usize - 1];
        (s, b as u32, e)
    } else {
        let (s, b, e) = dist_sym_large(u32::from(distance));
        (s, b, e)
    };
    u32::from(lit_lens[len_sym as usize])
        + u32::from(len_extra_bits)
        + u32::from(dist_lens[dist_sym as usize])
        + u32::from(dist_extra_bits)
}

/// Forward DP: pick the lowest-cost token sequence covering `data`
/// under the provided cost model.
fn dp_optimal_tokens(data: &[u8], max_chain: usize, lit_lens: &[u8], dist_lens: &[u8]) -> Vec<Token> {
    let n = data.len();
    if n == 0 {
        return Vec::new();
    }

    // cost[i] = min bits to encode data[0..i].
    let mut cost = vec![u32::MAX; n + 1];
    cost[0] = 0;
    // best[i] = (length-covering-to-i, distance). length=0 + distance=0
    // means "no incoming edge" (only valid for i=0).
    let mut best = vec![(0u16, 0u16); n + 1];

    let mut hash_head = vec![NIL; HASH_SIZE];
    let mut hash_prev = vec![NIL; WIN_SIZE];

    for i in 0..n {
        if cost[i] == u32::MAX {
            continue; // unreachable position (shouldn't happen w/ literals)
        }
        // Literal move.
        let lit_cost = cost[i].saturating_add(u32::from(lit_lens[data[i] as usize]));
        if lit_cost < cost[i + 1] {
            cost[i + 1] = lit_cost;
            best[i + 1] = (1, 0); // length=1 + distance=0 ⇒ literal
        }

        // Match moves: walk hash chain at position i; for each chain
        // entry compute max-extend length and consider all length sym
        // boundaries up to that extend.
        if i + MIN_MATCH <= n {
            let h = hash3(&data[i..]);
            let head = hash_head[h];
            let min_pos = i.saturating_sub(WIN_SIZE);
            let max_extend = (n - i).min(MAX_MATCH);
            let mut chain_pos = head;
            let mut depth = 0;
            while chain_pos != NIL && (chain_pos as usize) >= min_pos && depth < max_chain {
                depth += 1;
                let cp = chain_pos as usize;
                if data[cp] == data[i]
                    && data[cp + 1] == data[i + 1]
                    && data[cp + 2] == data[i + 2]
                {
                    let mut k = 3usize;
                    while k < max_extend && data[cp + k] == data[i + k] {
                        k += 1;
                    }
                    let dist = i - cp;
                    // Consider just the max-extend length per chain entry.
                    // (Length-symbol boundary variants would shave another
                    // ~ 0.5% but multiply DP work — defer to phase 1.5.)
                    let match_cost = cost[i].saturating_add(cost_of_match(
                        k as u16,
                        dist as u16,
                        lit_lens,
                        dist_lens,
                    ));
                    let target = i + k;
                    if match_cost < cost[target] {
                        cost[target] = match_cost;
                        best[target] = (k as u16, dist as u16);
                    }
                }
                let next = hash_prev[(chain_pos as usize) % WIN_SIZE];
                if next == NIL || next >= chain_pos {
                    break;
                }
                chain_pos = next;
            }
            insert_hash(&mut hash_head, &mut hash_prev, data, i);
        }
    }

    // Reconstruct tokens by walking back from n.
    let mut rev: Vec<Token> = Vec::new();
    let mut pos = n;
    while pos > 0 {
        let (len, dist) = best[pos];
        if dist == 0 {
            rev.push(Token::Literal(data[pos - 1]));
            pos -= 1;
        } else {
            rev.push(Token::Match { length: len, distance: dist });
            pos -= len as usize;
        }
    }
    rev.reverse();
    rev
}

/// Build per-symbol Huffman code lengths from a token sequence (the
/// same `lit_freq` / `dist_freq` accumulation that `DynamicPlan::build`
/// does, but returning bare length arrays for the cost model).
fn cost_lens_from_tokens(tokens: &[Token]) -> ([u8; NUM_LIT_LEN], [u8; NUM_DIST]) {
    let mut lit_freq = [0u32; NUM_LIT_LEN];
    let mut dist_freq = [0u32; NUM_DIST];
    for &t in tokens {
        let syms = token_syms(t);
        lit_freq[syms.lit_sym as usize] += 1;
        if let Some(d) = syms.dist {
            dist_freq[d.sym as usize] += 1;
        }
    }
    lit_freq[256] += 1; // EOB always present
    let lit_vec = limited_lengths(&lit_freq, MAX_LIT_LEN_BITS);
    let dist_vec = limited_lengths(&dist_freq, MAX_DIST_BITS);
    let mut lit_lens = [0u8; NUM_LIT_LEN];
    let mut dist_lens = [0u8; NUM_DIST];
    for i in 0..NUM_LIT_LEN {
        // Symbols with length 0 (unused) get a fake length large enough
        // to discourage the cost model from "discovering" them. Use
        // MAX_LIT_LEN_BITS (the natural maximum) so the cost-DP doesn't
        // overflow but treats unused symbols as expensive.
        lit_lens[i] = if lit_vec[i] == 0 { MAX_LIT_LEN_BITS } else { lit_vec[i] };
    }
    for i in 0..NUM_DIST {
        dist_lens[i] = if dist_vec[i] == 0 { MAX_DIST_BITS } else { dist_vec[i] };
    }
    (lit_lens, dist_lens)
}

/// Multi-pass cost-aware tokenisation. Initial pass uses RFC 1951
/// static Huffman as the cost model; subsequent passes use the
/// previous pass's token-frequency-fitted Huffman.
fn collect_tokens_iterative(data: &[u8], n_passes: usize) -> Vec<Token> {
    if data.len() < ITER_MIN_INPUT {
        // Below this size, single-pass lazy already matches DP since
        // there's little room for cost-based improvement.
        return collect_tokens_lazy(data, LAZY_CHAIN, LAZY_MAX);
    }

    // Pass 0: DP with static Huffman as cost model.
    let mut tokens = dp_optimal_tokens(data, ITER_CHAIN, &STATIC_LIT_LENS, &STATIC_DIST_LENS);

    // Passes 1..n_passes: refine cost model from previous tokens.
    for _ in 1..n_passes {
        let (lit_lens, dist_lens) = cost_lens_from_tokens(&tokens);
        let next = dp_optimal_tokens(data, ITER_CHAIN, &lit_lens, &dist_lens);
        // Cheap convergence check: if token count is identical and DP
        // cost matches, we're done. Otherwise replace.
        if next.len() == tokens.len() && tokens_equal(&tokens, &next) {
            tokens = next;
            break;
        }
        tokens = next;
    }
    tokens
}

/// Phase 1.5: DP-search for the optimal token sequence inside
/// `data[byte_start..byte_end]`, with the hash chain **pre-seeded**
/// from `data[..byte_start]` so cross-block LZ77 matches survive.
/// Cost model is provided per-block by the caller.
fn dp_optimal_tokens_window(
    data: &[u8],
    byte_start: usize,
    byte_end: usize,
    max_chain: usize,
    lit_lens: &[u8],
    dist_lens: &[u8],
) -> Vec<Token> {
    let block_len = byte_end - byte_start;
    if block_len == 0 {
        return Vec::new();
    }

    // Pre-seed hash chain with data[..byte_start]. Use insert_hash for
    // every position that has MIN_MATCH lookahead within `data`.
    let mut hash_head = vec![NIL; HASH_SIZE];
    let mut hash_prev = vec![NIL; WIN_SIZE];
    let seed_end = byte_start.min(data.len().saturating_sub(MIN_MATCH - 1));
    let mut j = 0usize;
    while j < seed_end {
        insert_hash(&mut hash_head, &mut hash_prev, data, j);
        j += 1;
    }

    // DP arrays span only the block window (not whole input).
    let mut cost = vec![u32::MAX; block_len + 1];
    cost[0] = 0;
    let mut best = vec![(0u16, 0u16); block_len + 1];

    for off in 0..block_len {
        let i = byte_start + off;
        if cost[off] == u32::MAX {
            continue;
        }
        // Literal move.
        let lit_cost = cost[off].saturating_add(u32::from(lit_lens[data[i] as usize]));
        if lit_cost < cost[off + 1] {
            cost[off + 1] = lit_cost;
            best[off + 1] = (1, 0);
        }

        // Match moves — search hash chain spanning data[0..i]. The
        // match must fit inside the current block (off + k ≤ block_len);
        // skip entirely when the block has < MIN_MATCH bytes left to
        // emit a length-3+ match.
        let max_extend_in_block = (block_len - off).min(MAX_MATCH);
        if i + MIN_MATCH <= data.len() && max_extend_in_block >= MIN_MATCH {
            let h = hash3(&data[i..]);
            let head = hash_head[h];
            let min_pos = i.saturating_sub(WIN_SIZE);
            let mut chain_pos = head;
            let mut depth = 0;
            while chain_pos != NIL && (chain_pos as usize) >= min_pos && depth < max_chain {
                depth += 1;
                let cp = chain_pos as usize;
                if data[cp] == data[i]
                    && data[cp + 1] == data[i + 1]
                    && data[cp + 2] == data[i + 2]
                {
                    let mut k = 3usize;
                    while k < max_extend_in_block && data[cp + k] == data[i + k] {
                        k += 1;
                    }
                    let dist = i - cp;
                    let match_cost = cost[off].saturating_add(cost_of_match(
                        k as u16,
                        dist as u16,
                        lit_lens,
                        dist_lens,
                    ));
                    let target_off = off + k;
                    if match_cost < cost[target_off] {
                        cost[target_off] = match_cost;
                        best[target_off] = (k as u16, dist as u16);
                    }
                }
                let next = hash_prev[(chain_pos as usize) % WIN_SIZE];
                if next == NIL || next >= chain_pos {
                    break;
                }
                chain_pos = next;
            }
            insert_hash(&mut hash_head, &mut hash_prev, data, i);
        }
    }

    // Reconstruct tokens.
    let mut rev: Vec<Token> = Vec::new();
    let mut pos = block_len;
    while pos > 0 {
        let (len, dist) = best[pos];
        if dist == 0 {
            rev.push(Token::Literal(data[byte_start + pos - 1]));
            pos -= 1;
        } else {
            rev.push(Token::Match { length: len, distance: dist });
            pos -= len as usize;
        }
    }
    rev.reverse();
    rev
}

/// Phase 1.5 outer loop: pick partition once via existing iterative,
/// then re-run cost-DP **per block** using each block's own Huffman
/// code-lengths fitted to that block's token frequencies. Hash chain
/// spans the whole input so cross-block LZ77 matches survive.
///
/// `block_lens` records the byte length of each block (cumulative
/// start positions are derived by walking).
fn refine_tokens_per_block(
    data: &[u8],
    tokens: &[Token],
    partition: &[&[Token]],
) -> Vec<Token> {
    // Compute byte boundary per block.
    let mut block_byte_ends: Vec<usize> = Vec::with_capacity(partition.len() + 1);
    block_byte_ends.push(0);
    let mut cumulative_bytes = 0usize;
    for block in partition {
        for &t in *block {
            match t {
                Token::Literal(_) => cumulative_bytes += 1,
                Token::Match { length, .. } => cumulative_bytes += length as usize,
            }
        }
        block_byte_ends.push(cumulative_bytes);
    }
    debug_assert_eq!(*block_byte_ends.last().unwrap(), data.len(),
        "partition byte coverage doesn't match input length");

    let _ = tokens; // initial tokens informed `partition`; not used directly here

    let mut refined: Vec<Token> = Vec::with_capacity(data.len() / 4);
    for (block_idx, block_tokens) in partition.iter().enumerate() {
        let byte_start = block_byte_ends[block_idx];
        let byte_end = block_byte_ends[block_idx + 1];
        // Block-local Huffman from this block's current tokens.
        let (lit_lens, dist_lens) = cost_lens_from_tokens(block_tokens);
        let block_refined = dp_optimal_tokens_window(
            data, byte_start, byte_end, ITER_CHAIN, &lit_lens, &dist_lens,
        );
        refined.extend(block_refined);
    }
    refined
}

#[inline]
fn tokens_equal(a: &[Token], b: &[Token]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).all(|(x, y)| match (x, y) {
        (Token::Literal(p), Token::Literal(q)) => p == q,
        (
            Token::Match { length: l1, distance: d1 },
            Token::Match { length: l2, distance: d2 },
        ) => l1 == l2 && d1 == d2,
        _ => false,
    })
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
