//! PNG row filtering per RFC 2083 §6 / spec §9.
//!
//! Five filter types (None / Sub / Up / Average / Paeth) are tried for
//! each row;the encoder picks the one with smallest sum of absolute
//! differences (Heckbert's heuristic, used by libpng and zlib). Each
//! filtered row is prefixed by its filter-type byte (0..=4) before the
//! whole stream is fed to DEFLATE.
//!
//! Indexed PNG (color type 3, bit depth 8) has 1 byte per pixel and
//! filter step `1`. We hardcode that here — the generic `bpp` table
//! is unnecessary for the only color type we currently emit.

/// PNG filter type bytes (RFC 2083 §6).
#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum FilterType {
    None = 0,
    Sub = 1,
    Up = 2,
    Average = 3,
    Paeth = 4,
}

const BPP: usize = 1; // 8-bit indexed → 1 byte per pixel

/// Apply a single PNG filter type to every row. Useful as a
/// global-baseline candidate for the `BestOf` strategy.
#[must_use]
pub fn filter_image_single(width: u32, height: u32, indices: &[u8], ft: FilterType) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let mut out: Vec<u8> = Vec::with_capacity(h * (1 + w));
    let mut buf = vec![0u8; w];
    for y in 0..h {
        let row = &indices[y * w..(y + 1) * w];
        let prev_row: &[u8] = if y == 0 { &[] } else { &indices[(y - 1) * w..y * w] };
        apply_filter(ft, row, prev_row, &mut buf);
        out.push(ft as u8);
        out.extend_from_slice(&buf);
    }
    out
}

/// **BestOf**: produce candidate filtered streams for 7 strategies
/// (5 single-filter + per-row min-SAD + per-row deflate-aware), measure
/// each via `nupic-deflate Level::Fast` as a cheap size proxy, return
/// the smallest.
///
/// Cost: ~ 7 × (filter pass + Level::Fast deflate of whole stream),
/// plus the deflate-aware candidate runs ~ 5 × n_rows trial deflates
/// itself (the NICE_MATCH guard in nupic-deflate prevents the perf
/// cliff that the 2.3-era code worked around by removing this
/// candidate). Final output re-deflates with `Level::Best` downstream
/// so proxy ranking only needs to be approximately correct.
///
/// **Phase 2.5 re-add**:`filter_image_deflate_aware` is back in the
/// candidate set after Cycle 6 Pass 1 gap decomposition showed filter
/// selection contributes 79-97% of the Path-B-vs-Path-A residual gap
/// on photo fixtures(04-portrait +91 KB filter vs +24 KB deflate).
/// NICE_MATCH=128 protects against the cliff that motivated its
/// removal in 0.5.21。
#[must_use]
pub fn filter_image_best_of(width: u32, height: u32, indices: &[u8]) -> Vec<u8> {
    let mut candidates: Vec<Vec<u8>> = Vec::with_capacity(7);
    for ft in [
        FilterType::None,
        FilterType::Sub,
        FilterType::Up,
        FilterType::Average,
        FilterType::Paeth,
    ] {
        candidates.push(filter_image_single(width, height, indices, ft));
    }
    candidates.push(filter_image(width, height, indices));
    candidates.push(filter_image_deflate_aware(width, height, indices));

    candidates
        .into_iter()
        .min_by_key(|filtered| {
            nupic_deflate::deflate_level(filtered, nupic_deflate::Level::Fast).len()
        })
        .unwrap_or_default()
}

