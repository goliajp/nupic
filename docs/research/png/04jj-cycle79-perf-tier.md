# 04jj — Cycle 79: 3-tier preset + cap for 2.5× perf at 5MP (v1.2.4)

## TL;DR

Per [[feedback-perf-nas-cdn-target]] KPI is 5MP encode < 250 ms.
Pre-Cycle 79 measured 5MP encodes at 0.76-1.36 s — 3-5× over budget.
Per-stage breakdown showed oxipng dominated 55-76 % of wall time
(preset=1 lossless re-optimisation).

Cycle 79 ships a 3-tier policy for both Lloyd iter cap AND oxipng
preset, based on n_pixels:

```
n_pixels      Lloyd cap   oxipng preset    fixtures
─────────────────────────────────────────────────────
≥ 5 MP        10          0                17 aurora, 25 sofia, ...
2-5 MP        30          1                19 iceberg (3MP), p244 (?)
< 2 MP        100 (def)   3 (def)          baseline-7
```

Baseline-7 unaffected (all <2 MP). 5MP+ perf 2-3× better:

```
fixture            v1.2.3 t   v1.2.4 t   Δt%     size_KB Δ
17 aurora 5.9MP    1.36 s     0.38 s     -72 %   +210 KB (vs preset=1)
25 sofia 5.5MP     0.90 s     0.28 s     -69 %   +185 KB
27 whale 5.5MP     0.76 s     0.37 s     -51 %   +153 KB
19 iceberg 3.0MP   1.14 s     0.74 s     -35 %   +6 KB
28 orca 14MP       1.79 s     0.92 s     -49 %   +599 KB
18 snow 17MP       3.54 s     1.12 s     -68 %   +518 KB
20 rainbow 19MP    3.90 s     1.23 s     -68 %   +800 KB
16 earthrise 25MP  2.78 s     1.39 s     -50 %   +473 KB
```

5MP avg: 1.0s → 0.34s. Still 1.4× over 250ms target — the remaining
gap is structural (oxipng's filter selection + libdeflate level=3 +
chunk emit). Cycle 80+ direction: bypass oxipng for ≥ 5MP via
direct libdeflate (per Cycle 71 backlog).

## Per-stage breakdown (pre-Cycle 79, 17 aurora 5.9MP)

```
classify   10ms   0.8%
train      34ms   2.6%
lloyd     188ms  14.5%
apply     116ms   8.9%
encode     12ms   0.9%
oxipng    938ms  72.3%   ← long pole
─────────────────────
total    1300 ms
```

oxipng preset=0 vs preset=1 sweep (3 fixtures):

```
fixture            preset=0          preset=1          ratio
17 aurora          170ms / 1543KB    1416ms / 1340KB   8.3x
27 whale           146ms / 3268KB     499ms / 3091KB   3.4x
28 orca            351ms / 10335KB   1188ms / 9749KB   3.4x
```

Size cost 5-15 % for 3-8× speedup. The Pareto operating point shifts
into "perf-first" budget for high-pixel content where oxipng's
filter heuristic has diminishing returns vs deflate-only.

## Cycle 78 negative result vindicated

Cycle 78 ruled out Lloyd cap (cap=10 vs cap=100 differs ≤ 0.5 SSIM
on p244) and stride (stride=4 vs stride=16 differs ≤ 0.13 SSIM at
+47 % time) as quality levers. Cycle 79 USES the same finding in
reverse: since cap doesn't matter for quality past cap=10, set it
to cap=10 for perf budget on 5MP+. The negative finding is the
positive finding for the perf knob.

## Visual verification (Read tool, 2026-06-17)

17 aurora 5MP @ v1.2.4: aurora gradient smooth, snow clean, stars
intact, trees crisp. Visually identical to v1.2.3 output.

## Files touched

- `crates/nupic-quantize/src/lib.rs::quantize_indexed_png`
  (refine_cap 3-tier, preset_default 3-tier)
- `Cargo.toml`: 1.2.3 → **1.2.4**
- `docs/research/png/04jj-cycle79-perf-tier.md` (this essay)

## What's next (Cycle 80+)

- **Direct libdeflate ≥ 5MP**: bypass oxipng's filter exhaustive
  search. Cycle 71 backlog. Potential ~50-100 ms savings on 5MP.
- **Apply SIMD**: 5MP apply is ~120 ms = ~24 ns/px. SIMD K-best
  search (current is scalar L2 with rayon outer parallelism)
  could hit 5-8 ns/px.
- **Smarter palette init for high-uniq photos** (Cycle 77 backlog
  k-means++) — may also speed Lloyd convergence at high MP.
- **Lossy threshold**: if we accept SSIM ≥ TinyPNG only (not perfect
  Lloyd convergence), can drop more iters → faster.
