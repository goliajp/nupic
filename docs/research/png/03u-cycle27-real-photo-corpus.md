# 03u — Cycle 27: real-photo corpus + tier-4d high-uniq split (v0.5.39)

## Motivation

User said: "去网上再多找一些 8mp photo,看看能不能带来更多信息,
上升一个数量级"。 Cycle 26 had deferred 13-very-large-photo (single-
fixture signal). To validate / generalize, extended the corpus with
5 real public-domain photos from Wikimedia Commons / NASA.

## New real-photo corpus (`assets/png-bench/inputs-ext-real/`)

All sourced from Wikimedia Commons via `curl` from
`upload.wikimedia.org`, then converted to PNG-lossless. All public
domain or PD-equivalent:

| file | source | dims | MP |
|---|---|---|---|
| 16-earthrise-25mp.png | [NASA Apollo 8, Wikimedia](https://commons.wikimedia.org/wiki/File:NASA_Earthrise_AS08-14-2383_Apollo_8,_1968-12-24,_from_print.jpg) | 5550×4446 | 24.7 |
| 17-aurora-5mp.png | [Aurora over Eielson AFB, Wikimedia](https://commons.wikimedia.org/wiki/File:Aurora_borealis_over_Eielson_Air_Force_Base,_Alaska.jpg) | 3008×1960 | 5.9 |
| 18-snowflake-17mp.png | [Stellar dendrite snowflake, Wikimedia](https://commons.wikimedia.org/wiki/File:Stellar_dendrite_snowflake.jpg) | 5182×3446 | 17.9 |
| 19-iceberg-3mp.png | [Glacial iceberg Argentina, Wikimedia](https://commons.wikimedia.org/wiki/File:Glacial_iceberg_in_Argentina.jpg) | 1987×1490 | 3.0 |
| 20-rainbow-19mp.png | [Arc en Ciel Plage de Radès, Wikimedia](https://commons.wikimedia.org/wiki/File:Arc_en_Ciel_Plage_de_Radès.jpg) | 5258×3732 | 19.6 |

## Bench + signal probe

Per-fixture default `--dither auto` (pre-Cycle-27 / v0.5.38):

| fixture | size MB | SSIM | opq | mean_run | adj_mn | var | uniq | classify_d |
|---|---|---|---|---|---|---|---|---|
| 16 earthrise | 12.9 | 85.72 | 1.000 | 1.20 | 1.23 | 2.1 | 43133 | 0.50 |
| 17 aurora | 1.5 | **63.98** | 1.000 | 1.08 | 2.81 | 26.1 | 159268 | 0.50 |
| 18 snowflake | 4.7 | 82.65 | 1.000 | 2.57 | 2.66 | 123 | 114359 | 0.70 |
| 19 iceberg | 1.4 | 83.23 | 1.000 | 1.15 | 3.80 | 52.0 | 65242 | 0.70 |
| 20 rainbow | 7.0 | **70.23** | 1.000 | 1.19 | 2.07 | 11.1 | 164183 | 0.50 |

17 / 20 stand out with low SSIM. Per-d sweeps reveal:

```
17 aurora:    d=0.0 → 53.4, d=0.25 → 59.4, d=0.5 → 63.98, d=0.7 → 66.22 (peak)
20 rainbow:   d=0.0 → 66.7, d=0.5  → 70.2, d=0.7 → 70.95 (peak)
16 earthrise: d=0.5 → 85.72 (peak), d=0.7 → 85.48 (regress)
```

17/20 want **d=0.7**; classifier gave d=0.5 → +2.24 / +0.72 SSIM
loss respectively.

## Signal — tier-4d high-uniq smooth photo

Looking for what differentiates 17/20 from 04/16:

| fixture | want d | uniq | var | adj_mn |
|---|---|---|---|---|
| 04 portrait | 0.5 | **25K** | 34 | 3.81 |
| 16 earthrise | 0.5 | **43K** | 2.1 | 1.23 |
| 13 very-large | 0.7 | **1.2M** | 29 | 2.84 |
| 17 aurora | 0.7 | **159K** | 26 | 2.81 |
| 20 rainbow | 0.7 | **164K** | 11 | 2.07 |

**uniq count cleanly separates**: 04/16 (≤ 43K) want 0.5, 13/17/20
(≥ 159K) want 0.7. Threshold **50K** safely splits. This 3-fixture
signal (vs Cycle 26's N=1) is reliable enough to ship.

Interpretation: high-uniq + low var = "smooth large photo with rich
palette demand". Local content is smooth (low var, looks like
tier-4a) but globally the image holds 100K+ distinct colors that
the 256-palette cannot fit without banding. Strong dither (d=0.7)
combats this banding.

## Implementation

`classify_for_auto_dither` tier-4 split now:

```rust
if mean < 1.0 { return 0.7; }     // tier-4c gradient (Cycle 24)
if var > 50.0 { return 0.7; }     // tier-4b textured
// Cycle 27: count uniq with early-exit at 50001
let mut uniq = HashSet::with_capacity(50_500);
for p in src_rgba.chunks_exact(4).step_by(step_u) {
    if p[3] != 255 { continue; }
    uniq.insert(...);
    if uniq.len() > 50_000 { break; }
}
if uniq.len() > 50_000 { 0.7 }    // tier-4d high-uniq smooth photo
else { 0.5 }                       // tier-4a portrait-class
```

Cost: O(N) with early-exit at 50K unique colors.

## Result

**Original 7-fixture corpus + 7/8 extended-synth: bit-exact
identical** (all fall in existing tiers, none crosses the new
threshold).

**Changed fixtures**:

| fixture | pre-Cycle-27 | post-Cycle-27 | Δ |
|---|---|---|---|
| 13 very-large-photo | 2697707 / 66.52 | **2944777 / 68.84** | **+2.32 SSIM** / +247 KB (+9%) |
| 17 aurora | 1591324 / 63.98 | **1698253 / 66.22** | **+2.24 SSIM** / +107 KB (+7%) |
| 20 rainbow | 7346018 / 70.23 | **7733589 / 70.95** | **+0.72 SSIM** / +388 KB (+5%) |

Cumulative across 3 fixtures: **+5.28 SSIM,+742 KB**。

219 workspace tests pass.

## Files

- `crates/nupic-quantize/src/lib.rs` — tier-4d high-uniq branch
- `crates/nupic-research/examples/probe_real_corpus.rs` — signal probe
- `assets/png-bench/inputs-ext-real/*.png` — 5 new real-photo fixtures
- `assets/png-bench/inputs-ext-real/README.md` — provenance
- `docs/research/png/03u-cycle27-real-photo-corpus.md` — this essay
