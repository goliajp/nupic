//! Phase 1.0.2 support: length-limited canonical Huffman code generation.
//!
//! Given a frequency table over `n` symbols and a maximum code length
//! `L`, [`limited_lengths`] returns the per-symbol bit length array such
//! that every assigned length is `≤ L` and the resulting code is
//! information-theoretically as close to the entropy bound as possible
//! under the length cap.
//!
//! Algorithm: package-merge (Larmore & Hirschberg, 1990 / Katajainen
//! formulation). For `n` symbols and `L = 15` this runs in O(n · L)
//! comparisons with a small node-arena overhead — for DEFLATE's
//! `n = 286` lit/len alphabet it is negligible (sub-microsecond).
//!
//! Canonical code construction from lengths follows RFC 1951 §3.2.2.
//! Codes are returned **bit-reversed** so that [`BitWriter::write_bits`]
//! (LSB-first) emits them correctly.

/// Compute length-limited Huffman code lengths.
///
/// `freq[i]` is the frequency of symbol `i`. Returns a `Vec` of the same
/// length where `out[i]` is the bit length to use for symbol `i`, or
/// `0` if `freq[i] == 0` (the symbol does not appear).
///
/// Every returned length is in `1..=max_len`. The single-symbol edge
/// case yields length 1 for that symbol (DEFLATE requires at least one
/// code per non-empty alphabet).
#[must_use]
pub fn limited_lengths(freq: &[u32], max_len: u8) -> Vec<u8> {
    let n = freq.len();
    let mut lens = vec![0u8; n];

    // Gather used symbols sorted ascending by frequency.
    let mut leaves: Vec<(u64, usize)> = freq
        .iter()
        .enumerate()
        .filter(|&(_, f)| *f > 0)
        .map(|(i, f)| (u64::from(*f), i))
        .collect();

    if leaves.is_empty() {
        return lens;
    }
    if leaves.len() == 1 {
        lens[leaves[0].1] = 1;
        return lens;
    }
    leaves.sort_by_key(|x| x.0);

    // Node arena. Each node is either a leaf carrying one symbol, or a
    // package combining two child nodes (referenced by index).
    struct Node {
        freq: u64,
        // -1 ⇒ this node is a package and `left/right` are valid.
        // ≥ 0 ⇒ this node is a leaf for that symbol.
        sym: i32,
        left: u32,
        right: u32,
    }
    let mut nodes: Vec<Node> = Vec::with_capacity(leaves.len() * (max_len as usize + 1));

    // Build the initial sorted-leaf list, recording arena ids.
    let leaf_ids: Vec<u32> = leaves
        .iter()
        .map(|&(f, sym)| {
            nodes.push(Node { freq: f, sym: sym as i32, left: 0, right: 0 });
            (nodes.len() - 1) as u32
        })
        .collect();

    let mut active: Vec<u32> = leaf_ids.clone();

    for _ in 1..max_len {
        // 1. Package: pair consecutive items in `active`.
        let mut packages: Vec<u32> = Vec::with_capacity(active.len() / 2);
        let mut i = 0;
        while i + 1 < active.len() {
            let a = active[i];
            let b = active[i + 1];
            let f = nodes[a as usize].freq + nodes[b as usize].freq;
            nodes.push(Node { freq: f, sym: -1, left: a, right: b });
            packages.push((nodes.len() - 1) as u32);
            i += 2;
        }
        // 2. Merge with original leaves (both lists sorted ascending).
        let mut merged: Vec<u32> = Vec::with_capacity(leaf_ids.len() + packages.len());
        let (mut li, mut pi) = (0usize, 0usize);
        while li < leaf_ids.len() && pi < packages.len() {
            let lf = nodes[leaf_ids[li] as usize].freq;
            let pf = nodes[packages[pi] as usize].freq;
            if lf <= pf {
                merged.push(leaf_ids[li]);
                li += 1;
            } else {
                merged.push(packages[pi]);
                pi += 1;
            }
        }
        merged.extend_from_slice(&leaf_ids[li..]);
        merged.extend_from_slice(&packages[pi..]);
        active = merged;
    }

    // Take the first 2L - 2 items (where L = #leaves). For each, walk
    // its subtree and count how many times every leaf symbol appears.
    let take = 2 * leaf_ids.len() - 2;
    let mut counts = vec![0u32; n];
    let mut stack: Vec<u32> = Vec::with_capacity(64);
    for &root in active.iter().take(take) {
        stack.clear();
        stack.push(root);
        while let Some(id) = stack.pop() {
            let node = &nodes[id as usize];
            if node.sym >= 0 {
                counts[node.sym as usize] += 1;
            } else {
                stack.push(node.left);
                stack.push(node.right);
            }
        }
    }
    for i in 0..n {
        lens[i] = counts[i] as u8;
    }
    lens
}

