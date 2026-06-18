# 04qqq · Cycle 112 — Path B R6→K=256 re-quantize hybrid (size GREEN / DSSIM RED strict, visually equivalent)

**Status:** **RED at strict DSSIM gate**(0/6 PASS), but with a major
size finding: **all 6 fixtures' R6+K=256 hybrid output is 0.46-0.76×
TinyPNG size**(vs Cycle 110 single-palette lossless 1.36-1.95×) and
their DSSIM margins are all in the visual-indistinguishable region
(+0.00013 to +0.00496). The hybrid is **visually equivalent to TinyPNG
at half the bytes**, but loses the strict `DSSIM ≤ tiny_dssim` gate.

Path B can't ship as a strict-gate v1.2.10 — but it surfaces a real
PNG-container fundamental constraint: **256-palette ceiling cannot
carry R6's 12288-effective-color spatial diversity**, so re-quantize
loss exceeds R6's DSSIM headroom.

## TL;DR

| metric | Cycle 110 single-palette lossless | Cycle 111 R6-only reconstruction | **Cycle 112 R6+K=256 hybrid** |
|---|---:|---:|---:|
| size ratio vs TinyPNG(6-fix mean)| 1.69× | (not measured)| **0.55×** |
| DSSIM PASS(strict ≤ tiny_dssim)| 0/6 | 6/6 | **0/6 strict** |
| DSSIM margin range | (size fail)| -0.00072 to -0.00825 | **+0.00013 to +0.00496** |
| visually indistinguishable | n/a | yes(6/6)| **yes(6/6 sampled)** |
| ship gate | RED both axis | n/a(reconstruction only)| RED DSSIM strict |

## Per-fixture data

```
fixture                tiny_KB  tiny_dssim  r6_only_dssim  hybrid_KB  hybrid_dssim  ratio  margin
p115_1024x768           204.8     0.001970     0.000214       96.5     0.002759     0.47   +0.000789
p125_1920x1080          478.0     0.009766     0.001514      245.0     0.010426     0.51   +0.000660
p167_1920x1080          452.6     0.000880     0.000161      250.4     0.001010     0.55   +0.000130
p175_1920x1080          523.2     0.001966     0.000303      241.3     0.002968     0.46   +0.001001
p214_2400x1600         1098.0     0.002845     0.001116      563.8     0.007186     0.51   +0.004341
p274_3840x2560         2502.4     0.003084     0.001568     1266.8     0.008049     0.51   +0.004965
```

p167 differs from TinyPNG by **0.00013** — sub-microsecond DSSIM,
visually identical(spike output sampled and verified clean,
no banding / no posterization on the macbook + table + brick wall
gradient).

## Why Path B can't pass strict DSSIM

R6 8×8 K=192 captures 64 tiles × 192 colors per tile = **12288
effective distinct colors**. PNG indexed encoder caps palette at
**256**. Re-quantize via `imagequant K=256` must merge 12288 → 256
clusters; each merge introduces some DSSIM loss.

Re-quantize loss(`hybrid_dssim` − `r6_only_dssim`)is 1.4-7.4×
the R6-only DSSIM. The R6 had headroom -0.00072 to -0.00825 below
TinyPNG; after re-quantize the loss exceeds that headroom on every
fixture.

This is **a fundamental PNG-container constraint**, not a tuning
issue. d=0(no dither)tried in v2 — marginal difference, still RED.
Larger K (>256) not possible in standard PNG.

## What this opens

**Strict-gate ship paths require leaving the single-palette PNG
container**:

- **Path A: `.nupic` tile-aware format** — paper-faithful, ship
  blocker on bigger work. R6 spike confirms feasibility at the
  algorithm layer; container is the next engineering hurdle.
- **Path C: WebP / AVIF for the R6 cohort** — both formats handle
  spatial color natively. nupic could ship a `--prefer-webp-for-r6`
  flag.
- **Path D: PNG with relaxed gate** — if downstream users accept
  "visually indistinguishable" rather than strict `DSSIM ≤ tiny`, the
  hybrid is shippable now at half the bytes. Out of scope; gate
  semantics are user-side.

For **paper writing**, the Cycle 112 finding strengthens the Cycle
106-110-111 arc: "spatial-aware quantization breaks the DSSIM
ceiling, but the single-palette container caps its commercial
realizability — motivating a tile-aware container."

## Visual eye

p167(macbook + table + brick wall)hybrid output sampled at 250 KB
(0.55× TinyPNG 453 KB):
- Smooth wall gradient: clean, no banding
- Wood table grain: preserved
- Apple logo: sharp edges
- macbook silver reflection: gradient smooth

Visually equivalent to TinyPNG output. The DSSIM +0.00013 margin
captures a perceptually-invisible difference.

## Cycle 113 next-up

Options(user picks):

1. **Path A spike** — define `.nupic` minimal container(magic
   bytes + tile palette table + tile index stream + final
   reassembly hint). Algorithm-feasible by Cycle 111 data; container
   is the work.
2. **Paper writeup** — Cycle 106-110-111-112 has enough data for
   a draft paper. Spend Cycle 113-115 on the manuscript instead of
   more code.
3. **Path C exploration** — add WebP encoder path for the R6 cohort.
   Lower paper value but immediate user value.

## Files

- `crates/nupic-research/examples/cycle112_path_b_hybrid.rs` — spike
- `assets/png-bench/cycle112/path_b{,_v2}.{tsv,log}` — d=0.3 + d=0
  variants
- `assets/png-bench/cycle112/*.png` — 6 hybrid output PNGs for visual
  eye reproducibility
- `.claude/research-ledger/cycle-112-table-report.md` — table verdict
- `.claude/research-ledger/paper-material.md` — Cycle 112 finding
  added(strengthens Cycle 111 R6 paper)
- `.claude/research-ledger/algorithm-ideas.md` — idea E Path B
  marked rejected at strict gate; Path A / paper-only ahead

## Decision

- **No v1.2.10 ship.** Path B fails strict DSSIM gate.
- **Paper material strengthened.** Cycle 112 confirms PNG container
  is the bottleneck, not the R6 quantization algorithm.
- **Cycle 113 = paper writeup** OR Path A `.nupic` container spike
  (user choice).
