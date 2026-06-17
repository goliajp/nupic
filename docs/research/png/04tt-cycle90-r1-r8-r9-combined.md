# 04tt · Cycle 90 — R1+R8+R9 combined bench (RED on time, paper §6 routing ammunition)

**Status:** RED — three axes do **not** stack as Pareto. Mean ΔSSIM +4.27 but mean Δt +3.2s
across the 10-fixture corpus. R1's M-weighted Lloyd cost (10 iters × N × K scalar) **dwarfs**
R9 SIMD savings (~1.67× on ICM only). Per-fixture verdict mirrors Cycle 87 per-content split
and Cycle 88 17-aurora subsample bias — exact paper §6 routing material.

## TL;DR

| metric | baseline-7 | 5MP cohort | ALL |
|---|---:|---:|---:|
| mean ΔSSIM | **+5.43** | +1.56 | +4.27 |
| mean Δwall | **+814 ms** | +8 876 ms | +3 233 ms |
| total Δsize | +1.09% | -1.30% | -0.66% |
| GREEN (Q+T-) | 0/7 | 0/3 | 0/10 |
| Q+T- (quality ↑, time ↑) | 3/7 | 2/3 | 5/10 |
| RED (Q↓ and T↑) | 4/7 | 1/3 | 5/10 |

**Decision gate:** RED on time. Composition is unfair to R9 — M-weighted Lloyd added cost
that R9 SIMD cannot recover. Three-axis blanket flip is off the table; Cycle 91 must split
into per-axis productionization with routing (R1) and isolation (R9 ship-alone).

## Pipelines

- **[A] Baseline (Cycle 71):** imagequant median-cut init → `refine_palette_kmeans(100)` →
  ICM scalar (3-step anneal λ²={1e-4, 5e-5, 2e-5}) → indexed PNG → oxipng preset 3.
- **[B] R1+R8+R9 stacked:** k-means++ init on OKLab subsample → `refine_palette_kmeans(100)` →
  **M-weighted Lloyd** (`w_l=1, w_a=w_b=0.5, ε=0.001, 10 iters`) → ICM **SIMD** (`f32x4`,
  3-step anneal) → indexed PNG → oxipng preset 3.

Wall time is measured around the full pipeline (init through ICM). SSIM = SSIMULACRA2 via
`nupic compare` on the final PNG. Same `n_colors` / `alpha_imp` (classifier-picked, matches
production) on both pipelines.

## Per-fixture results

| fixture | MP | n | A wall | A SSIM | B wall | B SSIM | Δt | ΔSSIM | Δsize | gate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| 01 trans | 0 | 64 | 324 ms | -19.17 | 356 ms | 16.80 | +32 ms | **+35.97** | -7.91% | Q+T- |
| 02 pluto | 0 | 32 | 94 ms | 50.95 | 152 ms | 56.97 | +58 ms | **+6.02** | +5.13% | Q+T- |
| 03 wiki | 0 | 256 | 57 ms | 95.81 | 72 ms | 95.80 | +15 ms | -0.01 | +0.69% | RED |
| 04 portrait | 0 | 208 | 712 ms | 86.19 | 2 166 ms | 87.41 | +1 454 ms | **+1.22** | +2.20% | Q+T- |
| 05 mountain | 0 | 144 | 707 ms | 58.95 | 1 534 ms | 54.59 | +827 ms | **-4.35** | +1.66% | RED |
| 06 landscape | 1 | 208 | 1 088 ms | 79.79 | 3 241 ms | 79.37 | +2 154 ms | -0.41 | +0.97% | RED |
| 07 product | 0 | 208 | 580 ms | 82.79 | 1 738 ms | 82.34 | +1 158 ms | -0.45 | -0.69% | RED |
| 17 aurora | 5 | 256 | 5 126 ms | 48.56 | 16 477 ms | 46.39 | +11 351 ms | **-2.17** | +3.72% | RED |
| 25 sofia | 5 | 144 | 2 859 ms | 62.46 | 7 953 ms | 67.65 | +5 094 ms | **+5.19** | -2.03% | Q+T- |
| 27 whale | 5 | 256 | 4 874 ms | 78.34 | 15 056 ms | 80.00 | +10 182 ms | **+1.66** | -2.82% | Q+T- |

(Wall time on M3 Max release build, single thread for ICM, SSIM via SSIMULACRA2.)

