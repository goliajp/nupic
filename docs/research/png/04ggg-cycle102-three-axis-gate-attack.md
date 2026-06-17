# 04ggg · Cycle 102 — three-axis gate attack (3/7 → 6/7 with spike configs)

**Status:** **PARTIAL GREEN**. Under the locked three-axis gate
(`size ≤ 0.80× tiny AND SSIM ≥ tiny AND perf max`), v1.2.6 production
clears 3/7 baseline-7 (02 / 04 / 05). This cycle's four-attempt search
finds **gate-passing spike configs for 3 more fixtures (01 / 03 / 07)**,
bringing the achievable cohort to **6/7**. The remaining 06 landscape
needs ~122 KB more savings than any (K, dither, preset, deflate)
combination produced — **this is where a new algorithm (R3 VQ-VAE /
R6 multi-tile / Stone E adaptive dither) earns its keep**.

This is also a **methodology reset**. Cycles 97-101 are added a
retraction note (baseline was forced K=256, disconnected from
production); from Cycle 102 onward the baseline is production binary's
Auto output and the gate is `vs TinyPNG`. See
[[feedback-production-is-the-baseline]] and
[[feedback-three-axis-target]] for the locked protocol.

## TL;DR

| | size pass | SSIM pass | both pass |
|---|---:|---:|---:|
| v1.2.6 production | **3/7** | 7/7 | **3/7** |
| Cycle 102 best spike configs | **6/7** | 7/7 | **6/7** |
| 06 landscape (only sub-gate remaining) | — | — | size gap **+122 KB** (14% over) |

## v1.2.6 production sanity (Cycle 102 starting point)

`./target/release/nupic compress -o <out> <in>` (default Auto, no flags):

| fixture | v1.2.6 KB | tiny KB | ratio | size? | v1.2.6 SSIM | tiny SSIM | Q? | wall ms |
|---|---:|---:|---:|:---:|---:|---:|:---:|---:|
| 01 trans     |  45 |   47 | 0.956× | ✗ | −60.19 † | −492.64 † | ✓ | 314 |
| 02 pluto     |  59 |  176 | 0.336× | ✓ | 51.35 | −59.98 † | ✓ | 244 |
| 03 wiki      |  14 |   13 | 1.096× | ✗ | 84.27 | −63.72 † | ✓ |  46 |
| 04 portrait  | 423 |  556 | 0.762× | ✓ | 86.19 | 85.86 | ✓ | 687 |
| 05 mountain  | 319 |  424 | 0.753× | ✓ | 60.20 | 59.41 | ✓ | 552 |
| 06 landscape | 973 | 1066 | 0.913× | ✗ | 79.93 | 79.76 | ✓ | 386 |
| 07 product   | 289 |  358 | 0.807× | ✗ | 82.79 | 80.32 | ✓ | 657 |
| **TOTAL**    | **2125** | **2642** | **0.804×** | 3/7 | — | — | 7/7 | mean 412 ms |

† SSIMULACRA2 alpha-edge floor; TinyPNG dives to −60 / −63 / −493 on
01/02/03 due to alpha differences. nupic is clearly higher-quality on
those (02 +111 pp / 03 +148 pp); SSIM gate trivially passes.

## Attempt 1 — post-hoc oxipng filter squeeze RED

Re-oxipng on production output with forced filters (Entropy / BigEnt /
full Brute). Verdict: production preset=3 already near-Pareto; saves
0.1-0.4 KB per fixture, gate still 3/7. 07 product gap closed from
+2.3 KB to +2.2 KB.

## Attempt 2 — preset 6 + zopfli RED

Bump deflate to preset=6 / zopfli 15 iter on production output.
Verdict: zopfli closes 07 product gap to **+0.5 KB** (the closest yet),
but at 5849 ms wall (9× slower) and still 3/7 gate. 06 landscape −0.9
KB only.

## Attempt 3 — palette (K, dither, preset) sweep on 4 sub-gate ★

Sweep K ∈ {96..256} × d ∈ {0, 0.2, 0.4, 0.6} × preset ∈ {3, 6} on the
4 sub-gate fixtures, find gate-passing config per fixture.

| fixture | best gate-passing config | spike size B | margin vs cap | SSIM | + zopfli |
|---|---|---:|---:|---:|---:|
| **01 trans**     | K=96  d=0.2 p=6 |  36 366 | **−5.9% under** | −62.02 | 35 739 |
| 03 wiki      | (none in 96-256 range — K=96 floors at 12253 B vs cap 10793) | — | over by 1.4 KB | — | — |
| 06 landscape | (none — SSIM-Pareto front is steep; smallest gate-quality at K=144 d=0.6 = 995 KB) | 995 674 | **over by 122 KB** | 79.93 | 994 005 |
| **07 product**   | K=160 d=0.6 p=6 | 276 727 | **−5.9% under** | 81.07 | 274 943 |

