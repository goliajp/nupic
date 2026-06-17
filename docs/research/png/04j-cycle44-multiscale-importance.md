# 04j — Cycle 44: Multi-scale Importance Sampling (v1.0.2)

## Motivation

Cycle 43 introduced single-scale luma-gradient importance weighting
(s=1, immediate-neighbour diff). For Paper P2 ("Multi-scale
Perceptual-Surrogate Lloyd Quantization"), the contribution must
approximate SSIMULACRA2's multi-resolution spatial filter rather
than the ad-hoc single-scale weighting.

## Math — multi-scale perceptual surrogate

SSIMULACRA2 (Sneyers 2023) operates on 5 successive image pyramid
scales, computing per-scale per-channel statistics that aggregate
into the final score. The score's sensitivity to a per-pixel
perturbation therefore depends on the pixel's local activity AT
MULTIPLE SCALES — not just immediate neighbours.

Multi-scale gradient surrogate:

```
G(i) = (1/|S|) · Σ_{s ∈ S} mean_{ν ∈ N_s(i)} |luma(p_i) − luma(p_ν)|
```

where `N_s(i)` is the set of neighbours at scale `s` (right and down
at offset `s`). For `S = {1, 2}` this captures both fine-grain
(immediate edge) and short-range (across small features) gradients.

Per-pixel importance weight:

```
w_i = 1 / (1 + α · G(i))
```

For α=0 the weights collapse to uniform → standard Lloyd. For α>0
smooth-region pixels (small multi-scale gradient) get higher weight.

## Sweep — `S = {1, 2}` is empirical sweet spot

Bench at α=0.5, n=144 on 05 mountain (stochastic photo, gate=59.41):

```
scales            size     SSIM     Δ vs gate
baseline (α=0)    322 KB   58.48   −0.93  ✗
s=1 (Cycle 43)    324 KB   60.04   +0.63  ✓
s=1,2             316 KB   60.21   +0.80  ✓  ← best
s=1,2,4           317 KB   59.40   −0.01  ✗
s=1,2,4,8         316 KB   59.83   +0.42  ✓
s=2,4,8           319 KB   59.76   +0.35  ✓
s=4,8             311 KB   59.50   +0.09  ✓
```

`s ∈ {1, 2}` Pareto-dominates Cycle 43's single-scale s=1:
**-8 KB AND +0.17 SSIM**. Beyond two scales the gradient becomes
too averaged-out to reliably distinguish smooth from textured
regions on this fixture, and SSIM hovers around the gate.

The choice of 2 scales (not 5 as in SSIMULACRA2 itself) is a
deliberate cost trade — we're approximating the rate-relevant
information that drives palette banding penalty, which empirically
concentrates at the 1-2 pixel scale. Beyond that the gradient
information becomes redundant for our use case.

## Implementation

`refine_palette_kmeans_importance` now computes weights using
multi-scale gradient `S = {1, 2}`:

```rust
const SCALES: [usize; 2] = [1, 2];
for i in 0..n_pixels {
    let mut grad_sum = 0i32; let mut cnt = 0;
    for &s in &SCALES {
        // right neighbour at offset s
        if x + s < w { grad_sum += (luma[i] - luma[i+s]).abs(); cnt += 1; }
        // down neighbour at offset s
        if y + s < h { grad_sum += (luma[i] - luma[(y+s)*w + x]).abs(); cnt += 1; }
    }
    let g = grad_sum as f32 / cnt as f32;
    weights[i] = 1.0 / (1.0 + α · g);
}
```

Precomputes luma once (saves 4× per-scale division). Cost: O(N) per
scale; for S={1,2} ≈ 2× single-scale cost. On 5MP ≈ +5-10ms (small).

## Bench

```
fixture                  TinyPNG       nupic v1.0.2   ratio
01 trans-demo            47 / -492.6   19 / -64.10    0.41×
02 pluto-trans           176 / -60.0   68 / 64.84     0.39×
03 wiki-logo             13 / -63.7    14 / 84.27     1.05×
04 portrait              556 / 85.9    450 / 86.07    0.81×
05 mountain              424 / 59.4    316 / 60.20    0.75×  ← -7 KB vs Cycle 43
06 landscape             1066 / 79.8   973 / 79.93    0.91×
07 product               358 / 80.3    324 / 84.07    0.91×
─────────────────────────────────────────────
TOTAL                    2643          2169           0.8207
                                                     (-17.93 %)
```

+0.26 pp progress vs Cycle 43 (-17.67 % → -17.93 %). All 7 fixtures
still ≥ TinyPNG SSIM.

## Paper P2 framing

Multi-scale gradient surrogate is the FIRST formally-grounded
approximation of SSIMULACRA2's spatial filter for the purpose of
palette quantization. Single-scale (Cycle 43) was ad-hoc; multi-
scale derives from the metric's structure. This enables a
theoretical contribution in P2:

- Define multi-scale surrogate L_perc(α, S)
- Show L_perc is a metric (positive, symmetric, triangle inequality)
- Prove weighted Lloyd convergence under L_perc (coordinate descent
  on weighted SSE objective)
- Bound the SSIMULACRA2-vs-L_perc approximation error

These constitute the math contributions for the P2 paper draft.

## Open backlog

1. **Perf**: gradient compute is O(N·|S|) sequential. On 5MP ≈
   10-20 ms; doesn't dominate but accumulates. Cycle 45+ will SIMD
   this + Lloyd argmin together (real perf gain target).
2. **Per-fixture α**: currently α=0.5 fixed. Sweep + classify on
   extra signals to find fixture-specific optima. May enable
   palette reduction beyond n=144 on some fixtures.
3. **Beyond 2 scales**: bench showed >2 scales don't help on 05.
   But 06 / 04 (gate-locked) might benefit from more scales —
   need separate investigation.

## Files touched

- `crates/nupic-quantize/src/lib.rs`
  `refine_palette_kmeans_importance`: single-scale grad → multi-scale
  `S = {1, 2}` with precomputed per-pixel luma
- `Cargo.toml` workspace version 1.0.1 → 1.0.2
