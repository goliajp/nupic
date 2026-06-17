# 04v — Cycle 59: Luma-sorted palette post-process (v1.1.6)

## Motivation

Cycle 58 ruled out frequency-based palette reordering. But the
deeper P3 hypothesis (palette order affects deflate compression
via LZ77 locality) deserves a second attempt with a SPATIAL signal,
not a popularity signal.

Hypothesis (refined): sort palette by **luma**. Adjacent pixels in
natural photos tend to have correlated luma. If palette is luma-
ordered, the integer difference between consecutive (raster-order)
indices is small. Deflate's LZ77 + Huffman pipeline encodes small
byte deltas more compactly.

## Implementation — 5-line post-process

After `compact_palette()`:

```rust
let lumas: Vec<i32> = palette.iter().map(|c| (c.r + c.g + c.b) as i32 / 3).collect();
let mut order: Vec<usize> = (0..n).collect();
order.sort_by_key(|&i| lumas[i]);
let inv_map = inverse_permutation(&order);
indices = indices.iter().map(|&i| inv_map[i as usize]).collect();
palette = order.iter().map(|&i| palette[i]).collect();
alpha = order.iter().map(|&i| alpha[i]).collect();
```

Zero new dependencies, < 10 lines.

## Bench

```
fixture            baseline_KB   luma_sort_KB    Δ%
04 portrait                451            451     0.00
05 mountain                317            317     0.00
06 landscape               974            974     0.00
07 product                 325            325     0.00
17 aurora 5MP             1266           1266     0.00
25 sofia 5MP              2152           2152     0.00
27 whale 5MP              2946           2921    -0.85
─────────────────────────────────────────────
TOTAL                     8429           8404    -0.30
```

**27 whale -25 KB (-0.85 %)** is the lone significant win. Others
flat. Whale-tail content has structured luma gradient (water + whale
surface curvature) where luma ordering aligns with raster index
sequence.

Baseline-7 marketing ratio: -17.93 % → -17.94 % (+0.01 pp).

## Why it works (when it works)

Whale-tail's luma is roughly monotone across raster scan: top
sky-tones (dark blue), middle water-tones, bottom whale-tones. Luma-
sorted palette puts these in order 0 → 256. Consecutive pixels then
have small index deltas, exploitable by:

- LZ77 finding longer matches in repeated index sequences
- Huffman coding with skewed delta distribution

Other photos have less luma-coherent raster scan (faces, textures,
varied scenery) → palette luma-order doesn't predict pixel-order.

## Negative/marginal value

This is at the very edge of detectable signal:
- 6 / 7 fixtures: 0 % effect
- 1 / 7: -0.85 %
- Total corpus: -0.30 %

For Paper P3 ("joint palette-order + filter co-opt"), this gives a
small confirmed signal but not a strong claim. Worth noting in
paper as evidence that **palette ordering can help on specific
content types**, even if not generically.

## Path forward for P3

To turn -0.3 % into a publishable contribution, need:
1. **Content-aware ordering**: detect "luma-coherent" content via
   signal (probably `var` low + `adj_mn` low) → apply luma-sort.
   Stochastic content → leave alone or use different ordering.
2. **2D spatial ordering**: not just luma (1D) but joint (luma,
   chroma) ordering. May find structure on more content types.
3. **Empirical sweep** over orderings (luma / hue / kmeans-distance
   from mean color / etc.) → demonstrate content-conditional best.

Cycle 59 lands as "free safe optimisation". P3 paper viability
requires those follow-ups.

## Files touched

- `crates/nupic-quantize/src/lib.rs::quantize_indexed_png`
  (luma-sort post-process after compact_palette)
- `crates/nupic-research/examples/speed_sweep.rs` (Cycle 59 bench)
- `docs/research/png/04v-cycle59-luma-sort-palette.md`
- `Cargo.toml` workspace 1.1.5 → 1.1.6
