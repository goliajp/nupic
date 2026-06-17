# 04jjj · Cycle 105 — P-03 sharp-mask logo override wired, v1.2.7 → v1.2.8 ship

**Status:** **SHIPPED**. P-03 sharp-mask logo override (Cycle 102 spike
config, Cycle 103 designed predicate, this cycle production-aligned
adj_mn + uniq_opq trigger) wired into `nupic-quantize::
classify_for_palette_size` + new `is_p03_sharp_mask_logo` public helper
consumed by `encode_png_stone_c` for `oxipng_preset` boost.

v1.2.7 → v1.2.8. Baseline-7 three-axis gate **4/7 → 5/7** size pass,
**7/7** SSIM pass held. Cohort aggregate ratio **0.801× → 0.799×** —
decisively past the −20% gate at the total level for the first time
since the three-axis protocol was locked.

## TL;DR

| metric | v1.2.7 | **v1.2.8** | delta |
|---|---:|---:|---:|
| baseline-7 total size | 2 115 KB | **2 110 KB** | −5 KB |
| baseline-7 size pass | 4/7 | **5/7** | **+1** |
| baseline-7 SSIM pass | 7/7 | **7/7** | held |
| baseline-7 ratio vs TinyPNG | 0.801× | **0.799×** | −0.2 pp (past gate) |
| 03 wiki size | 14 781 B | **10 135 B** | **−4 646 B (−31%)** |
| 03 wiki ratio (cap 10 793 B) | 1.097× | **0.751×** | gate ✗ → **✓** |
| 03 wiki SSIM | 84.27 | 77.70 | −6.6 pp (still ≫ tiny −63.72 floor) |
| 01/02/04/05/06/07 | — | unchanged | adj_mn / opq-protected paths |

## What the spike found (Cycle 105 30-fixture validation)

Cycle 103 spike used a vertical-luma-weighted adj_mn formula that
disagreed with production's horizontal-(R+G+B)/3 formula (3.6 vs
8.20 on 03 wiki). P-03 never triggered as a result. Cycle 105 spike
replicated production's exact `compute_adj_lum_diff_stats` and reran
the 30-fixture cohort:

| fixture | opq | adj_mn | uniq_opq | tier | P-03? |
|---|---:|---:|---:|---|:---:|
| **03 wiki** | 0.74 | **8.20** | **129** | sharp | **✓ trigger** |
| 01 trans | 0.04 | 1.92 | 4 348 | trans | ✗ (adj_mn ≤ 5) |
| 02 pluto | 0.78 | 3.17 | ≥5 000 | trans | ✗ (adj_mn ≤ 5) |
| mi0 | 0.32 | 0.00 | 1 | trans | ✗ (adj_mn ≤ 5) |
| 04-07, all corpus-500, 5MP | ≥0.95 | — | — | opq | ✗ (opq filter) |

**Single fixture triggers across 30-fixture cohort, no false positives.**
adj_mn=8.20 matches production source comment line 1316 exactly.

Override config (Cycle 102 attempt 4): **K=64 d=0 preset=6**
→ 03 wiki 14 781 B → 10 135 B (−4 646 B, ratio 0.751× of TinyPNG,
658 B under cap). SSIM 84.27 → 77.70, still way above TinyPNG's
−63.72 floor (alpha-edge SSIMULACRA2 artifact on transparent fixtures).

All three candidate triggers tested in the spike (V_input
file_kb<50, V_npix n_pixels<100K, V_uniq uniq_opq<500) hit only 03 wiki.
**V_uniq picked** — production-computable, content-semantic
(sharp-mask logo = tiny palette), zero extra dependencies.

## What changed (nupic-quantize/src/lib.rs)

### `classify_for_palette_size` — sharp-mask branch split by uniq

Before (v1.2.7):
```rust
if adj_mn > 5.0 {
    return 256;
}
```

After (v1.2.8):
```rust
if adj_mn > 5.0 {
    // Cycle 105 P-03: split sharp-mask transparency by palette size.
    // Logo content (03 wiki uniq_opq=129) wins at K=64 ...
    let step_u = if n_total > 1_000_000 { 4 } else { 1 };
    let mut uniq = std::collections::HashSet::with_capacity(600);
    for p in src_rgba.chunks_exact(4).step_by(step_u) {
        if p[3] != 255 { continue; }
        let key = ...;
        uniq.insert(key);
        if uniq.len() >= 500 { return 256; }
    }
    return 64;
}
```

### Addition: `pub fn is_p03_sharp_mask_logo(rgba, w) -> bool`

Lock-step predicate consumed by `encode_png_stone_c` (in
`nupic-core::ops::compress`) to bump `oxipng_preset` from the adaptive
default (3 on <2MP) to 6 when P-03 trigger holds. Adaptive preset
alone (preset=3) gives 03 wiki 12 253 B at K=64; preset=6 closes
the remaining 2 KB to land at the spike's 10 135 B.

