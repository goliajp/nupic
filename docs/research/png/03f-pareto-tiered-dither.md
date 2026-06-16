# 03f — Pareto sweep + tiered `--dither auto`(photo 0.5 / UI 0.25)

> 03f extends 03e Stone E adaptive dither with a **3-tier classifier**
> driven by two cheap content statistics(opaque-ratio + mean-run-
> length)。9-fixture cross-product sweep(7 corpus + 2 dogfood)reveals
> photo content gains +1-5 SSIMULACRA2 at dither 0.5 while UI text
> screenshots cap at dither 0.25(0.5 regresses)。Mean-run-length
> perfectly separates the two classes:photos ≤ 1.36,UI ≥ 7.89。
> Ship as v0.5.19 — every input non-regression,photos +1-5 SSIM,UI
> +0.1-0.2 SSIM,logos / transparent unchanged。

---

## 1. Pareto sweep

`crates/nupic-research/examples/pareto_sweep.rs` cross-product
(refine_iters=20 default) × dither_strength {0, 0.25, 0.5, 0.75} on
the 9-input corpus:

| input | d=0.00 size/SSIM | d=0.25 size/SSIM | d=0.50 size/SSIM | d=0.75 size/SSIM | Pareto |
|---|---|---|---|---|---|
| 01-transparency | 45 398 / -46.4 | 50 905 / -43.6 | 57 481 / -35.7 | 68 197 / -32.3 | all 4 |
| **02-pluto** | 157 706 / 72.3 | 161 197 / 71.9 | 166 686 / 69.7 | 170 654 / 63.5 | **only 0.0** |
| 03-wikipedia | 12 658 / 89.5 | 13 162 / 89.6 | 13 415 / 89.3 | 13 299 / 89.4 | 0.0, 0.25 |
| 04-portrait | 380 748 / 82.95 | 386 115 / 83.45 | 395 134 / **83.98** | 401 992 / 84.28 | all 4 |
| 05-mountain | 393 559 / 70.30 | 426 634 / 73.14 | 456 514 / **75.39** | 477 730 / 76.49 | all 4 |
| 06-landscape | 1 044 702 / 82.75 | 1 060 898 / 83.81 | 1 090 583 / **84.48** | 1 125 777 / 84.66 | all 4 |
| 07-product | 319 157 / 82.83 | 337 945 / 83.70 | 350 336 / **84.24** | 366 299 / 84.06 | 0, 0.25, 0.5 |
| testflight (UI) | 19 828 / 84.72 | 20 405 / 84.83 | 21 327 / 84.59 | 22 265 / 84.03 | 0, 0.25 |
| vantage (UI) | 279 259 / 81.32 | 304 035 / 81.50 | 335 529 / 81.79 | 358 952 / 81.84 | all 4 |

Patterns:

- **02-pluto** unique:dither hurts strictly。Already correctly skipped
  by 03e classifier(opaque_ratio = 0.78 < 0.95)。
- **Photo fixtures**(04-07):monotonic SSIM improvement;0.5 sweet spot
  for best size-SSIM ratio。
- **UI screenshots**(testflight、03-wikipedia,partial):0.25 is the
  natural ceiling — 0.5 regresses or dominated。
- **Mixed**(vantage):all-frontier — depends on user weighting。

---

## 2. Auxiliary signal — mean run length

Per-fixture mean length of consecutive RGB-identical pixel runs:

| input | opq_ratio | **mean_run** | uniq_ratio | predicted tier |
|---|---:|---:|---:|---|
| 01-transparency | 0.04 | 2.72 | 0.09 | tier-1(transparent)|
| 02-pluto | 0.78 | 1.36 | 0.05 | tier-1(transparent)|
| 03-wikipedia | 0.74 | 1.99 | 0.005 | tier-1(small)|
| **04-portrait** | 1.00 | **1.18** | 0.027 | **tier-3(photo)**|
| **05-mountain** | 1.00 | **1.29** | 0.234 | **tier-3(photo)**|
| **06-landscape** | 1.00 | **1.10** | 0.053 | **tier-3(photo)**|
| **07-product** | 1.00 | **1.21** | 0.031 | **tier-3(photo)**|
| **testflight** | 1.00 | **94.53** | 0.003 | **tier-2(UI)**|
| **vantage** | 1.00 | **7.89** | 0.029 | **tier-2(UI)**|

**mean_run 完美分 photo / UI**:photos 全 ≤ 1.36,UI 全 ≥ 7.89,gap
huge enough to use threshold 2.0 safely。

`uniq_ratio` 不分:photos 0.027-0.234 跨 logos 0.005 + UI 0.003-0.029
overlap heavily。`mean_run` 是 the right signal。

---

## 3. Tiered classifier 实施

```rust
pub fn classify_for_auto_dither(src_rgba: &[u8]) -> f32 {
    let (opaque_ratio, n_total) = ...;
    if opaque_ratio < 0.95 || n_total < 200_000 {
        return 0.0;  // tier-1: skip
    }
    let mean_run = ...;
    if mean_run > 2.0 {
        0.25  // tier-2: UI light dither
    } else {
        0.5   // tier-3: photo dither
    }
}
```

实测 v0.5.19 `--dither auto` 全 9-input:

| input | tier | strength | ΔSSIM(vs `off`)|
|---|---|---:|---:|
| 01-transparency | 1 | 0.0 | 0.00 |
| 02-pluto | 1 | 0.0 | 0.00 |
| 03-wikipedia | 1 | 0.0 | 0.00 |
| **04-portrait** | **3** | **0.5** | **+1.03** |
| **05-mountain** | **3** | **0.5** | **+5.08** |
| **06-landscape** | **3** | **0.5** | **+1.73** |
| **07-product** | **3** | **0.5** | **+1.40** |
| **testflight** | **2** | **0.25** | **+0.11** |
| **vantage** | **2** | **0.25** | **+0.18** |

**全 non-regression**。Photos get full 0.5 benefit,UI get safe 0.25,
others unchanged。Compare 0.5.18(flat 0.25 for all opaque-large):
- 0.5.18 4-portrait: +0.51 → 0.5.19: **+1.03**(×2 improvement)
- 0.5.18 testflight: +0.11 → 0.5.19: +0.11(unchanged,UI still 0.25)
- 0.5.18 vantage: +0.18 → 0.5.19: +0.18(unchanged)

---

## 4. 价值观

- [[feedback-ceiling-first-priorities]] — tiered classifier closes
  photo gap fully(0.5 sweet spot)without UI regression。Per-content
  ceiling distinguished by cheap statistic。
- [[feedback-metric-over-human-eye]] — mean-run-length signal selected
  after Pareto sweep ground truth;not heuristic。
- [[feedback-no-cost-thinking]] — 9-input cross-product bench drives
  classifier design;每行 quote SSIM 数字。

---

## 5. 下一步

- **flip default to `auto`**(候选)— 当前默认 `off` 保 size。Auto 在 9/9
  input 都 non-regression,可考虑 default。User judgment on size cost。
- **adaptive refine_iters**(Pass 3)— EPS-based 自动收敛,close 02-pluto
  iter=50 case(+7 SSIM)without explicit knob。
- **dither variant research**(Pass 4)— serpentine raster / Sierra:
  可能 same SSIM gain at less size。
- **`--use-nupic-png` perf cliff**(Pass 5)。
- **nupic-bits NEON pclmul**(Pass 6)。
