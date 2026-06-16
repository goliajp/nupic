# 03m — Cycle 11: tier-4 content-aware dither split (v0.5.30)

## Problem

Cycle 8 shipped 4-tier `--dither auto` classifier with a uniform
`d=0.5` for all tier-4 (opaque, large, low mean_run) photos. Cycle 9
sweep revealed this leaves SSIMULACRA2 on the table for textured
photos:

| fixture | SSIM @ d=0.5 | SSIM @ d=0.7 | gap |
|---|---|---|---|
| 04 portrait | **88.85** | 88.72 | 0.5 wins by 0.13 |
| 05 mountain | 75.73 | **76.82** | 0.7 wins by 1.09 |
| 06 landscape | 84.53 | **84.94** | 0.7 wins by 0.41 |
| 07 product | 86.12 | **86.50** | 0.7 wins by 0.38 |

04 portrait has a different shape: smooth skin + sharp features mean
stronger dither over-textures faces. Other 3 photos have noise-dense
content (rocks, foliage, fabric) where 0.7 dither blends with
existing texture and reduces palette banding.

A content signal is needed inside tier-4 to pick `d=0.5` vs `d=0.7`.

## Signal exploration

`tier4_signal_test` (in `crates/nupic-research/examples/`) computes
3 candidate signals on the 4 photo fixtures:

| fixture | mean_adj_diff | var_adj_diff | uniq_per_row | optimal_d |
|---|---|---|---|---|
| 04 portrait | 3.81 | **34.4** | 675 | 0.5 |
| 07 product | 4.13 | **84.6** | 404 | 0.7 |
| 05 mountain | 9.44 | **320** | 828 | 0.75 |
| 06 landscape | 21.70 | **665** | 1281 | 0.7 |

`mean_adj_diff` doesn't separate cleanly (04=3.81, 07=4.13 — only
8% delta but they want different d). `uniq_per_row` is non-monotonic
(07 has lowest 404 but wants d=0.7).

`var_adj_diff` (variance of horizontally-adjacent-pixel luminance
diff) **cleanly separates 04 (var=34) from {05, 06, 07} (var ≥ 85)**.
Threshold `var > 50` works on all 4 fixtures.

Intuition: variance measures local texture density. Smooth skin =
low local variance even when global gradient exists. Textured surfaces
= high local variance from per-pixel detail. Dither helps gradient
banding; on already-textured surfaces dither blends in and the
benefit dominates over the side-effect.

## Implementation

`classify_for_auto_dither` (in `nupic-quantize`) gains a tier-4
content split:

```rust
if mean_run > 2.0 {
    return 0.25; // tier-3: UI screenshot
}
// tier-4 split
let var = adjacent_luminance_diff_variance(src_rgba, width);
if var > 50.0 { 0.7 } else { 0.5 }
```

Cost: one extra pass over pixels (or 1/4 of pixels for images
> 1M pixels via `step_by(4)`). Negligible vs imagequant +
oxipng time.

Signature change: `classify_for_auto_dither(src_rgba: &[u8])`
→ `classify_for_auto_dither(src_rgba: &[u8], width: u32)`. Only
2 internal callers, both in `nupic-quantize::quantize_indexed_png`
and `quantize_with_dither` — both have width in scope.

## Result

`cycle11_tier4_split` bench (in `crates/nupic-research/examples/`):

| fixture | d=0.5 | d=0.7 | auto (Cycle 11) | Δ vs pre-Cycle-11 (all d=0.5) |
|---|---|---|---|---|
| 04 portrait | 499378 / 88.85 | 503492 / 88.72 | 499378 / **88.85** | 0 / 0 (correctly picks 0.5) |
| 05 mountain | 454934 / 75.73 | 473174 / 76.82 | 473174 / **76.82** | +18240 / **+1.09** |
| 06 landscape | 1085170 / 84.53 | 1109644 / 84.94 | 1109644 / **84.94** | +24474 / **+0.41** |
| 07 product | 389464 / 86.12 | 404312 / 86.50 | 404312 / **86.50** | +14848 / **+0.38** |

**Totals across the 3 textured photos**: SSIMULACRA2 +1.88
total / +0.63 mean; size +57562 bytes total / **+3.4% mean**.

Per-fixture trade:
- 05 mountain: +1.09 SSIM for +4.0% size (best trade)
- 06 landscape: +0.41 SSIM for +2.3% size
- 07 product: +0.38 SSIM for +3.8% size

All positive trades. 7-fixture corpus aggregate SSIM gain
+1.88 / 7 = **+0.27 SSIM/image average**.

## Why this is shippable as a default

User-facing behavior changes only in `--dither auto` mode (the
default). Explicit `--dither <strength>` is untouched. The auto
classifier becomes content-aware within tier-4 — no quality
regression on any fixture, gain on textured photos only.

## Files

- `crates/nupic-quantize/src/lib.rs` — tier-4 var-diff split logic
- `crates/nupic-research/examples/tier4_signal_test.rs` — signal
  exploration
- `crates/nupic-research/examples/cycle11_tier4_split.rs` — corpus
  validation bench