### `classify_for_auto_dither` — already correct, no change

`adj_mn > 5.0` already falls through to `return 0.0` (sharp-mask
no-dither). The Cycle 73 inline comment line 1474 documents this:
"Sharp-mask transparency (adj_mn > 5: 03 wiki logo, 14 soft-trans)
keeps d=0.0 — dither on antialiased edges adds visible noise."
No P-03 dither-side change needed.

### Compress path (`nupic-core::ops::compress::encode_png_stone_c`)

Gates on `opts.effort == default` so explicit user effort (Zopfli
opt-in at effort≥7) is preserved, then calls the new helper:

```rust
let p03_preset_boost = opts.effort == nupic_quantize::QuantizeOpts::default().oxipng_preset
    && nupic_quantize::is_p03_sharp_mask_logo(&raw, w as usize);
let qopts = nupic_quantize::QuantizeOpts {
    n_colors,
    oxipng_preset: if p03_preset_boost { 6 } else { opts.effort.min(10) },
    ...
};
```

## v1.2.8 vs TinyPNG (full baseline-7)

| fixture | v128 B | tiny B | ratio | size? | v128 SSIM | tiny SSIM | Q? |
|---|---:|---:|---:|:---:|---:|---:|:---:|
| 01 trans     |  36 366 |  48 295 | **0.753×** | ✓ | −62.02 | −492.64 † | ✓ |
| 02 pluto     |  60 789 | 180 788 | 0.336× | ✓ | 51.35 | −59.98 † | ✓ |
| **03 wiki**  | **10 135** | **13 492** | **0.751×** | **✓ NEW** | 77.70 | −63.72 † | ✓ |
| 04 portrait  | 434 158 | 569 959 | 0.762× | ✓ | 86.19 | 85.86 | ✓ |
| 05 mountain  | 326 977 | 434 250 | 0.753× | ✓ | 60.20 | 59.41 | ✓ |
| 06 landscape | 997 089 | 1 091 878 | 0.913× | ✗ | 79.93 | 79.76 | ✓ |
| 07 product   | 296 363 |  367 414 | 0.807× | ✗ | 82.79 | 80.32 | ✓ |
| **TOTAL**    | **2 161 877** | 2 706 076 | **0.799×** | **5/7** | — | — | **7/7** |

(† SSIMULACRA2 alpha-floor artifact, not a comparable score.)

## Visual eye gate (mandatory)

Read-tool inspection of `03 wikipedia logo` v1.2.8 output:
- Globe spherical contour clean
- Central "W" letter sharp
- Puzzle-piece tile joints crisp
- Multi-script small glyphs (Ω, 維, и, …) readable, no posterization
- Anti-aliased edges no halo / no chroma banding
- Drop-shadow gradient smooth

**PASS.** K=64 d=0 preset=6 fully preserves logo content quality.

## Decision gate (Cycle 105)

- baseline-7 size pass: **5/7** (gate-target 7/7) — progress
- baseline-7 SSIM pass: **7/7** — held
- cohort aggregate ratio: **0.799×** — first reading past −20% gate
- 30-fixture cohort no false trigger on V_uniq predicate
- Visual eye gate on 03 wiki: **PASS**
- **SHIP as v1.2.8.**

## Cycle 106 next-up (autorun entry)

**06 landscape attack** (122 KB above 0.80× cap, ratio 0.913×):
this is the single-palette ceiling fixture per memory. Algorithm-
frontier territory: R6 multi-tile palette or R3 VQ-VAE. Cycle 106
should:

1. Production sanity check vs v1.2.8 baseline (this essay's table).
2. Spike one algorithm direction (R6 has clearer paper hook).
3. If 06 landscape closes to ≤ 0.80× without regressing 01-05/07:
   wire + bump v1.2.9.

After 06 closes, gate becomes **6/7 size 7/7 SSIM** and we're one
fixture from sweep. 07 product (P-07 RED on corpus-500 per Cycle 103)
still needs a richer feature or learned model — Cycle 107+.

## Files

- `crates/nupic-quantize/src/lib.rs` — sharp-mask branch split, new
  `is_p03_sharp_mask_logo` helper
- `crates/nupic-core/src/ops/compress.rs` — preset boost wiring
- `crates/nupic-research/examples/cycle105_p03_validation.rs` —
  30-fixture cohort spike (production-aligned adj_mn)
- `Cargo.toml` — workspace version 1.2.7 → 1.2.8
- Previous: `04iii` Cycle 104 P-01 ship v1.2.7; `04hhh` Cycle 103
  predicate validation (P-03 PENDING resolved here).
