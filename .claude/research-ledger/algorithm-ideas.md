# 算法创新候选 ledger

跨 cycle 累积。每条带:
- **状态**(open / in-progress / shipped / dropped)
- **来源 cycle**
- **evidence**
- **可行性**(高/中/低)
- **论文级**(★ … ★★★★★)
- **下一步**

状态变更时**改原条目,不删**,留 cycle 来源 trail。

---

## [Cycle 106-108 · status:ceiling-hit · 可行性高 · ★★] A. Content-aware K predictor

**Idea**:基于 input image features(palette pre-cluster spread / luma gradient entropy / opaque fraction / chroma variance / n_pixels / bits-per-pixel)选 K。

**Cycle 108 实测结果**:
- 最简 rule:`n_pixels ≥ 5MP → K=224 d=0.3` 全 corpus-500 测
- 总 PASS 23.4%(106 → 120,**+14 净涨**)
- **PASS pile retention 99.1%(105/106)** — 1 张退化 p244
- 试遍 input features(bpp_in、bpp_v128、n_pixels、luma/chroma)**无法 cleanly 区分 p244 vs 11 张 wins**
- input-only feature 路径 **ceiling 在 99.1%**,无法到 100%

**Status**:**ceiling-hit** — input-feature 单独不够,Cycle 109 转 [J. 2-pass fail-safe](升级版)。

**Lesson kept**:Input feature 当**廉价 trigger gate**(决定要不要走 2-pass)仍然有用 — 不全废。

---

## [Cycle 108-109 · status:SHIPPED v1.2.9 · 可行性高 · ★★★] J. 2-pass K-up fail-safe routing(A 的升级)

**Idea**:production hot path 跑 2 次 quantize,根据 K=128 输出大小决定要不要升 K=224。
```rust
let bytes_v128 = quantize(raw, K=128, ...);  // production current
if n_pixels >= 5_000_000 && bytes_v128.len() > input_size * 78 / 100 {
    let bytes_v224 = quantize(raw, K=224, d=0.3);
    if bytes_v224.len() < bytes_v128.len() { return bytes_v224; }
}
return bytes_v128;
```

**Why this beats A**:
- **100% PASS pile retention by construction** — 选 min(K=128, K=224) 永远不让 output 比 K=128 大
- Trigger gate `n_pixels ≥ 5MP AND bytes_v128 > 0.78 × input_size` 是 input-only(不需 tiny baseline)
- production cost ~1.5× wall on 14.8% corpus(≥5MP)— 在 perf NAS/CDN target 内

**Evidence**:
- Cycle 108 数据显示 input-feature 路径 ceiling 99.1%,2-pass 是唯一 100% 路径
- Cycle 108 rule v3 数据 + p244 反例直接 motivate 这条 routing

**Cycle 109 spike**:
- 改 `crates/nupic-core/src/ops/compress.rs:225` 加 2-pass fail-safe
- bench 跑 32 quick + corpus-500 full(全 GREEN gate)
- 219 workspace tests + baseline-7 sanity 必过
- 通过 → bump v1.2.9 + ship + push

---

## [Cycle 106 · open · 可行性中 · ★★★] B. K-monotonicity Pareto curve analytical model

**Idea**:把 [paper-material `Palette-size monotonicity break`] 的 finding 数学化 — 拟合 `size_after_filter = f(K, content_features)`,找 K-dip 的 analytical sweet spot。

**Why now**:
- `pile_a_grid.tsv` 651 行就是采样表,可直接 plot K vs size 曲线 per fixture
- 多张 fixture 的 K=192 dip 可能跟某个 content feature(palette spread / gradient entropy)相关

**Evidence**:
- Cycle 106 5 张 fixture 的 K-size 曲线已采样
- 文献(Wallace JPEG / Wallach VQ)只覆盖 single-image RD,未触及 PNG filter-chain 跟 palette 的耦合

**下一步**:
- 写 plotting script(matplotlib + pandas),per-fixture K-size 曲线 + per-fixture K-DSSIM 曲线
- fit Gaussian-mixture 或分段线性 surrogate,看 K=192 dip 在 content features 空间的位置

---

## [Cycle 106 · open · 可行性高 · ★] C. Slow-tier zopfli 路由

