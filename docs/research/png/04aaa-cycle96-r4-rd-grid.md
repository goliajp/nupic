# 04aaa · Cycle 96 — R4 rate-distortion grid YELLOW (paper §5 framework)

**Status:** YELLOW headline (mean -3.0% / median -0.3% iso-SSIM size vs default), but
contains **strong per-fixture wins on UI/transparency content** (02 pluto -8.8%, 03 wiki
-11.0%) and **a counterintuitive quality win** on portrait (04 with dither=0.5 gives
+0.02 SSIM at -0.3% size). The per-fixture Pareto fronts give paper §5 a "framework
paper" headline data block.

**The bottleneck flips:** R-D config wins are content-class specific (UI/logo wants
small K + dither; portrait wants K=256 + light dither; smooth photos want K=256 + no
dither). The R1 routing thread closed at Cycle 95; a different but easier routing
problem (K + dither selection) opens here. Cycle 97 spike candidate.

## TL;DR

| metric | value |
|---|---:|
| grid size | 3 K × 3 dither × 3 preset = 27 configs / fixture |
| fixtures | baseline-7 (01-07) |
| total encodes | 189 |
| total wall time | 84.9 s (~0.45 s per config including SSIM subprocess) |
| **mean iso-SSIM size** | **−3.03%** |
| **median iso-SSIM size** | **−0.26%** |
| Pareto front size per fixture | 3-8 configs |
| fixtures where default is on the front | 3/7 (05, 06, 01 — partially) |

**Iso-SSIM band:** ΔSSIM ≥ −0.5 from default's SSIM. "Best" = smallest size in the
allowed band.

## Per-fixture iso-SSIM wins

| fixture | default config | default SSIM | default B | best config | best SSIM | best B | Δsize% |
|---|---|---:|---:|---|---:|---:|---:|
| 01 trans | K256 d0.0 p3 | −36.45 | 66 504 | (default on front) | −36.45 | 66 504 | 0.00% |
| **02 pluto** | K256 d0.0 p3 | 80.16 | 170 251 | **K192 d0.5 p3** | 79.67 | 155 277 | **−8.80%** |
| **03 wiki** | K256 d0.0 p3 | 84.27 | 14 781 | **K128 d0.0 p3** | 83.94 | 13 158 | **−10.98%** |
| **04 portrait** | K256 d0.0 p3 | 88.07 | 461 070 | **K256 d0.5 p3** | **88.09** | 459 862 | −0.26% (quality also up!) |
| 05 mountain | K256 d0.0 p3 | 70.37 | 390 764 | (default on front) | 70.37 | 390 764 | 0.00% |
| 06 landscape | K256 d0.0 p3 | 83.07 | 1 037 914 | (default on front) | 83.07 | 1 037 914 | 0.00% |
| 07 product | K256 d0.0 p3 | 83.92 | 311 563 | K256 d0.3 p3 | 83.81 | 307 921 | −1.17% |

## Pareto front analysis (per fixture, sorted by SSIM ascending)

### 01 trans (negative SSIM = SSIMULACRA2 lossless-floor artifact)
| K | dither | preset | size | SSIM |
|---:|---:|---:|---:|---:|
| 128 | 0.3 | 3 | 43 899 | −48.78 |
| 128 | 0.5 | 3 | 50 060 | −40.97 |
| 256 | 0.5 | 3 | 57 854 | −39.55 |
| **256** | **0.0** | **3** | **66 504** | **−36.45 ← default** |

01's "default on Pareto" status is fragile — the −0.5 band catches it because the
default already has the best SSIM. Steeper SSIM tolerance flips this to wins.

### 02 pluto (transparent, chroma-rich) — clearest win
| K | dither | preset | size | SSIM |
|---:|---:|---:|---:|---:|
| 128 | 0.3 | 3 | 133 955 | 78.33 |
| 128 | 0.5 | 3 | 137 040 | 78.68 |
| 192 | 0.3 | 3 | 152 038 | 79.44 |
| **192** | **0.5** | **3** | **155 277** | **79.67 ← iso-SSIM win** |
| 256 | 0.3 | 3 | 163 531 | 80.24 |
| 256 | 0.5 | 3 | 167 290 | 80.54 |

02 pluto's chroma-rich content benefits from light dither — and K=192 with dither
=0.5 gives **only -0.5 SSIM vs default but -8.8% size**. Default K=256 d=0 is over-
provisioning the palette.

### 03 wiki (logo, UI) — biggest %-win
| K | dither | preset | size | SSIM |
|---:|---:|---:|---:|---:|
| **128** | **0.0** | **3** | **13 158** | **83.94 ← iso win** |
| 192 | 0.0 | 3 | 14 781 | 84.27 |
| **256** | **0.0** | **3** | **14 781** | **84.27 ← default** |

K=192 already saturates 03 wiki (same as K=256). K=128 loses 0.3 SSIM but saves 11%.

### 04 portrait — counterintuitive: dither HELPS quality
| K | dither | preset | size | SSIM |
|---:|---:|---:|---:|---:|
| 128 | 0.5 | 3 | 376 451 | 83.81 |
| 192 | 0.3 | 3 | 423 745 | 84.90 |
| 192 | 0.5 | 3 | 423 917 | 84.94 |
| **256** | **0.5** | **3** | **459 862** | **88.09 ← iso win (Δ=+0.02!) ** |
| 256 | 0.3 | 3 | 459 984 | 88.14 |

