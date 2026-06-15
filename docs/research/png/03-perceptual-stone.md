# 03 — Stone-layer 设计:perceptual-driven PNG quantizer

> Anchor 篇 for stone-layer architecture。后续 03a/03b/03c/... 子 essay
> 按这篇排的依赖图实做。**这篇不写代码,只设计 + ceiling 数字**。
>
> Triggered by [[feedback-ceiling-first-priorities]]:从这篇起,所有研究
> + 工程按 ceiling-first 推,优先级硬序 **perf > mem > disk > cov > doc**。

---

## 1. 出发点

[`02-perceptual-metrics.md`](02-perceptual-metrics.md) 上的两个 unambiguous 结论:

1. nupic 0.4.0 cement 路径(`imagequant default` + `oxipng`)在
   SSIMULACRA2 上已经 5/7 胜 TinyPNG。**cement-fix 0.4.1 收益最多
   0.95× tinypng,边际效应**。
2. 02-pluto 在 SSIMULACRA2 上也 ceiling(score -65,跨 8 q_target 跨度
   0.48 分)。**没有任何 cement 调节能脱离这个 ceiling**。

要让 02 之外的"灾难区图"(SSIMULACRA2 < 0 或 < 30)进入 high-quality
带(>70),必须换 quantizer 内核。换 quantizer 内核 = 进入 stone 层。
这是 [`roadmap.md`](../../roadmap.md) 8 阶段路线的 stage 3-5,
[`png-pipeline.md` §1 Layer A/B/C](../../png-pipeline.md) 的核心。

这一篇把那个长期路线的 stone 设计 nail down 到具体 ceiling 数字 +
依赖图 + 子 essay roadmap。

---

## 2. Ceiling-first 设计原则(本研究面的新基础)

每个 stone 设计必须给出:

1. **数学 / 物理 ceiling**(信息论上界 / memory bandwidth /
   Voronoi-optimal / Shannon entropy / Sneyers-2023 metric upper bound …)
2. **当前 cement / SOTA 距离 ceiling 多远**(数字,不是 hand-wave)
3. **perf ceiling**(time/pixel,从 hardware 物理上界推算)
4. **mem ceiling**(working set 上限,跟 image size 关系)
5. **disk ceiling**(对最终 PNG size 的贡献上界)
6. **cov ceiling**(test 形态 + property-based 覆盖范围)
7. **doc ceiling**(math + algorithm + 跟既有 docs 的 cross-link)

5 stones,统一表见 §8。

**优先级硬序**: perf > mem > disk > cov > doc。冲突时按此排;不冲突时
全做。**5% 改进不算工作,需要 ceiling 数字驱动 justify**。

测量平台:Apple M2 / arm64-darwin(开发机)+ x86_64-linux(CI 复测),
Rust release(`opt-level=3, lto=thin`)。

---

## 3. 5 个待 ship 的 stones

### Stone A — `nupic-color`(OKLab)

把 RGB / linear / OKLab 这条色彩管线建出来作为后续 stone 的输入。

**Math ceiling.** OKLab(Ottosson 2020)是 perceptual uniformity 公认 SOTA;
在 CIELab 失败的蓝紫区、暗部色阶、渐变非线性区有数学证明的 improvement
(Ottosson §4–§5)。**作为 metric 空间使用没有更上层**(未来的 ICtCp /
JzAzBz 改善幅度 < OKLab 跟 Lab 的差,且工程复杂度高)。

**Perf ceiling.** 算法 = 3×3 matrix mul + per-channel cube root + 3×3 matrix mul。
推算:
- naive Rust:6 mul + 3 cube-roots / pixel ≈ 50 cycles @ 2.5 GHz = **20 ns/pixel**
- SIMD (NEON 4-lane / AVX2 8-lane):4×–8× speedup → **2.5–5 ns/pixel**
- 物理 ceiling = memory bandwidth:12 byte read + 12 byte write = 24 byte/pixel;
  M2 ~100 GB/s peak → **0.24 ns/pixel streaming** → 02-pluto (400K px) 0.1 ms 纯 IO

