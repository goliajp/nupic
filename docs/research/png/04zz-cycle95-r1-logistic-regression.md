# 04zz · Cycle 95 — R1 logistic regression RED (overfits + class-imbalance trap)

**Status:** RED. 7-feature logistic regression on 30 training fixtures, evaluated on
Cycle 94's same held-out 20, **cannot beat** the threshold-based 5-rule. Class
imbalance (7 FRIEND / 23 HOSTILE in train) traps the model between two bad regimes:
- **unweighted:** collapses to majority HOSTILE → 13/20 = 65% test acc but **5 FN**
- **class-weighted (3-5×):** over-predicts FRIEND → 6/20 = 30% test acc with 11-12 FP

No hyperparameter setting achieves test acc ≥ 80% AND FN ≤ 1. This **closes** the
R1-classifier investigation thread that ran across Cycles 91a → 92 → 93 → 94 → 95.

## TL;DR — 5 cycles of R1 routing classifier results

| cycle | approach | eval setting | acc | FP | FN | verdict |
|---|---|---|---:|---:|---:|---|
| 91a | 4-rule threshold | fit + eval on 10 | 9/10 = 90% | 1 | 0 | YELLOW→GREEN (training acc) |
| 92 | 4-rule threshold | held-out 20 | 12/20 = 60% | 8 | 0 | RED-acc, GREEN-FN |
| 93 | 5-rule threshold (+ bandpass) | fit + eval on 30 | 27/30 = 90% | 3 | 0 | GREEN (training acc) |
| 94 | 5-rule threshold | held-out 20 | 9/20 = 45% | 8 | 3 | RED (both) |
| **95** | **7-feature LR (unweighted)** | held-out 20 | 13/20 = 65% | 2 | **5** | RED-FN |
| 95 | 7-feature LR (cw=5, balanced) | held-out 20 | 6/20 = 30% | 12 | 2 | RED-acc |

**Net findings across the thread:**
- Threshold methods generalize from 90% (train) to 45-60% (held-out) — large overfit gap.
- LR shifts the failure mode but cannot beat threshold: balancing class weight trades FN
  for FP 1:1, never crossing both the acc and FN gates.
- **Simple feature methods (threshold or linear) at 30-fixture training scale cannot
  ship as production routing for R1.**

## Hyperparameter sweep (best of 8 configs by FN-first, then acc)

| lr | l2 | n_iter | cw+ | train_acc | test_acc | FP | FN | verdict |
|---:|---:|---:|---:|---:|---:|---:|---:|---|
| 0.10 | 0.01 | 2000 | 1.0 | 25/30 | 13/20 | 2 | 5 | Y(acc) |
| 0.10 | 0.01 | 2000 | 3.3 | 22/30 | 6/20 | 11 | 3 | R |
| 0.10 | 0.01 | 2000 | 5.0 | 16/30 | 5/20 | 12 | 3 | R |
| 0.10 | 0.10 | 2000 | 3.3 | 25/30 | 5/20 | 11 | 4 | R |
| **0.10** | **0.10** | **2000** | **5.0** | **14/30** | **6/20** | **12** | **2** | **R (best FN)** |
| 0.05 | 0.01 | 5000 | 3.3 | 22/30 | 5/20 | 12 | 3 | R |
| 0.05 | 0.01 | 5000 | 5.0 | 16/30 | 5/20 | 12 | 3 | R |
| 0.05 | 0.10 | 5000 | 3.3 | 25/30 | 5/20 | 11 | 4 | R |

LR learns sensible feature signs from the training data:

```
intercept b = +0.23
w_chroma   = −0.09   (near-zero — chroma is not predictive after edge/bandpass)
w_smooth   = −0.28   (negative: high smoothness ⇒ HOSTILE)
w_edge     = +0.35   (positive: high edge density ⇒ FRIEND)
w_trans    = +0.37   (positive: tRNS ⇒ FRIEND)
w_bandpass = +0.35   (positive: mid-scale energy ⇒ FRIEND — matches Cycle 93)
w_entropy  = +0.34   (positive: broad chroma distribution ⇒ FRIEND)
w_ec_corr  = −0.08   (near-zero — Pearson is too aggregated)
```

The weights match the threshold rules' direction. The problem is the **decision
boundary is in the wrong place relative to the test distribution** — not that the model
learned wrong features.

