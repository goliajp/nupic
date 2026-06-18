# Cycle 119 — effort=9 zopfli slow-tier validation — table 收尾报告

**Date**: 2026-06-19
**Verdict**: **YELLOW, doc-only (no version bump, no production code change)**
**Spike**: inline shell + `crates/nupic-quantize/src/lib.rs:412` saturation probe (reverted)
**Essay**: `docs/research/png/04sss-cycle119-effort9-zopfli.md`
**Data**: `/tmp/c119-*` (transient, not persisted)

## Existing wire status (pre-cycle 119)

| location | mapping | wired by |
|---|---|---|
| `crates/nupic-quantize/src/lib.rs:411-416` | `effort ≥ 7 → iters = (effort-6) × 5, cap 30` | Cycle 21 |
| `crates/nupic-core/src/ops/compress.rs:426-431` | same mapping | Cycle 21 follow-up |

Cycle 119 hypothesis "needs new wire" was wrong on grep. Hypothesis
shifted to: does pushing iters above 15 still pay?

## Saturation probe (the real Cycle 119 contribution)

5 fixtures that effort=9 (iters=15) leaves in 0.80-0.82× (close to PASS but
not crossing). Patched iters → 30:

| fixture | tiny KB | e9 iters=15 KB | e9 iters=30 KB | Δ% | flip? |
|---|---:|---:|---:|---:|---:|
| p436_sm_460x260 | 54 | 43 | 43 | 0.01 | no |
| p36_480x320 | 68 | 55 | 55 | 0.05 | no |
| p445_sm_380x320 | 50 | 40 | 40 | 0.02 | no |
| p410_sm_380x380 | 55 | 44 | 44 | 0.00 | no |
| p70_1024x768 | 333 | 270 | 270 | 0.00 | no |

**Saturation @ iters=15. 0/5 flip. Mapping reverted.**

Cross-check via existing effort=10 (iters=20): same picture, Δ ≤ 0.12% on
5 already-passing fixtures. Two-way confirmation.

## Size-edge cohort PASS flips via existing effort=9

| cohort band | n | flip to PASS | rate |
|---|---:|---:|---:|
| 0.80-0.85× tiny | 25 | **14** | 56% |
| 0.85-0.95× tiny | 25 | **2** | 8% |
| **total** | **50** | **16** | **32%** |

Pattern: large palette-quantized photos (1920×1080+) in the 0.80-0.85
band flip cleanly. Small icons (≤ 1024 px) plateau at ratio ≈ 0.81-0.82
regardless of zopfli; their bottleneck is the 256-palette ceiling.
0.85+ cohort needs the transcoder rescue path (cycle 116-118), not
deflate tuning.

## Perf KPI check (9.83 MP)

| fixture | e5 wall | e9 wall | e9 / 60 s KPI |
|---|---:|---:|---:|
| p245_3840x2560 | 5.68 s | **245.43 s** | **4.1×** over |
| p274_3840x2560 | 5.31 s | **218.36 s** | **3.6×** over |

`--effort 9` on 9.83 MP is opt-in slow-tier territory. Both fixtures
already PASS at effort=5; effort=9 only tightens (-13% to -15%) at
huge wall cost.

## v1.2.11 baseline-7 sanity (cycle 119 confirmation)

| metric | e5 | e9 | OK? |
|---|---|---|:---:|
| TOTAL ratio | 0.799× | 0.795× | ✓ tighter |
| per-fixture regression | n/a | 0/7 | ✓ none |
| DSSIM (zopfli is size-only) | unchanged | unchanged | ✓ |

## Tests + binary

| gate | result | OK |
|---|---|:---:|
| `cargo build --release` | clean | ✓ |
| `nupic --version` | 1.2.11 | ✓ (unchanged) |
| 219 workspace tests | **219 pass 0 fail** | ✓ |
| baseline-7 default byte-identical with v1.2.10/11 | ✓ | ✓ |

## Decision gate (vs cycle 119 kickoff §4)

| gate | required | actual | met? |
|---|---|---|:---:|
| ≥ 5 fixture flip to PASS | yes | 16 | ✓ |
| baseline-7 not regressed | yes | tighter | ✓ |
| wall ≤ 60 s on 9.83 MP | yes | **218-245 s** | ✗ |

GREEN gate (AND) **fails on wall**. Fall to YELLOW = effort=9 documented
as opt-in slow tier, no version bump, no production code change.

## Mini infra note

- Spike split: short ops on dev box, long wall + 50-fixture sweep on
  `mini` (Mac mini, arm64, rustc 1.96).
- Mini scratch dir `~/.c119-scratch` (binary + 27 fixture pairs = 77 MB),
  removed on cycle close. Zero zombie processes after run.
- Setup workflow: `rsync` binary + filtered fixture list (no git clone,
  no cargo build on mini). Cycle 120+ will follow the same pattern.

## What ships

**Nothing.** No code, no version bump. Just:
- this ledger
- essay `04sss-cycle119-effort9-zopfli.md`
- one comment block added to `nupic-quantize/src/lib.rs:411-419` documenting
  the cycle 119 saturation finding so the next cycle doesn't re-litigate
- memory sync to `cycle120_kickoff.md`

## algorithm-ideas update

idea C "slow-tier zopfli for size-edge rescue":
**RESOLVED (Cycle 21 already shipped, Cycle 119 validated + saturated)**.
Closed in `algorithm-ideas.md`.