**Idea**:加 `nupic compress --effort 9` / `--slow` flag,触发 oxipng + zopfli(30-iter)refine 后处理,救 size-edge fixture。

**Why now**:
- Cycle 106 zopfli probe 救活 2/4 size-edge(n24_sun, p283)
- ~30 sec/fixture wall cost,production 热路径不行,但批处理 / 离线 / CI 场景值得

**Evidence**:
- `assets/png-bench/cycle106-r4/emit.tsv` 4 张 edge fixture 的 plain vs zopfli 对比表

**下一步**:
- Cycle 108+(或 ship 阶段)加 CLI flag + bench + doc
- 不上 production routing,只 expose 给用户(主动 opt-in)

---

## [Cycle 106 · open · 可行性中 · ★★] D. Adaptive dither schedule(d 跟 image entropy 联动)

**Idea**:dither_strength 从全局 default 0.0 / 0.3 改成 input-feature-aware(low entropy → d=0;high entropy → d=0.3-0.6)。

**Why now**:
- Cycle 106 赢家 d 分布:d=0.3 占 11/23(48%),d=0.0 占 9/23(39%),d=0.6 占 3/23(13%)
- d=0.6 在 n24_sun(纹理太阳)+ p143 等高纹理 fixture 救活,d=0.0 在 photo gradient 类胜出 — 暗示 d 跟 image structure 相关

**Evidence**:
- 31 fixture × 21 config grid 给出 (image, d, dssim, size) 联合数据,可统计 (winning d × content feature)

**下一步**:
- 收集 (image entropy, optimal d) pair,看是否单调或可分段
- 若线性可分 → 直接 plug 入 [A] decision tree

---

## [Cycle 106 · open · 可行性低 · ★★★★★] E. Multi-tile palette(R6,Cycle 108-110+)

**Idea**:每张图切 N tile,每 tile 独立量化 palette,encoder side 用 spatial entropy coder 编码 tile-palette + tile-index。

**Why now**:
- Cycle 106 暴露 6 张 DSSIM-infeasible Pile A fixture,**单 palette 范式已到天花板**
- R6 是 Cycle 102 spike 阶段就提的 idea,Cycle 106 才有数据驱动的强 motivation

**Evidence**:
- Pile A 6 张 DSSIM-infeasible fixture 全是 high-frequency Picsum photo
- 已有 R6 multi-tile 文献(LCT-based codec, spatial VQ)给出工程参考点

**下一步**:
- Cycle 108-pre:量化 multi-tile 范式 oracle 上限(用 ImageMagick 切 tile + 各自 nupic quantize + bench)
- 若 oracle 把 6 张 DSSIM-infeasible 救回 ≥ 3 张 → Cycle 110 spike 入口

---

## [Cycle 106-110 · status:rejected · 可行性高 · ★] F. Per-pile lossless fallback routing

**Cycle 110 实测**:对 6 张 Cycle 106 DSSIM-infeasible fixture(p125/p274/p214/p115/p175/p167)跑 `nupic compress --lossless`:**0/6 PASS**,ratio 1.36-1.95× tiny。TinyPNG 是 lossy,nupic lossless 必然大很多。**rejected** for the rescue role — 这 6 张是 truly single-palette-infeasible,必须 R6 / R3 spatial-aware。

留作 **fall-through option**(如果 K=128 + K=224 都失败时路由到 lossless 至少保底)— 但 Cycle 110 数据显示这种 case 少见。

---

## [Cycle 106 · 原描述 · 已 rejected by Cycle 110] F. Per-pile lossless fallback routing

**Idea**:对 DSSIM-infeasible fixture(任意 K 都过不去 tiny_dssim),不量化,直接 oxipng lossless re-encode → 至少 size 不退。

**Why now**:
- Cycle 106 6 张 DSSIM-infeasible 在 v1.2.8 production 已是 FAIL-SIZE(size 1.3-2.85× tiny);若 lossless 路径能拿到 ≤ tiny size,**size 通过 + DSSIM 自然过(lossless = 0)**

**Evidence**:
- 待 Cycle 107 测:对 6 张 fixture 跑 oxipng lossless,看 ratio

