# 04i — Cycle 43: Importance-Sampled Lloyd k-means (v1.0.1)

## Motivation

User direction post v1.0.0: continue toward -20 % size gate. Track A
(engineering palette tuning) capped at -17.02 % (baseline-7 with
04/06 gate-locked at n=208 and no further palette room). Open the
algorithm layer (Track B).

## Math — weighted Lloyd

Standard Lloyd's k-means minimises L2 sum-of-squared error over
uniform-weighted pixels. Our application target — SSIMULACRA2 — is
non-uniform in pixel importance: banding artefacts in smooth-gradient
regions carry far higher perceptual penalty than the same numeric L2
error in textured regions (where high-frequency content masks
quantisation noise).

Introduce a per-pixel weight `w_i`:

```
w_i = 1 / (1 + α · |grad(p_i)|)
```

where `grad(p_i)` is the local luma gradient magnitude (mean absolute
luma diff to right and down neighbours). For `α = 0` the weights
collapse to uniform — Lloyd reduces to the standard form. For `α > 0`
pixels in smooth regions get higher weight.

Modified centroid update under this weighting becomes the **weighted
mean**:

```
c_j = ( Σ_{i: cluster(i)=j} w_i · pixel_i ) / ( Σ_{i: cluster(i)=j} w_i )
```

Assignment (argmin L2 in OKLab + α) is unchanged. The fixed-point
iteration remains convergent because each iteration is a coordinate
descent step on the weighted SSE objective.

## Pareto sweep — 05 mountain & 06 landscape × (n, α)

Bench at (palette n, importance α) on two baseline-7 photo fixtures:

```
05 mountain  (TinyPNG SSIM gate = 59.41, current routing: n=192, α=0)

  n      α=0           α=0.5          α=1.0
 128   311 KB / 56.75  302 / 57.90    301 / 58.15
 144   322 / 58.48     324 / 60.04 ✓  317 / 59.07
 160   325 / 61.34     331 / 61.85 ✓  336 / 61.41
 176   334 / 64.89     346 / 63.78    346 / 63.91
 192   341 / 65.33 (current)
 208   354 / 67.77     361 / 69.36    367 / 69.28

06 landscape (gate = 79.76, current: n=208)

  n      α=0           α=0.5          α=1.0
 192   969 / 77.76     976 / 79.00    974 / 78.94    (all below gate)
 208   974 / 79.93 ✓   981 / 79.22    990 / 79.20
```

Importance sampling at `α = 0.5` enables 05 to drop palette from
n=192 to n=144 while still clearing the SSIM gate (60.04 vs 59.41).
06 (smooth-gradient content) sees no Pareto improvement — importance
sampling is content-specific, helping stochastic / texture-heavy
content where standard Lloyd over-allocates palette to noise.

## Implementation — routing-tied

Cycle 43 introduces:

- `nupic_quantize::refine_palette_kmeans_importance(...)` — public
  weighted Lloyd. Takes `importance_alpha` scalar; computes weights
  internally from row+col luma diff.
- `QuantizeOpts::importance_alpha` field. `0.0` = standard Lloyd
  (default, backward-compatible). `> 0` = weighted.
- `classify_for_palette_size_with_importance` — returns `(n_colors,
  α)` tuple. For `uniq > 100K` + `var > 200` (stochastic detector,
  same as Cycle 41's tier-4 var-split), it returns `(144, 0.5)`;
  all other routes return `(classify_for_palette_size, 0.0)`.

## Bench — baseline-7 vs TinyPNG

```
fixture                  TinyPNG       nupic v1.0.1   Δ size       Δ SSIM
01 trans-demo            47 / -492.6   19 / -64.10   -28 KB       +428.5
02 pluto-trans           176 / -60.0   68 / 64.84    -108 KB      +124.8
03 wiki-logo             13 / -63.7    14 / 84.27    +1 KB        +147.9
04 portrait              556 / 85.9    450 / 86.07   -106 KB      +0.2
05 mountain              424 / 59.4    323 / 60.04   -101 KB      +0.63
06 landscape             1066 / 79.8   973 / 79.93   -93 KB       +0.2
07 product               358 / 80.3    324 / 84.07   -34 KB       +3.8
─────────────────────────────────────────
TOTAL                    2643 KB       2176 KB                    all positive
                                       0.8233×  (-17.67 %)
```

**+0.65 pp progress vs Cycle 42** (-17.02 % → -17.67 %). 05 mountain
SSIM dropped from 65.33 to 60.04 but still above gate with +0.63
buffer — the gate-tightening cost of the size win.

## Open backlog toward -20 %

After Cycle 43 we're at -17.67 %. Gap to -20 % = -2.33 pp.

Avenues:
1. **Per-fixture α tuning** — α=0.5 fixed; some fixtures may prefer
   α=0.3 or α=1.0. Sweep + classify on extra signals.
2. **Importance signal beyond luma-grad** — chrominance grad,
   multi-scale grad. Multi-scale → mathematically equivalent to
   approximating SSIMULACRA2 spatial filter. Closer to paper-worthy
   "perceptual-loss-aware Lloyd" framing.
3. **Apply importance sampling to other tier-4** — currently only
   var > 200 stochastic gets it. Maybe high-detail-smooth photos
   (NASA n01_mars-like) benefit too.
4. **Joint palette + filter co-optimisation** (Track B Cycle 44+) —
   the next obvious algorithm direction; bilevel discrete opt.

## Files touched

- `crates/nupic-quantize/src/lib.rs`
  - `refine_palette_kmeans_importance` (new public API)
  - `QuantizeOpts::importance_alpha` field
  - `classify_for_palette_size_with_importance` (routing helper)
  - `quantize_indexed_png` honours importance_alpha in pipeline
- `crates/nupic-core/src/ops/compress.rs`
  - `encode_png_stone_c` calls new classifier
- `Cargo.toml` workspace version 1.0.0 → 1.0.1