## Per-fixture test predictions (best-FN config, cw=5)

| fixture | ΔSSIM | actual | LR score | pred | verdict |
|---|---:|---|---:|---|---|
| mi2 | 0.00 | H | 0.411 | H | OK |
| n20_moon | +0.08 | H | 0.632 | F | FP |
| p35 | +0.75 | F | 0.537 | F | OK |
| p40 | +0.10 | H | 0.762 | F | FP |
| p432 | +2.96 | F | 0.620 | F | OK |
| p445 | −0.13 | H | 0.629 | F | FP |
| p6 | −2.97 | H | 0.759 | F | FP (the same boundary case from Cycle 94) |
| s086 / s090 / s094 | 0.00 | H | 0.666-0.680 | F | FP × 3 |
| n04_mars | −1.50 | H | 0.631 | F | FP |
| p100 | +0.01 | H | 0.703 | F | FP |
| p15 | +0.15 | H | 0.575 | F | FP |
| p3 | +0.34 | H | 0.715 | F | FP |
| **p427** | **+2.15** | F | 0.583 | F | OK (LR caught it, threshold missed) |
| p4 | −1.81 | H | 0.640 | F | FP |
| p73 | +0.41 | H | 0.485 | H | OK (just under threshold) |
| **p97** | **+0.61** | F | 0.487 | H | **FN** (just under) |
| **s031** | **+1.03** | F | 0.454 | H | **FN** (just under) |
| s061 | 0.00 | H | 0.165 | H | OK |

LR with class weighting recovered p427 (which threshold missed) but lost 9 fixtures
that the threshold got right. The score distribution is concentrated 0.5-0.7 — the
model is uncertain on most test cases. This is the **classic small-n high-FRIEND-weight
trap**.

## What the 5-cycle thread proved (paper §6 content)

1. **Hand-crafted 4-rule on 10 fixtures: 90% (overfit).** First-cycle classifier
   accuracy on small data does not predict held-out performance.
2. **Same 4-rule on 20 held-out: 60% (drop 30 pt).** Generalization gap visible
   immediately at scale.
3. **Add bandpass feature, 5-rule on 30: 90% (re-overfit).** New feature doesn't fix
   the generalization problem.
4. **Same 5-rule on 20 held-out: 45% (drop 45 pt).** Generalization gap widens with
   more features at same training scale.
5. **Switch to LR on same 30 → eval on same 20: 65% (acc) but 5 FN, or 30% with class
   balancing.** Linear learned model cannot beat threshold at this data scale.

**The bottleneck is data, not model class.** With 30 training fixtures, neither
thresholds nor LR can generalize. To proceed honestly with R1 routing, **Cycle 96
would need ≥ 80 ground-truth fixtures** — at ~3-10 s per bench, that's 5-15 minutes
of new compute per cycle.

## Recommendation: pivot R1 routing thread

Per the [[research-roadmap-1-2-x]] decision gates, the R1 thread has consumed 5 cycles
without reaching ship-readiness. The honest move is to **pivot the autorun cadence**
to a fresh paper path:

- **Option A:** Cycle 96 = R4 rate-distortion grid (★★★★, paper §5 framework). Different
  axis, independent of routing. Can ship as configuration knob without classifier.
- **Option B:** Cycle 96 = R10 oxipng filter prediction (★★★, paper §7 engineering).
  Different axis, ship as perf win.
- **Option C:** Cycle 96 = ship R1 unconditionally (per Cycle 90 mean +4.27 SSIM
  across mixed corpus). Document the per-content caveat. Pragmatic but accepts the
  outlier regressions.
- **Option D:** Continue Cycle 96 = grow ground truth to 80+. 15 min compute up
  front; if LR still RED at 80, declare R1 routing infeasible at simple-feature scale.

**Recommendation for autorun:** Option D (continue the thread to its honest natural
end) at this size, but if that also RED's, hard-pivot to Option A in Cycle 97. R1
should not consume more than one further cycle of investigation.

## Files

- `crates/nupic-research/examples/cycle95_r1_logistic_regression.rs` — LR + class
  weighting + hyperparameter sweep. Pure feature-only (no bench), runs in ~30 s.
- Thread essays: 04vv (91a) → 04ww (92) → 04xx (93) → 04yy (94) → 04zz (this).
