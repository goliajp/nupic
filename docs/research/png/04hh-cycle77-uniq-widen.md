# 04hh — Cycle 77: widen uniq detector + bump high-uniq photo to n=256 (v1.2.3)

## TL;DR

Cycle 76 real-Auto corpus probe found 34 outliers (6.7 %) below
SSIM 65, dominated by NASA-class + picsum HD photos at n=208
ceiling. Sweep confirmed n=256 lifts SSIM +3-8 on these at +5-7 %
size. Cycle 77 ships two routing changes:

1. **Widen Cycle 64's uniq threshold** 75 K → 50 K: catches outliers
   with uniq 50-75 K (n30 astronaut 68 K, n01 mars 72 K, p120 67 K,
   p124 etc).
2. **Bump `uniq > 100K && var ≤ 200` branch** from n=208 → n=256:
   catches the previous "high-uniq photo" branch (was clipping at
   n=208 for size budget) — n29 astronaut + n21 sun + Wikimedia 5K.

Baseline-7 unaffected (04 portrait uniq=25K < 50K, 06 var=663
> 150). Corpus outliers SSIM<65 drops 34 → 24 (-30 %).

## Outlier improvement table

```
fixture                 v1.2.2 SSIM   v1.2.3 SSIM   Δ        size_KB v1.2.2→v1.2.3
n29 astronaut           56.15         59.90         +3.75    314 → 321 (+2.2%)
n30 astronaut           57.06         59.86         +2.80    216 → 227 (+5.1%)
n01 mars                60.22         67.16         +6.94    296 → 314 (+6.1%)
n02 mars                62.28         70.60         +8.32    361 → 362 (+0.3%)
n21 sun                 58.91         (unchanged)   0        already n=256
p120 picsum HD          60.34         66.70         +6.36    290 → 305 (+5.2%)
p122 picsum HD          61.70         66.80         +5.10    253 → 271 (+7.1%)
p124 picsum HD          61.95         66.97         +5.02    288 → 303 (+5.2%)
p123 picsum HD          62.32         (unchanged)   0        uniq too low
p244 picsum 4K          60.86         (unchanged)   0        already gradient-routed
```

## Corpus distribution

```
                v1.2.2 (Cycle 76)      v1.2.3 (Cycle 77)
SSIM p1         57.96                  58.88        +0.92
SSIM p5         63.55                  65.39        +1.84
SSIM p10        67.43                  68.03        +0.60
SSIM p50        81.80                  81.93        +0.13
SSIM p90        100                    100          unchanged

outliers <65    34 (6.7 %)             24 (4.7 %)   -30 % count

total_out KB    470806                 475372       +4566 (+0.97 %)
```

## Routing logic changed

```rust
// Cycle 64 widened detector (was uniq > 75K, now uniq > 50K)
if adj_mn < 5.0 && var < 150.0 && uniq_count > 50_000 {
    return 256;
}

// Cycle 41 high-uniq branch (was 208, now 256)
if uniq_count > 100_000 {
    if var > 200.0 { 192 } else { 256 }  // was: { 208 }
}
```

## Visual verification (Read tool, 2026-06-17)

| fixture | size | verdict |
|---|---|---|
| n01 (alien-costume photo) | 322 KB | natural skin tones, costume color clean |
| p120 (laptop scene) | 312 KB | wood grain detail intact, hand smooth |
| baseline-7 (all 7) | unchanged | identical to v1.2.2 — branch doesn't trigger |

## Why this works

The NASA + picsum HD outliers share a content profile:
- High overall color diversity (uniq 50-100 K)
- Mid-frequency detail (adj_mn 1-5)
- Low-to-medium variance (var 5-200)
- Photo-class smoothness

Lloyd at n=208 leaves ~20 % of distinct colors un-represented in
the palette. The Cycle 71 joint anneal partially mitigates by
re-balancing palette around important regions, but the absolute
ceiling at n=208 is short. n=256 closes the gap.

The previous n=208 was a Cycle 41 size-conservative choice for the
"high-uniq photo" branch. Cycle 77 corpus probe shows this was
under-shooting — n=256 wins +3-8 SSIM at +0-7 % size, a clear
Pareto positive.

## What's next (Cycle 78+)

- **Synthetic noise** (s032-s039 ssim 57-60): truly stochastic content;
  joint anneal correctly skips. Could a content-aware
  "noise-preserving" path help? Maybe k-means++ init.
- **p244 4K still SSIM 60.86**: at n=256 currently. Check if Lloyd
  iter cap (5MP+ → 20) is starving it.
- **p123 picsum HD SSIM 62 unchanged**: uniq 48K < 50K threshold.
  Could drop further to 40K? Check baseline-7 risk.
- **Wikimedia 5K wm13 SSIM 61**: 10 MB output, already at lossy
  ceiling.

## Files touched

- `crates/nupic-quantize/src/lib.rs::classify_for_palette_size`
  (Cycle 64 uniq threshold 75K → 50K, Cycle 41 high-uniq photo
  n=208 → n=256)
- `Cargo.toml`: 1.2.2 → **1.2.3**
- `docs/research/png/04hh-cycle77-uniq-widen.md` (this essay)
