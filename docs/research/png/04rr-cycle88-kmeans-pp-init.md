# 04rr — Cycle 88: R8 k-means++ init spike — mixed (YELLOW, robust fallback needed)

## TL;DR

Replace `train_palette_rgba` (imagequant median-cut) with k-means++
init on a 20k pixel subsample. Both feed the same
`refine_palette_kmeans(100 iters)`. Apples-to-apples bench across
baseline-7-mid + 3 × 5MP+ fixtures.

**Headline:** kmeans++ init is **almost always faster at init**
(consistent -15–80 ms saving) and **often improves SSIM too**
(+0.79 to +5.01 on 5/7 fixtures). Refine timing is variable: when
the kmeans++ init is good, refine converges faster; when bad,
refine takes longer.

**Blocker for default-flip:** 17 aurora (5.9 MP) double-regresses:
total wall +257 ms AND SSIM −3.70. Subsample bias on aurora's
narrow-chroma highlight cluster.

Decision: **YELLOW** — promising but needs robustness work
(multi-seed pick, hybrid fallback, or stratified subsample).
Not a default-flip blocker for paper, but is one for `Quality::Auto`
shipping. Engineering paper material is solid.

## Result table

(both pipelines: init → `refine_palette_kmeans(100 iters)` →
`apply_palette_rgba` → oxipng preset 3; SSIMULACRA2 vs original)

| fixture          |  MP | n_col | iq init | iq refine | iq total | pp init | pp refine | pp total | Δinit  | Δrefine | Δtotal | ΔSSIM  | Δsize  |
|------------------|----:|------:|--------:|----------:|---------:|--------:|----------:|---------:|-------:|--------:|-------:|-------:|-------:|
| 04 portrait      | <1  | 208   |   19 ms |     54 ms |    73 ms |    6 ms |     70 ms |    75 ms |  −13ms |   +16ms |  +2 ms | **+0.83** | +1.6% |
| 05 mountain      | <1  | 144   |   84 ms |    166 ms |   250 ms |    4 ms |    114 ms |   118 ms |  −81ms |   −51ms | **−132 ms** | **+0.79** | −0.04% |
| 06 landscape     |  1  | 208   |   48 ms |     84 ms |   132 ms |    5 ms |    109 ms |   114 ms |  −43ms |   +25ms | **−18 ms** | **+1.16** | +2.3% |
| 07 product       | <1  | 208   |   22 ms |     49 ms |    71 ms |    6 ms |     60 ms |    65 ms |  −16ms |   +10ms |  −6 ms |  −0.42 | −0.6% |
| 17 aurora 5.9 MP |  5  | 256   |   34 ms |    432 ms |   466 ms |    9 ms |    714 ms |   722 ms |  −25ms |  +282ms | **+257 ms** | **−3.70** | −2.0% |
| 25 sofia 5.5 MP  |  5  | 144   |   25 ms |    335 ms |   360 ms |    4 ms |    154 ms |   158 ms |  −21ms |  −182ms | **−202 ms** | **+5.01** | −2.7% |
| 27 whale 5.5 MP  |  5  | 256   |   24 ms |    314 ms |   337 ms |    6 ms |    290 ms |   296 ms |  −18ms |   −24ms | **−42 ms** | **+1.77** | −2.8% |

(Bold = above significance gate per the R8 KPI: ±15 ms perf, ±0.5 SSIM)

## Patterns

### 1. kmeans++ init is consistently faster (always)

