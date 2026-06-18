# 04kkk · Cycle 106 — R4 Rate-Distortion routing over Pile A (YELLOW)

**Status:** **YELLOW**. Oracle K×d×p sweep on Pile A (31 fixtures —
v1.2.8 "size > 1.3× tiny ∧ DSSIM ≈ 0" cluster) clears **23/31 (74.2%)**
of the attacked set. Projected to the corpus-500 506-fix cohort this
lifts the two-axis PASS rate from v1.2.8 baseline **106/506 (20.9%)**
to **129/506 (25.5%)** — across the YELLOW gate (≥ 25%) but well shy
of the GREEN gate (≥ 35%). No production wiring this cycle; results
fund the Cycle 107+ attack on non-Pile-A regions.

baseline-7 sanity unchanged (size 5/7 PASS @ 0.799× cohort, DSSIM 6/7
nupic wins). Visual eye gate: 5/5 representative fixtures pass.

## TL;DR

| metric | v1.2.8 | Cycle 106 oracle (Pile A) | delta |
|---|---:|---:|---:|
| corpus-500 PASS (size ≤ 0.80× tiny ∧ DSSIM ≤ tiny) | 106/506 (20.9%) | 129/506 (25.5%) | **+23** |
| Pile A PASS | 0/31 (0%) | 23/31 (74.2%) | +23 |
| Pile A cohort bytes (winners only) | — | 0.59× tiny (15.86 MB vs 26.89 MB) | — |
| baseline-7 size cohort ratio | 0.799× tiny | 0.799× tiny | unchanged |
| baseline-7 DSSIM wins | 6/7 nupic | 6/7 nupic | unchanged |

## What changed vs Cycle 106-pre

Cycle 106-pre flagged Pile A as the size-bleed cluster — 31 corpus-500
fixtures where v1.2.8 wastes ≥ 30% bytes vs TinyPNG even though its
DSSIM is essentially 0 (round-off). Hypothesis: a per-fixture (K, d, p)
selector should trade a small DSSIM rise (still ≤ tiny_dssim) for a
large size drop.

This cycle implements the oracle: full K∈{64,96,128,160,192,224,256} ×
d∈{0.0,0.3,0.6} grid at preset=6, per-fixture DSSIM via in-process
`nupic_core::metrics::dssim` (~5× faster than CLI subprocess), Pareto
pick = smallest size with DSSIM ≤ tiny_dssim ∧ size ≤ 0.80 × tiny.

## Pile A verdict — 23/31 PASS

Winning config histogram (n=21 first-pass + 2 zopfli rescue = 23):

| K | wins | d | wins |
|---:|---:|---:|---:|
| 96  | 1 | 0.0 | 9 |
| 128 | 1 | 0.3 | 11 |
| 160 | 2 | 0.6 | 1+2 (zopfli edge) |
| 192 | 5 | | |
| 224 | 7 + 1 (zopfli rescue) | | |
| 256 | 5 | | |

**Center of mass: K = 192-256, d = 0.3.** Counter-intuitively, **K=192
often produces smaller files than K=128** (more palette → smoother
gradients → tighter PNG filter compression on photo content).

Failures (8 fixtures):

| reason | n | examples |
|---|---:|---|
| DSSIM-infeasible (no K in {64..256} beats tiny_dssim) | 6 | p125, p274, p214, p115, p175, p167 |
| Size-edge after zopfli refine | 1 | p295 (0.807× tiny, needs another −1%) |
| Tiny-input ceiling | 1 | n36_comet (12.8 KB tiny — K=64 already 15.2 KB, no headroom) |

The DSSIM-infeasible cluster shares a profile: TinyPNG quality is very
high (tiny_dssim ≤ 0.003) on noisy / high-frequency Picsum content
where index-quantized PNG can't preserve enough detail at any K ≤ 256.
**These are not Cycle 106 targets** — they need either lossless
preserve-mode routing or a different compression backend.

## Zopfli rescue probe — 2/4

Re-ran 4 size-edge failures (DSSIM passes, size 0.80-0.82× tiny) with
oxipng + zopfli (30 iters, full filter set):

