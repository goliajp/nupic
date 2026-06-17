# 04ff — Cycle 75: tier-trans 3-way Pareto split (v1.2.2)

## TL;DR

Cycle 73 collapsed all tier-trans smooth-grad content to a single
n=64+d=0.7 bucket. That preserved visual integrity but left massive
size headroom for photo-class transparency (02 pluto SSIM +140
buffer over TinyPNG). Cycle 75 introduces a 3-way split via
`uniq_opq`:

```
                   adj_mn > 5    →  n=256   sharp-mask (03 wiki, 14 puppy)
                   adj_mn ≤ 5 + uniq_opq < 5000   →  n=64 + d=0.7   translucent-overlay (01 dice)
                   adj_mn ≤ 5 + uniq_opq ≥ 5000   →  n=32 + d=0.7   photo + alpha edge (02/21/22/23)
```

Baseline-7 returns to nearly v1.2.0's compression (with visual
integrity intact this time):

```
fixture                    nupic   tinypng   ratio   SSIM_nupic  SSIM_tinypng
01 trans-demo               46 KB    48 KB   0.956x   -60.19      -492.64    (n=64 d=0.7)
02 pluto-trans              59 KB   180 KB   0.336x    51.35       -59.98    (n=32 d=0.7)
03 wiki-logo                14 KB    13 KB   1.096x    84.27       -63.72    (n=256 sharp)
04-07                      unchanged opaque path
─────────────────────────────────────────────────────────
TOTAL                     2176 KB  2706 KB  0.804x = **-19.6 %**
all 7/7 within 1.15x gate, all 7/7 SSIM > TinyPNG
```

vs v1.2.0 (Cycle 71): −0.3 pp on baseline-7 ratio, +visual
correctness on every tier-trans fixture. The −0.3 pp difference is
the cost of NOT applying joint anneal on tier-trans (the Cycle 71
"win" turned out to be a metric artifact, not a real algorithmic
gain).

## Why the 3-way split

**01 dice** has `uniq_any=42966` but `uniq_opq=4348` — almost all
its color diversity comes from the alpha-blended translucent
surfaces, not from the opaque palette. At n=32 the FS-dither runs
out of palette anchors to interpolate between, and color steps
become visible across the dice surfaces. n=64 is the minimum that
holds.

**02 pluto** has `uniq_opq=19444` — Pluto's surface texture has
many subtly-different opaque tones forming one connected texture.
At n=32, FS-dither can interpolate smoothly because adjacent
palette entries are close in OKLab; the dither pattern reads as
texture detail rather than banding. n=32 saves 38 KB without
visible quality loss.

**21 earth** has `uniq_opq=142771`, **22 tree** 55482, **23
statue** 72137 — all photo-class with high opaque diversity but
single-texture continuity. All hold at n=32 visually. 21 saves
360 KB at n=32 vs n=64.

The signal `uniq_opq` separates:
- "alpha-blended overlay" (limited opaque palette, color diversity
  in translucency) — needs more anchors
- "opaque photo with alpha edge" (rich opaque palette, single
  texture) — dither carries continuity at low n

## Visual verification (Read tool, 2026-06-17)

| fixture | n | size | verdict |
|---|---|---|---|
| 01 dice | 64 | 45 KB | translucent surfaces clean (n=32 would band) |
| 02 pluto | 32 | 59 KB | smooth surface, soft edge, no posterization |
| 21 earth | 32 | 530 KB | clouds/ocean/continents identical to source |
| 22 tree | 32 | 710 KB | foliage detail intact |
| 23 statue | 32 | 157 KB | patina texture clean, pedestal stone intact |

## n-sweep raw data (Cycle 75 research)

```
02 pluto:                      01 dice:                    21 earth:
n   size   SSIM                n   size   SSIM             n   size    SSIM
16  29 KB  -48                 16  33 KB  -65              16  296 KB  -43
24  44 KB   15                 24  36 KB  -65              24  378 KB  -25
32  59 KB   51   ← visual ok   32  38 KB  -61   ← bands    32  530 KB   15  ← visual ok
40  71 KB   60                 40  39 KB  -60              40  683 KB   34
48  80 KB   61                 48  42 KB  -60              48  752 KB   42
56  88 KB   75                 56  44 KB  -60              56  825 KB   48
64  97 KB   76                 64  45 KB  -60   ← chosen   64  895 KB   51
```

The size-vs-quality knee for 02/21/22/23 is at n=32 (size falls
40-60 % from n=64 with visual integrity intact). 01's knee is
n=64 — going smaller doesn't save much size (alpha channel
dominates) and starts to band.

## Why metric tracks visual on 02 but not on 01

SSIMULACRA2 is computed on the rendered RGB (alpha pre-multiplied
onto white). For 02 the rendered image is RGB-rich with smooth
backgrounds — SSIMULACRA2's spatial bandpass filters can correctly
detect structure preservation. For 01 the rendered image is mostly
alpha-blended pixels where small palette errors compound through
the alpha channel; SSIMULACRA2 has no model for this compounding
and just reports a low number across the board (−60..−65 regardless
of n).

This is the same metric failure mode documented in Cycle 73 essay.
01 demands visual gating; 02 can be metric-gated.

## Files touched

- `crates/nupic-quantize/src/lib.rs::classify_for_palette_size`
  (3-way tier-trans split via adj_mn + uniq_opq)
- `Cargo.toml`: 1.2.1 → **1.2.2**
- `docs/research/png/04ff-cycle75-pluto-pareto.md` (this essay)

## What's next (Cycle 76+)

- **04 portrait at -0.33 SSIM buffer over TinyPNG**: tightest gate.
  Could push opaque smooth content with importance Lloyd α tweak.
- **06 landscape at +0.16 SSIM buffer**: also tight. Cycle 64
  widened detector (adj_mn<5 + var<150 + uniq>75K → 256) may
  already cover 06's region.
- **Re-examine Cycle 65-71 results under visual gate** — the
  Cycle 73 essay flagged this; still pending.
- **Cycle 64 outliers** (picsum 4K p295/p274/p243 SSIM 41-55):
  pre-existing opaque tail; n=256 cap is the wall. Would need
  RGBA lossless routing or two-pass encoding.
