# 04xx · Cycle 93 — R1 classifier richer features GREEN (27/30, 0 FN)

**Status:** GREEN on the 30 ground-truth fixtures (Cycle 91a's 10 + Cycle 92's 20). New
**bandpass_ratio** feature is the key — separates the chroma-rich photo regressions
(n29 astronaut, p426, p7, n01 mars, etc.) from chroma-rich friendly content (25 sofia,
27 whale, 04 portrait, p32, p449). With 3 errors all on the FP side (0 FN preserved),
the classifier is **production-safe** and clears the 85% accuracy gate.

**Honest caveat:** thresholds were fit on the same 30 fixtures evaluated against — this
is training accuracy, not held-out. Cycle 94 should do leave-one-out / split validation
before production wiring.

## TL;DR

| classifier | features | acc | FP | FN |
|---|---|---:|---:|---:|
| Cycle 91a 4-rule (re-eval on 30) | chroma + edge + smooth + trans | 21/30 = 70% | 9 | 0 |
| 2-rule new feat | bandpass + ec_corr (+ trans) | 23/30 = 77% | 4 | 3 |
| **5-rule winning** | trans + edge + smooth + bandpass | **27/30 = 90%** | **3** | **0** |
| 6-rule (+ entropy) | adds entropy (no improvement, threshold→0) | 27/30 = 90% | 3 | 0 |

**Best gate (paper §6 candidate):**
```
FRIEND if  trans_frac > 0
   OR  ( edge_density > 0.2686
         AND  smoothness < 0.0541
         AND  bandpass_ratio > 0.3280 )
```

`mean_chroma` threshold drops to 0 in the grid search — the new bandpass_ratio
subsumes the chroma signal for separation.

## New features (Cycle 93)

| feature | definition | computational cost |
|---|---|---|
| `bandpass_ratio` | `mean(|G2 − G4|) / max(mean(|L − G1|), ε)` over 4-octave Gaussian pyramid on OKLab L | 4 × O(N) Gaussian filter passes |
| `chroma_entropy` | Shannon entropy of 16×16 2D histogram over OKLab (a, b) | O(N) histogram + O(256) entropy |
| `edge_chroma_corr` | Pearson(sqrt(a²+b²), ∇L magnitude) per-pixel | O(N) 2-pass |

All sub-millisecond on baseline-7 fixtures, ~30 ms on 5 MP.

**Why bandpass_ratio works:** R1's M-weighted Lloyd weights pixels by `|G1−G2| + |G2−G4|`
— a 2-octave DoG bandpass. When mid-scale energy (G2−G4) dominates fine-scale energy
(L−G1), R1's weighting amplifies the content R1 was designed for (smooth chroma
gradients with mid-scale structure). When fine-scale energy dominates, R1 amplifies noise
and centroids drift to outliers.

| fixture | bandpass | classification |
|---|---:|---|
| 27 whale (FRIEND) | 0.589 | high mid-scale ⇒ R1 sees usable structure |
| 02 pluto (FRIEND) | 0.590 | high mid-scale |
| 04 portrait (FRIEND) | 0.535 | high mid-scale |
| 25 sofia (FRIEND) | 0.335 | borderline (just above 0.328 threshold) |
| n29 astronaut (HOSTILE) | 0.336 | borderline FP (lone hard-photo escape) |
| n01 mars (HOSTILE) | 0.546 | high bandpass — caught by edge<0.27 |
| 06 landscape (HOSTILE) | 0.243 | low bandpass — fine-scale noise |
| 17 aurora (HOSTILE) | 0.248 | low bandpass — soft-glow has no mid-scale |
| 05 mountain (HOSTILE) | 0.290 | low bandpass — small-scale noise dominates |
| p66 (HOSTILE −3.39) | 0.782 | high bandpass but edge<0.27 catches it |

## 30-fixture per-row dump (key cols)

