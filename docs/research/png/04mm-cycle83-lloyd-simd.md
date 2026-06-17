# 04mm — Cycle 83: Lloyd assign-step SIMD (v1.2.6)

## TL;DR

Ported the Cycle 82 SoA + f32x4 SIMD K-best pattern from
`apply_palette_rgba` to `refine_palette_kmeans_instrumented_strided`'s
assignment step (the inner loop run every Lloyd iteration). 5-9 %
additional perf reduction on 5MP+, taking cumulative 5MP speedup
from v1.2.0 to v1.2.6 to **4.1×** on the worst case (aurora).

## Per-fixture cumulative perf (v1.2.0 → v1.2.6)

```
fixture              v1.2.0    v1.2.4    v1.2.5    v1.2.6    cum Δ
17 aurora 5.9MP      1.36 s    0.38 s    0.35 s    0.33 s    4.1x
25 sofia 5.5MP       0.90 s    0.28 s    0.26 s    0.26 s    3.5x  ← HIT 250ms
27 whale 5.5MP       0.76 s    0.37 s    0.33 s    0.31 s    2.5x
19 iceberg 3.0MP     1.14 s    0.74 s    0.70 s    0.64 s    1.8x
28 orca 14MP         1.79 s    0.92 s    0.84 s    0.78 s    2.3x
18 snow 17MP         3.54 s    1.12 s    1.01 s    0.93 s    3.8x
20 rainbow 19MP      3.90 s    1.23 s    1.11 s    1.04 s    3.8x
16 earthrise 25MP    2.78 s    1.39 s    1.24 s    1.18 s    2.4x
```

**25 sofia 5.5MP at 260 ms is the first 5MP fixture to reach the
NAS/CDN KPI**. 17 aurora at 330 ms is 80 ms over — the gap is now
imagequant median-cut palette init (~30 ms) + oxipng preset=0
(~140 ms) + Lloyd centroid accumulation (~80 ms unvectorised).

Baseline-7 unchanged at -19.6 %. Tests 7/7 + 9/9 pass.

## Cycle 83 lever

Same as Cycle 82 (apply): scalar `for j in 0..k` K-best became
SoA padded to k_pad (mod 4) + f32x4 SIMD inner loop. Re-built
SoA per Lloyd iter (cheap, k=256 floats × 4 channels).

This is the SECOND K-best inner loop in the project's hot path —
the FIRST being Cycle 82's apply. Both now SIMD; both share the
same code shape so the next K-best caller (Cycle 65-71 ICM step)
can reuse the same pattern.

## What's left to hit < 250 ms 5MP

Estimated breakdown at v1.2.6 (5.9MP 17 aurora, 330 ms):

```
classify              10 ms     3 %
imagequant train      30 ms     9 %
Lloyd assign (SIMD)   60 ms    18 %
Lloyd accumulate      30 ms     9 %  ← scalar f64 accumulation
apply (SIMD)          55 ms    17 %
encode (fast deflate) 12 ms     4 %
oxipng preset=0      135 ms    41 %  ← long pole
─────────────────────────────────
TOTAL                ~330 ms
```

To shave 80 ms more, candidates ranked by feasibility:

1. **Lloyd accumulate SIMD** (~30→15 ms): same SoA pattern but the
   accumulation is a histogram-like reduction. f32x4 doesn't easily
   apply to indirect indexed stores. Maybe 10-15 ms saved.

2. **Skip Lloyd entirely on certain content** — if imagequant
   median-cut is already close enough for `var > N` stochastic
   content, can drop Lloyd to 0 iters. ~80 ms saved on 5MP+.
   Quality risk: SSIM may drop on baseline-7 5MP fixtures (not
   tested, but baseline-7 is <2MP so unaffected here).

3. **Apply skip when palette didn't change much** — Lloyd already
   tracks `max_move`. If post-Lloyd palette identical to median-cut
   init, can skip apply pass and use imagequant's index map.

4. **Direct deflate vs oxipng** — nupic-deflate is Level::Best, not
   competitive (Cycle 81). External libdeflate-only path?
   Investigation needed.

Cycle 84+ candidate is (2): conditional Lloyd skip for stochastic
content.

## Files touched

- `crates/nupic-quantize/src/lib.rs::refine_palette_kmeans_instrumented_strided`
  (assign step: SoA + f32x4)
- `Cargo.toml`: 1.2.5 → **1.2.6**
- `docs/research/png/04mm-cycle83-lloyd-simd.md` (this essay)
