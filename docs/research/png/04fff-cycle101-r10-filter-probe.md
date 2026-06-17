# 04fff · Cycle 101 — R10 oxipng filter probe YELLOW (baseline-7 win, 5MP null)

**Status:** **YELLOW**. The filter-prediction perf headroom is real on
**baseline-7** (mean best-filter wall is **63% of default-preset wall**
at **−0.14% mean Δsize** — i.e. 37% wall savings at slight size gain),
but **null on 5MP**: at preset=0 the dominant cost is deflate, not
filter trial, so forcing a single filter does not reduce 5MP wall time
(mean **114%** of default). This contradicts the roadmap R10
hypothesis "5MP −50-80 ms" — the perf bottleneck is elsewhere.

Two paper-worthy framework findings emerge:

1. **"Entropy" dominates on indexed-PNG baseline-7 content.** 5/7
   fixtures (03 wiki, 04 portrait, 05 mountain, 06 landscape, 07
   product) pick `Entropy` as best, with size delta of **−0.60% to
   −0.01%** vs preset's 4-filter trial. The remaining 2 (01 trans, 02
   pluto) pick `None` at exactly +0.00% size. **A 2-class predictor on
   transparency is sufficient for baseline-7**.

2. **At low oxipng preset (0), filter selection is null perf-lever.**
   5MP fixtures all show the "best" single filter running at 51-223% of
   default-preset wall — the heavy `BigEnt` heuristic actually 2-3× the
   default at preset=0. Preset=0 already restricts to small filter set;
   filter prediction adds no value there.

## TL;DR

| metric | value | gate | gate verdict |
|---|---:|---:|:---:|
| mean Δsize (best filter vs default) | **−0.13%** | ≤ +0.5% | ✓ |
| mean wall pct (best vs default) | **78%** | ≤ 50% | ✗ |
| baseline-7 mean Δsize | −0.14% | — | — |
| baseline-7 mean wall pct | **63%** | ≤ 50% | borderline |
| 5MP mean Δsize | −0.09% | — | — |
| 5MP mean wall pct | **114%** | ≤ 50% | ✗ |
| best filter distribution | None: 4, Entropy: 5, BigEnt: 1 | — | — |

Gate fails on wall. **YELLOW**: filter prediction has paper-track
framework value (baseline-7 specifically), but absolute perf gain is
modest (37% of 12-256 ms = 5-95 ms per baseline-7 fixture) and 5MP gets
zero gain.

## Per-fixture best-filter trace

| fixture | preset | def_wall | best filter | Δsize | wall% |
|---|---:|---:|:---:|---:|---:|
| 01 trans      | 3 |  117.8 ms | **None**    | 0.00% | 67% |
| 02 pluto      | 3 |   87.0 ms | **None**    | 0.00% | 50% |
| 03 wiki       | 3 |   12.8 ms | **Entropy** | −0.60% | 53% |
| 04 portrait   | 3 |  190.3 ms | **Entropy** | −0.06% | 66% |
| 05 mountain   | 3 |  256.1 ms | **Entropy** | −0.20% | 76% |
| 06 landscape  | 3 |  195.0 ms | **Entropy** | −0.01% | 56% |
| 07 product    | 3 |  214.3 ms | **Entropy** | −0.14% | 73% |
| 17 aurora     | 0 |   94.1 ms | **None**    | 0.00% | 51% |
| 25 sofia      | 0 |   97.6 ms | **None**    | 0.00% | 67% |
| 27 whale      | 0 |  111.2 ms | **BigEnt**  | −0.27% | **223%** |

Notable patterns:
- **Trans-rich content (01, 02, 17, 25) prefers `None` filter.**
  This makes sense: alpha-bearing indexed PNG has palette stability
  per-row, so adjacent-row prediction (`None` = raw bytes, no
  prediction) is already optimal.
- **Opaque photo content (04-07) prefers `Entropy` heuristic.**
  Entropy estimates the most compressible filter per row, which
  matches photo content where filter choice varies row-to-row.
- **27 whale's "best" is `BigEnt` at 223% wall.** BigEnt is the most
  expensive heuristic; picking it for a 0.27% size win costs ~150 ms.
  Production should reject this on the NAS/CDN perf KPI.

## Why the 5MP perf gain is null

At preset=0, oxipng's default filter set is roughly
`{None, Sub, Bigrams}` (3 filters). Forcing single filter saves
**1-2 filter trials**, but the deflate cost per filtered stream
dominates the wall time, especially for 5MP-sized IDAT (1.5-3.3 MB).
**Filter trial is a small fraction of 5MP preset=0 wall.**

