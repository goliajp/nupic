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

## [Cycle 106 · open · 可行性高 · ★★] A. Content-aware K predictor

**Idea**:基于 input image features(palette pre-cluster spread / luma gradient entropy / opaque fraction / chroma variance)训一个 decision tree 选 optimal K ∈ {96, 128, 160, 192, 224, 256}。

**Why now**:
- Cycle 106 数据(`pile_a_grid.tsv`)给出 31 fixture × 21 config 的 oracle ground truth,直接作训练集
- 赢家 K 分布跟 tiny_dssim 强相关,但 production 看不到 tiny_dssim,需 input-only surrogate feature
- 简单 decision tree / piecewise linear 就可能 ship,不需要 ML 框架

**Evidence**:
- Pile A 23 winners 中 K=224 占 8/23(35%),K=192 占 5/23(22%)— 即使**朴素永远选 K=224 也可能拿到 ~50% Pile A oracle gain**
- 在 baseline-7 上要避免回退(05 mountain 已经微输 0.001,经不起再退)

**下一步**:
- Cycle 108-pre:写 input feature extractor + decision tree fitter,用 Pile A oracle ground truth 训练,leave-one-out cross-validation
- 在非 Pile A 的 corpus-500 fixture 上验证 routing 不退步

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

## [Cycle 106 · open · 可行性高 · ★] F. Per-pile lossless fallback routing

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

## 看板:Cycle 107+ 优先级建议

| rank | 候选 | 原因 |
|---:|---|---|
| 1 | A(content-aware K predictor) | 立刻能 ship,把 oracle 25.5% 投影到 production routing(YELLOW → GREEN 的最近路径) |
| 2 | F(lossless fallback) | 工程量小,可能解锁 6 张 DSSIM-infeasible(若可行 → 直接进 GREEN gate) |
| 3 | B(K-monotonicity 分析) | 论文级最高,paper 主线;但不是 ship 路径 |
| 4 | E(R6 multi-tile) | ★★★★★ 远景但工程量大,等 Cycle 107 Pile B/C 分类后再决定是否进入 |
| 5 | G(filter-entropy guided K) | [B] 升级版,paper-track |
| 6 | C(slow-tier zopfli) | 易做 ship 路径,但只覆盖 edge case,价值低 |
| 7 | D(adaptive dither) | 跟 [A] 自然合并,不独立做 |