对 02-pluto 全图(400K px,1.6 MB 输入)总时间:
| | 时间 | vs ceiling |
|---|---:|---:|
| naive | 8 ms | 80× |
| SIMD | 1–2 ms | 8× |
| bandwidth ceiling | 0.1 ms | 1× |

target:**SIMD 实现,~2 ms / 02-pluto**(off by 20× from bandwidth ceiling,
acceptable for first stone)。

**Mem ceiling.** OKLab 用 f32 3 通道 = 12 B/pixel,vs RGBA8 4 B/pixel,**3×**
膨胀。
- 02-pluto:1.6 MB → 4.8 MB,小问题
- 4K(3840×2160,8M px):31 MB → 96 MB,**超 L2 / L3,要 streaming**
- tile-based 实现:64×64 tile = 48 KB → L2 友好

target:**tile-based**,working set per tile ≤ 64 KB。

**Disk impact.** 间接 — 通过 stone C 转化为更准的 palette。无直接 size
贡献。

**Cov.**
- Property:RGB → OKLab → RGB roundtrip within `1e-5` per channel
- Property:OKLab L axis monotonic in luminance(Ottosson §3)
- Cross-test:对照 `colour-science`(Python reference)5+ fixture
- Reference:`palette` crate / `oklab` crate 在 crates.io 上;我们的 stone
  output 必须 1:1 match within float epsilon

target:~50 property + 5 fixture,**测契约不测内部 SIMD lane 选择**(
[[feedback-not-rotting-tests]])。

**Doc.**
- Math:Ottosson 2020 §3 matrix coefficients,reproduce 完整推导
- Code:跟 [`png-pipeline.md` §1 Layer A](../../png-pipeline.md) 互引
- Sub-essay:`03a-oklab-design.md`

**实施成本估计**:**1–2 周**(arm/x86 双 SIMD 路径)

---

### Stone B — `nupic-ssimulacra`(self-built SSIMULACRA2)

self-built metric — 接管 02 essay 在 cement 里用的 `ssimulacra2` v0.5.1
crate。

**Math ceiling.** SSIMULACRA2 = MS-SSIM(Wang et al. 2003)+ 2 asymmetric
error maps + L_p aggregation(Sneyers et al. 2023)。**跟人眼实验数据
correlation 是当前 SOTA**(JPEG XL CFP test set:Spearman ρ ~0.93)。
Butteraugli ρ ~0.85;PSNR ρ ~0.50。

**Perf ceiling.**
- Cement baseline:`metric_sweep` 实测 02-pluto 单次 SSIMULACRA2 ~100 ms
  (推算自 total 217 ms - quantize ~80 ms - oxipng ~30 ms)
- 算法:5-octave Gaussian pyramid + 多个 MS-SSIM map + 2 asymmetric maps
- Per pixel per scale:~50 mul/add
- 02-pluto:400K px × 50 ops × ~5 scales = ~100M ops + 5 MB bandwidth
- SIMD ceiling:8-lane → 100M / 8 = 12.5M cycles @ 2.5 GHz = **5 ms**
- Memory bandwidth ceiling:5 MB × 5 scales / 100 GB/s = **0.25 ms 纯 streaming**

| | 时间 | vs ceiling |
|---|---:|---:|
| cement (rust-av/ssimulacra2) | ~100 ms | 400× |
| stone target (SIMD) | ~5–10 ms | 20× |
| bandwidth ceiling | 0.25 ms | 1× |

target:**自研 stone ≤ 10 ms / 02-pluto**(10× faster than cement),仍距
bandwidth ceiling 40× — calibration 留给 03b sub-essay。

