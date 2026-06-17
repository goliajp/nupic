# 04ll — Cycle 82: apply_palette_rgba SIMD (v1.2.5)

## TL;DR

`apply_palette_rgba` previously did scalar inner-loop K-best search
(L2 in OKLab+α) per pixel. Vectorised to f32x4 SIMD via `wide`,
matching the SoA pattern already used in
`refine_palette_kmeans_importance`. **5-11 % wall-clock reduction
on 5MP+ encodes**, with zero size or quality change.

## Per-fixture perf (v1.2.4 → v1.2.5)

```
fixture              v1.2.4 t   v1.2.5 t   Δ
17 aurora 5.9MP      0.38 s     0.35 s    -8 %
25 sofia 5.5MP       0.28 s     0.26 s    -7 %
27 whale 5.5MP       0.37 s     0.33 s   -11 %
19 iceberg 3.0MP     0.74 s     0.70 s    -5 %
28 orca 14MP         0.92 s     0.84 s    -9 %
18 snow 17MP         1.12 s     1.01 s   -10 %
20 rainbow 19MP      1.23 s     1.11 s   -10 %
16 earthrise 25MP    1.39 s     1.24 s   -11 %
```

Baseline-7 unchanged at -19.6 % vs TinyPNG (small fixtures < 2 MP
get a tiny speedup but it doesn't reach the bench timing
resolution). Tests 7/7 + 9/9 pass.

## Code change

```rust
// Before: scalar K-best
for j in 0..k {
    let pj = palette_oklab[j];
    let dl = p.l - pj.l;
    let da = p.a - pj.a;
    let db = p.b - pj.b;
    let d_alpha = pa_scaled - palette_alpha_scaled[j];
    let d2 = dl*dl + da*da + db*db + d_alpha*d_alpha;
    if d2 < best_d2 { best_d2 = d2; best_j = j; }
}

// After: f32x4 SoA SIMD
let k_pad = (k + 3) & !3;
// pal_l/pal_a/pal_b/pal_as Vec<f32> built once, INFINITY-padded
src_rgba.par_chunks_exact(4).zip(indices.par_chunks_exact_mut(1)).for_each(|(px, idx)| {
    let px_l = f32x4::splat(p.l); ...
    let mut min_d2 = f32x4::splat(f32::INFINITY);
    let mut min_idx = f32x4::from([0.0, 1.0, 2.0, 3.0]);
    let mut j = 0;
    while j < k_pad {
        let pj_l = f32x4::new([pal_l[j], ..., pal_l[j+3]]);
        ...
        let d2 = dl*dl + da*da + db*db + das*das;
        let mask = d2.cmp_lt(min_d2);
        min_d2 = mask.blend(d2, min_d2);
        min_idx = mask.blend(idx_iter, min_idx);
        idx_iter += four;
        j += 4;
    }
    // horizontal min over 4 lanes → write idx
});
```

The K-best inner loop steps by 4 palette entries per iteration
instead of 1. For n=256 palette this is 64 SIMD iters vs 256
scalar iters, theoretical 4× inner-loop speedup. Actual 5-11 %
because:

- Rayon outer parallelism already saturates cores on 5MP+
- Apply was only ~10-25 % of total wall time after Cycle 79's
  oxipng cuts
- Memory bandwidth for palette table dominates on the small
  n=256 inner loop (palette fits in L1 cache, so SIMD's
  arithmetic gain is partially offset by load throughput)

## Cumulative perf progression (5.9MP 17 aurora)

```
version     5MP encode    notes
v1.2.0      1.36 s        baseline (preset=1, joint anneal, scalar)
v1.2.4      0.38 s        Cycle 79 preset=0 + 3-tier cap
v1.2.5      0.35 s        Cycle 82 SIMD apply
target      < 0.250 s     NAS/CDN KPI
```

Still 1.4× over the 250 ms target. Remaining stages on 5MP at
v1.2.5 (estimated):

- classify: ~10 ms
- train (imagequant): ~30 ms
- Lloyd (cap=10, stride=16): ~100 ms
- apply (SIMD): ~60 ms
- encode (intermediate PNG, fast deflate): ~12 ms
- oxipng preset=0: ~140 ms
- TOTAL: ~352 ms

To hit 250 ms, need ~100 ms more from Lloyd or oxipng. The
Cycle 81 negative result rules out nupic-png as the perf bypass.
Remaining levers: Lloyd SIMD (Cycle 83 candidate), imagequant
replacement, or a parallel pipeline where Lloyd runs while
oxipng is also working.

## Files touched

- `crates/nupic-quantize/src/lib.rs::apply_palette_rgba` (SoA-SIMD)
- `Cargo.toml`: 1.2.4 → **1.2.5**
- `docs/research/png/04ll-cycle82-apply-simd.md` (this essay)

## Paper material

Apply SIMD is the cleanest perf optimisation in the cycle — it
trades exactly nothing (size, quality, complexity all preserved)
for measurable latency reduction. The pattern (SoA + f32x4 +
masked-blend min-reduce) is reusable wherever the project does
K-best search in OKLab+α. Cycle 65-71's joint anneal ICM step
could also be vectorised this way (Cycle 84+ candidate).
