# 01 — Case study: 02-pluto 在 imagequant 上的 algorithmic ceiling

> Backing experiment: `cargo run --release -p nupic-research --example pluto_sweep`
> → raw output `target/research-out/01-pluto-sweep.{csv,md}`.
>
> Anchor: [`00-attack-surface.md`](00-attack-surface.md) attack point #1
> (stage 1b 内部 quality–entropy trade-off)。

---

## 1. 设问

00 essay 给的诊断:nupic 0.4(imagequant default,`set_quality(70, 95)`,
`set_dithering_level(1.0)`)和 TinyPNG 在 02-pluto 上是 stage 1b 内部
trade-off 上的两个固定操作点 —— 选了不同 (size, DSSIM) 平衡。

这一篇要确认 / 推翻这个诊断,并回答 00 essay 的 open questions 1–3:

- **Q1**: dither_level 0 → 1 的 trade-off 曲线长什么样?单调还是 sweet spot?
- **Q2**: 把 quality range 真正交给 metric 驱动,02-pluto 的 DSSIM 能压
  到 < 0.02 吗?
- **Q3**: 把上面这些做成 `Quality::Auto` 内部自适应,是否会回退 04 / 07
  现有的胜利?

数据来源:`pluto_sweep` 在 02-pluto 上跑 5 dither × 12 (q_min, q_target)
= 60 cfg 全网格,在其他 6 张图上跑 6 cfg cross-check。

---

## 2. 第一发现:imagequant 在 02-pluto 上 DSSIM 卡死 0.075–0.080(metric ceiling)

60 个配置全部 02-pluto 行的 DSSIM 跨度:

| 统计 | 值 |
|---|---|
| n_configs | 60 |
| min DSSIM | 0.075206 |
| max DSSIM | 0.079671 |
| **span** | **0.0045** |

跨 60 个配置 DSSIM 只动 **0.0045**。dither_level 0 → 1 贡献 < 0.0001
(噪音级),q_target 50 → 100 贡献 < 0.001。**这不是 trade-off 曲线,
是一条紧贴 ceiling 的近水平线**。

Q1 答案:**不单调,也没 sweet spot。是平的。**

Q2 答案:**不能。** 改 quality_min / quality_target / dither 都 不能让
DSSIM 走出 0.075–0.080 区间。

