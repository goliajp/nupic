# 04y — Cycle 66: Adaptive λ ICM (NEGATIVE refinement, v1.1.12)

## Hypothesis

Cycle 65 demonstrated constant-λ ICM gives 8× steeper Pareto slope
than the palette-size knob, but a single λ trades fidelity globally.
Hypothesis: per-pixel **bilateral-style adaptive λ**:

```
λ_i = λ_max / (1 + α · local_grad_i²)
```

- High λ in smooth regions (where neighbour agreement helps)
- Near-zero λ at edges (preserve boundary fidelity)

This is the natural bilateral-filter adaptation applied to MRF ICM.

## Experiment

Same fixtures as Cycle 65 (04/05/06/07). Sweep (λ_max, α) combos.
Edge gradient is reused from Cycle 44's multi-scale gradient
infrastructure.

## Results — adaptive does NOT outperform Cycle 65 constant

### 04 portrait (1MP)

```
                      size       SSIM    Δsize    Δssim     slope_%/SSIM
baseline (no ICM)     451 KB    86.07     0 %      0           —
Cycle 65 λ²=0.0001    382 KB    85.09   -15.48   -0.99       -15.6  ← winner
Cycle 66 λmax=0.01 α=1.0  361   81.36   -20.05   -4.71        -4.3
Cycle 66 λmax=0.005 α=0.1 341   78.06   -24.39   -8.01        -3.0
```

Cycle 65 constant-λ at -15.6 %/SSIM beats every Cycle 66 adaptive
config (best -4.3 %/SSIM).

### 05 mountain (1.4 MP)

```
                              Δsize    Δssim     slope
baseline                       0 %      0          —
λmax=0.010 α=1.0              -8.1   -2.00      -4.0  ← best C66
λmax=0.001 α=0.001            -13.3  -3.71      -3.6
λmax=0.005 α=0.100            -9.9   -3.22      -3.1
```

(Cycle 65 didn't bench 05 specifically; the comparison point is
the palette-size knob at -1.9 %/SSIM which Cycle 66 -4.0 still
beats — but by less than Cycle 65 constant).

### 06 landscape, 07 product

Both noticeably worse than Cycle 65. 06 even degraded to -1.4 %/SSIM
which is the SAME slope as the palette-size knob — no improvement.

## Why adaptive doesn't help here

Inspection of grad_sq values reveals two issues:

1. **High-gradient regions are RARE.** In typical photos < 15 % of
   pixels have gradient² > moderate threshold. So the bilateral
   adaptation mostly acts as a global λ ≈ λ_max anyway, with edges
   getting a tiny λ that does nothing. The "edge preservation" gain
   is in a small set of pixels.

2. **Boundary preservation isn't the right hypothesis.** ICM's
   SSIM cost concentrates at *texture transitions*, not at *high
   gradients*. Texture variation produces gradients but also produces
   pixels with palette-distance gradient that ICM happily smooths.
   The bilateral heuristic doesn't distinguish these cases.

3. **Lack of perceptual weighting.** SSIMULACRA2 weights luma errors
   more than chroma errors. A truly perceptually-aware adaptive λ
   would consider WHICH pixel changes the eye notices, not just
   "is there a gradient here".

## What WOULD work as adaptive direction (Cycle 67+ candidates)

1. **Per-cluster λ**: clusters with high split-on-empty churn or
   high boundary density get higher λ. Aligns with content
   structure.

2. **SSIM-buffer-aware λ**: fixture-level signal that detects how
   much SSIM headroom there is vs TinyPNG (or our internal
   reference) — high-buffer fixtures get aggressive λ, gate-
   sensitive ones (baseline-7) get λ ≈ 0.

3. **Learned λ predictor**: small NN takes local stats (var,
   adj_mn, uniq_in_neighborhood) → predicts optimal λ. Requires
   training data (could be self-generated from labelled corpus).

## Paper P3 framing impact

Cycle 65 ICM stays as paper P3's primary positive contribution.
Cycle 66 negative result strengthens the paper:

> "Naive bilateral edge-aware adaptation of λ is sub-optimal — the
> ICM advantage comes from a global trade-off, not edge-aware
> selectivity. Future work: cluster-aware or learned adaptive λ."

This is the kind of negative result that demonstrates the authors
have tried the obvious refinement and ruled it out empirically.
Improves paper credibility.

## Files touched

- `crates/nupic-research/examples/speed_sweep.rs` (Cycle 66 sweep)
- `docs/research/png/04y-cycle66-adaptive-icm-negative.md`
- `Cargo.toml` workspace 1.1.11 → 1.1.12
- (no runtime change — research-only)
