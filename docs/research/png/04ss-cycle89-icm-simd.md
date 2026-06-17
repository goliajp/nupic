# 04ss — Cycle 89: R9 ICM SIMD (wide::f32x4) — GREEN clean ship-ready

## TL;DR

ICM scalar inner loop (per-pixel iterate K=192-256 palette entries
computing OKLab L² + Potts smoothness) replaced with SoA palette
+ `wide::f32x4` 4-lane SIMD. Same pattern as Cycle 82/83
`apply_palette_rgba` SIMD.

**Result: clean perf win across all 6 fixtures, bit-exact output.**

| fixture          |   MP  | n_col | scalar ms | SIMD ms |    Δms    | speedup |
|------------------|------:|------:|----------:|--------:|----------:|--------:|
| 04 portrait      |   <1  |   208 |    728.3  |   401.3 |  **−327** |  1.81×  |
| 06 landscape     |    1  |   208 |   1014.3  |   602.6 |  **−412** |  1.68×  |
| 07 product       |   <1  |   208 |    557.6  |   332.0 |  **−226** |  1.68×  |
| 17 aurora 5.9MP  |    5  |   256 |   5102.8  |  3048.7 | **−2054** |  1.67×  |
| 25 sofia 5.5MP   |    5  |   144 |   2544.8  |  2039.1 |   −506    |  1.25×  |
| 27 whale 5.5MP   |    5  |   256 |   5126.5  |  3066.4 | **−2060** |  1.67×  |

All fixtures: **ΔSSIM = +0.000, Δsize = +0.00 %** (bit-exact output).

**R9 gate (per roadmap):** baseline-7 04/06/07 −50–200 ms ICM time.
Achieved −226 / −327 / −412 ms — comfortably exceeds.

**Status: ship-ready, no opt-in needed.** Zero algorithmic risk; only
the inner loop changed.

## Why bit-exact?

The SIMD inner loop computes the **same** OKLab L² + Potts smoothness
cost as scalar, just 4 palette entries at a time. The argmin reduction
preserves the scalar tie-breaking rule (first-min-wins via blend
mask) because:

1. `cost.cmp_lt(min_d2)` is strict `<` (not `≤`), so ties don't
   blend in.
2. Lane-by-lane reduction at the end uses `arr[k] < best_d` (also
   strict). Same tie-breaking direction as the scalar `if cost <
   best_cost` pattern.

The padded lanes (`pal_l[k_real..k_pad] = 1e9`) never win the
argmin because their data² >> any real palette entry's data².

Result: identical indices, identical palette retrain inputs,
identical final encoding.

## Why 1.25× on 25 sofia (vs 1.67× elsewhere)

25 sofia has `n_colors = 144` (classifier-picked) — only ~36 SIMD
chunks of 4 (with k_pad = 144). Per pixel: ~36 SIMD iterations vs
~64 SIMD iterations for n=256 fixtures. The SIMD inner loop has
some fixed setup cost (neighbor masks, splats, etc.) that's
amortized less effectively at lower K.

64-chunk fixtures get ~1.67× speedup; 36-chunk fixture gets ~1.25×.
This is expected behavior; not a bug.

## 5MP+ implications

R9 was sized as "engineering paper material" (★★) but the 5MP+
savings are paper-worthy on their own:

- 17 aurora: 5.10 s → 3.05 s
- 27 whale:  5.13 s → 3.07 s

That's −2 s per fixture, large fraction of total encode time. Combined
with Cycle 82/83's `apply_palette_rgba` SIMD, the OKLab inner loops
across the entire pipeline now have a unified SIMD attack surface.

Direct contribution to the 5MP < 250 ms perf KPI: ICM dominates
encode time on these fixtures, so a 1.67× ICM speedup is a ~25 %
total wall-time reduction for 5MP+ content.

## What this rules in / out

**Ruled in for production (next minor):**
- Merge `icm_step_simd` into `nupic-quantize`'s ICM code path
  (`crates/nupic-quantize/src/lib.rs:190` area). Strict 1.67× ICM
  speedup, bit-exact output. Risk-free engineering win.
- Same SIMD pattern (SoA palette + f32x4 + cmp_ne smoothness count)
  may apply to other ICM-class loops the codebase may grow (e.g.,
  R1 M-weighted Lloyd's assignment step — Cycle 90 should bench that
  too).

**Considered + skipped for spike:**
- 8-lane (f32x8 via `wide` or AVX2 256-bit): more complex on
  aarch64 (no native 256-bit), would need scalar fallback. f32x4
  works on both NEON and SSE; deferred 8-lane to a future spike if
  needed.
- Rayon parallelism over pixel rows: orthogonal to SIMD; can stack.
  Deferred — Cycle 90+ may combine.
- LUT-based Potts count: precompute palette-index → bit-pattern
  for fast neighbor-difference count. SIMD path already vectorizes
  the count; LUT may not improve on it. Skipped.

## See also

- `crates/nupic-research/examples/icm_simd.rs` — spike harness
  (scalar baseline + SoA + f32x4 path, bit-exact verification).
- `docs/research/png/04mm-cycle83-lloyd-simd.md` — Cycle 83 SIMD on
  Lloyd assign step; same `wide::f32x4` pattern.
- `docs/research/png/04ll-cycle82-apply-palette-simd.md` (if exists)
  — Cycle 82 origin.
- `crates/nupic-quantize/src/lib.rs:190-244` — ICM scalar code that
  the merge will replace.
- `memory/research_roadmap_1_2_x.md` — R9 GREEN.