/// Choose per-row PNG filter using the **deflate-aware** strategy:
/// for each row,try all 5 filters,deflate the resulting bytes
/// independently,pick the one with smallest compressed size. Optimal
/// per-row (modulo cross-row context) but costs `5 × deflate_per_row`.
///
/// Backed by `filter_image_aware` below — the canonical
/// [`filter_image`] entry uses a cheap min-SAD heuristic instead.
#[must_use]
pub fn filter_image_deflate_aware(width: u32, height: u32, indices: &[u8]) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let mut out: Vec<u8> = Vec::with_capacity(h * (1 + w));
    let mut buf = vec![0u8; w];
    let mut best = vec![0u8; w];

    for y in 0..h {
        let row = &indices[y * w..(y + 1) * w];
        let prev_row: &[u8] = if y == 0 { &[] } else { &indices[(y - 1) * w..y * w] };
        let mut best_filter = FilterType::None;
        let mut best_size = usize::MAX;
        for &ft in &[
            FilterType::None,
            FilterType::Sub,
            FilterType::Up,
            FilterType::Average,
            FilterType::Paeth,
        ] {
            apply_filter(ft, row, prev_row, &mut buf);
            // Use static-Huffman cost as a fast proxy for deflate size.
            // Cycle 10 reversion: per-row Level::Best ranking was tested
            // (commit thread in 03n essay) — uniformly produced larger
            // output than Level::Fast ranking because per-row deflate
            // cost (1200-byte rows) doesn't correlate with cross-row
            // final-stream cost. Stay on Level::Fast as the proxy.
            let size = nupic_deflate::deflate_level(&buf, nupic_deflate::Level::Fast).len();
            if size < best_size {
                best_size = size;
                best_filter = ft;
                best.copy_from_slice(&buf);
            }
        }
        out.push(best_filter as u8);
        out.extend_from_slice(&best);
    }
    out
}

/// Filter every row of `indices` (width × height bytes, row-major) and
/// return the prefix-byte + filtered-bytes stream ready for DEFLATE.
///
/// Per-row, all 5 filters are evaluated and the one with the smallest
/// sum of absolute (signed) differences is committed. Empty rows /
/// images return an empty Vec.
///
/// Strategy = per-row min-SAD (Heckbert's heuristic). For natural-image
/// payloads `filter_image_deflate_aware` typically picks 5-15% better
/// filters by paying the cost of trial DEFLATE per row.
#[must_use]
pub fn filter_image(width: u32, height: u32, indices: &[u8]) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 {
        return Vec::new();
    }

    // Output: per row, 1 byte filter type + w bytes filtered → h × (1 + w).
    let mut out: Vec<u8> = Vec::with_capacity(h * (1 + w));
    let mut buf = vec![0u8; w]; // reusable per-filter scratch
    let mut best = vec![0u8; w]; // best filter's output for current row

    for y in 0..h {
        let row = &indices[y * w..(y + 1) * w];
        let prev_row: &[u8] = if y == 0 { &[] } else { &indices[(y - 1) * w..y * w] };

        let mut best_filter = FilterType::None;
        let mut best_score = u64::MAX;
        for &ft in &[
            FilterType::None,
            FilterType::Sub,
            FilterType::Up,
            FilterType::Average,
            FilterType::Paeth,
        ] {
            apply_filter(ft, row, prev_row, &mut buf);
            let score = sad(&buf);
            if score < best_score {
                best_score = score;
                best_filter = ft;
                best.copy_from_slice(&buf);
            }
        }
        out.push(best_filter as u8);
        out.extend_from_slice(&best);
    }
    out
}

/// Apply `ft` to `row`, writing the filtered bytes into `out`.
/// `prev_row` is empty for y=0.
fn apply_filter(ft: FilterType, row: &[u8], prev_row: &[u8], out: &mut [u8]) {
    let w = row.len();
    debug_assert_eq!(out.len(), w);
    for x in 0..w {
        let a = if x >= BPP { row[x - BPP] } else { 0 };
        let b = if !prev_row.is_empty() { prev_row[x] } else { 0 };
        let c = if x >= BPP && !prev_row.is_empty() { prev_row[x - BPP] } else { 0 };
        let v = row[x];
        out[x] = match ft {
            FilterType::None => v,
            FilterType::Sub => v.wrapping_sub(a),
            FilterType::Up => v.wrapping_sub(b),
            FilterType::Average => v.wrapping_sub(((u16::from(a) + u16::from(b)) / 2) as u8),
            FilterType::Paeth => v.wrapping_sub(paeth_predictor(a, b, c)),
        };
    }
}

