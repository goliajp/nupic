# 03o — Cycle 14: Lloyd's k-means perf -22% (v0.5.31)

## Problem

Per `cycle14_perf_breakdown` on 05-photo-mountain (1200×800,
the largest opaque fixture):

| stage | ms | % |
|---|---|---|
| image decode | 8 | 0.3% |
| imagequant train | 126 | 4.6% |
| **Lloyd's refine 100 iter** | **2270** | **82.5%** |
| apply_palette (no dither) | 21 | 0.8% |
| apply_palette_fs_dither | 265 | 9.6% |
| encode PNG | 12 | 0.4% |
| oxipng preset=5 | 314 | 11.4% |
| **Total** | **2751** | 100% |

Lloyd's refine dominates 82.5% of encode time. Real perf bottleneck.

## Root cause

`refine_palette_kmeans` was calling `srgb_u8_to_oklab` **three times
per pixel per iter**:
1. Parallel assign loop (line 317): convert pixel → match nearest centroid
2. Sequential sum-accumulate (line 347): convert pixel → add to cluster sum
3. Sequential SSE compute (line 363): convert pixel → compute squared error

For 05 (960K pixels × 100 iters × 3 conversions) that's **288 million
sRGB→OKLab conversions**. Each conversion involves cube-root and 3×3
matrix multiply.

## Fix

**Two changes, both algebraic + cache-friendly:**

### 1. Precompute OKLab once

Convert each pixel to OKLab + alpha **once**, store in
`pixels_oklab_alpha: Vec<(f32, f32, f32, u8)>`. All three iter loops
read from this precomputed vec.

Memory cost: ~15 MB for a 1200×800 image (16 bytes/pixel).
Conversion cost: 960K (one upfront pass) vs 288M (per-iter ×3).

Also rewrote the parallel-assign loop with `par_chunks` + paired
`par_chunks_mut` for `assigned`, avoiding per-element parallelism
overhead.

### 2. Collapse SSE pass via Σx² identity

Previous: two sequential O(N) passes per iter — one to accumulate
sum/count, one to compute per-cluster SSE (needs the mean from pass 1).

New: accumulate Σx² in the same pass as Σx, then compute
**SSE_j = Σx² − (Σx)² / count** by closed-form post-pass. One pass
instead of two.

For 05 at 100 iters: drops from 200 × 960K = 192M sequential ops to
100 × 960K = 96M.

## Result

Same `cycle14_perf_breakdown` re-run:

| stage | pre | post | Δ |
|---|---|---|---|
| imagequant train | 126 | 131 | +5 (noise) |
| **Lloyd's refine 100 iter** | **2270** | **1669** | **-601 (-26%)** |
| apply_palette_fs_dither | 265 | 267 | +2 |
| oxipng preset=5 | 314 | 314 | 0 |
| **Total** | **2751** | **2162** | **-589 (-21%)** |

7-fixture corpus encode wall-clock (with --dither auto):
- pre: not measured this cycle; per-image savings ≈ 0.5 s extrapolates
  to ~3.5 s saved across 7 fixtures.
- post: 12.0 s total wall (01: 1.13s, 02: 0.95s, 03: 0.33s, 04: 1.51s,
  05: 3.78s, 06: 3.15s, 07: 1.15s).

## Output equivalence

All 7 fixtures produce **bit-exact identical** PNG bytes + SSIMULACRA2
scores after the optimization:

```
fixture                       size       SSIM    pre→post
01-png-transparency-demo     45364    -46.426    identical
02-pluto-transparent        162009     80.441    identical
03-wikipedia-logo            14718    100.000    identical
04-photo-portrait           499378     88.854    identical
05-photo-mountain           473174     76.818    identical
06-photo-landscape         1109644     84.936    identical
07-photo-product            404312     86.500    identical
```

The Σx² identity preserves floating-point ordering across the
split-on-empty heuristic, so palette evolution path is unchanged.

## Files

- `crates/nupic-quantize/src/lib.rs` — refine_palette_kmeans rewrite
- `crates/nupic-research/examples/cycle14_perf_breakdown.rs` — profile bench