**Mem ceiling.**
- 5-octave pyramid:1 + 1/4 + 1/16 + 1/64 + 1/256 ≈ **1.33× source**
- 两张图(reference + distorted):2.66× source ≈ **4.3 MB per 02-pluto pair**
- accumulator buffers:~smallest-scale size ~16 KB
- target:**streaming pyramid build** = working set per scale ≤ 64 KB

**Disk impact.** 间接 — drive stone C(differentiable codebook)的 loss。

**Cov.**
- Property:identical-image score = perfect calibration value(cement crate
  对 black==black 给定值)
- Cross-test:跟 cement v0.5.1 在 JPEG XL CFP test set 上,score 误差
  < 0.5 分(SSIMULACRA2 自然 noise 上限,Sneyers §6.2)
- Reference fixtures:10–20 cjxl reference images with calibration scores
  30 / 50 / 70 / 90
- Tests:~30 property + 20 reference

**Doc.**
- Math:Sneyers 2023 §3-§4 完整推导(MS-SSIM weights + asymmetric maps)
- Code:跟 stage 4 [`roadmap.md`](../../roadmap.md) 互引
- Sub-essay:`03b-ssimulacra2-design.md`(含 cement→stone migration plan)

**实施成本估计**:**3–4 周**

---

### Stone C — `nupic-quantize`(SSIMULACRA2-driven differentiable codebook)

这一步是 02-pluto 那条 algorithmic ceiling 的破除。所有前序 stone 为
此服务。

**Math ceiling.**
- Voronoi-optimal palette w.r.t. SSIMULACRA2 是 **NP-hard**(色彩量化对
  L_2 metric 都 NP-hard, perceptual metric 更严)
- 当前 SOTA approximation:differentiable codebook + Gumbel-softmax
  + straight-through estimator(STE)+ Adam optim
- 跟 NP-hard 真上界距离没有解析估计(只能跑大数据集 + 暴力小图对照)

**Perf ceiling.**
- Training(per image):1000-iter Adam on K=256 palette in OKLab 空间
- Per iter:
  - forward:每像素 K-NN soft-assignment ≈ N × K × 3 mul = 02-pluto 上 300M mul
  - backward:类似 cost
  - SSIMULACRA2 forward + backward:依赖 stone B,~10–20 ms × 2 (with autograd)
- Per iter:~50 ms with SIMD
- 1000 iter:**~50 s per 02-pluto** 训练
- Inference(用预训练 palette 量化新图):
  - per pixel K-NN argmin = N × K × 3 mul = 300M ops on 02-pluto
  - SIMD:~30 ms
- Compare cement(`imagequant`):quantize ~80 ms on 02-pluto

|  | 时间 | vs ceiling |
|---|---:|---:|
| training(stone naive)| 50 s | 50× (vs SIMD ceiling 1s)|
| training(stone SIMD)| 5–10 s | 5× |
| inference(stone)| ~30 ms | comparable to cement |
| Voronoi-opt(NP-hard)| ∞ | n/a |

target:**training < 10 s per image, inference < 50 ms**。training cost 是
"per-image one-time" — 长期可以 cache 训练好的 palette 给类似图复用,降
到 ~0 amortized。

**Mem ceiling.**
- naive soft-assignment buf:`n_px × K × f32` = 400K × 256 × 4 = **400 MB**
  for 02-pluto — infeasible
- **mandatory**:tile-based or streaming
- tile = 1024 px × K × f32 = 1 MB per tile,small enough
- Adam state(K × 3 × {param, m1, v1}):~12 KB
- Stone B(SSIMULACRA2)5 MB working set
- Per-iter total working set:~50 MB for 02-pluto

target:**tile-based with 50 MB working-set ceiling**。fail loud 当 4K 图
预估超 200 MB 时,要求用户给 tile size 旋钮。

**Disk impact.**
- 这是终极 stone:**02-pluto 从 SSIMULACRA2 -65 → 30+** 的关键
- 同时 size 应保持(palette quantization 体积是 stone C 决定的)
- 跨 7 张图预估:nupic 0.4.0 已经 0.92× tinypng / SSIMULACRA2 5/7 胜;
  stone C 后预估 ~0.85× tinypng / 7/7 胜 SSIMULACRA2

