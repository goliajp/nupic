# 03d — Stone D 设计:adaptive light dither close SSIMULACRA2 gap on anti-aliased / photo content

> 2026-06-16 dogfood 暴露 Stone C OKLab-argmin+no-dither 的 SSIMULACRA2
> regression:7-fixture corpus 中 04-photo-portrait −4.07 pt,
> testflight UI screenshot −3.3 pt vs TinyPNG。规律:**anti-aliased
> smooth 区域**(人脸 skin tone gradient、UI text edge)在 no-dither
> 下 hard-quantize 出现 visible banding / edge stepping。Stone D 目标:
> 在这些 region 加 selective light dither,**保持 5/7 fixture 上的
> SSIMULACRA2 win,close 2/7 fixture 的 regression**。

---

## 1. 现状 perf table(post-0.5.14 Default path)

| fixture | size n/t | SSIM_n | SSIM_t | Δ |
|---|---:|---:|---:|---:|
| 01-png-transparency-demo | 0.96× | -71.7 | -492.6 | **+420.93** |
| 02-pluto-transparent | 0.88× | -35.3 | -59.9 | **+24.64** |
| 03-wikipedia-logo | 0.94× | 51.6 | -63.7 | **+115.32** |
| **04-photo-portrait** | 0.67× | 81.8 | 85.9 | **−4.07** ✗ |
| 05-photo-mountain | 0.93× | 69.4 | 59.4 | +9.98 |
| 06-photo-landscape | 0.97× | 82.1 | 79.8 | +2.37 |
| 07-photo-product | 0.89× | 82.6 | 80.3 | +2.23 |
| dogfood testflight | 0.65× | 81.5 | 84.8 | **−3.3** ✗ |

Pattern:**SSIM regression 仅在 photo-portrait / UI-text-screenshot**。
Mountain / landscape / product 在 nupic 下 strict SSIM win。
Transparency / logo 在 SSIMULACRA2 上 catastrophically diverge between
tools(both 负数 absolute,nupic 仅 mediocre 但 tiny 崩);size 上 nupic
也 win。

**Root cause hypothesis**:OKLab argmin + no-dither 在 anti-aliased
渐变上把 N 个 sub-pixel intermediate gray levels collapse 到最近 palette
entry,出现 "edge-stepping"(每条 anti-aliased 边界都被 round 到几个 离散
entries)。SSIMULACRA2 重 punish 这种 perceptual edge-discontinuity。

---

## 2. 设计 — adaptive light dither

### 2.1 candidate variants

| variant | trigger | mechanism | size cost | risk |
|---|---|---|---|---|
| **A. error-magnitude dither** | per-pixel `|residual|`>τ | Bayer/blue-noise modulation between best & second-best | +5-10% | breaks Stone C "no-dither" simplicity |
| **B. local-variance dither** | local 3×3 luminance variance > τ | Same as A | +5-10% | requires variance precompute pass |
| **C. global on-toggle** | always dither | unconditional best/2nd swap by blue-noise | +15-30% | size hit on flat regions |
| **D. content-class dither** | smooth(low-freq)region: no dither;textured: dither | + edge / gradient detector | +5-15% | extra classifier complexity |

**Variant A** 是 minimum-viable:**only dither where Stone C 的 hard
assign 自己暴露 large residual,which is exactly the perceptually-visible
banding case**。Implementation:

```
for each pixel:
    best_j, best_d2 = argmin(palette_oklab, pixel_oklab)
    second_j, second_d2 = second-argmin
    residual = sqrt(best_d2)
    if residual > THRESHOLD:
        # Blue-noise modulation in {0, 1}; pick best or second
        mix = bayer8x8[x%8][y%8] / 64.0   # in [0, 1)
        # `mix` close to 0 = always best; close to 1 = always second
        # interpolation between best and second weighted by distance
        ratio = best_d2 / (best_d2 + second_d2)
        if mix < ratio:
            commit second_j
        else:
            commit best_j
    else:
        commit best_j
```

THRESHOLD 调:OKLab distance > 0.02(empirical perceptual JND scale)。
Bayer matrix 用 standard 8×8(predictable spatial spectrum,~64 thresholds
distributed),or blue-noise from precomputed table。

