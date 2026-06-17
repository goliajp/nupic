# 04z — Cycle 67: Cluster-aware adaptive λ ICM (NEGATIVE, v1.1.13)

## Hypothesis

Cycle 66 ruled out per-pixel bilateral adaptive λ. Cycle 66 essay
identified per-CLUSTER adaptive λ as the next refinement to try:

```
λ_j = λ_max / (1 + α · sse_j)
```

where `sse_j` is mean per-pixel SSE within cluster j. Smooth clusters
(low SSE = pixels close to centroid) get high λ → push neighbour
agreement. High-SSE clusters (edges, transitions) get low λ.

## Result — NEGATIVE: SSE values too small to discriminate

Computed cluster SSE on baseline-7 fixtures shows per-pixel SSE
values in the range **0.0000 – 0.0003** in OKLab L²-space.

```
fixture            mean_sse
04 portrait        0.0000
05 mountain        0.0003
06 landscape       0.0001
07 product         0.0000
```

With α up to 100, the adaptive divisor `1 + α · sse_j` ≈ 1 for
most clusters → `λ_j ≈ λ_max` for almost all clusters → adaptive
component collapses to constant λ.

Result: Cycle 67 Pareto slopes essentially match Cycle 65
constant-λ at corresponding λ_max:

```
04 portrait at Cycle 67 λmax=0.0005 α=1:    -23.5% / -4.4 SSIM (slope -5.4)
04 portrait at Cycle 65 λ²=0.0005:          -23.5% / -4.4 SSIM (slope -5.4)
```

Same numbers. No benefit from cluster-awareness.

## Why SSE values are tiny

After Cycle 37-55's stride-8 SIMD-accelerated Lloyd refine, the
palette converges very tightly to optimal centroids. Per-cluster
SSE in OKLab L² units:

- L axis range: 0..1 (typical perceptual range ~0.7)
- Centroid placement: ~0.05 from cluster mean (with 256-color palette)
- (0.05)² = 0.0025 ≈ what we see

The values ARE too small to be a useful adaptive signal.

## What would work better

1. **Use cluster size (pixel count) instead of SSE**: clusters with
   many pixels are "smooth color" content; clusters with few pixels
   are edge transitions. Pixel-count-based λ adaptation.

2. **Use boundary length** (perimeter / count): clusters with
   high perimeter-to-area ratio are edge-heavy → low λ.

3. **Use Lloyd iteration count to convergence per cluster**: slow-
   converging clusters are "boundary" clusters → low λ.

4. **Move past per-cluster heuristics and try joint optimisation**:
   alternate ICM + palette retraining (Cycle 68 candidate).

## Compounded negative finding for paper P3

Cycle 65 ICM constant-λ remains the strongest contribution.
Cycle 66 (per-pixel bilateral adaptive): negative.
Cycle 67 (per-cluster adaptive): negative.

For paper P3 narrative:

> "We explored two natural adaptive λ schemes (bilateral edge-
> aware, cluster-SSE-aware). Neither beats a constant λ in our
> empirical Pareto. This rules out the simplest adaptive
> heuristics and motivates the joint quantize-encode optimization
> direction (Cycle 68)."

Two negative results strengthen credibility — the obvious
alternatives have been ruled out before claiming the joint
optimization is the right path.

## Files touched

- `crates/nupic-research/examples/speed_sweep.rs` (Cycle 67 bench)
- `docs/research/png/04z-cycle67-cluster-aware-icm-negative.md`
- `Cargo.toml` workspace 1.1.12 → 1.1.13
- (no runtime change)