/// Mean length of consecutive identical-byte runs in `data`. Used as
/// a cheap classifier in `encode_indexed_png` to detect highly-
/// compressible run-heavy input(transparent regions, flat UI panels)
/// and skip Level::Best iterative cost-DP overhead。
#[must_use]
pub fn mean_run_length(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut runs: u64 = 0;
    let mut total_runs: u64 = 0;
    let mut prev = data[0];
    let mut cur_run: u64 = 1;
    for &b in &data[1..] {
        if b == prev {
            cur_run += 1;
        } else {
            runs += cur_run;
            total_runs += 1;
            cur_run = 1;
        }
        prev = b;
    }
    runs += cur_run;
    total_runs += 1;
    runs as f64 / total_runs as f64
}

/// Heckbert's per-row heuristic: sum of |signed-byte| values. PNG spec
/// §12.8 calls this "minimum sum of absolute differences";libpng / zlib
/// use it because it correlates well with DEFLATE compressibility while
/// being O(w) per row(vs trying each filter through full DEFLATE).
fn sad(buf: &[u8]) -> u64 {
    buf.iter()
        .map(|&b| {
            let s = b as i8;
            if s < 0 { (-(s as i16)) as u64 } else { s as u64 }
        })
        .sum()
}

/// PNG Paeth predictor (spec §12.6). Returns whichever of `a`, `b`, `c`
/// is closest to `a + b - c`.
fn paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
    let p = i32::from(a) + i32::from(b) - i32::from(c);
    let pa = (p - i32::from(a)).abs();
    let pb = (p - i32::from(b)).abs();
    let pc = (p - i32::from(c)).abs();
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paeth_basic() {
        // From PNG spec sample table.
        assert_eq!(paeth_predictor(0, 0, 0), 0);
        assert_eq!(paeth_predictor(10, 10, 10), 10);
        assert_eq!(paeth_predictor(100, 50, 25), 100); // a closest to 125
    }

    #[test]
    fn none_filter_passes_through() {
        let row = [1u8, 2, 3, 4];
        let mut out = [0u8; 4];
        apply_filter(FilterType::None, &row, &[], &mut out);
        assert_eq!(out, [1, 2, 3, 4]);
    }

    #[test]
    fn sub_filter_zeros_constant_row() {
        let row = [5u8, 5, 5, 5];
        let mut out = [0u8; 4];
        apply_filter(FilterType::Sub, &row, &[], &mut out);
        assert_eq!(out, [5, 0, 0, 0]); // first byte is row[0]-0
    }

    #[test]
    fn up_filter_zeros_repeated_row() {
        let row = [1u8, 2, 3, 4];
        let prev = [1u8, 2, 3, 4];
        let mut out = [0u8; 4];
        apply_filter(FilterType::Up, &row, &prev, &mut out);
        assert_eq!(out, [0, 0, 0, 0]);
    }

    #[test]
    fn filter_image_empty() {
        assert!(filter_image(0, 5, &[]).is_empty());
        assert!(filter_image(5, 0, &[]).is_empty());
    }

    #[test]
    fn filter_image_constant_picks_zero_score() {
        // All bytes identical → Up filter zeros everything (except row 0).
        // Row 0: Sub zeros all except first byte → score = 5.
        // Subsequent rows: Up zeros all → score = 0.
        let img = vec![7u8; 5 * 3];
        let out = filter_image(5, 3, &img);
        assert_eq!(out.len(), 3 * (1 + 5));
        // Row 1 and 2 should have filter type = Up = 2 and all zero data.
        assert_eq!(out[1 + 5], FilterType::Up as u8);
        assert_eq!(&out[1 + 5 + 1..1 + 5 + 1 + 5], &[0, 0, 0, 0, 0]);
    }
}