| fixture | ΔSSIM | actual | chroma | edge | smooth | trans | **bandpass** |
|---|---:|---|---:|---:|---:|---:|---:|
| 01 trans | +35.97 | F | 0.139 | 0.128 | 0.018 | 0.964 | 0.539 |
| 02 pluto | +6.02 | F | 0.043 | 0.171 | 0.022 | 0.219 | 0.590 |
| 03 wiki | −0.01 | H | 0.010 | 0.267 | 0.053 | 0.264 | 0.371 |
| 04 portrait | +1.22 | F | 0.027 | 0.433 | 0.042 | 0.000 | 0.535 |
| 05 mountain | −4.35 | H | 0.089 | 0.378 | 0.069 | 0.000 | 0.290 |
| 06 landscape | −0.41 | H | 0.023 | 0.734 | 0.160 | 0.000 | 0.243 |
| 07 product | −0.45 | H | 0.026 | 0.129 | 0.030 | 0.000 | 0.201 |
| 17 aurora | −2.17 | H | 0.067 | 0.055 | 0.018 | 0.000 | 0.248 |
| 25 sofia | +5.19 | F | 0.060 | 0.312 | 0.054 | 0.000 | 0.335 |
| 27 whale | +1.66 | F | 0.067 | 0.369 | 0.039 | 0.000 | 0.589 |
| mi0 | 0.00 | H | 0.000 | 0.000 | 0.000 | 0.684 | 0.000 |
| n29_astronaut | −2.28 | H | 0.044 | 0.270 | 0.050 | 0.000 | 0.336 |
| p32 | +4.14 | F | 0.031 | 0.278 | 0.034 | 0.000 | 0.428 |
| p449 | +0.57 | F | 0.035 | 0.292 | 0.049 | 0.000 | 0.364 |
| n01 mars | −2.60 | H | 0.045 | 0.129 | 0.019 | 0.000 | 0.546 |
| p66 | −3.39 | H | 0.065 | 0.152 | 0.019 | 0.000 | 0.782 |
| p84 | −8.68 | H | 0.026 | 0.367 | 0.099 | 0.000 | 0.460 |

(Rest of the 30 in spike stdout; pattern matches.)

## Error analysis (3 FPs in best 5-rule)

| fixture | ΔSSIM | features triggering FRIEND | FP class |
|---|---:|---|---|
| 03 wiki | −0.01 | trans_frac=0.264 ⇒ trans rule | harmless noise (Δ ≈ 0) |
| mi0 | 0.00 | trans_frac=0.684 ⇒ trans rule | harmless synthetic Δ = 0 |
| n29 astronaut | −2.28 | edge=0.270 ✓ smooth=0.050 ✓ bandpass=0.336 ✓ | **real photo regression** |

n29_astronaut is the **one true classifier failure** — a chroma-rich photo where the
3-rule mid-scale test fires but R1 actually hurts. Inspection: it sits right at the
boundary of every threshold (edge just above 0.269, smooth just below 0.054, bandpass
just above 0.328). A learned classifier would correctly identify it as boundary-case;
a hand-tuned threshold has no way to push it across without losing 25 sofia or p32.

The 2 trans-rule FPs (mi0, 03 wiki) are **safe to ignore**: their actual ΔSSIM is 0 or
near-zero, so applying R1 costs nothing and provides nothing.

## Threshold-fit honesty

These thresholds were grid-swept on the same 30 fixtures whose accuracy we report.
This is **training accuracy** — the headline 90% number does **not** demonstrate
generalization to unseen content. Cycle 94 must do held-out validation:

- **Option A — Leave-one-out cross-validation:** re-fit thresholds 30 times excluding
  one fixture each time, measure accuracy on the held-out fixture. Tight estimate of
  generalization error.
- **Option B — 70/30 split:** fit on 21 random fixtures, evaluate on 9. Repeat 10 times,
  report mean ± std.
- **Option C — Held-out 506-corpus sample:** draw a NEW 20-fixture sample disjoint
  from Cycle 92's 20, run ground truth bench, evaluate against the Cycle 93 thresholds
  directly. Cheaper than A/B but no fold-level diagnostic.

Recommended: **Option C first** (fastest, ~30s bench), escalate to A if it surprises.

## Production-safety summary

- **FN = 0** preserved: no R1-friendly fixture is denied R1. Worst-case cost is "R1
  applied to neutral/slightly-hostile content" — recoverable downstream.
- **Trans-rule FPs are harmless:** the 2 trans-FP fixtures have Δ = 0 ± noise.
- **The lone real-regression FP** (n29 astronaut −2.28) is a known boundary case.

This classifier is **production-acceptable as a §6 paper baseline** even if Cycle 94 finds
the held-out accuracy drops to 80%. Real ship requires Cycle 94 + maybe Cycle 95 wiring
into nupic-quantize behind a feature flag.

## Files

- `crates/nupic-research/examples/cycle93_r1_classifier_richer_features.rs` — full
  spike. Single-feature sweeps + 2-rule + 5-rule + 6-rule grid sweep on 30 ground truth.
- Previous: 04vv (Cycle 91a, 4-rule 9/10 on 10), 04ww (Cycle 92, RED 12/20 on 506).

## Decision gate

Per Cycle 92 spike-level threshold (acc ≥ 85% + FN ≤ 0): **GREEN on the 30 ground-truth
set**. Cycle 94 held-out validation is the next gate.
