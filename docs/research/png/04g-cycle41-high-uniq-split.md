# 04g — Cycle 41: high-uniq branch var-split (v0.5.52)

## Motivation

506-corpus bench (corpus-500/ + Wikimedia + NASA + synthetic) revealed:

- median SSIMULACRA2: 82.0
- 90 fixtures (17.8 %) route through `high-uniq` (n=192) — median SSIM
  **71.0**, p10 **61.8**, min **54.0** — worst-performing branch
- Many high-uniq outliers (NASA / Wikimedia high-resolution photos)
  have moderate var (35–180) not the 320+ that "truly stochastic"
  content (05 mountain) carries.

Cycle 39 assumed all `uniq > 100K` content was stochastic enough to
hide palette noise. The 500-corpus refutes this — high-uniq +
moderate-var = high-detail photo where palette gradient quality
matters.

## Sweep — n=192 vs n=208 on outliers

```
fixture              n=192            n=208            Δ size   Δ SSIM
p259 (3840×2560)     2068 KB / 53.98  2101 KB / 57.18  +1.6 %   +3.20
n29 (astronaut)       312 KB / 54.36   321 KB / 56.30  +2.9 %   +1.94
p199 (2400×1600)      948 KB / 58.79   967 KB / 63.35  +2.0 %   +4.56
n04 (mars)           [smooth branch — not in scope]
05 mountain (TRUTH)   341 KB / 65.33   354 KB / 67.77  +3.8 %   +2.44
```

05 mountain is the canonical stochastic photo (adj_mn=9.44, var=320).
Bumping it to n=208 costs +13 KB on the marketing baseline → ratio
drops to -15.78 % (still over the -15 % gate but with thinner buffer).

## Fix — split high-uniq by var

```rust
if uniq_count > 100_000 {
    // Cycle 41: split by content variance.
    if var > 200.0 { 192 } else { 208 }
} else {
    208
}
```

`var > 200` separates 05 mountain (320) and similar stochastic content
from the high-detail-photo outliers (var 35–180).

## Verification

```
7-baseline:
  fixture          size      SSIM
  01 trans-demo     27 KB    -63.72
  02 pluto-trans    84 KB     73.13
  03 wiki-logo       9 KB     77.70
  04 portrait      450 KB     86.07
  05 mountain      340 KB     65.33  ← stays n=192 (var=320)
  06 landscape     973 KB     79.93
  07 product       324 KB     84.07
  TOTAL: 2213 KB / TinyPNG 2643 KB  =  -16.28 %  ✓ gate hit unchanged

corpus outliers (now n=208 branch):
  p259  53.98 → 57.18  (+3.20)
  n29   54.36 → 56.30  (+1.94)
  p199  58.79 → 63.35  (+4.56)
```

Marketing baseline ratio unchanged at -16.28 %. ~70-80 corpus high-uniq
fixtures get bumped, each ~+2 % size for ~+3-5 SSIM.

## Limitations / open backlog

Outliers in the **smooth** branch (n=208) remain unfixed:
- `n04_mars` (var=110, uniq=83K) — moderate var + low uniq → smooth
  branch → SSIM 55.88 unchanged
- `p120_1920x1080`, `n01_mars` etc — similar profile

For these, a finer split inside the smooth branch would need new
signals (e.g. local-contrast histogram) to discriminate "high-detail
smooth" from "casual portrait". Cycle 42 candidate.

## Files touched

- `crates/nupic-quantize/src/lib.rs`
  `classify_for_palette_size`: high-uniq branch now splits on var
- `Cargo.toml` workspace version 0.5.51 → 0.5.52
