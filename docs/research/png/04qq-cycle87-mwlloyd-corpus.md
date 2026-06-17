# 04qq — Cycle 87: R1 M-weighted Lloyd cross-corpus — per-content split (YELLOW)

## TL;DR

Cycle 86 gave R1 GREEN on 04 portrait (+2.59 SSIM at w_chrom=0.5,
ε=0.001). Cycle 87 runs the same config across baseline-7 to ask:
does the win generalise, or is it portrait-specific?

**Answer: per-content split.** R1 helps content with sharp edges +
chroma detail (portraits, transparency-blended, vivid colour); hurts
content dominated by smooth gradients (sky, snow). **Single-config R1
won't ship as default.** Per-content routing (Cycle 91) is the path.

## Result table

(M-Lloyd → ICM at w_chrom=0.5, ε=0.001, 10 iters; ICM = Cycle 71
anneal schedule; n_colors and alpha-importance picked by classifier)

| fixture       | n_col | imp  | ICM KB | ICM SSIM | MWL KB | MWL SSIM | ΔSSIM    | Δsize    | gate    |
|---------------|------:|-----:|-------:|---------:|-------:|---------:|---------:|---------:|:--------|
| 01 trans      | 64    | 0.00 | 26     | −19.168  | 25     | 14.980   | **+34.148** | −6.6%   | GREEN†  |
| 02 pluto      | 32    | 0.00 | 58     | 50.950   | 58     | 54.014   | **+3.064**  | −0.5%   | GREEN   |
| 03 wiki       | 256   | 0.00 | 14     | 95.810   | 14     | 95.801   | −0.008   | +0.4%    | tied    |
| 04 portrait   | 208   | 0.00 | 423    | 86.191   | 423    | 87.643   | **+1.451**  | −0.04%  | GREEN   |
| 05 mountain   | 144   | 0.50 | 311    | 58.949   | 311    | 54.305   | **−4.645**  | +0.08%  | **RED** |
| 06 landscape  | 208   | 0.00 | 964    | 79.785   | 964    | 79.196   | −0.589   | −0.02%   | RED     |
| 07 product    | 208   | 0.00 | 289    | 82.787   | 290    | 82.966   | +0.179   | +0.3%    | ≥0      |

| TOTAL | 2089 KB | 2088 KB | Δsize −0.06% | mean ΔSSIM **+4.80** | median ΔSSIM **+0.18** |

(† 01 trans `+34` is misleading — both ICM and MWL produce broken-looking
output on transparency; ICM has SSIM = −19. The harness's transparency
handling is incomplete vs. production pipeline. Use median, not mean.)

GREEN(≥+0.5) = 3/7 — 02 / 04 / 01
tied/≥0    = 2/7 — 03 / 07
RED        = 2/7 — 05 / 06

## What the split tells us

### Where R1 wins

- **02 pluto** (+3.06): photo with vivid Pluto colour + transparency.
  Chroma-respecting palette (w_chrom=0.5) matters; M-Lloyd palette
  packs chroma better than plain L² k-means.
- **04 portrait** (+1.45): skin tones at edge structures (eyes, hair,
  cheek shadow). Per-pixel `b_i` boosts those edges' weight in
  centroid; bandpass gets aligned with SSIM-sensitive regions.

### Where R1 hurts

- **05 mountain** (−4.65): smooth sky + soft snow texture dominate.
  `b_i` from bandpass DoG = small-ish for these "low-frequency"
  regions → palette gets pulled toward the (rare-but-large-`b_i`)
  high-frequency mountain ridgeline detail, starving the smooth
  gradient regions of palette entries. **SSIM penalises sky banding
  more than mountain edges; we optimised for the wrong region.**
- **06 landscape** (−0.59): similar mechanism, milder (more textural
  variation than 05).

### Where R1 ties

- **03 wiki** (≈0): logo content. Already at SSIM ≈ 96; little
  headroom. Bandpass doesn't change much because most pixels are
  near solid colours, low DoG.
- **07 product** (+0.18): white background + product. Two flat
  regions + one detailed object. M-Lloyd's bandpass weight does
  bias palette toward the product, but the white background takes
  most pixels regardless. Marginal win.

## Mechanism diagnosis (paper Section 6 material)

R1's `b_i = |DoG_low| + |DoG_high| + ε` **down-weights smooth
regions** because their bandpass response is at ε. For
content where **smooth regions are the SSIM-critical regions**
(05 mountain sky, 06 landscape), this is the wrong bias.

Two routing-design candidates:

1. **Content-class classifier**: detect "smooth-dominated" content
   (e.g., low high-DoG variance over the image, low colour entropy)
   and **skip M-Lloyd** for those classes, falling back to plain ICM.
2. **Adaptive `w_chrom` per class**: photo-with-skin → 0.5; landscape
   → 0.25 (more luma-dominant restores L-channel quantisation
   fidelity in gradient regions).
3. **Smooth-region ε boost**: replace constant ε with content-aware
   `ε_i` that's larger in smooth regions (e.g., based on local L
   variance), so smooth regions get a guaranteed-floor weight in the
   centroid.

Option 1 is simplest (binary gate), 2 is moderate (1 hyperparam per
class), 3 is most principled (paper-flavoured).

## Decision

Per-content routing needed. **Cycle 91** (per existing TaskList /
roadmap) is now scoped as **R1 routing design**, not blanket
productionization.

Mean +4.80 is **NOT a green-light signal** — it's dominated by 01
trans's harness artifact. Median +0.18 is the honest summary: R1 is
roughly neutral on average, with high content-class variance.

For paper: this is a **clean Section 6** ("when does perceptual k-means
help vs. hurt?") with a per-class breakdown. The mechanism story
(bandpass under-weights smooth) is testable and falsifiable.

## Compute

Per-fixture wall time:
- ICM (Cycle 71 anneal): ~0.5 s
- M-Lloyd (10 iter) + ICM: ~2 s
- 7 fixtures total: ~17 s plus SSIMULACRA2 compare calls

Overall corpus run < 1 minute. Easy to add to nightly regression once
we settle on routing.

## What this rules in / out

**Ruled in:**
- **Cycle 91 = R1 routing design** (was already on the TaskList;
  this cycle gave it specific shape: classifier-driven or per-class
  hyperparam).
- **R8 / R9 perf cycles** (Cycle 88-89) remain on track — perf
  improvements are content-class-independent.

**Ruled out:**
- **Blanket R1 default-flip**. Two RED fixtures (05, 06) are enough
  to block it.
- **Tuning `w_chrom` higher (e.g., 0.7+)** as a single global value
  — Cycle 86 sweep showed 0.5–0.6 was already saturated on portrait;
  pushing higher won't fix the smooth-region under-weighting (which
  is a `b_i` problem, not a `w_chrom` problem).

## See also

- `crates/nupic-research/examples/m_weighted_lloyd_corpus.rs` —
  cross-corpus harness.
- `docs/research/png/04pp-cycle86-m-weighted-lloyd-spike.md` — Cycle
  86 single-fixture spike.
- `memory/research_roadmap_1_2_x.md` — Cycle 87 GREEN→YELLOW
  reclassification, Cycle 91 scope sharpened to routing.