**Cov.**
- Property:training loss monotonic decreasing
- Property:trained palette ≥ imagequant baseline on SSIMULACRA2(≥ 5 分
  improvement on 02-pluto)
- Cross-test:对照 imagequant baseline 在 10+ fixture set
- Stability test:相同 image + 不同 random init 跑 10 次,SSIMULACRA2 std
  < 1 分(确保 training 收敛性 reproducible)

**Doc.**
- Math:可微分量化推导(soft-assignment temperature schedule + STE)
- Code:跟 stage 5 [`roadmap.md`](../../roadmap.md) 互引
- Sub-essay:`03c-codebook-design.md`(最长的一篇,含 training loop +
  hyperparameter calibration)

**实施成本估计**:**6–8 周**(高复杂度,要 Adam 实现 + float
precision 处理 + numerical stability)

---

### Stone D — `nupic-dither`(blue-noise)

[`png-pipeline.md` §1 Layer B](../../png-pipeline.md) 已经 spec 过。
这里给 ceiling 数字。

**Math ceiling.** Void-and-cluster blue-noise mask(Ulichney 1993)。**比
Floyd-Steinberg / Riemersma 在视觉上有 paper 数据支撑的 improvement**(高频
均匀分布;无扫描线 artefact)。跟 stone C 联合可以学习式 dither,但 V&C 已
经是 hand-tuned ceiling。

**Perf ceiling.**
- Mask 预生成(one-time):几秒
- Apply per pixel:~10 cycles = **4 ns/pixel**
- 02-pluto:**1.6 ms** apply

**Mem ceiling.**
- Mask:64 × 64 × f32 = 16 KB(pre-generated, stored)
- Apply in-place:0 extra
- Working set:< 32 KB total — L1 友好

**Disk impact.** 视觉提升为主,size 0–1%(po png-pipeline 估计)。
blue-noise 频谱对 DEFLATE 略不友好(高频)— size 可能略涨,但视觉提升 trade-off。

