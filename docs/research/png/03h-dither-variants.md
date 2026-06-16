# 03h — Stone E dither variants research(no ship,FS vanilla wins)

> Bench 4 dither variants on photo fixtures × strength {0.25, 0.5}:
> Floyd-Steinberg vanilla(currently shipping)、serpentine-raster FS、
> Sierra-3(7-neighbor),Sierra-Lite(4-neighbor different weights)。
> **No variant strict-Pareto-better** vs vanilla FS across the photo
> fixtures。Sierra-3 wins on 04-portrait specifically(smaller at
> essentially same SSIM)but trades SSIM for size on 05 / 07。No
> general ship — vanilla FS stays default。

---

## 1. Variants benched

| variant | neighbors | weights | notes |
|---|---|---|---|
| FS vanilla | 4 | 7/16, 3/16, 5/16, 1/16 | currently shipping |
| FS serpentine | 4 | same | rows alternate LTR/RTL direction |
| Sierra-3 | 7 | /32 see code | denser diffusion |
| Sierra-Lite | 3 | /4 | minimum-cost diffusion |

Tested on 04 / 05 / 06 / 07 photo fixtures × strength {0.25, 0.5}。
Initial palette via train_palette_rgba + refine_palette_kmeans(50 iter)。

---

## 2. Results

### Strength 0.25

| fixture | FS_size/SSIM | FS-snake | Sierra-3 | Sierra-Lite |
|---|---|---|---|---|
| 04-portrait | 640 724 / 83.50 | 639 925 / 83.49 | 631 984 / 83.45 | 642 551 / 83.42 |
| 05-mountain | 582 881 / 73.26 | 582 966 / 73.26 | 566 920 / 72.80 | 588 233 / 73.31 |
| 06-landscape | 1 253 215 / 83.79 | 1 252 553 / 83.82 | 1 248 039 / 83.59 | 1 253 499 / 83.79 |
| 07-product | 544 915 / 83.70 | 545 452 / 83.68 | 533 783 / 83.53 | 548 160 / 83.70 |

### Strength 0.50

| fixture | FS_size/SSIM | FS-snake | Sierra-3 | Sierra-Lite |
|---|---|---|---|---|
| 04-portrait | 655 245 / 84.00 | 656 147 / 84.06 | **637 457 / 83.99** | 661 495 / 83.99 |
| 05-mountain | 629 328 / 75.62 | 626 331 / 75.60 | 594 371 / 74.99 | 644 036 / 75.61 |
| 06-landscape | 1 268 272 / 84.53 | 1 268 082 / 84.52 | 1 259 540 / 84.32 | 1 269 460 / 84.55 |
| 07-product | 568 306 / 84.24 | 569 604 / 84.20 | 545 982 / 84.02 | 572 626 / 84.21 |

---

## 3. 分析

**FS serpentine vs vanilla**:basically identical(±100 bytes, ±0.03
SSIM)。OKLab-space dither + Lloyd-refined palette already balanced;
directional pattern doesn't accumulate measurable bias。**No 优势**。

**Sierra-3 vs vanilla FS**:
- 04-portrait @ s=0.5:**-17 KB at -0.01 SSIM**(Pareto-strict win)
- 05-mountain @ s=0.5:-35 KB at **-0.63 SSIM**(trade-off,size-leaning)
- 07-product @ s=0.5:-22 KB at -0.22 SSIM(small trade-off)
- 06-landscape @ s=0.5:-9 KB at -0.21 SSIM(small trade-off)

Sierra-3 always smaller(7-neighbor diffusion spreads error to thinner
high-freq noise → deflate ratio better)but slight SSIM drop。**04-portrait
specifically wins** but the general pattern is "smaller at slightly
less SSIM"。

**Sierra-Lite vs vanilla**:nearly equivalent。3-neighbor doesn't change
much。

---

## 4. Ship decision

**Not shipping any variant**。Reasons:

1. FS vanilla already near Pareto-optimal across photo fixtures
2. Sierra-3 's win on 04-portrait is single-fixture (not general)
3. Sierra-3 's SSIM regression on 05/07 (>0.2 pt) is meaningful
4. Adding `--dither variant <fs|sierra3>` knob without strict-win
   benefit is feature bloat

**Future research candidate**:per-fixture variant auto-pick(if
input is sky-class smooth → vanilla,if input is human-face-class
busy → Sierra-3)。Likely diminishing return given Stone E ceiling
is asymptoting near 84-85 pt on portrait-class。

---

## 5. 价值观

- [[feedback-ceiling-first-priorities]] — bench 4 variants × 4 fixtures
  × 2 strengths = 32 data points,no variant strict-better → don't ship。
  Negative-result research has value(prevents future "we should add
  Sierra"speculation)。
- [[feedback-no-cost-thinking]] — 不 评估 "Sierra implementation cost"
  vs benefit;data shows no benefit → don't ship,that's it。

---

## 6. cross-link

- 上游 Stone E ship:[03e](03e-stone-e-fs-dither.md) FS-light dither
- 上游 Pareto sweep:[03f](03f-pareto-tiered-dither.md) tiered classifier
- 实施 bench:[`crates/nupic-research/examples/dither_variants.rs`](../../../crates/nupic-research/examples/dither_variants.rs)
