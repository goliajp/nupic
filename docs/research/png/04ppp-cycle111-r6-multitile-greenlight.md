# 04ppp · Cycle 111 — R6 multi-tile DSSIM ceiling break (★★★★★ paper kernel GREEN)

**Status:** **GREEN at the algorithm level**. 8×8 tile × K=192 per-tile
quantization breaks the single-global-palette DSSIM ceiling on all
**6/6** Cycle 106 DSSIM-infeasible fixtures (p115, p125, p167, p175,
p214, p274). Best DSSIM margins range -0.00072 to -0.00825 below
tiny_dssim — comfortable visual-indistinguishable headroom on every
fixture.

Encoder integration (squeezing 64 tiles × 192 colors = 12288 unique
colors into PNG's 256-palette ceiling, OR shipping a tile-aware
container) is **Cycle 112+ engineering work**. The algorithm-level
result is the paper finding.

## TL;DR

| metric | result |
|---|---:|
| Cycle 106-110 single-global-palette ceiling | 0/6 PASS DSSIM(K ∈ {64..256} × d × lossless all fail)|
| **Cycle 111 R6 8×8 K=192 reconstruction** | **6/6 PASS DSSIM(margin -0.00072 to -0.00825)** |
| Spike wall(54 jobs × 4 cores) | **9 s** ≤ workflow target ✓ |
| Winning tile-K mode | 8×8 K=192 unanimous (6/6) |

## Method

Cycle 106-110 showed 6 Pile A fixtures un-rescuable under any global
quantization config(p125, p274, p214, p115, p175, p167 — all
high-frequency Picsum HD photos). The hypothesis: per-tile
independent quantization captures distinct chromatic regions
(sky / sea / sand / vegetation / shadow) on different palettes,
collectively spanning what a single palette can't.

`crates/nupic-research/examples/cycle111_r6_multitile_probe.rs`:

1. For each fixture, split into `N×N` tile grid (`N ∈ {2,3,4,6,8}`).
2. For each tile: `imagequant::Attributes::new()` + `K ∈ {64,128,192}`
   + dither=0.3 → quantized RGBA per tile.
3. Reassemble tiles into full quantized RGBA buffer.
4. PNG-encode (single-image, lossless, via `image::ImageFormat::Png`)
   → measure `metrics::dssim(orig, reassembled)`.
5. Compare to cached `tiny_dssim`.

**Note:** This spike measures **reconstruction DSSIM only**, not
encoded size. Size is a downstream encoder problem (see Cycle 112+
section below).

## Per-fixture results

| fixture | tile_n × K winner | R6 DSSIM | tiny_DSSIM | margin |
|---|---|---:|---:|---:|
| p115_1024x768  | 8×8 × 192 | 0.000214 | 0.001970 | **-0.001756** |
| p125_1920x1080 | 8×8 × 192 | 0.001514 | 0.009766 | **-0.008252** |
| p167_1920x1080 | 8×8 × 192 | 0.000161 | 0.000880 | **-0.000719** |
| p175_1920x1080 | 8×8 × 192 | 0.000303 | 0.001966 | **-0.001663** |
| p214_2400x1600 | 8×8 × 192 | 0.001116 | 0.002845 | **-0.001729** |
| p274_3840x2560 | 8×8 × 192 | 0.001568 | 0.003084 | **-0.001516** |

Every fixture's winning config converges to 8×8 K=192. Coarser tiles
(2×2 / 3×3) at K=128 already pass 5/6 — but 8×8 K=192 is unanimous
sweet spot.

## What this proves

1. **Single-global-palette is a real ceiling, not a tuning issue.**
   Cycle 106 found 6 fixtures un-rescuable; Cycle 110 confirmed
   lossless fallback also fails. Cycle 111 R6 breaks them all.
2. **8×8 spatial decomposition is sufficient** at K=192 per tile.
   No need for variable-block-size, fractal partition, or
   content-adaptive tiling — fixed 8×8 grid works.
3. **The path to GREEN gate on these fixtures exists** if encoder
   tooling can ship tile-aware indexing.

## What stays open(Cycle 112+ engineering)

The 64 tiles × 192 colors = **12288 unique colors total**, far
exceeding PNG's 256-palette ceiling. Production ship paths:

- **A. Tile-aware container** (`.nupic` file format with per-tile
  palette table + index stream + spatial entropy coder). Big work,
  ship blocker, but most paper-faithful to R6.
- **B. R6→single-palette re-quantize hybrid**: run 8×8 K=192 to get
  reconstruction → then global K=256 imagequant on the reassembled
  RGBA. The single-pass acts as an oracle palette pre-seed, possibly
  giving K=256 a better starting point than naive global K=256
  quantize would. Cycle 112-pre would measure how much R6 advantage
  survives the re-quantize.
- **C. WebP / AVIF transcoder for R6 cohort**: skip PNG entirely on
  R6-eligible fixtures, output WebP or AVIF. These formats handle
  spatial color variation natively. Out-of-scope for PNG codec.

**Cycle 112 spike should measure path B first** — cheapest, ships
inside existing PNG, tests the actual production-realizable
improvement. If path B survives ≥ 3/6, it's a v1.2.10 ship candidate.

## Paper material

This is the third major paper finding(after Cycle 106 K-monotonicity
and Cycle 107-108 cohort routing methodology):

**"Spatial-aware quantization (8×8 tile × K=192) breaks the single-
global-palette ceiling on 100% of an externally-defined infeasible
cohort."**

Evidence:
- `assets/png-bench/cycle106-r4/pile_a_grid.tsv` — Cycle 106 oracle
  showing 6 fixtures un-rescuable under K∈{64..256} × d∈{0,0.3,0.6}
- `assets/png-bench/cycle110/full_verify_v3.tsv` — Cycle 110 same
  6 fixtures fail with `nupic compress --lossless`
- `assets/png-bench/cycle111/r6_probe_v2.tsv` — Cycle 111 R6 8×8
  K=192 passes 6/6 with comfortable margin

The three-cycle progression(ceiling diagnosed → fallback exhausted
→ paradigm shift validated)is the paper's narrative spine.

## Cycle 112 next-up

- **Path B re-quantize hybrid spike** — measure how much R6 advantage
  survives K=256 global re-quantize. If ≥ 3/6 still pass DSSIM ≤
  tiny_dssim, write production wiring for v1.2.10 ship.
- If path B fails(R6 advantage lost in re-quantize): document the
  finding as "R6 is paper-only, no ship path in single-palette PNG"
  and turn to WebP transcoder or `.nupic` container.

## Files

- `crates/nupic-research/examples/cycle111_r6_multitile_probe.rs` —
  spike
- `assets/png-bench/cycle111/r6_probe_v2.{tsv,log}` — 9 configs × 6
  fixtures = 54 jobs, wall 9 s
- `.claude/research-ledger/cycle-111-table-report.md` — table verdict
- `.claude/research-ledger/paper-material.md` — Cycle 111 R6 finding
- `.claude/research-ledger/algorithm-ideas.md` — idea E (R6 multi-tile)
  status → algorithm-feasible-GREEN, encoder-ship-still-open

## Decision

- **No v1.2.10 ship this cycle.** R6 algorithm proven viable but
  encoder integration not done. Cycle 112 path B measures whether a
  hybrid can ship without a new container format.
- **Paper material captured.** R6 ceiling-break is the third
  paper finding in the Cycle 106-110-111 arc.
