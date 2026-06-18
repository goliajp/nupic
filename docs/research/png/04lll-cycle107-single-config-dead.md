# 04lll · Cycle 107 — single-config K=224 routing is dead (RED) + workflow tooling

**Status:** **RED for single-config production routing**.

Cycle 106 found Pile A oracle winners cluster at K=224 d=0.3 p=6
(7/23 = 35% of winners). Cycle 107 tested whether **that single config
applied to all corpus-500 fixtures** could function as v1.2.9
production default. It cannot — **flipping K from the v1.2.8 default
(128) to 224 regresses the original PASS cohort by 16-25%** (4/25 on a
100-sample stratified bench, confirmed by a second 32-sample run with
6/8 retained = 25% regression). Pile A / B / C net new wins are tiny
(1/8 — 0/8 — 0/8 in the 32-sample run).

Net cohort PASS rate drops from baseline 20.9% to ~22% — a wash, and
under the production constraint of "no PASS regression," **net
negative**.

**This kills "no input classifier, pick K=224 everywhere" as a v1.2.9
path.** Production routing must be input-feature-aware (per-fixture K
selection). That's [`algorithm-ideas.md` idea A — content-aware K
predictor], next cycle's spike.

Cycle 107 also delivered the **workflow tooling** that restores the
Cycle 102-105 SSIM-era iteration cadence after the corpus-500 / DSSIM
shift made naive sweeps ≥ 30 min.

## TL;DR

| spike | result | wall |
|---|---|---:|
| single-config K=224 d=0.3 p=6 on 100 stratified-sample | **22/100 PASS**, original PASS pile regresses 4/25 | 57s (13-core overspend, crushed UI) |
| single-config K=224 d=0.3 p=6 on 32 quick-bench sample | **7/32 PASS** (PASS 6/8, PileA 1/8, PileB 0/8, PileC 0/8) | 54s (4-core capped, machine responsive) |
| (abandoned) full-corpus 506 × 1 config single-thread | n/a — wall-budget ~30 min, killed by user | — |
| (abandoned) oracle K×d sweep on 100 sample 4-core | n/a — work × core math gave ~18 min wall, killed | — |

## What was tested

K=224 d=0.3 p=6 emerged from Cycle 106 Pile A oracle sweep as the
single-most-common winning slot (7 of 23 winners). The Cycle 107
hypothesis: replace v1.2.8 production default (K=128 + Cycle 105
P-01/P-03 overrides) with **K=224 default**, no input classifier, no
predicate routing. If PASS rate goes up across PASS+PileA+PileB+PileC
cohort, that's the simplest possible production wiring path.

100-fixture stratified sample (25 from each of PASS / PileA / PileB /
PileC, deterministic stride-sample by sorted filename):

| pile | n | PASS both | size_pass | dssim_pass | verdict |
|---|---:|---:|---:|---:|---|
| PASS(原 v1.2.8 过) | 25 | 21 (84%) | 23 (92%) | 23 (92%) | **regressed 4 fixtures** |
| Pile A(size 退,DSSIM 已赢) | 25 | 0 (0%) | 2 (8%) | 23 (92%) | K=224 不够小,DSSIM 反而被压破 |
| Pile B(size 过,DSSIM 微退) | 25 | 1 (4%) | 20 (80%) | 5 (20%) | K=224 让 size 越界 |
| Pile C(双轴微退) | 25 | 0 (0%) | 2 (8%) | 11 (44%) | size 和 DSSIM 都没 fix |
| **TOTAL** | 100 | **22 (22%)** | 47 (47%) | 62 (62%) | net +1.1 pp vs v1.2.8 baseline 20.9% but **−4 fixtures regress** |

32-fixture quick re-test confirmed the trend (PASS 6/8 = 25% regression
rate, PileA 1/8, PileB 0/8, PileC 0/8; total 7/32 = 21.9%).

## Why single-config can't work

The four piles have **contradictory K needs**:

- **PASS pile** (mi / s synthetic / small p / wm small): K=128 was
  already enough. Pushing to K=224 grows palette overhead without
  perceptual gain — they were already DSSIM-pass under v1.2.8 K=128.
- **Pile A** (Picsum HD photo): K=128 generated dither artifacts +
  high filter-residual entropy → size bleed. K=224 helps **only when
  tiny_dssim is loose (≥ 0.005)** — the head of Pile A. The
  mid/tail of Pile A has tiny_dssim ≤ 0.002 and K=224 isn't enough.
