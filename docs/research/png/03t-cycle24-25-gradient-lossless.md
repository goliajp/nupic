# 03t — Cycle 24-25: gradient → lossless auto-routing (v0.5.38)

## Cycle 24 — adj_mn < 1.0 tier-4c gradient bucket

After Cycle 23's uniq-guard fixed 08 misclassification, 08 landed in
tier-4a (var_diff ≈ 0.1 < 50) → d=0.5 → SSIM 58.98 / 364 KB. But
the sweep showed d=0.7 = SSIM 68.08 / 497 KB.

Why? Smooth gradient content has very low local variance (looks
"flat") BUT suffers heavy palette banding without strong dither.
Real photos (`adj_mn ≥ 2.84`) want moderate dither; extreme-smooth
gradients (adj_mn=0.06) want strong dither.

Fix: in `classify_for_auto_dither`, before the var > 50 split, check
`adj_mn < 1.0 → return 0.7`. All 7 original photo fixtures have
adj_mn ≥ 2.84 so unaffected.

Result on 08: 58.98 → **68.08 SSIM** (+9.1).

## Cycle 25 — gradient detector → lossless routing

Cycle 24 left 08 still ~10× larger than its true ceiling. Probed
all 15 corpus fixtures lossless-vs-auto:

```
fixture                  auto(B)    SSIM     lossless(B)   winner
01-trans-demo              45364   -46.4         225727    auto
02-pluto                  163674    80.7         379876    auto
03-wiki-logo               14718   100.0          14718    tie
04-portrait               499378    88.9         880939    auto
05-mountain               473174    76.8        1348699    auto
06-landscape             1109644    84.9        2377470    auto
07-product                404312    86.5         646208    auto
08-gradient               496883    68.1          52780    LOSSLESS WINS (size + SSIM)
09-ui-checker               2805   100.0           2805    tie
10-comic-flat               3039   100.0           3039    tie
11-photo-noisy            674431    81.5        2222415    auto
12-tiny-icon                 302   100.0            302    tie
13-very-large            2697707    66.5        6224599    auto
14-soft-trans             148361    66.9         562822    auto
15-mono-text                2882   100.0           2882    tie
```

**Only 08 has lossless dominating** on both axes. The pattern:
extreme-smooth + many distinct colors = gradient.

Lossless path on 08:
- 256-palette quantize CANNOT represent 117K distinct colors in a
  3.84 MP gradient — banding error catastrophic.
- Raw RGBA8 + libdeflate = high spatial redundancy, compresses to
  53 KB (12% of source) with bit-exact reconstruction.

## Implementation

New `nupic-quantize::is_gradient_candidate(rgba, w) -> bool`:
- Cheap O(N) scan: opaque-ratio ≥ 0.95
- Adjacent-pixel luminance mean < 1.0 (extreme-smooth)
- Unique RGB color count ≥ 1000 (gradient, not flat block)
- Early-exits at uniq 1000 → ≤ 0.5 ms for big images

In `encode_png_stone_c`, route to `encode_png_lossless` when
`is_gradient_candidate` returns true. No quantize, no dither —
just oxipng on raw RGBA8.

## Result (v0.5.38, full 15-fixture corpus, `--dither auto`)

Original 7-fixture: **bit-exact identical** (gradient detector
trivially false on all 7).

Extended 8-fixture: **only 08 changes** — and dramatically:

| fixture | pre-Cycle-23 | post-Cycle-25 | Δ |
|---|---|---|---|
| **08 gradient-large** | **190 KB / SSIM 37.72** | **53 KB / SSIM 100** | **-72% size AND +62 SSIM** |
| 09-15 | (unchanged) | (unchanged) | 0 |

## Cumulative 08 journey

  pre-Cycle-23 :  190 KB / 37.72  (tier-3 misclassified)
  Cycle 23     :  364 KB / 58.98  (uniq guard → tier-4a d=0.5)
  Cycle 24     :  497 KB / 68.08  (adj_mn < 1.0 → tier-4c d=0.7)
  Cycle 25     :   53 KB / 100.00 (gradient detected → lossless)

The progression demonstrates that ceiling discovery is corpus-bound.
The original 7 fixtures had no gradient-class content; the
extended corpus exposed it. Once exposed, all three cycles built on
the same signal stack.

## 全 corpus 现状 (v0.5.38, `--dither auto`)

```
fixture                  size(B)    SSIM         class
01-trans-demo              45364   -46.43         tier-1
02-pluto                  163674    80.73         tier-2
03-wiki-logo               14718   100.00         tier-1 (small)
04-portrait               499378    88.85         tier-4a
05-mountain               473174    76.82         tier-4b
06-landscape             1109644    84.94         tier-4b
07-product                404312    86.50         tier-4b
08-gradient                52780   100.00         tier-G (gradient → lossless)
09-ui-checker               2805   100.00         tier-3
10-comic-flat               3039   100.00         tier-3
11-photo-noisy            674431    81.47         tier-4b
12-tiny-icon                 302   100.00         tier-1 (small)
13-very-large            2697707    66.52         tier-4a (large smooth photo)
14-soft-trans             148361    66.90         tier-1 (transparency-dominant)
15-mono-text                2882   100.00         tier-3
```

15 fixture total: 5.4 MB. mean SSIM (excluding -46.4 outlier and
100s): 78.1.

## Files

- `crates/nupic-quantize/src/lib.rs` — adj_mn < 1.0 branch + new
  `is_gradient_candidate`
- `crates/nupic-core/src/ops/compress.rs` — gradient routing in
  `encode_png_stone_c`
- `docs/research/png/03t-cycle24-25-gradient-lossless.md` — this essay