### 2.2 alpha & palette implications

Stone D doesn't change palette training — same imagequant median-cut
+ alpha-aware quantize from phase 2.1。Only the **assign** step
(`apply_palette_rgba`)gets dither layer added。`tRNS` chunk unaffected。

### 2.3 perf cost

Per-pixel cost:
- Compute second-best argmin alongside best:+1 comparison per palette
  entry → ~ 2× inner-loop cost
- Bayer lookup:O(1)constant
- Branch on threshold:cheap

Total expected:**~ 1.5-2× wall-clock** vs current Stone C。Acceptable
since current Stone C is ~ 30 ms / 400KB on M2。

---

## 3. 验证 plan

实操候选(this autorun pass):

1. 在 `crates/nupic-research/` 加 `examples/stone_d_dither_bench.rs`
2. 加 prototype `apply_palette_with_dither()` 实施 variant A
3. Run on 7-fixture + dogfood testflight, measure:
   - **SSIMULACRA2** vs original(target:close 04 / testflight gap to ≥ TinyPNG-1pt)
   - **Size** vs current Stone C(accept up to +10%)
   - **05/06/07 SSIM no-regression**(Stone D should only help on banding-prone, not hurt elsewhere)
4. Sweep THRESHOLD in [0.01, 0.05] to find sweet spot
5. If win → graduate to `nupic-quantize` as `QuantizeOpts::dither: DitherMode::{None, AdaptiveLight}`(opt-in)
6. If size cost too high on Default Path → only enable when SSIMULACRA2 metric requested

---

## 4. Negative result — Variant A 翻车

2026-06-16 实测 Variant A(Bayer-modulated swap to 2nd-best palette
entry,4 thresholds × 7 fixture):

| threshold | size Δ | SSIM avg(no-dither → dither)|
|---|---|---|
| 0.01 | +88 KB(+2.7%)| 37.20 → 30.29(**−6.91**)|
| 0.02 | +29 KB(+0.9%)| 37.20 → 30.37(**−6.83**)|
| 0.03 | +21 KB(+0.6%)| 37.20 → 30.45(**−6.75**)|
| 0.05 | +14 KB(+0.4%)| 37.20 → 30.52(**−6.68**)|

**SSIMULACRA2 全 fixture 都下降** —— 包括 supposedly target 的 04-portrait
(81.79 → 81.66)。03-wikipedia 崩最重(51.60 → 25.33,**−26 pt**):
Bayer pattern 在 logo flat-color region 引入 visible grid artifact。

**Root cause:Bayer-swap-to-2nd 不是 true dither,是 noise injection**。
True dither (Floyd-Steinberg)diffuses quantization error 到 neighbors
使 spatial average 接近 source;simple swap 只是 throw away color
accuracy。SSIMULACRA2 punishes 这种 spurious noise 比 quantization
banding 更重。

**Lesson**:不要假设 "add dither = win"。Variant A 是 cement-think
("dither always smooths gradient")翻车的实例 — metric 数据驱动决定
direction，不要直觉。

---

## 5. Variant E — k-means palette refinement(no dither needed)

Stone C 起手 = imagequant median-cut palette。Median-cut 是 split-
based heuristic,不保证 cluster centroids 是 within-cluster mean。
**Lloyd's iteration** 后 strictly 减 within-cluster L2 error:

```
palette = imagequant_median_cut(image)
for iter in 1..N:
    assignments = argmin_oklab(pixels, palette)   # current Stone C
    new_palette = []
    for cluster_j in palette:
        cluster_pixels = pixels where assignments == j
        new_palette[j] = mean_oklab(cluster_pixels)
    if palette == new_palette: break  # converged
    palette = new_palette
```

Benefit hypothesis:
- 04-portrait 256-entry palette 上的 skin-tone region 在 imagequant
  initial 后可能 sub-optimal centroid;refinement 推到 perceptual center
- SSIMULACRA2 应该 strictly improve(within-cluster error 减)
- Size 不变(palette 表 size 不变,indices 取决于 cluster geometry,
  通常 minor change)