| fixture | floor cfg | plain ratio | zopfli ratio | rescued? |
|---|---|---:|---:|:---:|
| n24_sun | K=224 d=0.6 | 0.817× | **0.788×** | ✓ |
| p283 | K=64 d=0.0 | 0.807× | **0.790×** | ✓ |
| p295 | K=192 d=0.6 | 0.810× | 0.807× | ✗ (−0.7% short) |
| n36_comet | K=64 d=0.3 | 1.184× | 1.173× | ✗ (no fit) |

n24_sun (sun corona photo) and p283 (low-contrast beach fog) both
cleared the 0.80× cap once the encode chain went through zopfli. This
gives us **2 free PASSes** for ~30 sec wall-clock per fixture extra
encode time — not cheap enough for production hot path but useful as a
"slow tier" option.

## Projection to corpus-500 — 25.5% YELLOW

Replacing Pile A's 31 baseline outputs with the 23 winners (and
keeping the 8 fails at their baseline) yields oracle projected cohort
PASS of **129/506 = 25.5%**, clearing YELLOW (≥ 25%) but not GREEN
(≥ 35%). The remaining ~50 fixtures needed for GREEN are not in Pile
A — they're in Pile B/C (size-pass-DSSIM-fail or both-fail) regions
that this cycle didn't attack.

## What this rules out

- **K ≤ 128 is wrong for Pile A.** The Cycle 105 ship at K=128 leaves
  ≥ 35% bytes on the table for 21 of 31 Pile A fixtures. Production
  routing must take K up to 192-256 for photo-class content where
  tiny_dssim ≥ 0.002.
- **d = 0.0 is not always optimal.** d = 0.3 wins as often as d = 0.0
  (11 vs 9), particularly on Picsum HD photos. The "off-by-default
  sweet spot" assumption from Cycle 41 holds only for low-K /
  transparent regimes.
- **Pile A is not where GREEN gate lives.** Even with perfect oracle
  routing on all 31 attacked fixtures, we'd lift cohort PASS by ~6 pp.
  The remaining 9.5 pp gap to GREEN sits in the 475 non-Pile-A
  fixtures.

## What this opens up

- **Production routing slot for K = 224 ± d=0.3 ± p=6.** This config
  wins 7/23 — the single biggest cluster. A routing predicate
  triggering on "photo content with input_size > tiny_size × 1.5" (no
  DSSIM inspection at production-time) should hit this slot
  effectively.
- **Zopfli "slow tier".** Worth a future `--slow` / `nupic compress
  --effort 9` mode that runs the zopfli refine pass — 2 free PASSes
  per 4 candidates is a defensible ROI for users with no time
  pressure.
- **Cycle 107 — Pile B/C attack surface.** The 50 fixtures needed for
  GREEN aren't in size-bloat territory. Next cycle should classify
  the non-Pile-A FAILs (which one of {FAIL-SIZE, FAIL-QUAL, FAIL-BOTH})
  and attack whichever cluster has the largest oracle headroom.

## Visual eye gate

Sampled 5 fixtures across the spectrum (n24_sun zopfli-rescue, n17
galaxy, p243 office photo K=256, p107 macbook K=256 d=0.3, p283 beach
fog K=64 zopfli). All visually clean — no banding on smooth gradients,
no posterization on photo content, dither artifacts on n24's sun
corona within "acceptable for −21% bytes" envelope.

## Files

- `crates/nupic-research/examples/cycle106_r4_rd_pile_a.rs` — first-pass
  K∈{64,96,128,192} × d×p sweep
- `crates/nupic-research/examples/cycle106_r4_rd_pile_a_grid.rs` —
  extended grid K∈{64..256} × d∈{0,0.3,0.6} with full per-config dump
- `crates/nupic-research/examples/cycle106_r4_rd_emit.rs` — emit
  winner PNGs + zopfli edge-rescue probe
- `assets/png-bench/cycle106-r4/pile_a_grid.tsv` — 651-row per-config
  grid dump
- `assets/png-bench/cycle106-r4/pile_a_winners.tsv` — 21 first-pass
  winners (config + size + dssim)
- `assets/png-bench/nupic-corpus-500-c106-r4/*.png` — 23 winner PNGs
  (21 first-pass + 2 zopfli rescue)

## Decision

Not shipping v1.2.9 — gate is YELLOW, not GREEN. Pile A oracle data
funds Cycle 107 routing-predicate design but does not justify a
production wiring on its own. Next cycle starts with non-Pile-A
classification + oracle headroom mapping.