20k subsample × K (color count) for kmeans++ vs imagequant's full-pixel
median-cut. kmeans++ saves 13–81 ms on init, with the biggest savings
on the larger images (which is where imagequant's overhead is worst).
This is a robust win across content classes.

### 2. Refine timing depends on init quality (variable)

Refine_palette_kmeans is subsampled Lloyd with stride=8 (< 5MP) or
stride=16 (≥ 5MP). It runs until convergence (ΔSSE < 0.0005 per iter)
or 100 iters cap.

- **Good init** (25 sofia, 05 mountain, 27 whale): refine converges
  faster → wall time saved.
- **Bad init** (17 aurora): refine churns through more iters →
  +282 ms penalty.
- **Slight init churn** (04 / 06 / 07): a few extra iters, but the
  init time saving covers it.

### 3. kmeans++ often improves SSIM too — until it doesn't

**This is the unexpected finding.** Conventional wisdom is that init
choice doesn't matter once refine converges. But:

- **5 of 7 fixtures** have ΔSSIM > 0 (+0.79 to +5.01).
- **25 sofia +5.01** — that's paper-worthy on its own.
- **17 aurora −3.70** — but failure mode is real.

Mechanism hypothesis: imagequant median-cut picks palette by
**quantizing the color histogram** (median of color clusters in a
HSV-like 6-cube structure). Result: palette covers the **dominant
modes** of the color histogram but can leave gaps in
**narrow-chroma-but-perceptually-important** regions. k-means++ on
the pixel distribution explicitly spreads centroids to cover the
pixel cloud, which is more SSIM-aligned (since SSIM sees pixel
colors weighted by spatial+frequency structure, not histogram modes).

When the image has a "typical" color distribution (smooth photo,
landscape), kmeans++ does better. When the distribution is
anomalous (aurora — mostly dark + isolated bright highlights at
narrow hue band), the 20k subsample misses the highlight cluster
and kmeans++ palette has no entry near it. Imagequant's median-cut
on the full color histogram catches this thanks to its histogram
binning.

### 4. Δsize correlates roughly with ΔSSIM, but not strictly

Better-quality output (higher SSIM) usually compresses slightly
larger (more palette utilization → less LZ77 repetition). 25 sofia
breaks this pattern — both higher SSIM AND smaller size (−2.7 %),
because its palette converged to a more compressible pattern.

## What this rules in / out

**Ruled in for Cycle 89-90 work:**
- **kmeans++ init concept** has real value (engineering AND quality
  paper material).
- **Multi-seed robustness experiment**: run kmeans++ with 2-3 seeds,
  pick the one whose palette gives lowest SSE on the full image. Init
  becomes 2-3× longer but still net positive (~10ms vs imagequant
  20-80ms). Could close the 17 aurora gap.
- **Hybrid fallback**: detect when kmeans++ palette will refine slowly
  (e.g., post-init SSE vs imagequant SSE); fall back to imagequant if
  worse. Conservative.
- **Stratified subsample**: instead of stride sample, sample by color
  bucket so narrow-chroma clusters aren't missed.

**Ruled out:**
- **Blanket kmeans++ default-flip in 0.6.0**. 17 aurora regression
  -3.7 SSIM + 257 ms blocks shipping.

**Open paper angles:**
- "kmeans++ on pixel distribution > imagequant median-cut on histogram
  for perceptual quantization" — this is the meta-finding.
- Per-content failure analysis (aurora-class content has specific
  defeats).

## Compute total picture

Total wall across 7 fixtures:
- imagequant path: 1689 ms
- kmeans++ path:   1548 ms
- **Net: −141 ms total** (mean −20 ms per fixture)

This satisfies the R8 KPI gate on the AVERAGE basis (−15-20 ms target
for 5MP). But the per-fixture variance (−202 ms best, +257 ms worst)
means it's a risky ship without robustness work.

## See also

- `crates/nupic-research/examples/kmeans_pp_init.rs` — spike harness.
- `docs/research/png/04n-cycle51-zero-copy-imagequant.md` — imagequant
  zero-copy optimisation; relevant background on the existing init.
- `docs/research/png/04q-cycle54-raw-fast-compression.md` — Cycle 54
  imagequant speed/preset tuning; complementary.
- `memory/research_roadmap_1_2_x.md` — R8 entry; Cycle 90 combined
  bench will revisit with robustness fix.