**Cov.**
- Property:mask spectrum is blue(高频能量 > 低频)
- Property:dithered SSIMULACRA2 ≥ FS-dithered SSIMULACRA2(在 stone B 上验证)
- Cross-test:跟现有 cement(`image` crate's FS dither)对比 SSIMULACRA2

**Doc.**
- Math:Ulichney 1993 algorithm reproduce
- Sub-essay:`03d-dither-design.md`

**实施成本估计**:**~1 周**(算法成熟)

---

### Stone E — `nupic-filter-search`(filter beam search)

[`png-pipeline.md` §2](../../png-pipeline.md) 已经 spec。1-3% IDAT 收益。

**Math ceiling.**
- 全局最优 = NP-hard(filter 选择跟 LZ77 dictionary state 耦合,
  5^N 组合)
- Beam search + entropy estimator = practical SOTA approximation
- Distance to NP-hard upper bound:0.5–1.5%(经验估计)

**Perf ceiling.**
- Beam width 16 × N rows × 5 filters × per-state entropy estimate O(W)
- 02-pluto 632 rows × 16 × 5 × 632 = 30M ops main loop
- Plus entropy estimation ~10× → 300M ops
- SIMD ceiling:~50 ms / 02-pluto
- Compare:oxipng `--filters all` greedy ~50 ms 同
- **Filter beam search 不应该比 greedy 慢**(同 order)— ceiling 平价

**Mem ceiling.**
- Beam states:16 × row buffer ~16 × 2528 = 40 KB
- LZ77 fingerprint per beam:几 KB
- Total working set:< 100 KB → L2 友好

**Disk impact.** **1–3% IDAT**(per png-pipeline 估计)。在 stone C 之后
应用 — 因为 indexed pixel stream 才是被 filter 的对象。

**Cov.**
- Property:filter-search output size ≤ oxipng greedy on all fixtures
- Property:decoded pixels identical(filter selection is lossless)
- Cross-test:对照 oxipng 在 100 张 fixture 上 size + decode

**Doc.**
- Math:LZ77-entropy-aware filter selection 推导
- Code:跟 [`roadmap.md` 阶段 7](../../roadmap.md) 互引
- Sub-essay:`03e-filter-search-design.md`

**实施成本估计**:**4–6 周**

---

## 4. Dependency 图 + ship 顺序

```
              ┌─────────────────────┐
              │   Stone A (OKLab)   │
              └──────────┬──────────┘
                         │
              ┌──────────▼──────────┐
              │  Stone B (SSIM2)    │
              └──────────┬──────────┘
                         │
              ┌──────────▼──────────┐
              │  Stone C (Codebook) │
              └──────────┬──────────┘
                         │
                ┌────────┴────────┐
                │                 │
       ┌────────▼─────┐  ┌────────▼──────┐
       │ Stone D      │  │ Stone E       │
       │ (Dither)     │  │ (Filter)      │
       └──────────────┘  └───────────────┘

  (D 和 E 可并行,但都依赖 C 的 indexed pixel stream)
```

**ship 顺序**:A → B → C → (D + E 并行)。

C 是 ceiling-breaker(02-pluto 真正脱困);D 和 E 是 polish layer。

---

## 5. Stone-by-stone Ceiling 总表

| Stone | perf ceiling (02-pluto) | mem ceiling (02-pluto) | disk impact (cross-set) | cov | impl cost |
|---|---|---|---|---|---|
| A. OKLab | 2 ms (vs bw 0.1 ms = 20×) | 4.8 MB | 间接 | ~50 prop | 1–2 周 |
| B. SSIMULACRA2 | 10 ms (vs bw 0.25 ms = 40×) | 4.3 MB (2 imgs pyramid) | 间接 | ~30 prop + 20 ref | 3–4 周 |
| C. Codebook | training 5–10 s, inf 30 ms | 50 MB working / 1 MB tile | **02 disaster → high quality;全集 0.92×→0.85× tinypng** | ~20 prop + stability | 6–8 周 |
| D. Dither | 1.6 ms apply | < 32 KB | -1 ~ +1% size | ~10 prop | 1 周 |
| E. Filter search | 50 ms (≈ greedy) | < 100 KB | -1 ~ -3% | ~10 prop | 4–6 周 |

**全部 ship 后预估 02-pluto SSIMULACRA2 数字**:从 -65 → 30+(从 disaster
进入 low quality 带)。这是 stone roadmap **首次给出 02-pluto 视觉脱困
路径**。

**全集 nupic / tinypng**:size 0.92× → 0.85×;SSIMULACRA2 5/7 胜 → 7/7
胜(包括 02 + 04 翻盘)。

---

## 6. 跟 cement 的关系 / migration plan

cement 不退役。stone 落地时:

1. Stone 接到 `nupic-research/examples/*.rs` 验证 ceiling 数字 ≥ 估计
2. 跟 cement 跨 fixture 跑对照,确认 stone 不输 cement(at least in 95%
   of cases)
3. Stone graduates to `crates/nupic-<name>/`,有自己 contract test +
   bench
4. `nupic-core` 通过 feature flag 暴露 stone:`compress` op 通过
   `Quality::Auto` 默认仍走 cement,新加 `Quality::Stone` 路径走 stone
5. **cement 永久保留**,作为 stone 落地的 regression baseline + fallback

这跟 [`feedback_bump_version_each_update.md`] 的 cement/stone 分离咬合:
**stone 上线是 minor bump**(新功能),**cement 不动**(行为不变)。

---

## 7. ceiling-first 在工程流程中的体现

每个 stone PR / commit 要带:

- [ ] ceiling 数字(本 essay §3 各 stone 估计的 perf / mem / disk)
- [ ] 当前实测距离 ceiling 多少倍 / 多少 ms / 多少 MB
- [ ] 不达 ceiling 80% 的(perf / mem / disk)— 在 commit msg 给 next-step
- [ ] cov ≥ stone 表里的最低 prop / ref 数
- [ ] doc:跟既有 docs cross-link

没满足 ceiling table 的 stone 不能 graduate from `nupic-research/` 到
`crates/nupic-<name>/`。

---

## 8. Sub-essay roadmap

按 dependency 顺序:

| seq | sub-essay | 内容 | 状态 |
|---|---|---|---|
| 03a | `03a-oklab-design.md` | Stone A detail:matrix, transfer, SIMD 路径,benchmarks | 待写 |
| 03b | `03b-ssimulacra2-design.md` | Stone B detail:MS-SSIM 谱系,asymmetric maps,migration from cement crate | 待写 |
| 03c | `03c-codebook-design.md` | Stone C detail:differentiable codebook + Adam + tile-based,validation 方法 | 待写 |
| 03d | `03d-dither-design.md` | Stone D detail:void-and-cluster algorithm | 待写 |
| 03e | `03e-filter-search-design.md` | Stone E detail:beam search + LZ77-entropy estimator | 待写 |

下一篇默认是 **03a-oklab-design.md**(依赖图最底层)。但 perf / mem
ceiling 推算需要小型实验(nupic-research/examples/oklab_bench.rs)— 那
篇 essay 会同时落 design + 实测 baseline。

---

## 9. Open questions

1. **TinyPNG 在 stone 上跑到哪了**?目前我们只有 SSIMULACRA2 / DSSIM 数字
   能 reverse-engineer,看不见它们的算法。假设它们也是 cement 级 +
   私有 heuristics,可能没有进入 stone 层。如果是这样,这条 stone 路线
   产出的 codec **第一次有机会真正超越 TinyPNG**,而不是 catch up
2. **D 是否能跟 C 联合训练**(可微分 codebook + 可微分 dither pattern)?
   per png-pipeline.md §1 Layer C 已经 hint。值得做 → 进 03c sub-essay
3. **GPU 加速对 C 的必要性**?上面估计 SIMD 已经够(5–10 s training);
   GPU 主要给 4K+ 大图。短期不必,留作 future 优化
4. **stone-pipeline 跨平台一致性**(arm/x86/win):differentiable 训练
   收敛点可能跨平台略不同(浮点 NaN/Inf 处理 / SIMD 数值精度)。需要
   property 测的"approximate equal"边界

---

## 10. 验收材料

- 触发本篇的 feedback:[`feedback_ceiling_first_priorities.md`](../../../../../.claude-profile-1/projects/-Users-doracawl-workspace-labs-lab29-nupic/memory/feedback_ceiling_first_priorities.md)
- 数学 anchor:[`docs/png-pipeline.md` §1](../../png-pipeline.md)、
  [`docs/roadmap.md`](../../roadmap.md)、
  [`docs/references.md`](../../references.md)
- 经验数据:[`target/research-out/02-metric-sweep.csv`](../../../target/research-out/) — encode_ms / SSIMULACRA2 baseline
- 引用:
  - [Björn Ottosson, *A perceptual color space for image processing* (2020)](https://bottosson.github.io/posts/oklab/)
  - [Sneyers et al., *SSIMULACRA 2* (2023)](https://github.com/cloudinary/ssimulacra2)
  - [Wang et al., *Multiscale Structural Similarity for Image Quality Assessment* (2003)](https://ieeexplore.ieee.org/document/1292216)
  - [Ulichney, *The Void-and-Cluster Method for Dither Array Generation* (1993)](https://cv.ulichney.com/papers/1993-void-cluster.pdf)
  - [Adam optim, Kingma & Ba 2014](https://arxiv.org/abs/1412.6980)