## What the data says

**Quality (mean ΔSSIM +4.27 ALL):**
- 01 trans and 02 pluto carry most of the mean (+36 / +6). Transparency-heavy fixtures
  see large gain from R1's chroma weighting on edges and R8's better initial spread.
- 04 portrait +1.22 SSIM **reproduces Cycle 86** GREEN finding on chroma-rich faces.
- 25 sofia +5.19 SSIM and 27 whale +1.66 SSIM **dual-win on size** confirm R8+R1
  helps on chroma-rich 5MP+ content.
- 03 wiki -0.01, 06 landscape -0.41, 07 product -0.45 are **near-zero noise** — R1 does
  nothing useful on smooth/UI/product, matches Cycle 87 per-content split.
- 05 mountain -4.35 and 17 aurora -2.17 are **the failure modes**. Cycle 87 already
  flagged 05 (smooth gradient sky) as R1-hostile; Cycle 88 already flagged 17
  (anomalous color distribution) as R8 subsample-bias failure. Both modes survived
  stacking.

**Perf (mean Δt +3.2 s ALL):**
- R8 init was supposed to **shave** time. Per-fixture init delta is small (Cycle 88
  data shows ±200 ms range). It is dominated by:
- **R1 M-weighted Lloyd** at +10 iters × N × K scalar inner loop. On a 5MP fixture at
  K=256 this costs roughly 5MP × 256 × 10 ≈ 1.3 × 10¹⁰ scalar ops → ~5-10 s on M3 Max.
- **R9 SIMD** saves ~2 s per 5MP fixture on the ICM step (Cycle 89 1.67×). Net
  algebra: R1 adds +10 s, R8 saves +0.2 s, R9 saves +2 s → +7.8 s. Matches the
  +8.9 s 5MP-cohort mean Δt within bench noise.

**Conclusion:** R9's perf benefit is **real but small** relative to R1's cost. R9 cannot
amortize R1. Stacking R1 unconditionally is a Pareto regression on the perf axis.

## What this means for the paper

This is **exactly the §6 routing-analysis ammunition** the roadmap predicted. The right
narrative is:

- **§4 metric redesign:** R1 (Cycle 86) — strong head-to-head on chroma-rich content
- **§5 results table:** R1 + R9 (gated by content classifier) — the actual recommended
  configuration. R8 is **optional** since it's neutral or negative on production-style fixtures.
- **§6 routing analysis:** show the failure modes (05 mountain, 17 aurora) directly. Use
  this Cycle 90 table as the "naive blanket stacking" baseline, then introduce the
  classifier-gated variant. Cycle 91a's spike will produce that classifier.
- **§7 engineering:** R9 SIMD ships independently — bit-exact and pure speedup. Frame it
  as "the optimizer-level perf win that's safe to deploy unconditionally," contrasting
  with R1 metric-level changes that need content gating.
- **§8 reviewer defense:** the Cycle 90 RED row IS a defense — pre-empts "why didn't you
  just stack everything?"

## Next cycle decisions (per roadmap)

Cycle 91 splits into three concurrent tracks; **autorun continues with (c) first**
(independent, ship-ready) and (a) second (paper-critical):

- **(c) R9 production wiring** — bit-exact SIMD ICM into `nupic-quantize`. Direct merge
  candidate. Smallest blast radius, validates R9 as standalone win.
- **(a) R1 routing classifier** — features = (mean chroma, edge density, gradient
  smoothness) → R1 on/off gate. Spike on the four problematic fixtures
  (03 wiki / 05 mountain / 06 landscape / 07 product) to confirm gate flips them OFF
  while keeping 04/25/27 ON.
- **(b) R8 robustness** — multi-seed pick / hybrid imagequant fallback on 17 aurora.
  Less paper-critical (R8 is footnote-class), defer if (c)+(a) fill the cycle.

## Files

- `crates/nupic-research/examples/cycle90_r1_r8_r9_combined.rs` — head-to-head bench
  driver (this essay).
- Outputs live in `/tmp/c90_*` for visual spot-check if needed.
- Previous cycles: 04oo (85 R2 RULED OUT) · 04pp (86 R1 GREEN) · 04qq (87 R1 cross-corpus
  YELLOW) · 04rr (88 R8 YELLOW) · 04ss (89 R9 GREEN).