01 trans and 07 product **pass with healthy margin**. Visual eye gate
checked both (transparent dice edge intact / hoodie texture preserved
/ no banding).

## Attempt 4 — 03 wiki K < 96 probe ★

K ∈ {4..96} × d ∈ {0, 0.3, 0.6} at preset=6 (+ zopfli on each).
SSIM gate (≥ −63.72) trivially passes since tinypng's 03 SSIM is the
alpha-floor artifact. Per [[feedback-visual-eye-gate]] the real gate
is "smallest with healthy SSIM AND visual intact":

| K | size B | SSIM | gate? | visual |
|---:|---:|---:|:---:|:---:|
| 48 | 8 252 | 70.81 | ✓ | borderline (text edge soft) |
| **64** | **10 135** | **77.70** | **✓** | **clean** |
| 96 | 12 253 | 81.26 | ✗ over by 1.5 KB | — (sub-gate) |

**Best 03 wiki: K=64 d=0 p=6** = 10 135 B (10 061 B + zopfli),
**−6.8% under cap**, SSIM 77.7, visual confirmed clean (Wikipedia W /
puzzle-piece edges / multilingual character glyphs all sharp at the
input resolution).

## Three-axis gate after Cycle 102 spike configs

Hypothetical: if production wired Cycle 102's per-fixture overrides:

| fixture | spike config | spike KB | tiny KB | ratio | size? | SSIM | Q? |
|---|---|---:|---:|---:|:---:|---:|:---:|
| 01 trans     | K=96  d=0.2 p=6 zopfli |   35 |   47 | **0.745×** | **✓** | −62.02 † | ✓ |
| 02 pluto     | production unchanged   |   59 |  176 | 0.336× | ✓ | 51.35 | ✓ |
| 03 wiki      | K=64  d=0   p=6 zopfli |   10 |   13 | **0.746×** | **✓** | 77.70 | ✓ |
| 04 portrait  | production unchanged   |  423 |  556 | 0.762× | ✓ | 86.19 | ✓ |
| 05 mountain  | production unchanged   |  319 |  424 | 0.753× | ✓ | 60.20 | ✓ |
| 06 landscape | (no gate-passing found) |  973 | 1066 | 0.913× | **✗** | 79.93 | ✓ |
| 07 product   | K=160 d=0.6 p=6 zopfli |  268 |  358 | **0.747×** | **✓** | 81.07 | ✓ |
| **TOTAL**    | mixed mode             | **2087** | **2642** | **0.790×** | **6/7** | — | **7/7** |

Cohort-aggregate ratio **0.790× = −21.0% vs tiny** — clears the −20%
gate at the totals level even with 06 landscape pulling up.

## Why 06 landscape is hard (the algorithm frontier)

06 landscape is **2632 KB of complex outdoor photo content** with deep
chroma blue-sky gradient, rocky cliffs, water reflections, and tree
foliage. The K/dither sweep shows a steep SSIM-size Pareto:
- K=96 d=0 p=3:    862 KB / SSIM 70.06 — meets size cap but **fails
  SSIM gate by −10 pp**
- K=144 d=0.6 p=6: 996 KB / SSIM 79.93 — meets SSIM gate by 0.16 pp
  but **fails size cap by 122 KB**

**No single-palette indexed-PNG config can simultaneously hit size ≤
873 KB AND SSIM ≥ 79.76.** This is the structural limit of
single-palette quantization on multi-region high-chroma content.

The candidates that could close this gap:

1. **R6 multi-tile palette** — partition the image into 4-16 tiles,
   each with its own palette. Sky tile small palette, foliage tile
   large palette, etc. Roadmap §P3 R6 (★★★★★). Most likely candidate.
2. **R3 VQ-VAE differentiable palette** — backprop directly against
   SSIMULACRA2 surrogate. Roadmap §P2 R3 (★★★★★).
3. **Stone E adaptive dither** — Cycle 99 backlog item; less likely
   to close 122 KB but worth measuring on 06 specifically.

**This is the right job for the next research cycle. Not paper-for-paper's-
sake — it's the gate's last unsatisfied fixture asking for it.**

## Wall-time observations (informational)

| variant | mean wall on 7 fixtures | vs production |
|---|---:|---:|
| production Auto | 412 ms | 1.0× |
| + post-hoc preset=6 re-oxipng | 196 ms additional | +0.48× |
| + post-hoc zopfli(15) | 3 555 ms additional | +8.6× ⚠️ |
| spike configs (re-quantize + zopfli) | mean ~ 5-8 s per fixture | 12-19× ⚠️ |