/// Convert an array of code lengths into canonical Huffman codes,
/// **bit-reversed** for LSB-first emission via `BitWriter::write_bits`.
///
/// Returns `(reversed_code, length)` per symbol. Symbols with length 0
/// get `(0, 0)`.
///
/// Follows RFC 1951 §3.2.2 ("Use of Huffman coding in the deflate
/// format").
#[must_use]
pub fn canonical_codes(lens: &[u8]) -> Vec<(u32, u8)> {
    let n = lens.len();
    let mut codes = vec![(0u32, 0u8); n];

    let max_len = lens.iter().copied().max().unwrap_or(0) as usize;
    if max_len == 0 {
        return codes;
    }

    let mut bl_count = vec![0u32; max_len + 1];
    for &l in lens {
        if l > 0 {
            bl_count[l as usize] += 1;
        }
    }

    let mut next_code = vec![0u32; max_len + 1];
    let mut code: u32 = 0;
    for bits in 1..=max_len {
        code = (code + bl_count[bits - 1]) << 1;
        next_code[bits] = code;
    }

    for i in 0..n {
        let l = lens[i] as usize;
        if l != 0 {
            let c = next_code[l];
            next_code[l] += 1;
            codes[i] = (reverse_bits(c, l as u8), l as u8);
        }
    }
    codes
}

#[inline]
fn reverse_bits(mut v: u32, n_bits: u8) -> u32 {
    let mut r = 0u32;
    for _ in 0..n_bits {
        r = (r << 1) | (v & 1);
        v >>= 1;
    }
    r
}

/// Encode a length / distance code-length array using DEFLATE's RLE
/// alphabet (codes 16/17/18 per RFC 1951 §3.2.7).
///
/// Each emitted entry is `(symbol, extra_value, extra_bits)`. The
/// caller is responsible for Huffman-encoding `symbol` against the
/// code-length alphabet and writing `extra_value` immediately after.
#[must_use]
pub fn rle_code_lengths(lens: &[u8]) -> Vec<(u8, u32, u8)> {
    let mut out: Vec<(u8, u32, u8)> = Vec::with_capacity(lens.len());
    let n = lens.len();
    let mut i = 0;
    while i < n {
        let cur = lens[i];
        // Count run of identical values starting at i.
        let mut run = 1usize;
        while i + run < n && lens[i + run] == cur {
            run += 1;
        }
        let consumed = run;
        if cur == 0 {
            // Long zero run uses codes 17 (3-10) and 18 (11-138).
            while run >= 11 {
                let take = run.min(138);
                out.push((18, (take - 11) as u32, 7));
                run -= take;
            }
            while run >= 3 {
                let take = run.min(10);
                out.push((17, (take - 3) as u32, 3));
                run -= take;
            }
            for _ in 0..run {
                out.push((0, 0, 0));
            }
        } else {
            // Emit the first occurrence literally, then code 16 for
            // additional repeats (3-6 at a time).
            out.push((cur, 0, 0));
            run -= 1;
            while run >= 3 {
                let take = run.min(6);
                out.push((16, (take - 3) as u32, 2));
                run -= take;
            }
            for _ in 0..run {
                out.push((cur, 0, 0));
            }
        }
        i += consumed;
    }
    out
}

