# 04eee · Cycle 100 — R4 widened router RED (closes simple-feature routing thread)

**Status:** **RED**. Four widened router variants (C1 UI-widen, C2 +edge gate,
C3 +bandpass sweep, C4 strict thresholds) **all land at 13/20 pass and
mean-wc 65%** on the corpus-500 sample — **identical to Cycle 99's B1
score** within the formula's rounding. Widening the predicates kills
some false-Chroma routes (p66 +0.47% gone) but also drops real wins
(n01_mars −0.39% lost), netting **zero cohort-level improvement**.

The bandpass_ratio sweep — the strongest remaining feature lever —
**finds no threshold that improves the gate**: best C3 chose `bp_t=0.0`
(no constraint), reverting to C2 behavior. The bandpass and
edge_chroma_corr features that earlier looked promising in the per-cycle
inspection do not separate the corpus-500 oracle classes cleanly enough
to close the gate.

This is the same wall the **R1 classifier thread hit at Cycle 95**:
hand-tuned simple-feature predicates clear on the 10-fixture cohort but
**do not generalize to the 20-fixture corpus-500 sample**. Two threads,
same diagnosis. Paper §6 reviewer defense gets a second concrete
ammunition point.

**The R4 simple-feature routing thread is closed RED.**

## TL;DR

| variant | UI rule | Chroma rule | pass | mean-wc | mean Δsize |
|---|---|---|---:|---:|---:|
| C99 B1 (baseline) | ent<3 AND edge>.2 | trans>0 OR (chr>0.025 AND sm<.05) | 13/20 | 66% | **−0.20%** |
| **C1** UI-widen | ent<3 AND (edge>.2 OR trans>.5) | B1 unchanged | 13/20 | 65% | −0.12% |
| **C2** +edge gate | C1 | + (edge > 0.2) on Chroma | 13/20 | 65% | −0.13% |
| **C3** +bandpass sweep | C2 | + (bandpass > t) | 13/20 | 65% | −0.13% (bp_t=0.0) |
| **C4** strict | C1 | chr>.04 AND sm<.04 AND edge>.2 | **11/20** | 55% | +0.00% |

Gate: **mean-wc ≥ 80% AND pass-fraction ≥ 60% → GREEN**. No variant
clears. **Best mean Δsize is still B1 from Cycle 99 at −0.20%**, which
the widened variants underperform by ≈ 0.07 percentage points.

## Why widening doesn't help (the trade-cancel)

Each widening change moves a fixture's classification, but the net effect
on cohort scores is zero because corpus-500's oracle structure is
**bimodal in a way the features don't separate**:

| change | wins | losses | net |
|---|---|---|---|
| UI predicate adds `trans > 0.5` | mi0 → UI (no longer Chroma+1.56%) | mi0 → K=128 d=0 still 0% wc (router can't hit K=128 d=0.3 oracle) | ~0 |
| Chroma adds `edge > 0.2` | kills p66 +0.47% | kills n01_mars −0.39% win | ~0 |
| Chroma adds `bandpass > t` | (no t improves anything) | tightening drops more wins than it kills false-Chroma | < 0 |
| Strict thresholds (C4) | kills 2 false-Chroma | kills p409 -2.11%, p449 -0.42%, others | strictly worse |

The C2 edge gate's diagnostic pair:
- **p66** chr=.065 sm=.019 **edge=.152** → fails edge gate → Stoch (correct, B1 had it false-Chroma)
- **n01_mars** chr=.045 sm=.019 **edge=.129** → fails edge gate → Stoch (regression: B1 had it correct Chroma)

These two fixtures look nearly identical in the (chroma, smoothness,
edge) feature space, but **their oracles diverge** (p66 wants default,
n01_mars wants K=256 d=0.5). The 5-feature set can't tell them apart.

## Bandpass_ratio observed distribution

The C3 sweep confirms what was visible in the per-fixture trace: bandpass
does not split oracle classes:

| fixture | oracle | bandpass | "expected by bp" |
|---|---|---:|---|
| p409 (Chroma -2.11%) | (256, 0.5) | 0.249 | low |
| p449 (Chroma -0.42%) | (256, 0.5) | 0.364 | mid |
| n01_mars (Chroma -0.39%) | (256, 0.5) | 0.546 | mid-high |
| n29 (Chroma -0.14%) | (256, 0.5) | 0.336 | mid |
| p119 (Chroma -0.67%) | (256, 0.5) | 0.456 | mid |
| p66 (false Chroma) | (256, 0.0) | **0.782** | high |
| p426 (false Chroma) | (256, 0.0) | 0.321 | mid |
| p38 (Stoch correct) | (256, 0.0) | **0.800** | high |
| p84 (Stoch correct) | (256, 0.0) | 0.460 | mid |

If high-bp signaled Chroma, p38 and p66 would be Chroma per oracle —
they're not. The bandpass feature carries a different signal than the
(K,d) selection problem requires.

## Closing the thread (paper §6 reviewer-defense material)

The R4 simple-feature routing thread spans 4 cycles, mirrors the R1
classifier thread (Cycles 91-95), and reaches the same wall:

| thread | classifier task | cohort | cycle | result |
|---|---|---|---|---|
| **R1** | friend/hostile for M-weighted Lloyd | 10-fix baseline | 91a | 9/10 train acc YELLOW |
| | | 20-fix corpus-500 | 92 | 12/20 = 60% YELLOW |
| | | 30-fix train+test | 93 | 27/30 fit (6-rule) YELLOW |
| | | 20-fix held-out | 94 | 9/20 = 45% RED, 3 FN |
| | | logistic regression | 95 | 13/20 = 65% best RED, 5 FN |
| **R4** | 3-class K/dither router | 10-fix baseline+5MP | 97 | 5/10 = 50% YELLOW |
| | | 10-fix (B1 threshold tune) | 98 | **8/10 = 80% GREEN** |
| | | 20-fix corpus-500 (B1) | 99 | 13/20 = 65%, wc 66% YELLOW |
| | | 20-fix corpus-500 (C1-C4) | 100 | **13/20 = 65%, wc 65% RED** |

Both threads exhibit the **train-test gap pattern**: hand-tuned features
that clear on calibration cohort do not generalize. Both use the same
feature set (chroma, smoothness, edge_density, bandpass_ratio,
chroma_entropy, edge_chroma_corr) and the same Cycle 92 corpus-500
ground truth.

**Paper §6 conclusion (after both threads):** *simple-feature
hand-routing is insufficient for content-aware perceptual PNG
quantization. The 6-feature set captures necessary signal for ≤ 10
hand-picked fixtures but does not generalize to ≥ 20 corpus-sampled
fixtures. A learned model or substantially richer feature representation
is required.*

## What CAN ship from the R4 thread

Even though no router clears the gate, **two concrete artifacts remain
useful**:

1. **R-D grid framework (Cycle 96 essay 04aaa)** — per-fixture
   (K, dither, preset) Pareto fronts are paper §5 framework material.
   The "default config sits on Pareto for 4/7 baseline-7 / ~13/20
   corpus-500" finding is itself a result: **most production content
   has near-optimal default settings; routing only helps a small
   fraction**.

2. **B1 router mean Δsize −0.20% on corpus-500** (Cycle 99) — even
   though wc gate fails, the router produces real cohort savings.
   This could ship as **opt-in `Quality::Auto-R4`** with documented
   per-fixture risk profile (+0.5% size cost on ~15% of content, large
   wins on the rest). Whether this passes user-facing release gates is
   a product decision, not a research one.

## Decision gate

- best variant mean win-capture = **65%** (gate ≥ 80%) ✗
- best variant pass-fraction = **65%** (gate ≥ 60%) ✓
- **RED on the GREEN/YELLOW/RED scale per Cycle 99's escalation logic.**
  Pass-fraction clears but mean-wc plateaus at 65% across all four
  widened variants and the C99 baseline — the simple-feature ceiling.

## Files

- `crates/nupic-research/examples/cycle100_r4_widened_corpus500.rs` —
  full driver. 180 encodes + 180 SSIM subprocess calls + 4 variant
  evaluations in 75 s.
- Previous: 04ddd (Cycle 99 B1 corpus-500 YELLOW), 04ccc (98 GREEN
  baseline), 04bbb (97 YELLOW), 04aaa (96 R-D grid framework).

## Cycle 101 next-up (autorun entry)

**Pivot to R10 (oxipng filter prediction)** — the remaining ★★★
paper-track item per [[research-roadmap-1-2-x]] § P4. R10 is the
"engineering / reproducibility" stone analog of R9 ICM SIMD: bench
oxipng filter selection cost on baseline-7 + 5MP, propose a 5-feature
linear classifier predicting optimal filter, target 5MP −50-80ms perf.

Lower-risk than R3/R6 (multi-tile / VQ-VAE) which are paper-major
spikes requiring infrastructure setup.

R4 closure means **no R4-derived production wiring** is queued. If a
future cycle revives R4 as a learned model (R3-adjacent), it pulls in
the Cycle 96-100 ground-truth datasets directly.