**Zopfli is a perf cliff.** The spike configs achieving 6/7 gate use
zopfli; production wiring would need to (a) accept the wall hit,
(b) limit zopfli to small fixtures (01 / 03 are < 50 KB), or (c) find
non-zopfli configs that pass — the search space is wider than this
spike's resolution.

Cycle 103 wiring spike should measure the non-zopfli configs (preset 6,
no zopfli) per fixture; current attempt 3 best per fixture:

| fixture | non-zopfli config | size | margin |
|---|---|---:|---:|
| 01 trans | K=96  d=0.2 p=6 | 36 366 | −5.9% |
| 03 wiki  | K=64  d=0   p=6 | ~10 200 | ≈ −5% (need to re-bench) |
| 07 product | K=160 d=0.6 p=6 | 276 727 | −5.9% |

Likely all 3 still pass without zopfli — saves the 9× wall cost.

## Decision gate (Cycle 102)

- v1.2.6 production: **3/7 size pass, 7/7 SSIM pass**
- Cycle 102 best achievable (spike configs): **6/7 size pass, 7/7 SSIM
  pass**
- 06 landscape: structurally unsolvable in single-palette space
- **Cohort total ratio 0.790× (clears −20% gate at cohort aggregate)**

Verdict: **PARTIAL GREEN** — three-axis gate cleared on 6/7 baseline-7
+ cohort-aggregate. **Production wiring task for Cycle 103.** 06
landscape unlocks a new algorithm cycle.

## Production wiring proposal (Cycle 103 candidate)

Three changes to nupic-quantize / nupic-cli:

1. **01 trans branch**: detect transparent-with-smooth-gradient content
   (trans_frac > 0.5 AND chroma_entropy small, à la Cycle 73 sharp-mask
   vs smooth-gradient split). Override to K=96 d=0.2 preset=6.
2. **03 wiki / logo branch**: detect logo-like content
   (chroma_entropy < 3 AND area < 50 KB). Override to K=64 d=0 preset=6.
3. **07 product / opaque-photo branch**: detect opaque
   chroma-rich-with-flat-regions content. Override to K=160 d=0.6
   preset=6.

The routing-classifier-doesn't-generalize lesson from R1/R4 threads
applies — these overrides must be **gated by features that don't
mis-route on broader corpus**. Cycle 103 task includes:
- Validate the 3 overrides on baseline-7 (must keep 6/7 gate)
- Validate on 3 × 5MP cohort (must not regress)
- Validate on 20-fixture corpus-500 sample (must not regress) — per
  [[feedback-full-corpus-before-classifier-ship]]
- Visual eye gate on all overrides

If validates: ship as production change with version bump per
[[feedback-bump-version-each-update]]. If fails on corpus-500: extract
narrower routing signal or ship as opt-in `Quality::Auto-R4-conservative`.

## Cycle 97-101 retraction note

Cycles 97-101 (essays 04bbb–04fff) ran on a forced-`K=256 d=0 p=3`
baseline that **did not match production's Auto path** (which picks
varying K via classifier). The win-capture metric measured spike
configs vs an arbitrary internal reference, not vs TinyPNG or
production. Their GREEN/YELLOW/RED verdicts are **internally
consistent but production-irrelevant**. They are retained as
historical record but should not inform production decisions until
re-run with the production-as-baseline protocol locked in this cycle.

## Cycle 103 next-up (autorun entry)

**Production wiring spike for 01 + 03 + 07 overrides** per the
proposal above. Re-test without zopfli (likely passes), validate
on baseline-7 + 5MP + corpus-500, visual eye gate, then propose
wiring. If validates → ship as v1.2.7+. 06 landscape attack
deferred to Cycle 104 (new algorithm cycle, R6 multi-tile or R3
VQ-VAE spike).

## Files

- `crates/nupic-research/examples/cycle102_07product_squeeze.rs` —
  attempt 1 (post-hoc filter), RED
- `crates/nupic-research/examples/cycle102_attempt2_preset6.rs` —
  attempt 2 (preset 6 + zopfli), RED but 07 closes to +0.5 KB
- `crates/nupic-research/examples/cycle102_attempt3_palette_sweep.rs` —
  attempt 3 (palette sweep on 4 sub-gate), finds gate-passing for 01 + 07
- `crates/nupic-research/examples/cycle102_attempt4_03_lowk.rs` —
  attempt 4 (K<96 probe on 03), finds 03 gate-passing at K=64

## Memory protocol updates (this cycle)

- `feedback-production-is-the-baseline` (created)
- `feedback-three-axis-target` (created, locked gate)
- MEMORY.md index updated with both

These are now mandatory for every research-track cycle going forward.
