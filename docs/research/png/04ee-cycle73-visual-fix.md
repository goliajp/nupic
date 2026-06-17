# 04ee — Cycle 73: Visual quality fix on tier-trans (v1.2.1)

## TL;DR

v1.2.0 (Cycle 71) shipped joint anneal universally on `opq < 0.95`
content. SSIMULACRA2 reported +23.45 SSIM on 01 transparency-demo
and the baseline-7 ratio crossed −20 % for the first time. The user
ran a Read-tool visual inspection of the v1.2.0 output and reported:

> "tinypng 这个没问题,我们家这个完全用不了"

**The metric was lying.** 01 dice looked posterized. 02 pluto had a
harsh black ring at the alpha boundary. SSIMULACRA2's
near-zero-fidelity scoring on transparency content (both nupic at
−41 and TinyPNG at −493) inverted the visual ordering: TinyPNG was
fine, nupic was unusable.

Cycle 73 restores visual correctness on tier-trans by:

1. **Skip joint anneal for translucent content** — `opq < 0.95`
   branches `should_anneal = false`. Joint was overwriting any
   FS-dithered indices with smooth piecewise-constant assignments,
   destroying alpha-edge fidelity.
2. **Restore Cycle 35-era dither d=0.7** for smooth-gradient
   transparency — `classify_for_auto_dither` returns 0.7 when
   `opq<0.95 && adj_mn≤5`. Plus a defensive override in
   `quantize_indexed_png`: explicit `dither_strength=0.0` upgrades
   to the classifier-derived value (otherwise the bench / CLI
   default of 0.0 would bypass the fix).
3. **Restore tier-trans split via adj_mn** — sharp-mask logos
   (adj_mn > 5) keep n=256; smooth-gradient translucency
   (adj_mn ≤ 5) uses n=64 with the dither carrying the gradient
   smoothness that the smaller palette cannot.

## Baseline-7 result

```
fixture                          nupic   tinypng   ratio   ok?    SSIM    Δgate
01-png-transparency-demo.png      46 KB    47 KB   0.956x  ✓     -36.45  +456.19
02-pluto-transparent.png          99 KB   180 KB   0.548x  ✓     +80.16  +140.14
03-wikipedia-logo.png             14 KB    13 KB   1.096x  ✓     +84.27  +147.99
04-photo-portrait.png            434 KB   569 KB   0.762x  ✓     +88.07    +2.21
05-photo-mountain.png            326 KB   434 KB   0.753x  ✓     +70.37   +10.96
06-photo-landscape.png           997 KB  1091 KB   0.913x  ✓     +83.07    +3.31
07-photo-product.png             296 KB   367 KB   0.807x  ✓     +83.92    +3.59
────────────────────────────────────────────────────────
TOTAL                          2214 KB  2706 KB   0.818x = -18.2 %
                               all 7/7 within 1.15x gate, all 7/7 SSIM > TinyPNG
```

vs v1.2.0 (Cycle 71): −1.7 pp ratio, +visual correctness on every
tier-trans fixture. **Quality is the gate** — the user's mandate.

## Visual verification (Read tool, 2026-06-17)

| fixture | size | verdict |
|---|---|---|
| 01 dice | 46 KB | translucent dice intact, soft shadow preserved |
| 02 pluto | 99 KB | smooth alpha edge, no ring, surface tone clean |
| 03 wiki logo | 14 KB | unchanged (n=256 via sharp-mask branch) |
| 14 soft-trans puppy | 156 KB | soft alpha intact |
| 21 earth-hemisphere | 916 KB | matches source (source itself has hard edge) |
| 22 tree-trans | 1493 KB | edge intact |
| 23 statue-of-liberty | 224 KB | soft alpha intact |

## Routing logic

```rust
// quantize_indexed_png
let auto_d = classify_for_auto_dither(src_rgba, width);
let resolved_strength = if opts.dither_strength.is_nan()
                       || opts.dither_strength == 0.0 {
    auto_d  // force classifier for default callers
} else {
    opts.dither_strength  // honor explicit positive override
};

let should_anneal = if opq < 0.95 {
    false  // Cycle 73: skip joint for tier-trans
} else {
    var < 200.0  // opaque smooth content keeps Cycle 71 wins
};

// classify_for_palette_size
if opq < 0.95 {
    if adj_mn > 5.0 { return 256; }  // sharp logo
    return 64;                        // smooth-grad translucent
}
// opaque path unchanged

// classify_for_auto_dither
if opq < 0.95 && adj_mn <= 5.0 {
    return 0.7;  // restore Cycle 35
}
0.0
```

## Why the metric lied

SSIMULACRA2 was tuned on the BAPPS / TID2013 / Kodak corpora — all
opaque sRGB photos. Translucent content was never in scope.

When the algorithm sees a translucent dice pixel:
- Source alpha: 0.4 → rendered against white background = pale red
- Encoder loses alpha precision → reconstructs as opaque red
- SSIMULACRA2 RGB-compares pale-red vs opaque-red → "moderate
  mismatch" in absolute terms
- But the human sees: "wait, my dice is no longer translucent"

The metric measures color error but not perceived translucency
(which depends on background context, layering, depth, all of which
SSIMULACRA2 has no model for).

In v1.2.0's case the joint anneal posterised 01 into a small
palette of opaque-ish colors. SSIMULACRA2 awarded −41 because the
average RGB error was acceptable. Visually it was destroyed.

This is now documented in [[feedback-visual-eye-gate]] as a
standing constraint: **periodic Read-tool inspection of codec
output is now a ship gate, ranked above SSIMULACRA2 numbers on
transparency-tier content**.

## Paper implications

P3's "Cycle 71 alternating minimisation breakthrough" narrative was
based on metric improvement. The visual-correctness rebuttal means
the algorithm needs:

- **Different metric**: alpha-aware perceptual loss for the joint
  cost function's data fidelity term (not just OKLab+α L2)
- **Content-conditional routing** at finer grain: joint anneal
  works for opaque smooth content but breaks translucent gradient
- **Honest characterisation**: the Cycle 70/71 results were "beats
  SSIMULACRA2 on transparency" — a clamor of the metric, not the
  algorithm.

For a top-tier ML/CV venue submission this is actually a strong
finding: "we report a quality metric failure mode our codec
revealed". The negative-result framing fits CV venues that value
honest empirical work.

## Files touched

- `crates/nupic-quantize/src/lib.rs`
  - `quantize_indexed_png`: tier-trans joint anneal skip,
    explicit `dither_strength=0.0` override to classifier
  - `classify_for_palette_size`: tier-trans split by adj_mn
  - `classify_for_auto_dither`: smooth-grad transparency → 0.7
- `Cargo.toml`: 1.2.0 → **1.2.1**
- `docs/research/png/04ee-cycle73-visual-fix.md` (this essay)
- Memory: `feedback_visual_eye_gate.md` standing constraint

## What's next (Cycle 74+)

- **Re-examine Cycle 65-71** results under visual gate: how many of
  the "wins" were SSIMULACRA2-only and visually neutral or worse?
- **Alpha-aware metric**: extend SSIMULACRA2 score or add a
  Butteraugli-style multi-scale alpha term to the joint cost.
- **506-corpus visual sampling**: pick 30 random outputs, Read,
  catalogue any "metric-good visual-bad" cases.
- **02 pluto Pareto**: at n=64+d=0.7 we're at 99 KB / SSIM 80.16
  vs TinyPNG's 180 KB / SSIM −60. We're CRUSHING TinyPNG on 02.
  Room to push n down further if visual holds — sweep n=48, n=32
  with adj_mn=3.17 routing.