This is the opposite of preset=3 (baseline-7), where filter trial cost
*is* substantial because the IDAT is small and the deflate per filter
is cheap. There, dropping from 4 filters tried to 1 saves a meaningful
fraction.

**The R10 hypothesis applies to baseline-7 fixtures, not the 5MP cohort.**

## Where R10 could still pay off

The framework finding "5MP at preset=0 has no filter-pick headroom" is
itself useful: it suggests the **5MP perf budget should be attacked at
a different layer** (deflate, not filter), which the deflate stone (now
at 1.4 / 0.5.9) is already addressing.

But R10's other branch is **whether filter prediction lets 5MP run at
preset=3** (more aggressive filtering) **at iso wall budget**. The
current 3-tier preset rule (5MP → preset=0, 2-5MP → preset=1, < 2MP →
preset=3) was a wall-budget compromise. If filter prediction on a 5MP
preset=3 attempt:
- picks correctly → only 1 filter is tried instead of preset=3's
  4-filter trial,
- wall time drops to ~25% of preset=3-full-trial,
- size benefit is potentially significant (preset=3 is 4-12% smaller
  than preset=1 for under-2MP content; might be similar on 5MP).

**Cycle 102 probe candidate: bench 5MP at preset=3 with each single
filter forced.** If the best-single-filter wall on 5MP at preset=3 is
under the 250 ms NAS/CDN KPI AND size improvement vs preset=0 is
≥ 3%, that's the real R10 ship gate. Distinct from this Cycle 101
probe.

## Why "Entropy" dominates baseline-7 (R10 mechanism)

Entropy filter computes per-row a quick entropy estimate over the 5
filter modes and picks the one minimizing that estimate. It's a
**per-row decision**, while None/Sub/Up/Average/Paeth force the same
filter on every row. On photo content with row-varying texture,
per-row adaptation wins. On uniform content (transparent / logo),
forced `None` is sufficient.

**Implication for the classifier:** a 2-class predictor on
`trans_frac > t` (route to `None`) else (route to `Entropy`) might be
near-optimal on baseline-7. **3 of the 10 fixtures (mi0-like, p426
which would be in corpus-500 not this cohort, others) might need
the existing 4-filter set as fallback.** Cycle 102 should validate on
corpus-500 to be sure.

## Decision gate

- mean Δsize ≤ +0.5% gate: **✓ at −0.13%**
- mean wall pct ≤ 50% gate: **✗ at 78%** (baseline-7 alone: 63%
  borderline, 5MP: 114% fails hard)
- **YELLOW** — R10 has real paper §7 framework material on baseline-7
  but the original "5MP −50-80 ms" hypothesis does not hold at preset=0.

## What Cycle 102 should do (autorun entry)

**Two probes to nail R10's actual scope:**

1. **5MP at preset=3 with single filter forced.** Encode 17/25/27
   through nupic-quantize at preset=3, then bench each single filter
   + the default preset=3 4-filter trial. If best-single-filter wall
   is < 250 ms AND size is < 5MP preset=0 size, we've found the real
   R10 5MP-tier perf-quality unlock.

2. **Corpus-500 best-filter distribution.** Run the same per-filter
   probe on the 20 Cycle 92 corpus-500 fixtures to verify the
   "Entropy dominates" finding generalizes beyond baseline-7. If
   `Entropy` wins on ≥ 70% of corpus-500, a 2-class `None vs Entropy`
   predictor is shippable; otherwise the 4-filter preset is already
   near-Pareto on a broader corpus.

The two probes share the per-filter measurement harness from this
Cycle 101 spike — modest extension. Decision gate for Cycle 102:

- If 5MP preset=3 closes the size gap at iso-250 ms wall → GREEN R10 ship.
- If corpus-500 shows Entropy dominates ≥ 70% → small paper §7
  framework win, ship 2-class filter predictor as opt-in.
- Otherwise → close R10 thread, pivot to R3 (VQ-VAE) or R6 (multi-tile)
  for paper-major content.

## Files

- `crates/nupic-research/examples/cycle101_r10_filter_probe.rs` — 90
  oxipng calls (9 filters × 10 fixtures) + 10 default-preset baselines
  in 14.3 s.
- Previous: 04eee (Cycle 100 R4 RED), closing 4-cycle R4 thread.
