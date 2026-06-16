# 03v — Cycle 28-29: tier-1c / tier-2c sharp-mask transparency (v0.5.40)

## Motivation

After Cycle 27's `inputs-ext-real/` fetched 5 real photos and found
2 ceilings (17 aurora, 20 rainbow), continued the corpus expansion
with 3 partial-transparent fixtures from Wikimedia Commons:

- 21 earth-hemisphere-trans (2048×2048, 4.19 MP, opq=0.596)
- 22 tree-trans (2079×3249, 6.75 MP, opq=0.302)
- 23 statue-liberty-trans (1464×2022, 2.96 MP, opq=0.164)

Bench revealed all 3 had SSIM 65-80 with default `--dither auto`.

## Signal — a_partial ratio splits "sharp mask" from "smooth gradient"

Looking at alpha distributions:

| fixture | opq | a0 (transparent) | a_partial | currently | want d |
|---|---|---|---|---|---|
| 01 trans-demo | 0.036 | 0.673 | **0.291** | 0 | 0.5 (+11.5 SSIM, +27% size) |
| 02 pluto | 0.781 | 0.211 | **0.008** | 0.35 | 0.5 (+0.14 SSIM, +1.5% size) |
| 14 soft-trans | 0.009 | 0.000 | **0.991** | 0 | 0.5 (+3.1 SSIM, +33% size) |
| 21 earth-hemi | 0.596 | 0.357 | **0.046** | 0.35 | 0.5 (+0.50 SSIM, +2.6% size) |
| 22 tree | 0.302 | 0.646 | **0.052** | 0 | 0.5 (+1.54 SSIM, +2.9% size) |
| 23 statue | 0.164 | 0.835 | **0.001** | 0 | 0.5 (+0.30 SSIM, +0.5% size) |

**Two patterns emerge**:

1. **a_partial < 0.10 = sharp-mask transparency** (object on
   transparent BG, few partial-alpha edge pixels):
   - 22 (0.052), 23 (0.001), 21 (0.046), 02 (0.008)
   - **dither cheap**: 0.5-2.9% size cost for +0.14 to +1.54 SSIM

2. **a_partial > 0.10 = smooth-gradient transparency** (alpha
   smoothly varies across image):
   - 01 (0.291), 14 (0.991)
   - **dither expensive**: 27-33% size cost for SSIM gain
   - Mission "又小又好" tilts toward "small" — keep d=0

Threshold 0.10 cleanly separates the two regimes.

## Implementation

`classify_for_auto_dither`:

```rust
// tier-1 (opaque_ratio < 0.50): split sharp-mask vs smooth-gradient
if opaque_ratio < 0.50 {
    let n_partial = n_total - n_opaque - n_zero_alpha;
    if (n_partial as f64 / n_total as f64) < 0.10 {
        return 0.5; // tier-1c: sharp-mask
    }
    return 0.0;     // tier-1: smooth-gradient or mixed
}

// tier-2 (0.50 ≤ opaque_ratio < 0.95): same split
if opaque_ratio < 0.95 {
    let n_partial = n_total - n_opaque - n_zero_alpha;
    if (n_partial as f64 / n_total as f64) < 0.10 {
        return 0.5; // tier-2c: sharp-mask
    }
    return 0.35;    // tier-2: smooth-gradient (Cycle 20 default)
}
```

Pass through n_zero_alpha alongside n_opaque in the initial scan
(one extra `if px[3] == 0` branch per pixel).

## Result

Full 23-fixture corpus `--dither auto`:

| fixture | pre-Cycle-28 | post-Cycle-29 | Δ |
|---|---|---|---|
| 02 pluto | 163674 / 80.73 | **166057 / 80.87** | **+0.14** / +2.4 KB |
| 21 earth-hemi | 1385530 / 65.92 | **1421648 / 66.42** | **+0.50** / +36 KB |
| 22 tree | 1482347 / 65.19 | **1524724 / 66.73** | **+1.54** / +42 KB |
| 23 statue | 326995 / 80.33 | **328773 / 80.63** | **+0.30** / +1.8 KB |

**Cumulative: +2.48 SSIM across 4 fixtures, +82 KB**

01 / 12 / 14 (a_partial > 0.10 or n_total < 200K) bit-exact
unchanged. All non-transparent fixtures bit-exact unchanged.

219 workspace tests pass.

## Generalization confidence

- tier-1c: 22 + 23 confirm (N=2 same direction, but with 12 / 14 / 01
  as counter-examples that correctly stay at d=0)
- tier-2c: 02 + 21 confirm (N=2 same direction, no counter-examples
  in tier-2 corpus yet)

Future cycles should monitor for hypothetical "tier-2 smooth-gradient"
fixtures that would prefer d=0.35; if found, the threshold may
need tuning. Until then, this 4-fixture validation is reliable.

## Files

- `crates/nupic-quantize/src/lib.rs` — tier-1c + tier-2c branches
- `crates/nupic-research/examples/probe_trans_corpus.rs` — signal probe
- `crates/nupic-research/examples/probe_02_partial.rs` — 02 alpha dist
- `assets/png-bench/inputs-ext-real/21-earth-hemisphere-trans.png`
- `assets/png-bench/inputs-ext-real/22-tree-trans.png`
- `assets/png-bench/inputs-ext-real/23-statue-liberty-trans.png`
- `docs/research/png/03v-cycle28-29-trans-sharpmask.md` — this essay