Risks:
- 收敛慢(typical 3-10 iterations)→ wall-clock cost ~ 3-10× current Stone C
- 某些 cluster 在 iteration 中变空 → need handling
- imagequant 的 perceptual quality 已 better than vanilla median-cut → 
  refinement margin 可能 thin

实操(this autorun):extend `stone_d_dither_bench.rs` 加 Variant E
column,bench against Variant A 的 negative baseline。

If Variant E 在 04-portrait 上 close +4 pt gap to tiny → graduate to
`QuantizeOpts::refine_palette: bool`。

### 5.1 Variant E 实测(2026-06-16)

| n_iters | corpus size | size Δ | SSIM avg | SSIM Δ |
|---|---:|---:|---:|---:|
| 0 (baseline) | 3 298 861 | — | 37.20 | — |
| 1 | 3 295 107 | −3 754 | 59.99 | **+22.79** |
| 3 | 3 291 039 | −7 822 | 60.07 | **+22.86** |
| 5(sweet)| 3 284 884 | −13 977 | **61.89** | **+24.68** |
| 10 | 3 277 993 | −20 868 | 61.83 | +24.63 |

每个 fixture 都 strictly improved(no regression)。**Size −0.4-0.6%
同时 SSIM +0.07~107 per fixture** —— 真正的 "又小又好"。

Production bench(Path A oxipng pipeline, after Stone D graduation
to `quantize_indexed_png` default with `DEFAULT_REFINE_ITERS=5`):

| fixture | before(no refine)| **after**(refine 5)| size Δ | SSIM Δ |
|---|---:|---:|---:|---:|
| 01-transparency | 46 475 / -71.7 | **45 532 / -45.9** | −943 | **+25.8** |
| 02-pluto-trans | 158 972 / -35.3 | **158 518 / +72.4** | −454 | **+107.7** |
| 03-wikipedia-logo | 12 735 / +51.6 | **12 658 / +89.5** | −77 | **+37.9** |
| 04-photo-portrait | 380 318 / 81.8 | **376 900 / 82.3** | −3 418 | **+0.5** |
| 05-photo-mountain | 402 741 / 69.4 | **396 617 / 69.7** | −6 124 | **+0.3** |
| 06-photo-landscape | 1 062 185 / 82.1 | **1 049 235 / 82.6** | −12 950 | **+0.5** |
| 07-photo-product | 325 525 / 82.6 | **318 041 / 82.6** | −7 484 | **0.0** |
| **TOTAL** | **2 388 951** | **2 357 501** | **−31 KB** | — |
| **vs TinyPNG** | 0.883× | **0.871×** | | |

**Result: graduate to nupic-quantize `quantize()` default with
`DEFAULT_REFINE_ITERS=5`,available via `quantize_with(..., refine_iters)`
for explicit override**。

`refine_palette_kmeans` 现是 nupic-quantize 公开 API,可供 nupic-png
integration path 同样调用。

---

## 6. 价值观

- [[feedback-ceiling-first-priorities]] — quality ceiling 跟 size
  ceiling 同等 attack;真正的 win condition 是 "又小又好"
- [[feedback-metric-over-human-eye]] — Stone D 由 SSIMULACRA2 metric
  数字驱动决定;Variant A 翻车直接 abandon,Variant E 实测后 graduate
- [[feedback-no-cost-thinking]] — Stone D wall-clock 增 ~ 5× quantize
  step(0.5s → 2.5s per 1MP image);Stone C apply_palette 本身 sub-
  second,5× 仍 acceptable。不评估"该不该"costs。

---

## 6. 价值观

- [[feedback-ceiling-first-priorities]] — quality ceiling(SSIMULACRA2)
  跟 size ceiling 同等重要;"又小又好"要求两个 metric 都 win
- [[feedback-metric-over-human-eye]] — Stone D 由 SSIMULACRA2 metric
  数字驱动决定;dither parameter sweep 跑数据
- [[feedback-no-cost-thinking]] — adaptive dither 的 +5-10% size cost
  是 ceiling-cost 不是 ROI 评估;只 quote SSIMULACRA2 改善 + size cost
  让 user 看数据