imagequant 在 02-pluto(RGBA 渐变 alpha 边)上,256-color palette 装下
的颜色 + Floyd-Steinberg 系列 dither,**在 Lab L2 metric 下已经是局部
最优 + 全局最优**。再调 imagequant 自带的旋钮调不动。这是 [`png-pipeline.md` §1](../../png-pipeline.md#layer-a)
所说 "pngquant 在 Lab 下最小化欧氏距离找出的调色板,**不是人眼最小化
感知误差的调色板**" 的实测印证。

---

## 3. 第二发现:同 DSSIM ceiling 下,size 跨 34K → 158K(4.7×)

q_target 维度(dither=0,q_min=0,02-pluto):

| q_target | palette | bytes | DSSIM |
|---:|---:|---:|---:|
| 10 | 11 | 33,846 | 0.0797 |
| 30 | 16 | 39,712 | 0.0779 |
| 50 | 20 | 46,643 | 0.0764 |
| 80 | 49 | 82,225 | 0.0763 |
| 90 | 127 | 123,459 | 0.0753 |
| 95 | 256 | 158,390 | 0.0752 |
| 100 | 256 | 158,357 | 0.0752 |

DSSIM 跨 q_target 全程 **0.005**(几乎不动);size 跨 **4.7×**。

**关键 actionable**:nupic 0.4 default `set_quality(70, 95)` 在 02-pluto
上得到 256 palette / 158K / DSSIM 0.075。同一张图同一个 imagequant
**只用 q_target=30 就能得到 16 palette / 40K / DSSIM 0.078**(DSSIM 只差
0.003,人眼几乎察觉不到;**size 砍掉 75%**)。

**这是 free-lunch 体积削减**,不是质量牺牲。

跟 TinyPNG 180K / 0.018 比对:
- TinyPNG 在 size 维度 = 180K
- nupic q_target=30 = 40K → 体积只有 TinyPNG 的 **22%**
- DSSIM 仍 4× 比 TinyPNG 差(0.078 vs 0.018) — 但 ceiling 改不了

---

## 4. 跨图对比 — 02 是特例还是常态

3 张代表性图的 trade-off curve(`dither=1.0, q_min=0`,扫 q_target):

### 02-pluto(metric ceiling 图)

| q_target | palette | bytes | DSSIM | 评 |
|---:|---:|---:|---:|---|
| 80 | 49 | 90,830 | 0.0758 | best size cliff |
| 95 | 256 | 158,610 | 0.0752 | 0.4 default,**浪费 76%** |
| 100 | 256 | 158,610 | 0.0752 | 同 |

DSSIM 几乎不动,size 单调。

### 04-photo-portrait(正常 trade-off 图)

| q_target | palette | bytes | DSSIM | 评 |
|---:|---:|---:|---:|---|
| 80 | 26 | 205,083 | 0.0136 | 质量明显损失 |
| 95 | 114 | 384,433 | 0.0016 | 0.4 default,**最佳点** |
| 100 | 256 | 485,661 | 0.0010 | 微小 DSSIM gain,大量 size |

DSSIM 跟 size 单调反向,正常 trade-off,q=95 是 knee。

### 01-png-transparency-demo(ceiling 图,质量已胜 TinyPNG)

| q_target | palette | bytes | DSSIM | 评 |
|---:|---:|---:|---:|---|
| 80 | 256 | 54,043 | 0.1683 | — |
| 95 | 256 | 54,043 | 0.1683 | 0.4 default |
| 100 | 256 | 54,043 | 0.1683 | — |

q_target 完全不影响输出 — imagequant 强制 256 palette(因为图本身就是
~256 unique colors,quality 不影响 palette 选择 决策)。**reference
DSSIM**: TinyPNG 0.2196 (我们 0.168 已经胜过)。

---

## 5. 解读:per-image 的 trade-off 性质完全不同

把 6 张图 cross-check 的 DSSIM span 和 size span 汇总:

| image | DSSIM span (over 6 cfg) | size span | 类型 |
|---|---:|---:|---|
| 01-png-transparency-demo | 0.0001 | 5 K | ceiling, flat |
| 02-pluto-transparent | 0.0011 | 76 K | **ceiling, size-tunable** |
| 03-wikipedia-logo | 0.0004 | 5 K | flat |
| 04-photo-portrait | 0.0149 | 290 K | **normal trade-off** |
| 05-photo-mountain | 0.0001 | 21 K | near-ceiling |
| 06-photo-landscape | 0.0007 | 228 K | normal, knee far right |
| 07-photo-product | 0.0012 | 168 K | normal |

三个类别:
- **flat**:01, 03, 05 — 改参数动不了什么
- **ceiling-with-size-leverage**:02 — DSSIM 卡死,size 可大幅压缩
- **normal trade-off**:04, 06, 07 — DSSIM 跟 size 单调反向,有 knee

**ceiling 跟 normal 的边界,nupic 0.4 default(fixed q_target=95)看不出
来**。结果 = 02 这种图浪费体积;04 这种图刚好选对 knee。

---

## 6. Cement-layer fix spec(0.4.1 工程任务,**不是这篇 essay 的工作**)

### 目标

让 `Quality::Auto` 在 02 类图上滑到 q_target ≈ 30(40K / DSSIM 0.078),
在 04 类图上滑到 q_target ≈ 95(384K / DSSIM 0.0016),自适应。

### Heuristic 0 — 最朴素 elbow detection

1. 在 q ∈ {30, 60, 90, 100} 4 个点跑 imagequant,记 (palette_size, encoded_bytes, dssim)
2. 在 size-DSSIM 平面上找 knee:**最小 q 使 marginal_dssim_gain / marginal_size_cost > 阈值**(阈值候选 1e-6 / byte,需 calibration)
3. 返回 knee 的 PNG bytes

### 成本

每张图 4× quantize + 4× DSSIM。`pluto_sweep` 实测时间(release build):
- 02-pluto(632×632 RGBA8):quantize ~170-250 ms × 4 = ~1 s
- 04-photo-portrait(1200×800 RGB):~400 ms × 4 = ~1.6 s
- 06-photo-landscape(1600×900 RGB):~525 ms × 4 = ~2 s

可以接受。但要做成 `Quality::Auto` 默认,需要给 `--fast-auto` 一个旋钮
跳过 elbow(直接用现 q=95 行为)。

### 期望落地数字(基于 sweep 数据外推)

| image | nupic 0.4 default | spec 后(elbow) | 比例 |
|---|---:|---:|---:|
| 01-png-transparency-demo | 54 K | 49 K | 0.91 |
| 02-pluto-transparent | **159 K** | **~40 K** | **0.25** |
| 03-wikipedia-logo | 13 K | 10 K | 0.77 |
| 04-photo-portrait | 384 K | 384 K | 1.00 |
| 05-photo-mountain | 464 K | ~442 K | 0.95 |
| 06-photo-landscape | 1090 K | ~862 K | 0.79 |
| 07-photo-product | 347 K | ~200 K | 0.58 |
| **TOTAL** | **2,511 K** | **~1,787 K** | **0.71** |

跟 TinyPNG 比对:
- 现 0.4 default: 2,497 K / TinyPNG 2,706 K = **0.92×**
- elbow 之后(外推): 1,787 K / 2,706 K = **0.66×**

外推非常乐观;真正实现要跑实验 calibrate 阈值。但方向明确。

### 风险

- **DSSIM-elbow 不等于人眼-elbow**。02 上选 q=30 → DSSIM 0.078 跟
  q=95 → DSSIM 0.075 在数字上几乎一样,但 49 colors 跟 256 colors 的
  视觉色彩深度差异可能比 DSSIM 显示的大。需要肉眼对照,或换 metric。
- 这是 03-perceptual.md(SSIMULACRA2 stone)要回答的事;cement-layer fix
  先用 DSSIM 阈值跑一波,UAT 反馈再说。

---

## 7. 那 02-pluto 的 DSSIM 0.018 目标怎么办

**改不了**,在 imagequant 框架内。从 sweep 数据看,imagequant 4.x 的
Lab L2 内核 + Floyd-Steinberg / Riemersma dither 在 02 上能力到顶。
要追平 TinyPNG 的 DSSIM 0.018,只能:

- 换 metric(SSIMULACRA2 / Butteraugli / 类 LCH 感知模型 in-loop)
- 换 colorspace(OKLab → 量化损失更接近视觉损失)
- 换 quantizer(可微分 codebook / k-means++ + perceptual loss / 学习
  式 palette 选择)

详见 [`docs/png-pipeline.md` §1 Layer A/B/C](../../png-pipeline.md#第一段颜色空间--量化--dither--最大数学松弛在这里),
都是 stone-layer 工作(`roadmap.md` 阶段 3-4-5)。**这条路 0.4.x cement
打不了**。

→ 计划下一篇 `03-perceptual.md` 拆这条路的具体 stones。

---

## 8. Open questions(留给 03 或独立 follow-up)

1. **q_target=10 → q_target=100 时 DSSIM 反而稍升** (0.080 vs 0.075):
   是 quantize-induced blur 把 DSSIM 数字拉低 0.005 吗?换 SSIMULACRA2
   会反向吗?
2. **TinyPNG 的 quantizer 用了什么 metric?**只能 reverse-engineer
   indexed PNG 的 index map noise pattern 推断(Floyd-Steinberg
   scanline vs blue-noise spectrum)。
3. **dither_level 在视觉上的差异**:数字上 dither=0 跟 dither=1 在 02
   上 DSSIM 差 < 0.0001,但 dither=0 应该是 banding 明显,dither=1 平
   滑。这个差异 DSSIM 完全捕捉不到 —— 又是 metric 失败的实证。
4. `pluto_sweep` 不覆盖 16-bit PNG 输入,也不覆盖 RGB-without-alpha
   场景。后续 essay 要补的 boundary。

---

## 9. 引用与材料

- 实验代码:[`crates/nupic-research/examples/pluto_sweep.rs`](../../../crates/nupic-research/examples/pluto_sweep.rs)
- raw 输出:`target/research-out/01-pluto-sweep.{csv,md}`(generated;not committed)
- [`docs/png-pipeline.md` §1](../../png-pipeline.md) — Layer A/B/C 理论
- [Kornel Lesiński, *libimagequant 4.x* — `set_quality` / `set_dithering_level`](https://docs.rs/imagequant/4.4.1/)
- [Heckbert 1982, *Color Image Quantization for Frame Buffer Display*](https://dl.acm.org/doi/10.1145/965145.801294) — median cut 原始 paper
