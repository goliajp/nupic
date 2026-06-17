# 04yy · Cycle 94 — R1 classifier held-out validation RED (45% acc, 3 FN)

**Status:** RED. Cycle 93's 5-rule classifier (frozen, no re-fit) achieves only
**9/20 = 45% accuracy** on a held-out 20-fixture sample from corpus-500, with **3 False
Negatives** — the production-safety "0 FN" property from Cycle 93 does not survive
generalization. This is the unambiguous "Cycle 93 was overfit" verdict the cycle was
designed to deliver.

## TL;DR

| classifier evaluation | acc | FP | FN |
|---|---:|---:|---:|
| Cycle 91a 4-rule, fit on 10 + eval on 10 | 9/10 = 90% | 1 | 0 |
| Cycle 91a 4-rule, eval on 20 corpus-500 (Cycle 92) | 12/20 = 60% | 8 | 0 |
| Cycle 93 5-rule, fit + eval on 30 (training acc) | 27/30 = 90% | 3 | 0 |
| **Cycle 94 5-rule, held-out on 20 NEW corpus-500** | **9/20 = 45%** | **8** | **3** |

**Drop:** 90% training → 45% held-out is a 45-pt gap. Classic small-n threshold overfit.

## Held-out sample composition

Disjoint from Cycle 92's 20: explicit exclusion of those filenames. Pool drawn from
the remaining 486 corpus-500 fixtures, filtered to < 1.5 MP, stratified by Cycle 93's
own FRIEND/HOSTILE prediction.

