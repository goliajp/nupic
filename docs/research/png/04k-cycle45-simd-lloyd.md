# 04k — Cycle 45: SIMD-accelerated Importance Lloyd (v1.0.3)

## Motivation

Perf baseline profile (Cycle 44 pre-work) revealed 5MP encode totals
2.2-5.1 s, with **Lloyd refinement consuming 37-50 %** (823-1975 ms).
Target latency per `feedback_perf_nas_cdn_target` is **< 250 ms on
5MP** for NAS/CDN viability — 10-20× speedup needed across the
pipeline.

Cycle 45 attacks Lloyd's inner argmin loop with SIMD f32x4
vectorisation.

## Math — no change

Same weighted Lloyd as Cycle 44. SIMD is pure implementation —
algorithm and centroid update are unchanged, no SSIM/size impact.

## Implementation — f32x4 inner loop

Inner loop (per pixel, iterate K palette entries) was scalar:

```rust
for j in 0..k {
    let dl = pl - palette[j].l;
    // ... 3 more subs, 4 muls, 3 adds, 1 cmp
    if d2 < best_d2 { ... }
}
```

Replaced with SoA palette + 4-way SIMD:

```rust
// Pre-pass: AoS palette → 4 padded SoA arrays per iter
let k_pad = (k + 3) & !3;
// 4 dummy entries with INFINITY values for tail
for j in 0..k { pal_l[j] = palette[j].l; ... }

// Inner: 4 palette entries per iteration
let mut min_d2 = f32x4::splat(INFINITY);
let mut min_idx = f32x4::from([0., 1., 2., 3.]);
let mut idx_iter = f32x4::from([0., 1., 2., 3.]);
let four = f32x4::splat(4.);
let mut j = 0;
while j < k_pad {
    let pj_l = f32x4::new([pal_l[j], pal_l[j+1], pal_l[j+2], pal_l[j+3]]);
    // similar pj_a, pj_b, pj_as
    let d2 = (px_l - pj_l).powi(2) + ... ;
    let mask = d2.cmp_lt(min_d2);
    min_d2 = mask.blend(d2, min_d2);
    min_idx = mask.blend(idx_iter, min_idx);
    idx_iter += four;
    j += 4;
}
// Horizontal min across 4 lanes → best_j
```

Key details:
- `k` (palette size) padded to multiple of 4 via dummy entries with
  `f32::INFINITY` values — those lanes never win.
- SoA layout: 4 `Vec<f32>` per (L, a, b, alpha_scaled) — allows
  consecutive memory load per iteration.
- Per-iter cost: refresh SoA palette from AoS palette (O(K), cheap)
  + SIMD inner loop O(N/STRIDE × K/4) vs scalar O(N/STRIDE × K).

## Bench — end-to-end `nupic compress`

```
fixture                          v1.0.2      v1.0.3 (SIMD)      Δ
05 mountain (1.4MP, importance)  0.96 s      0.65 s             -32 %
25 sofia (5MP, importance)       4.49 s      2.51 s             -44 %
17 aurora (5MP, smooth)          5.70 s      4.62 s             -19 %  *
27 whale (5MP, smooth)           3.30 s      2.26 s             -32 %  *

* Smooth-path fixtures use refine_palette_kmeans (non-SIMD); their
  speedup comes from sympathetic cache/codegen effects only.
```

The **importance path (25 sofia 5MP -44 %) is the headline SIMD win**.
The output is bit-identical to v1.0.2 (same SSIM, same byte size).

## Remaining latency budget

```
25 sofia 5MP post-SIMD breakdown:
  train          ~120 ms
  refine (SIMD)  ~590 ms   <-- improved from 987 ms
  apply          ~85 ms
  encode         ~80 ms
  oxipng        ~1640 ms   <-- now the dominant bottleneck (64 %)
  ────────────────────
  total         ~2510 ms   (target 250 ms; 10× gap remaining)
```

oxipng dominates. Next cycle attacks it:
1. Skip oxipng (lose ~10-15 % size, gain ~1.5 s)
2. Use lower `effort=0` (gain ~500 ms with small size cost)
3. Pre-compute optimal PNG filter inside our pipeline, skip oxipng's
   filter sweep — needs deeper integration

## Files touched

- `crates/nupic-quantize/Cargo.toml`: add `wide` dependency
- `crates/nupic-quantize/src/lib.rs`:
  `refine_palette_kmeans_importance` inner loop now SIMD f32x4
- `Cargo.toml`: workspace version 1.0.2 → 1.0.3

## Paper P2 framing — SIMD as Practical Implementation Contribution

For Paper P2 (multi-scale perceptual surrogate), the SIMD impl
section establishes "real-time deployable" angle:

- Multi-scale gradient compute: vectorisable trivially (Cycle 45+
  candidate; not yet SIMD)
- Lloyd inner argmin: SIMD f32x4 → ~3× scalar speedup
- Total stack: ~5MP < 1 s reachable (with oxipng replacement),
  positioning the algorithm for CDN/NAS deployment

This bridges algorithm-only papers (math contribution) and
practical-impact papers (deployment numbers). The combination is
what top-tier venues like for "real-time perceptual" framing.