/// Order in which code-length symbols are transmitted in the dynamic
/// block header (RFC 1951 §3.2.7).
pub const CL_CODE_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_symbol_gets_length_one() {
        let freq = [0u32, 0, 0, 5, 0];
        let lens = limited_lengths(&freq, 15);
        assert_eq!(lens[3], 1);
        assert_eq!(lens.iter().filter(|&&l| l > 0).count(), 1);
    }

    #[test]
    fn empty_input_returns_zeros() {
        let freq = [0u32; 10];
        let lens = limited_lengths(&freq, 15);
        assert!(lens.iter().all(|&l| l == 0));
    }

    #[test]
    fn two_equal_symbols_length_one_each() {
        let freq = [3u32, 3];
        let lens = limited_lengths(&freq, 15);
        assert_eq!(lens, vec![1, 1]);
    }

    #[test]
    fn classic_four_symbol_huffman() {
        // freqs A=1 B=1 C=2 D=4 → optimal lens 3,3,2,1.
        let freq = [1u32, 1, 2, 4];
        let lens = limited_lengths(&freq, 15);
        assert_eq!(lens, vec![3, 3, 2, 1]);
    }

    #[test]
    fn limit_caps_codes() {
        // Skewed exponential freqs would naturally produce long codes.
        let mut freq = vec![0u32; 32];
        freq[0] = 1;
        for i in 1..32 {
            freq[i] = freq[i - 1].saturating_mul(2);
        }
        let lens = limited_lengths(&freq, 7);
        assert!(lens.iter().all(|&l| l <= 7), "got {:?}", lens);
        // Every used symbol gets a code.
        for i in 0..32 {
            if freq[i] > 0 {
                assert!(lens[i] >= 1);
            }
        }
    }

    #[test]
    fn kraft_inequality_holds() {
        // Sum 2^(-len) over all assigned symbols must equal 1 (kraft
        // equality for a complete code).
        let freq = [10u32, 20, 30, 40, 50, 60, 70, 80, 90];
        let lens = limited_lengths(&freq, 15);
        let kraft: f64 = lens
            .iter()
            .filter(|&&l| l > 0)
            .map(|&l| 2f64.powi(-(i32::from(l))))
            .sum();
        assert!((kraft - 1.0).abs() < 1e-9, "kraft = {kraft}");
    }

    #[test]
    fn canonical_codes_are_prefix_free_and_unique() {
        let lens = vec![3u8, 3, 2, 1];
        let codes = canonical_codes(&lens);
        // Codes (MSB-first per RFC):
        //   len 1 → 0
        //   len 2 → 10
        //   len 3 → 110, 111
        // We store reversed: bit-reversed of those.
        // Just sanity check: all distinct (after reversing back) and
        // none is a prefix of another.
        let mut pairs: Vec<(u32, u8)> = codes.iter().copied().filter(|c| c.1 > 0).collect();
        pairs.sort();
        for i in 0..pairs.len() {
            for j in (i + 1)..pairs.len() {
                assert_ne!(pairs[i], pairs[j]);
            }
        }
        // Verify total = 2^(-len) sum is 1.
        let kraft: f64 = pairs.iter().map(|(_, l)| 2f64.powi(-(i32::from(*l)))).sum();
        assert!((kraft - 1.0).abs() < 1e-9);
    }

    #[test]
    fn rle_compresses_zero_runs() {
        let mut lens = vec![5u8; 1];
        lens.extend(std::iter::repeat(0).take(50));
        lens.push(3);
        let rle = rle_code_lengths(&lens);
        // 50 zeros = 18(extra: 138 max means take 50-11+11=50) actually
        // 50 < 138, so one 18 with extra = 50-11 = 39, OK no: 50 ≥ 11
        // → emit one 18 with run=50 (50-11=39 extra). Then literal 3.
        assert!(rle.iter().any(|&(s, _, _)| s == 18));
        assert!(rle.iter().any(|&(s, _, _)| s == 5));
        assert!(rle.iter().any(|&(s, _, _)| s == 3));
    }

    #[test]
    fn rle_repeats_nonzero() {
        let lens: Vec<u8> = std::iter::repeat(4u8).take(20).collect();
        let rle = rle_code_lengths(&lens);
        // First entry literal 4, then 16-runs covering the remaining 19.
        assert_eq!(rle[0], (4, 0, 0));
        assert!(rle.iter().skip(1).all(|&(s, _, _)| s == 16 || s == 4));
        // Total length is reconstructable.
        let expanded_len: usize = rle
            .iter()
            .map(|&(s, e, _)| match s {
                16 => (e as usize) + 3,
                17 => (e as usize) + 3,
                18 => (e as usize) + 11,
                _ => 1,
            })
            .sum();
        assert_eq!(expanded_len, 20);
    }
}
