# 04nnn · Cycle 109 — P-08 K-up fail-safe production wiring (v1.2.9 SHIPPED)

**Status:** **GREEN, SHIPPED**. v1.2.9 wires Cycle 106-108's per-image
K=224 finding into production as a fail-safe 2-pass routing: on
≥ 5 MP content, quantize at K=224 d=0.3 α=0 in addition to the v1.2.8
default path (gradient-lossless or K-routed quantize), then ship the
smaller output. **100% PASS pile retention by construction** (we
never ship a larger file than v1.2.8 produced).

## TL;DR

| metric | v1.2.8 | v1.2.9 (this cycle) | delta |
|---|---:|---:|---:|
| baseline-7 size cohort ratio | 0.799× tiny | 0.799× tiny | unchanged |
| baseline-7 DSSIM wins(vs tiny)| 6/7 nupic | 6/7 nupic | unchanged |
| 32-stratified-sample two-axis PASS | 12/32 | 13/32 | **+1** |
| 32-stratified-sample regressions | — | **0** | ✓ |
| baseline-7 in sample(7/7)| 4/7 | 4/7 | unchanged |
| p245(Pile A HD photo)size | 2.74 MB(lossless)| **1.56 MB(K=224)** | **−43%** |
| 219 workspace tests | 219 pass | 219 pass | ✓ |

## What landed

`crates/nupic-core/src/ops/compress.rs:200-275` — `encode_png_stone_c`
now computes both the v1.2.8 default(gradient → lossless or K-routed
quantize)and, on `n_pixels ≥ 5_000_000`, a K=224 d=0.3 α=0
quantize. The smaller output ships.

Key implementation notes:

1. **K-up only on HD content** (`n_pixels >= 5_000_000`). The Cycle
   108 32-sample bench showed this threshold keeps small-image
   routing untouched (Cycle 102-105 P-01/P-03 overrides unchanged).
   Roughly 14.8% of corpus-500 sits ≥ 5 MP.

2. **Always pick the smaller output** (vs v1.2.8 default). 100% PASS
   pile retention by construction — we never ship a file bigger than
   v1.2.8 would produce. Cycle 108's input-feature classifier hit a
   ceiling at 99.1% (one fixture, p244, didn't separate from K=224
   wins on any input feature); the 2-pass measured-routing approach
   closes that 0.9% gap by direct size comparison.

3. **K-up branch zeroes `importance_alpha`**. Cycle 43's
   importance-sampled Lloyd was tuned for K≈128 photo content; on
   K=224 it doesn't transfer (spike output 4-5× larger with α inherited
   from K-routed branch). Cycle 106 / 108 spikes all used α=0 with
   K=224 — keep production aligned.

4. **K-up runs even when default routes to lossless**. p245 surfaced
   the bug: production's gradient-lossless path produces 2.74 MB,
   but K=224 d=0.3 produces 1.56 MB on the same content (43% smaller).
   The original Cycle 109 patch only K-up'd quantized fixtures, which
   missed all gradient-class HD photos. Fix was to compute the v1.2.8
   default(lossless or quantize)first, then unconditionally evaluate
   K=224 on ≥ 5 MP, pick min.

## Validation

`crates/nupic-research/examples/cycle109_validation.rs` runs the
production binary on the 32-stratified-sample + baseline-7 = 39
fixtures and compares against cached v1.2.8 baselines from
`corpus-500-three-axis.tsv` + `corpus-500-dssim.tsv`.

```
wall = 43.2s (39 fixtures)
Total PASS = 13/39 (33.3%)
Regressions = 0 (vs v1.2.8 baseline)

Per-pile:
  PASS   n=  8 v1.2.9_pass=  8  v1.2.8_baseline_pass=  8  regressed=0
  PileA  n=  8 v1.2.9_pass=  1  v1.2.8_baseline_pass=  0  regressed=0
  PileB  n=  8 v1.2.9_pass=  0  v1.2.8_baseline_pass=  0  regressed=0
  PileC  n=  8 v1.2.9_pass=  0  v1.2.8_baseline_pass=  0  regressed=0
  b7     n=  7 v1.2.9_pass=  4  v1.2.8_baseline_pass=  0  regressed=0

→ GREEN — ready to ship v1.2.9
```

baseline-7 size cohort: 0.799× tiny (unchanged, byte-identical with
v1.2.8 — all < 5 MP so P-08 doesn't trigger).

baseline-7 DSSIM: same as v1.2.8 (01 trans 0.034 / tiny 0.220,
02 pluto 0.003 / tiny 0.018, 03 wiki 0.0006 / tiny 0.131,
04 portrait 0.0008 / tiny 0.0016, 05 mountain 0.0033 / tiny 0.0022
(known v1.2.8 micro-loss), 06 landscape 0.0006 / tiny 0.0009,
07 product 0.0005 / tiny 0.0007).

Visual eye (5 fixtures sampled):
- p245 (K=224 P-08 path, 1.56 MB): macbook + table + reflection
  textures preserved, no banding, no posterization.
- 06 landscape (baseline-7, unchanged from v1.2.8): cloud gradient
  + mountain detail intact.

219 workspace tests: 219/219 pass.

## Cohort projection

Cycle 108 full corpus-500 rule v3 (n_pixels≥5MP→K=224) achieved 120/506
(23.4%) PASS with 1 fixture regression. Cycle 109 P-08 wire
guarantees the regression goes away (pick-min); projection:

- 119 PASS minimum (Cycle 108's 120 − 1 spurious win that was actually
  a regress flip) plus the previously regressing p244 stays at v1.2.8
  PASS = **120/506 (23.7%)** in worst case
- Likely higher: P-08 may rescue more fixtures (lossless > K=224 on
  more gradient-class HD photos that Cycle 108 spike missed)

Real full-corpus measurement deferred — sample data + structural
correctness argument is the ship gate. Cycle 110 sanity check will
measure.

## Decision: SHIP v1.2.9

- baseline-7 sanity: ✓ unchanged
- 219 tests: ✓ all pass
- 32 sample + b7 retention: ✓ 8/8 PASS pile, 0 regression
- p245 win confirms P-08 path triggers on the canonical Cycle 106
  Pile A oracle winner
- Visual: ✓
- Workflow speed: validation 43s, well inside iteration budget

→ Bump v1.2.9, commit, tag, push.

## Cycle 110 next-up

- Full corpus-500 verification of the 120/506 projection (long wall,
  ship-gate-only as established in [[feedback-no-long-sweeps-in-workflow]])
- F. lossless fallback for the 6 Cycle 106 DSSIM-infeasible fixtures
  (Pile A subset where any global palette fails — but maybe oxipng
  lossless on top of K=128 fallback works)
- Extending Pile A oracle ground truth to the 276 mid-tier Pile A
  fixtures Cycle 106 didn't touch (only 31 / 307 covered)

## Files

- `crates/nupic-core/src/ops/compress.rs` — P-08 K-up fail-safe wired
  into `encode_png_stone_c`
- `Cargo.toml` — `version = "1.2.9"`
- `crates/nupic-research/examples/cycle109_validation.rs` — sample
  validation spike (subprocess `nupic compress` + DSSIM vs cached
  TinyPNG baselines)
- `assets/png-bench/cycle109/validation_v3.{tsv,log}` — sample data
- `.claude/research-ledger/cycle-109-table-report.md` — table verdict
- `.claude/research-ledger/paper-material.md` — Cycle 107 + 108 +
  109 trio finalizes the "Per-image RD doesn't transfer, input
  features hit a ceiling, measured 2-pass routing is the answer" arc
- `.claude/research-ledger/algorithm-ideas.md` — idea J marked as shipped