**下一步**:
- Cycle 107 内顺手测:oxipng max preset on 6 张 → 看 vs TinyPNG size ratio
- 若 ≤ 0.80× tiny → 入 production routing predicate(无 quality 风险,只检查 input 是否高频)

---

## [Cycle 106 · open · 可行性中 · ★★★★] G. Filter-chain entropy guided K(B 的 deep 版)

**Idea**:把 [B] 的现象 → 算法。每张图:
1. 跑廉价 pre-cluster(MiniBatchKMeans K=128 dry run)
2. 估算 palette_overhead(K) + filter_residual_entropy(K) ≈ f(K)
3. argmin f(K) 选 K

**Why now**:
- [B] 的 finding 自然指向这套 RD-optimal K 选择
- 比 [A] 的 decision tree 更可解释、更 paper-able

**Evidence**:
- Cycle 106 给出 PNG filter chain + palette 联合 size 行为的实测数据
- 文献无人做过(rate-distortion theory 通常假定 palette overhead 单调单减)

**下一步**:
- Cycle 108-110:数学建模 filter_residual_entropy 跟 palette spread 的关系
- ablation on Pile A 31 张

---

## [Cycle 107 · status:rejected · 可行性高 · ★] H. Single-config K↑ as production default

**Idea**:不做 input classifier,直接把 `nupic-quantize` default 从 K=128 改成 K=224 d=0.3(Cycle 106 Pile A 最频繁 winning slot)。

**为什么有人会想试**:Cycle 106 oracle 数据里 K=224 是 35% winner share — 看起来"平均最优"。如果能这样省 input classifier 工程,production wire 一行代码搞定。

**Cycle 107 实测结果(rejected)**:
- 100 张 stratified sample: 22/100 PASS,**原 PASS pile 退化 4/25 = 16%**
- 32 张 quick bench 复现:7/32 PASS,**原 PASS pile 退化 2/8 = 25%**
- Pile A 几乎零收益(0/25 / 1/8)— K=224 在 Pile A 尾部 tiny_dssim ≤ 0.002 那段不够紧
- Pile B/C 也几乎零收益

**Status**:**rejected** — 任何 production replace 都不能接受让现有 PASS pile 退步。
**Lesson kept**:per-image oracle 不能直接当 cohort-wide single-config 用,必须 input-aware routing(idea A)。
**Evidence**:`assets/png-bench/cycle107/single_config_sample.tsv`,`docs/research/png/04lll-cycle107-single-config-dead.md`

---

## 看板:Cycle 111+ 优先级建议(2026-06-18 Cycle 110 实测后更新)

| rank | 候选 | 状态变化 | 原因 |
|---:|---|---|---|
| 1 | **E(R6 multi-tile)** | ↑ 升 rank 1 | Cycle 110 数据明确:6 DSSIM-infeasible + 9 perf-locked Pile A = 15 fixture motivation;single-palette 范式已到 perf+quality 双天花板 |
| 2 | **preset=6 perf 优化**(rayon parallel oxipng)| 新加 | 解锁 Cycle 108 预测的 9 张 Pile A wins;Cycle 110 数据测出 preset=6 perf 死(p245 10s)是 ship blocker;parallel oxipng 可能解锁 |
| 3 | C(slow-tier `--effort 9`)| 保持 | 用户 opt-in,perf 不影响 default,价值有限但工程量小 |
| 4 | B(K-monotonicity 分析)| 保持 | 论文级最高,paper 主线 |
| 5 | G(filter-entropy guided K)| 保持 | [B] 升级版,paper-track |
| 6 | D(adaptive dither)| 保持 | 跟 [J] 2-pass 自然合并(已带 d=0.3),不独立做 |
| (已)| J(2-pass K-up fail-safe)| **SHIPPED v1.2.9** | 100% retention by construction,Pile A 真实 wins 2/307 production preset=5 |
| (已)| A(input-only K predictor)| **ceiling-hit by Cycle 108(99.1%)** | 被 [J] 取代 |
| (已)| F(lossless fallback)| **rejected by Cycle 110(0/6)** | 6 DSSIM-infeasible lossless 全在 1.36-1.95× tiny |
| (已)| H(single-config K↑ default)| **rejected by Cycle 107** | 留作 anti-pattern 记录 |
