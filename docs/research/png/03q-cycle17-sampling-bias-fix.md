# 03q — Cycle 17: var-diff sampling bias fix (v0.5.33)

## Bug

Cycle 11's `classify_for_auto_dither` tier-4 var-diff signal samples
every 4th row when `n_total > 1M`, then breaks early when sample
count exceeds 500 K. For images larger than ~4 MP, this **truncates
sampling to the top half of the image**, biasing the var-diff signal
toward whatever content appears in the upper rows.

Discovered by `cycle17_var_diff_sampling` (Cycle 17 coverage attack):

```
=== part 3: 4-MP+ adversarial ===
config                                          w x h          d
stripes: smooth(top4) + textured(bot4)      1200x6400      0.50  ← only sees smooth
stripes: textured(top4) + smooth(bot4)      1200x6400      0.70  ← only sees textured
```

Identical pixel pool, different vertical orientation → different
d-strength chosen. The classifier was effectively blind to the bottom
half of large images.

Real-world impact: 4K photo (8 MP, 4000×2000) with sky on top and
ground below would get classified by sky var-diff only, picking
d=0.5 for the smooth-sky reading, missing the textured ground that
wants d=0.7.

## Fix

Replace fixed `step=4 with break-at-500K` with
**proportional step**: target 500 K samples total, spread evenly
across the full image height.

```rust
const TARGET_SAMPLES: usize = 500_000;
let samples_per_row = (w - 1).max(1);
let target_rows = TARGET_SAMPLES.div_ceil(samples_per_row);
let step = (h / target_rows.max(1)).max(1);
for y in (0..h).step_by(step) { ... }
// no break
```

For 1200 × 6400 (7.68 MP) image:
- pre-fix: step=4, samples=500 K → only top 1666/6400 rows touched
- post-fix: step=15, samples≈500 K → all 6400 rows touched (every 15th)

## Result

`cycle17_var_diff_sampling` post-fix:

```
=== part 3: 4-MP+ adversarial — does sampling reach bottom? ===
stripes: smooth(top4) + textured(bot4)      1200x6400      0.70  ✓
stripes: textured(top4) + smooth(bot4)      1200x6400      0.70  ✓

classifier robust even at 4-MP+ scale
```

Both orientations now correctly identify the image as textured-class
(d=0.7) regardless of which half the textured content occupies.

## Output equivalence

7-fixture corpus (`--dither auto`) **bit-exact identical** to v0.5.32:

```
fixture                size     SSIM     pre→post
01-png-transparency    45364   -46.426   identical
02-pluto-transparent  162009    80.441   identical
03-wikipedia-logo      14718   100.000   identical
04-photo-portrait     499378    88.854   identical
05-photo-mountain     473174    76.818   identical
06-photo-landscape   1109644    84.936   identical
07-photo-product      404312    86.500   identical
```

All 7 fixtures ≤ 2.7 MP, fall into the `step=1` (≤ 1 MP) or
`step=4`-fully-reachable (1-4 MP) regime; pre-fix and post-fix
take the same sampling path.

## Files

- `crates/nupic-quantize/src/lib.rs` — proportional step in
  `classify_for_auto_dither`
- `crates/nupic-research/examples/cycle17_var_diff_sampling.rs` —
  3-part sanity / adversarial bench