| pool | size (Cycle 93 5-rule applied to corpus-500 minus Cycle 92's 20, <1.5MP) |
|---|---:|
| predicted FRIEND pool | 43 |
| predicted HOSTILE pool | 223 |

Stride-sampled 10 from each with offset=1 to further reduce overlap probability.

## Held-out 20 results

| fixture | MP | chroma | edge | smooth | bandpass | ΔSSIM | predict | actual | verdict |
|---|---:|---:|---:|---:|---:|---:|---|---|---|
| mi2 | 0 | 0.000 | 0.000 | 0.000 | 0.000 | 0.00 | F | H | **FP** (synth) |
| n20_moon | 0 | 0.014 | 0.364 | 0.044 | 0.428 | +0.08 | F | H | **FP** (noise) |
| p35_480x320 | 0 | 0.012 | 0.281 | 0.045 | 0.509 | +0.75 | F | F | OK |
| p40_480x320 | 0 | 0.022 | 0.310 | 0.036 | 0.792 | +0.10 | F | H | **FP** (noise) |
| p432_sm | 0 | 0.032 | 0.276 | 0.034 | 0.467 | +2.96 | F | F | OK |
| p445_sm | 0 | 0.052 | 0.337 | 0.048 | 0.349 | −0.13 | F | H | **FP** (noise) |
| **p6_480x320** | 0 | 0.064 | 0.336 | 0.037 | 0.714 | **−2.97** | F | H | **FP (real photo regression)** |
| s086_trans_circle | 0 | 0.054 | 0.009 | 0.005 | 0.596 | 0.00 | F | H | **FP** (synth) |
| s090_trans_circle | 0 | 0.051 | 0.007 | 0.004 | 0.596 | 0.00 | F | H | **FP** (synth) |
| s094_trans_circle | 0 | 0.024 | 0.006 | 0.003 | 0.596 | 0.00 | F | H | **FP** (synth) |
| n04_mars | 1 | 0.034 | 0.227 | 0.032 | 0.497 | −1.50 | H | H | OK |
| p100 | 0 | 0.023 | 0.119 | 0.017 | 0.720 | +0.01 | H | H | OK |
| p15 | 0 | 0.046 | 0.325 | 0.048 | 0.300 | +0.15 | H | H | OK |
| p3 | 0 | 0.039 | 0.168 | 0.022 | 0.626 | +0.34 | H | H | OK |
| **p427_sm** | 0 | 0.100 | 0.885 | 0.144 | 0.280 | **+2.15** | H | F | **FN** |
| p4 | 0 | 0.043 | 0.220 | 0.033 | 0.547 | −1.81 | H | H | OK |
| p73 | 0 | 0.015 | 0.370 | 0.061 | 0.282 | +0.41 | H | H | OK |
| **p97** | 0 | 0.024 | 0.068 | 0.021 | 0.251 | **+0.61** | H | F | **FN** |
| **s031_noise** | 0 | 0.088 | 0.930 | 0.208 | 0.088 | **+1.03** | H | F | **FN** |
| s061_solid | 0 | 0.074 | 0.000 | 0.000 | 0.000 | 0.00 | H | H | OK |

## Failure analysis

### 8 FPs (predicted FRIEND, actual HOSTILE/neutral)

- **4 synthetic trans-circle FPs** (mi2, s086/s090/s094): trans_frac > 0 trigger but R1
  is no-op on synthetic alpha-circle patterns (Δ = 0). Could be filtered with
  `trans_rule AND edge_density > 0.05` — the synthetics have edge < 0.01.
- **3 noise-level FPs** (n20 moon +0.08, p40 +0.10, p445 −0.13): borderline content
  where R1 has near-zero effect. Borderline cases will always exist near the gate at
  ±0.5 ΔSSIM.
- **1 real photo regression FP** (p6 −2.97): all features pass the 5-rule gate
  (chroma=0.064, edge=0.336, smooth=0.037, bandpass=0.714) but R1 hurts. The
  bandpass_ratio of 0.714 is **higher** than 27 whale (0.589, FRIEND), so this single
  feature cannot separate them.

### 3 FNs (predicted HOSTILE, actual FRIEND) — **the production-safety break**

- **p427_sm +2.15**: smoothness=0.144 well above 0.054 threshold. The 4-rule "smooth <
  0.054 ⇒ R1 helps" heuristic was wrong here.
- **p97 +0.61**: edge=0.068 below 0.150 threshold. Yet R1 helps. The "edge > 0.27 ⇒
  needed for R1" heuristic was wrong here too.
- **s031_noise +1.03**: smoothness=0.208 (highest in sample, pure noise content!) yet
  R1 helps on it. The strongest counterexample to Cycle 93's smooth<0.054 rule.

These three FN cases collectively show **the 4-feature decision boundary is wrong** —
not just badly calibrated. No threshold tweak will recover all three without losing
the existing OK rows.

## What this means

**Production-safety property lost.** Cycle 93's headline "0 FN preserved" was a small-n
artifact. Held-out FN > 0 means a deployed classifier would silently lose ≥ 2 SSIM on
content R1 should have helped — exactly the failure mode the routing was meant to
prevent.

**The 4-feature space cannot capture R1-friendliness.** The hard cases (p6 photo FP,
all 3 FN rows) sit in feature regions that overlap between FRIEND and HOSTILE in the
30-fixture training set. Either:
- (a) need fundamentally different features (per-pixel structure, not aggregate
  statistics), or
- (b) the underlying problem is not linearly/threshold-separable in any cheap feature
  space, requiring a learned model with more capacity.

**Honest paper §6 framing:** "We attempted simple-feature routing classifiers
(threshold-based on 4 → 7 OKLab statistics) and reached 90% training / 45% held-out
accuracy. Held-out generalization is the bottleneck, not feature engineering. A learned
classifier with more capacity or richer features (e.g., per-octave wavelet response,
patch-level structure) is needed before R1 can ship behind a routing gate." This is a
defensible negative result that **strengthens** the paper — it shows we did the validation
work to rule out the cheap solution before reaching for the expensive one.

## Decision gate

- Acc 45% (gate ≥ 80%) — **RED**
- FN = 3 (gate ≤ 1) — **RED**

**R1 cannot ship behind this classifier.**

## Options for Cycle 95

1. **Larger ground-truth corpus + learned classifier.** Run combined-vs-baseline on
   ~80 fixtures (24 minutes per Cycle 92's budget), fit a 7-feature logistic regression
   or random forest. Cross-validate with proper 70/30 split. If learned model achieves
   ≥ 80% on the held-out 20 + ≥ 75% on a new held-out, that's the ship gate.

2. **Drop the routing gate entirely.** Per Cycle 90's combined-bench data, R1's per-fixture
   ΔSSIM averages +4.27 across the 10-fixture mixed corpus and +5.43 on baseline-7. The
   median is also positive. Maybe **ship R1 unconditionally** with `--enable-r1` flag and
   document the per-content caveat. Most production traffic is portrait / product content
   anyway. Trade some 5MP outliers for headline quality win.

3. **Pareto-conditional R1.** Run R1 inside the pipeline, compute SSIM of R1 vs baseline
   output before final encode, keep whichever is better. Costs 2× SSIM but provides
   exact routing. Likely too slow for production, but useful as a "ground truth" upper
   bound for any classifier's accuracy.

4. **Pivot research direction.** Per roadmap, R4 (rate-distortion grid) and R3 (VQ-VAE /
   differentiable palette) are paper-paths that don't depend on classifier routing.
   Spend Cycle 95+ on those instead of patching the routing classifier.

**Autorun recommendation: option 1 (learned classifier on larger ground truth) is the
honest continuation of the R1 routing thread.** If that also fails, escalate to option 2
or 4 in Cycle 96.

## Files

- `crates/nupic-research/examples/cycle94_r1_classifier_heldout.rs` — held-out
  validation driver. 36 s stage-1 features + 25 s stage-4 bench.
- Previous: 04vv (91a 9/10 on 10), 04ww (92 RED 12/20), 04xx (93 27/30 GREEN
  training acc), 04yy (this — held-out brutal).
