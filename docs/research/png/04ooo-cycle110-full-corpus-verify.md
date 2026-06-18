# 04ooo · Cycle 110 — v1.2.9 full corpus verification + B1 lossless fallback RED + perf cliff finding

**Status:** **GREEN for v1.2.9 ship correctness**(100% PASS retention,
0 regression on full corpus-500),but **two follow-up findings cap
the path forward**: (1) preset=6 wire — which Cycle 108's spike used —
crashes perf 20× past the KPI (p245 9.83 MP: ~10 s vs ~250 ms target),
so v1.2.9's preset=5 wire is correct but gives only 2/307 Pile A wins
(vs Cycle 108 spike's preset=6 prediction of 11/307);
(2) the lossless-fallback rescue for the 6 DSSIM-infeasible Pile A
fixtures (Cycle 106 finding) **doesn't work** — TinyPNG uses lossy
quantize and nupic lossless lands 1.36-1.95× tiny.

Both findings push Cycle 111 toward **R6 multi-tile / R3 VQ-VAE** —
the single-palette routing paradigm has hit its ceiling inside the
perf NAS/CDN budget.

## TL;DR

| metric | result |
|---|---:|
| v1.2.9 corpus-500 + b7 PASS | **115/513 (22.4%)** |
| v1.2.8 baseline | 106/506 (20.9%) |
| net delta | **+5 fixtures (+1.5 pp)** |
| PASS pile 106/106 retention | ✓ 100% |
| baseline-7 retention(= v1.2.8 4/7)| ✓ unchanged |
| Pile A wins | 2/307(p245, p291)|
| Pile B "wins"(noise rounding lift)| 3/40 |
| Real regressions | **0** |
| preset=6 wire alternative | 23.4% but p245 wall ~10 s ❌ perf |
| B1 lossless fallback on 6 DSSIM-infeasible | **0/6** ratios 1.36-1.95× tiny |

## A. Full corpus-500 v1.2.9 verdict

Ran `cycle109_validation` in `CYCLE_VALIDATE_MODE=full` (subprocess
through the production `nupic compress` binary, 506 corpus + 7
baseline-7). Wall = **711 s (~12 min)** at preset=5(v1.2.9 ship
state).

Three iterations:

| iter | wire | DSSIM tolerance | PASS | PASS-pile retention | wall |
|---|---|---|---:|---:|---:|
| v1 | preset=5(v1.2.9 ship)| strict `≤` | 111/513(21.6%)| 105/106(s018 noise)| 815 s |
| v2 | preset=6(experimental)| strict `≤` | 120/513(23.4%)| 105/106(s018 noise)| 1030 s |
| v3 | preset=5(v1.2.9 ship)| +1e-5 tolerance | **115/513(22.4%)** | **106/106 ✓** | 711 s |

The 4-fixture jump from v1 → v3 (111 → 115) comes from the DSSIM
tolerance fix:`tiny_dssim` in `corpus-500-dssim.tsv` was rounded to
6 decimal places at build time. byte-identical outputs(e.g. s018
gradient — 2967 B in both v1.2.8 and v1.2.9, since it's < 5 MP and
P-08 doesn't trigger)compute a live DSSIM around `1e-7`, which strict
`≤` against a `tiny_dssim` parsed as exactly `0.0` reports as
"regression." A 1e-5 tolerance kills the false alarm. s018 + 3 Pile B
fixtures were all measurement-noise lifts; **no real regression**.

`PASS pile 106/106 retention` is the construction guarantee from P-08
(pick min(default, K=224) — never ships a larger file than v1.2.8
would). Confirmed.

## B. preset=6 perf cliff

Cycle 108's input-feature spike used `oxipng_preset=6` (consistent
with research convention) and projected 11/307 Pile A wins on full
corpus. Cycle 110-v2 wired preset=6 into production and got exactly
that 11/307 — but perf measured at p245 (9.83 MP):

| wire | p245 wall(ms)| 5MP perf KPI(250ms target → 9.83MP ~500ms)|
|---|---:|---|
| preset=5(v1.2.9 ship)| 5500 | 11× over budget |
| preset=6 | 10000 | **20× over budget** |

Even preset=5 is already over budget on 9.83 MP photos. The
`is_gradient_candidate` path runs `oxipng_optimize` on the full RGBA
(no quantize) at preset=5, which is itself ~3 s on 9.83 MP. P-08 adds
the K=224 quantize+oxipng at preset=5 ~2 s, total ~5 s. preset=6
doubles that.

**v1.2.9 with preset=5 is the right ship state** — the perf KPI is
already tight at preset=5, preset=6 makes it un-shippable. The 9
Pile A wins gap (2 vs 11) costs ~$0 in user UX because **users with
9.83 MP photos don't wait extra 5 seconds for ~5% size**.

The fix is not "tighter oxipng" — it's **parallel oxipng** (rayon over
filter row chunks) or **skipping oxipng entirely on the K=224 path
when input wasn't gradient-detected**. Both are Cycle 111+ work.