Both d=0.3 and d=0.5 at K=256 beat **the default** (K=256 d=0) on **both** size and
SSIM — the default is **off the Pareto front for 04 portrait**. Production's
"dither_strength=0.0 default" is leaving free quality on the table for photo
content.

### 05 mountain — default IS optimal
| K | dither | preset | size | SSIM |
|---:|---:|---:|---:|---:|
| 128 | 0.0 | 3 | 313 379 | 56.90 |
| 192 | 0.0 | 3 | 352 421 | 65.33 |
| **256** | **0.0** | **3** | **390 764** | **70.37 ← default, on front** |
| 192 | 0.5 | 3 | 419 214 | 72.50 |
| 256 | 0.3 | 3 | 434 507 | 73.78 |
| 256 | 0.5 | 3 | 453 735 | 75.72 |

Stochastic noise content gives up huge SSIM to smaller K. Dither pushes SSIM up but
costs > 0.5 SSIM band-wise — out of iso range. **Default is correctly placed.**

### 06 landscape — same as 05 + dither helps
Default on front in iso band. K=256 d=0.5 gives +1.94 SSIM at +4.7% size — outside
iso band but a Pareto-optimal "+quality" config.

### 07 product — moderate win
| K | dither | preset | size | SSIM |
|---:|---:|---:|---:|---:|
| 128 | 0.5 | 3 | 265 845 | 78.29 |
| 192 | 0.5 | 3 | 289 421 | 82.13 |
| 192 | 0.0 | 3 | 291 513 | 82.33 |
| **256** | **0.3** | **3** | **307 921** | **83.81 ← iso win** |
| **256** | **0.0** | **3** | **311 563** | **83.92 ← default** |

Light dither (d=0.3) at K=256 gives -1.17% size for -0.11 SSIM — modest.

## What patterns emerge (paper §5 content)

1. **UI/logo content (03 wiki) wants smaller K.** K=192 is iso to K=256; K=128 loses
   0.3 SSIM but saves 11%. **Routing on the "chroma_entropy < 3.0" or "n_unique_colors
   classifier" axis already established in production could pick this up.**

2. **Chroma-rich content (02 pluto, 04 portrait) wants light dither.** dither=0.3 or
   0.5 at K=192-256 sits on the Pareto front. The production "dither_strength=0.0
   default" is conservative — Stone E roadmap already flagged this; R4 sweep confirms
   it across multiple fixtures.

3. **Stochastic noise content (05 mountain, 06 landscape) wants K=256 d=0.** Default
   is correctly placed; no R-D move available.

4. **Preset doesn't matter at iso-SSIM.** All 7 fixtures' Pareto fronts use preset=3
   (the production default for < 2 MP). Preset 0/1 saves wall time but loses 1-4%
   size at same SSIM — fine for 5MP+ NAS/CDN KPI, not for baseline-7. The R-D
   sweep confirms preset=3 is correct for baseline-7.

## Decision gate

- median Pareto-front −%size at iso-SSIM = **−0.26%** (gate ≥ 3% for GREEN)
- mean = **−3.03%** (right at the gate)
- **3/7 fixtures (02, 03, 04) show ≥1% win, two of them substantial** (-8.8%, -11.0%)

**Verdict: YELLOW** by median, but per-fixture distribution is **bimodal** — wins or
zeros. This is exactly the pattern that justifies content-aware routing (Cycle 97 spike
candidate).

## Production implication

The current production default `K=256, dither=0, preset=auto-tier` is **on the Pareto
front for 4/7 baseline-7 fixtures** (01, 05, 06, plus implicit 03 since K=192=256). For
2/7 it leaves 9-11% size on the table (02, 03). For 1/7 it leaves both quality AND size
on the table (04 portrait dither=0.5).

**Cycle 97 spike candidate:** simple content router on (chroma_entropy, n_unique_colors,
trans_frac) — predicting which preset of (K, dither) wins per fixture. This is a 3-class
or 4-class problem, **likely easier than R1 binary routing** because (a) the classes are
visually obvious from features and (b) the misrouting cost is bounded (worst case = small
size/quality loss, never the −3 SSIM of R1 misroute).

If Cycle 97 succeeds at routing, ship as `Quality::Auto-R4` mode — paper §5 framework
result with concrete deployment.

## Files

- `crates/nupic-research/examples/cycle96_r4_rd_grid.rs` — full driver. 189 encodes +
  189 SSIM subprocess calls in 85 s.
- Previous: 04tt-04zz (Cycle 90-95 R1 thread).

## Cycle 97 next-up (autorun entry)

Spike a 3-class hand router on (chroma_entropy, edge_density, smoothness):
- **UI / logo class:** chroma_entropy < 3.0 AND edge_density > 0.2 → route K=128 d=0
- **Chroma-rich class:** trans_frac > 0 OR (chroma > 0.04 AND smoothness < 0.05) →
  route K=256 d=0.5
- **Stochastic noise class** (default): all others → K=256 d=0 (production current)

Validate against the 7 baseline-7 fixtures (use Cycle 96 grid as oracle). Extend to
5MP cohort (17 aurora, 25 sofia, 27 whale) with a smaller grid (K × dither, fix
preset to 5MP-tier 0 or 1) — ~50 encodes, 1-2 min wall.

Decision gate: route-picked config achieves ≥80% of full-grid Pareto-optimal iso-SSIM
size win on ≥6/10 fixtures → GREEN, candidate for `Quality::Auto-R4` ship.