- **Pile B** (size pass, DSSIM micro-fail): Already lean on bytes;
  K=224 makes them bigger and bumps them out of the 0.80× gate.
- **Pile C** (both micro-fail): Different content types — no single
  K helps both DSSIM (need higher K) and size (need lower K).

This is the classical **per-image RD vs cohort routing trade-off**.
The Cycle 102-103 P-01 / P-03 / P-07 predicate framework was the
right shape; Cycle 107 confirms it experimentally on a much larger
corpus.

## Workflow tooling

Cycle 106 single-thread sweep wall = ~30 min for Pile A 31 × 21 grid.
Cycle 107 attempted scale-up to 100-506 fixtures hit two failure modes:

1. **Full-corpus single-thread sweep** (`cycle107_single_config_full_corpus.rs`)
   estimated 30-50 min wall — killed by user as not fitting workflow
2. **Default `par_iter` over big fixtures** crushes the M2 with 13-core
   saturation (1304% CPU sustained) — UI freezes during multi-minute
   sweeps

The fix lives in `crates/nupic-research/src/bench.rs`:

- **`Fixture` struct + `load_corpus_500_with_baseline()`** — Pre-loads
  v1.2.8 baseline `(tiny_size, tiny_dssim, baseline_nupic_size,
  baseline_nupic_dssim, pile, family)` from `corpus-500-three-axis.tsv`
  + `corpus-500-dssim.tsv` + `cycle107/pile_classification.tsv`. Spike
  inner loop **never re-computes TinyPNG DSSIM** — that data is fixed
  per fixture and was already cached.
- **`pile_sample_24()`** — Deterministic stride-sample 8 per
  {PASS, PileA, PileB, PileC} for a 32-fixture stratified bench.
- **`bench_pool()`** — Returns `rayon::ThreadPool` capped to 4 threads
  (override via `NUPIC_BENCH_THREADS` env var). On M2 this leaves
  9 cores for the user's UI / build / browser.

`cycle107_quick_single.rs` demonstrates the template:

```
sample: 32 fixtures (pile stratified, 8 each)
wall = 54.0s (1.69s/fixture, 4 cores via bench_pool)
PASS 7/32 (21.9%)
```

Per-fixture wall (~1.7s) matches the SSIM-era baseline-7 pace. Sample
size ↑ 4.5× (7 → 32) so total wall ↑ 4.5× but **trend is visible in
under 1 min, not 30 min**.

## Decision

- **No v1.2.9 ship.** v1.2.8 stays production.
- **Drop "K=224 single-default" path.** It's a 16-25% PASS regression
  with marginal gains elsewhere.
- **Cycle 108 = input feature classifier (algorithm-ideas idea A).**
  Train a decision tree on Cycle 106 Pile A oracle ground truth
  (per-fixture optimal (K, d) from `assets/png-bench/cycle106-r4/pile_a_grid.tsv`),
  validate against the quick-single bench (32 stratified) so it does
  not regress PASS pile.
- **Workflow tooling becomes the standard.** Future cycles import
  `nupic_research::bench` and stay under 2-min walls for first-pass
  diagnostic spikes.

## Files

- `crates/nupic-research/src/bench.rs` — bench helper module (graduates
  to lib so future spikes import, not copy)
- `crates/nupic-research/examples/cycle107_quick_single.rs` — demo /
  template using the new helpers
- `crates/nupic-research/examples/cycle107_single_config_sample.rs` —
  the 100-sample single-config negative-result spike (kept for the
  bigger n)
- `crates/nupic-research/examples/cycle107_single_config_full_corpus.rs`
  + `cycle107_oracle_sample.rs` — **abandoned**, kept as evidence of
  workflow-breaking sweep attempts
- `assets/png-bench/cycle107/pile_classification.tsv` — corpus-500
  per-fixture (PASS / PileA / PileB / PileC) classification, baseline
  for future cycle's pile sampling
- `assets/png-bench/cycle107/single_config_sample.{tsv,log}` — 100-sample
  raw data
- `assets/png-bench/cycle107/single_config_full.{tsv,log}` — abandoned-run
  empty artifacts (left for trace)
- `.claude/research-ledger/cycle-107-table-report.md` — table-format
  per-pile verdict + decision matrix
- `.claude/research-ledger/paper-material.md` — new entry: "RD per-image
  vs cohort routing trade-off — confirmed experimentally"
- `.claude/research-ledger/algorithm-ideas.md` — idea A promoted to
  rank 1, idea F promoted to rank 2; B/E remain ★★★★★ paper kernels
