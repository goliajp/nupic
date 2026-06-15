# 02 — 用 SSIMULACRA2 替代肉眼对照:让 metric 替我们做质量判断

> Backing experiment: `cargo run --release -p nupic-research --example metric_sweep`
> → raw output `target/research-out/02-metric-sweep.{csv,md}`.
>
> Triggered by feedback: **用户拒绝"叫他做人眼对照"作为 fix 风险条款的
> 兜底**。研究面要靠 metric 替代,不是请用户判断。

---

## 1. Why this essay exists

01 essay 给的 0.4.1 cement-fix spec(q-target elbow detection)有一个明
显的风险条款:**"DSSIM-elbow 不等于人眼-elbow"**,fallback 是请用户做
肉眼对照。用户立刻否决:他不是 image quality 评测专家,做不了。

那只有一个路径 —— 换 metric。DSSIM 看不到的差异,SSIMULACRA2 / Butteraugli
等更强 metric **应该**能看到。如果换了更强 metric 后差异仍然看不见,那
就是 algorithmic ceiling 真正存在(用户即使能做对照,大概也判不出差);
如果换了 metric 之后差异显著,说明 DSSIM elbow 是 mis-calibration,直
接拿来 ship 会损害真质量。

这两种结果都比"请用户看"好 —— 都是 metric-grounded 决策。

---

## 2. 选 metric

`docs/png-pipeline.md` §1 已经给出方向:**SSIMULACRA2**(Sneyers et al.,
JPEG XL 团队)是当前公认综合表现最好的 perceptual quality metric,跟
人眼实验数据(JPEG XL CFP test set)相关性最高。