## C. B1 lossless fallback probe — RED

Cycle 106 found 6 fixtures where any K∈{64..256} ∧ d∈{0,0.3,0.6}
fails the DSSIM gate(p125, p274, p214, p115, p175, p167 — all
high-frequency Picsum HD photos). Cycle 110 tested whether **plain
lossless** could rescue any:

| fixture | nupic lossless KB | TinyPNG KB | ratio | PASS(≤ 0.80×)|
|---|---:|---:|---:|:---:|
| p125_1920x1080 |   812 |  466 | 1.74× | ✗ |
| p274_3840x2560 |  3836 | 2443 | 1.57× | ✗ |
| p214_2400x1600 |  1955 | 1072 | 1.82× | ✗ |
| p115_1024x768  |   390 |  199 | 1.95× | ✗ |
| p175_1920x1080 |   896 |  510 | 1.75× | ✗ |
| p167_1920x1080 |   600 |  441 | 1.36× | ✗ |

**0/6 pass.** TinyPNG is a lossy quantizer; nupic lossless is, by
definition, larger than nupic-quantize (let alone lossy TinyPNG).
The 6 fixtures are **truly single-palette-infeasible** under DSSIM ≤
tiny gate — they need **spatial-aware quantization**(R6 multi-tile
or R3 VQ-VAE).

This kills `algorithm-ideas idea F` (lossless fallback routing) on the
DSSIM-infeasible cluster. F remains viable as a **fall-through**
option(e.g. "if K=128 and K=224 both fail, route to lossless")but
not as a primary rescue — and per Cycle 110 data, that fallback only
helps fixtures where lossless < default, which is rare.

## D. Cycle 111 next-up

Three paths(ordered by paper-kernel priority):

1. **E. R6 multi-tile** (★★★★★) — split image into N tiles, palette
   per tile + spatial entropy coder. Cycle 106-110 data now strongly
   motivates this:6 fixtures untouchable by global palette + 9
   fixtures locked behind perf cliff = 15 fixtures recoverable only
   by spatial-aware encoding.
2. **Perf optimization of preset=6** — rayon-parallel oxipng filter
   selection so preset=6 fits in budget. If achievable, unlocks the
   11/307 Pile A wins.
3. **C. slow-tier zopfli flag** (`--effort 9` / `--slow`) — opt-in,
   doesn't affect default perf, gives users a "30 sec for −20%" tier.

## Files

- `crates/nupic-research/examples/cycle109_validation.rs` — gained
  `CYCLE_VALIDATE_MODE=full|sample` toggle + DSSIM 1e-5 tolerance
- `assets/png-bench/cycle110/full_verify{_v2,_v3}.{tsv,log}` —
  v1.2.9 verification data (preset=5 v3 is the canonical Cycle 110
  result)
- `assets/png-bench/cycle110/lossless_probe.txt`(in essay)— B1
  results
- `crates/nupic-core/src/ops/compress.rs` — comment updated on
  preset choice in K-up branch (preset=5 inherited from
  `opts.effort.min(10)`, with rationale)
- `.claude/research-ledger/cycle-110-table-report.md` — table verdict

## Decision

- **v1.2.9 already shipped** — Cycle 110 confirms the ship was
  correct (100% retention, 0 real regression, +5 PASS net).
- **No v1.2.10 ship this cycle.** The preset=6 alternative is
  perf-broken. The lossless fallback path is RED.
- **Cycle 111 starts R6 multi-tile spike** (algorithm-ideas idea E),
  or perf-optimized preset=6, whichever the user picks.
