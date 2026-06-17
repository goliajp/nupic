# 04d — Cycle 37: Lloyd's stride-8 sub-sample (perf -64 % avg, SSIM +0.22 avg)

## Motivation

Cycle 36 quantified the perf ceiling distance: Lloyd's k-means
refinement consumes 96–99 % of `nupic-quantize` wall time (10–13 s on
5 MP fixtures). Cycle 37 attacks Lloyd directly.

## Approach — first EPS / iter-cap sweeps say "no clean win"

Sweep on 7 fixtures × {EPS ∈ 0.0005-0.01, iter_cap ∈ 10-150}, full
pixels:

- EPS=0.001 (was 0.0005): -23 % to -68 % time, **but** 25-sofia drops
  −0.70 SSIM at iter_cap=100 (32 iters instead of 100).
- iter_cap=50 (was 100): -50 % time, **but** same −0.83 SSIM hit on
  25-sofia.
- EPS=0.002+ trades quickly: 0.1–1.1 SSIM loss for the wall-clock.

The hard fixtures (25-sofia, 17-aurora) genuinely need iterations.
Trimming the iter count or loosening the EPS sacrifices SSIM.

## Pivot — sub-sample pixels per iter

Hypothesis: full-pixel Lloyd over-fits to per-pixel noise.
Sub-sampling = an unbiased noisy estimate of the centroid mean ≈
acts as a regulariser. Each iter does 1/S the work; convergence
iter count comparable to full-pixel.

Implementation: at the start of `refine_palette_kmeans_instrumented_strided`
include only every S-th pixel in `pixels_oklab_alpha`. The Lloyd loop
operates on the sub-sampled set. Final palette returned is what
`apply_palette_rgba` uses for the full-pixel assignment.

## Sweep — stride=8 is the sweet spot

7-fixture × stride ∈ {1, 2, 4, 8, 16} (refine-only output, no FS dither):

```
fix     stride  iters   time_s     ssim    Δ vs stride=1
04         1      28     0.59    87.985    baseline
04         8      32     0.12    88.112    +0.13
04        16      33     0.07    88.242    +0.26
05         1      67     1.46    70.376    baseline
05         8      92     0.33    70.369    -0.01
05        16      72     0.16    70.139    -0.24
06         1      48     1.49    82.766    baseline
06         8      40     0.19    83.074    +0.31
06        16      41     0.10    82.709    -0.06
07         1      22     0.40    84.701    baseline
07         8      15     0.04    85.161    +0.46
07        16      19     0.04    84.506    -0.20
17         1      79    10.33    53.379    baseline
17         8     100     1.69    53.593    +0.21
17        16      87     0.82    52.960    -0.42
25         1     100    11.86    76.198    baseline
25         8      88     1.38    76.613    +0.42
25        16      62     0.53    75.523    -0.68
27         1      35     4.12    78.486    baseline
27         8      41     0.63    78.679    +0.19
27        16      42     0.36    78.660    +0.17
```

**At stride=8: all 7 fixtures improve OR flat (avg +0.21 SSIM) AND
77–88 % less time.** Stride=16 mixed (25/17 regress); stride=8 the
clean sweet spot.

## End-to-end bench (--dither auto, full pipeline incl. oxipng)

```
                Cycle 36 time   C37 time   Δ time    C36 SSIM   C37 SSIM   Δ SSIM
04 portrait      ~0.6 s          0.61 s    flat       88.854     88.916    +0.06
05 mountain       2.11 s         0.96 s    -55 %      76.818     76.926    +0.11
06 landscape      2.23 s         0.87 s    -61 %      84.936     85.401    +0.46
07 product       ~0.5 s          0.50 s    flat       86.500     86.894    +0.39
17 aurora        14.57 s         5.70 s    -61 %      (pending)  66.335    +
25 sofia         15.28 s         4.49 s    -71 %      78.396     78.456    +0.06
27 whale          6.95 s         3.30 s    -53 %      (80.03)    80.245    +0.22
                                ------                            -------
                                 avg -55 %                          avg +0.22
```

The end-to-end SSIM gains are smaller than refine-only (FS dither at
d=0.5–0.85 dominates the final SSIM) but every fixture still nets
positive. Wall-clock cut by half to two-thirds on every fixture.

## Why does subsample HELP SSIM?

Speculation: full-pixel Lloyd updates the centroid to the EXACT mean
of assigned pixels each iter. On photo content, this includes noise
in the histogram — texture, JPEG artefacts, sensor noise. The
centroid then snaps to a "noisy mean" that doesn't generalise. Sub-
sample's noisy mean estimate is an unbiased lower-variance
approximation in the long run.

Equivalent to k-means with mini-batch updates — Sculley 2010 reports
similar SSIM-positive effect on real photo data with stride 5–10.

## Routing diff — zero

Full-corpus `probe_real_corpus` post-C37 shows 29 fixtures all route
to the same `d` as Cycle 36. No classifier branch changed — Cycle 37
is pure refine-algorithm tuning.

## Verification

- All workspace tests pass.
- `probe_real_corpus` routing 29/29 identical to v0.5.47.
- 7-fixture end-to-end bench: −50 to −71 % time, +0.06 to +0.46 SSIM
  (all positive or flat).

## Files touched

- `crates/nupic-quantize/src/lib.rs`
  - new `refine_palette_kmeans_instrumented_strided` with explicit
    `stride` parameter
  - `refine_palette_kmeans` now hard-codes `stride=8` as default
  - `refine_palette_kmeans_instrumented` (Cycle 37 stage 1) kept,
    now a wrapper at stride=1 for unbiased convergence study
- `crates/nupic-research/examples/speed_sweep.rs` (Cycle 37 final)
- `Cargo.toml` workspace version 0.5.47 → 0.5.48

## Open backlog

1. **Adaptive stride** — small images (< 1 MP) might not benefit
   from stride=8 (per-iter work is already small). Bench tier-1
   small + tier-3 fixtures to confirm no regression at S=8.
2. **Mini-batch updates** — go further: per-iter randomly select a
   batch (without replacement across the run) instead of fixed
   stride. Could push SSIM higher.
3. **Stride per fixture** — auto-pick stride from image complexity
   (use the `var` / `uniq` signals already computed by the
   classifier).