可用 rust crate:[`ssimulacra2` v0.5.1](https://crates.io/crates/ssimulacra2)。
**pure Rust**(deps: `num-traits` / `thiserror` / `yuvxyb`),无 system
C 依赖,跨平台一致。接进 `crates/nupic-research/Cargo.toml` workspace
dep 即可,**不**进 nupic-core 公开 API(等 stone-layer 路线决定再 promote)。

Score interpretation(crate docs):
- 30 = low quality(cjxl -q 30 / mozjpeg -quality 30 的 p10 worst)
- 50 = medium
- 70 = high
- 90 = very high quality, likely indistinguishable from original

Score 可以**负数**,表示极差超出 cjxl 校准范围。

---

## 3. 实验:7 张图 × 8 q_target,DSSIM 和 SSIMULACRA2 同时测

`metric_sweep.rs`(详见 `crates/nupic-research/examples/`)对每张图:

- 8 个 q_target ∈ {10, 30, 50, 70, 80, 90, 95, 100}(固定 `dither=1.0`,`q_min=0`)
- 每点:imagequant → indexed PNG → oxipng,decode 后算 **DSSIM + SSIMULACRA2**
- 加 nupic-0.4-default 行 和 tinypng 行,7 张图全部对照

---

## 4. 主要 finding

### 4.1 跨 7 张图:SSIMULACRA2 上 nupic 5/7 胜 TinyPNG

`metric_sweep` 输出的 nupic-0.4-default vs tinypng 汇总:

| 图 | size ratio (n/t) | DSSIM (n / t) | SSIMULACRA2 (n / t) | SSIM 上谁胜 |
|---|---:|---:|---:|---|
| 01-png-transparency-demo | 1.12 | 0.168 / 0.220 | -443.0 / -492.6 | **nupic** (灾难内) |
| 02-pluto-transparent | 0.88 | 0.075 / 0.018 | -65.1 / -60.0 | tinypng |
| 03-wikipedia-logo | 0.97 | 0.005 / 0.131 | **+50.9** / -63.7 | **nupic 大胜 +114 分** |
| 04-photo-portrait | 0.67 | 0.0016 / 0.0016 | 81.5 / 85.9 | tinypng (略 +4) |
| 05-photo-mountain | 1.07 | 0.0014 / 0.0022 | **71.1** / 59.4 | **nupic +11 分** |
| 06-photo-landscape | 1.00 | 0.0005 / 0.0009 | 82.8 / 79.8 | **nupic +3** |
| 07-photo-product | 0.94 | 0.0005 / 0.0007 | 82.3 / 80.3 | **nupic +2** |

**SSIMULACRA2 上 nupic 胜 5 / 输 2**;TinyPNG 只在 02 和 04 上微胜。综合
体积(0.92× tinypng)+ 质量,**nupic 0.4.0 已经胜过 TinyPNG**。

DSSIM 单一 metric 给出的画面是误导的 — 它没看到 03 上 TinyPNG 的灾难(0.131
是 noticeable 边缘,SSIMULACRA2 -63.7 是确凿灾难),也没看到 05 上 nupic
的明显优势。

### 4.2 02-pluto 在 SSIMULACRA2 上也 ceiling

q-sweep 在 02 上:

| q_target | palette | bytes | DSSIM | SSIMULACRA2 |
|---:|---:|---:|---:|---:|
| 10 | 11 | 49,084 | 0.0781 | -65.61 |
| 30 | 16 | 52,328 | 0.0772 | -65.42 |
| 50 | 20 | 58,358 | 0.0763 | -65.35 |
| 70 | 36 | 71,648 | 0.0760 | -65.26 |
| 80 | 49 | 90,830 | 0.0758 | -65.22 |
| 90 | 127 | 125,503 | 0.0753 | -65.18 |
| 95 | 256 | 158,610 | 0.0752 | -65.13 |
| 100 | 256 | 158,610 | 0.0752 | -65.13 |

SSIMULACRA2 span **0.48 分**(8 个 q),DSSIM span **0.0029**。**两个 metric
共同确认**:02 是 imagequant 算法 ceiling。256 palette 装不下 RGBA 渐
变 alpha,无论 dither / q_target 怎么调,都是 disaster 区域。**01 essay
的诊断(metric ceiling)被 SSIMULACRA2 second-pass 印证,不是 DSSIM 偏
差**。

### 4.3 04-portrait 在 SSIMULACRA2 上是真 trade-off,**01 essay 的 elbow 阈值是错的**

| q_target | palette | bytes | DSSIM | SSIMULACRA2 |
|---:|---:|---:|---:|---:|
| 10 | 9 | 134,753 | 0.0880 | 16.0 |
| 30 | 11 | 152,610 | 0.0656 | 31.0 |
| 50 | 14 | 154,180 | 0.0643 | 34.1 |
| 70 | 18 | 183,269 | 0.0273 | 55.2 |
| 80 | 26 | 205,083 | 0.0136 | 64.7 |
| 90 | 50 | 277,065 | 0.0045 | 76.1 |
| 95 | 114 | 384,433 | 0.0016 | 81.5 |
| 100 | 256 | 485,661 | 0.0010 | 85.4 |

DSSIM 上 q=70 → q=95 是 0.027 → 0.0016 — 数字看上去**都 "tiny"**,容易
误以为没差。**SSIMULACRA2 上同一段是 55 → 81**,差 26 分(从 medium 升
到 high)。

如果 01 essay 提的 cement-fix 用 **DSSIM 阈值** 当 elbow 检测,会让 04
在 nupic 0.4 default(q=95 → SSIM 81)上**回退到 q=70 或更低**,SSIM 暴
跌到 55(medium quality)。**用户会看到质量损失,但 01 essay 给的阈值
不会预警**。

这就是 "DSSIM-elbow ≠ 人眼-elbow" 风险条款的本质 —— 用 SSIMULACRA2 替代
人眼,**直接看见就是了**:DSSIM 0.027 跟 0.0016 数字相近,SSIMULACRA2
55 跟 81 不相近。

### 4.4 跨图 SSIMULACRA2 跨度看 elbow 形态

| 图 | SSIM span (q=10→100) | 形态 | elbow 在哪 |
|---|---:|---|---|
| 01-透明骰子 | 7.7(-450 → -443)| 灾难 ceiling | smallest q (q=10) |
| 02-pluto | 0.5(-65.6 → -65.1)| 平直灾难 ceiling | smallest q (q=10) |
| 03-logo | 16.7(36.9 → 53.5) | 单调,medium range | q=95 |
| 04-portrait | 69.5(16.0 → 85.4) | 标准 trade-off,陡 | q=95 |
| 05-mountain | 36.9(34.1 → 71.1)| 单调,q=70 后平 | q=70 |
| 06-landscape | 48.8(34.0 → 82.8)| 单调,q=90 后微动 | q=90 |
| 07-product | 43.0(41.4 → 84.4)| 单调 | q=95 |

跨图模式清晰:
- **灾难 ceiling 图(01, 02)**:SSIM 跨度 < 8 分,任何 q 都灾难,选 smallest q free-lunch size 削减
- **正常 trade-off 图(03-07)**:SSIM 跨度 30+ 分,有真 knee,选 SSIM-saturation 点

---

## 5. Metric-grounded 0.4.1 cement-fix spec(替换 01 essay 的 spec)

### 算法

```
Quality::Auto for PNG:
  1. Quantize at q ∈ {30, 60, 90, 100}, all with dither=1.0
  2. Encode each → oxipng → decode → SSIMULACRA2 vs original
  3. SSIM_max = max(SSIM(q))
  4. tolerance T = 5  (SSIMULACRA2 points)
  5. Select smallest q such that SSIM(q) >= SSIM_max - T
  6. Return that q's bytes
```

### Behaviour 表(基于 sweep 数据外推)

| 图 | SSIM_max(q*) | 阈值(SSIM_max-5)| 选中 q | 现 default size | 新 size | 比例 |
|---|---:|---:|---:|---:|---:|---:|
| 01-png-transparency-demo | -443(q=50+) | -448 | q=30 (-445) | 54,043 | ~48,709 | 0.90 |
| 02-pluto-transparent | -65.13(q=95) | -70 | **q=10** (-65.61) | 158,610 | **49,084** | **0.31** |
| 03-wikipedia-logo | 53.5(q=100) | 48.5 | q=95 (50.9) | 13,136 | 13,136 | 1.00 |
| 04-photo-portrait | 85.4(q=100) | 80.4 | q=95 (81.5) | 384,433 | 384,433 | 1.00 |
| 05-photo-mountain | 71.1(q=70+) | 66.1 | q=70 (70.5) | 463,511 | ~450,112 | 0.97 |
| 06-photo-landscape | 82.8(q=95) | 77.8 | q=90 (82.2) | 1,090,366 | ~1,080,509 | 0.99 |
| 07-photo-product | 84.4(q=100) | 79.4 | q=95 (82.3) | 346,729 | 346,729 | 1.00 |
| **TOTAL** | | | | **2,510,828** | **~2,372,756** | **0.945** |

**关键变化跟 01 essay 比:**
- 01 essay 估计总体 **-28%(0.66× tinypng)**,基于 DSSIM elbow,会 误伤
  04 等正常 trade-off 图的质量
- 02 essay 估计总体 **-5.5%(0.87× tinypng)**,基于 SSIMULACRA2 elbow,
  **不伤质量**(每张图 SSIM_drop ≤ 5 分)
- 02 单图仍能省 **69%**(因为 SSIMULACRA2 在 02 上也 ceiling,任何 q 都
  灾难,选 smallest q 是无质量代价的)

### 风险条款(全 metric-grounded,不依赖人眼)

- ❌ ~~"SSIMULACRA2-elbow ≠ 人眼-elbow,需要肉眼对照"~~ — SSIMULACRA2 跟
  人眼实验相关性是当前 SOTA(JPEG XL CFP),不需要肉眼复查
- ⚠️ 阈值 T=5 是初始猜测。calibration 跑法:
  - 跨更大数据集(比如 [Kodim](https://r0k.us/graphics/kodak/) + screenshots)
    扫 T ∈ {2, 5, 10},计算每个 T 下 SSIMULACRA2 平均跌幅 + size 节省
  - 选 "size 节省 / SSIM 跌幅" 比最高的 T
  - 这是后续 essay `04-elbow-calibration.md`(待写)
- ⚠️ ssimulacra2 crate v0.5.1 跟 cloudinary 参考实现的 score 一致性
  没复查 — 加一个 test 用 cjxl reference image / score 对照(待写
  experiment)

### 实施时机

**等 02 essay merge 后再开 0.4.1 feature branch**。1-2 个开发 cycle 落
地;essay 04 calibration 可以并行做,等 calibration 完了 fine-tune T。

---

## 6. 关于把 SSIMULACRA2 promote 到 nupic-core

ssimulacra2 crate 是 pure Rust,加进 nupic-core 不会破跨平台 / 不会引
入 system deps。可以考虑下个 minor 把 `metrics::ssimulacra2` 从 `NotImplemented`
改为真实实现。

但 [`roadmap.md`](../../roadmap.md) 阶段 4 是 "自研 SSIMULACRA2 stone"。
现在用 `ssimulacra2` cement 接,跟 stone roadmap **不冲突**(将来自研版
本可以 swap 进来,API 保持不变),但**会增加 nupic-core 的非自研依赖
表面**,需要明确文档化。

**建议**:
- 暂时 research-only(本 essay 状态)
- 等 0.4.1 cement-fix 上线、用户反馈 confirm 改进时,再单独一个 PR 把
  `ssimulacra2` 接到 nupic-core 当 cement-layer metric
- `nupic compare --metric ssimulacra2` 在那个 PR 里开工

---

## 7. Open questions / 下一步 essay 候选

1. **TinyPNG 在 02-pluto 上 SSIMULACRA2 -60 vs nupic -65**:这 5 分差从
   哪来?是 TinyPNG 的 dither pattern(blue-noise?)? 还是 metric / colorspace
   切换?能否在 nupic 复现?
2. **calibration**(essay 04 候选):T=5 在大数据集上的 size/quality 收
   益曲线
3. **Butteraugli 第二意见**(essay 05 候选):JPEG XL 团队的另一个 metric,
   独立 sanity-check SSIMULACRA2 结论;如果两者在 02 上都说 ceiling,那
   ceiling 是真实的
4. **stone-layer 真正去解 02-pluto**:essay 03 (待写) 该聚焦 OKLab + SSIMULACRA2-driven
   quantizer 设计 — 这是脱离 02 disaster 的唯一路径

---

## 8. 验收材料

- 实验代码:[`crates/nupic-research/examples/metric_sweep.rs`](../../../crates/nupic-research/examples/metric_sweep.rs)
- 原始 CSV / MD:`target/research-out/02-metric-sweep.{csv,md}`(generated;not committed)
- 引用:
  - [Sneyers et al., *SSIMULACRA 2 — Improved Perceptual Image Quality Metric*](https://github.com/cloudinary/ssimulacra2) — 参考实现
  - [rust-av/ssimulacra2 v0.5.1](https://docs.rs/ssimulacra2/0.5.1/) — 我们用的 rust crate
  - [JPEG XL CFP test set](https://github.com/cloudinary/ssimulacra2#correlations) — score calibration 源
- 触发本篇的 feedback:[`feedback_metric_over_human_eye.md`](../../../../../.claude-profile-1/projects/-Users-doracawl-workspace-labs-lab29-nupic/memory/feedback_metric_over_human_eye.md)
- 01 essay 的 spec 被这篇 supersede:[`01-pluto-case.md`](01-pluto-case.md) §6
