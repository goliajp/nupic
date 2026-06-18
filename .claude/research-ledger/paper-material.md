# 论文素材累积

跨 cycle 追加。每条带:
- **★ 等级**(★ 短 note → ★★ workshop → ★★★ 小 venue → ★★★★ 主流 venue → ★★★★★ top-tier)
- **来源 cycle**
- **核心 claim**
- **evidence 数据资产路径**
- **可能投稿目的地**
- **风险 / 待补**

---

## [Cycle 112 · ★★★★] PNG 256-palette container caps R6 spatial-aware quantization commercial realizability

**Claim**:Cycle 111 R6 8×8 K=192 reconstruction 在 6 张 fixture 上 PASS DSSIM 6/6(margin -0.00072 to -0.00825)。但 R6 reconstruction 总 effective unique colors = 64 tile × 192 = **12288**,远超 PNG indexed palette ceiling **256**。**Cycle 112 测试 R6 → K=256 imagequant re-quantize hybrid:size 6/6 PASS(0.46-0.55× tiny,比 v1.2.8 lossless 紧 2-4×),但 strict DSSIM 0/6 PASS(margin +0.00013 to +0.00496,4 张在视觉不可分辨区)**。Re-quantize loss (1.4-7.4× R6 DSSIM margin)吃光 R6 headroom。

**机制**:
- R6 突破 DSSIM ceiling 靠 spatial-aware color diversity(12288 effective colors distributed by tile)
- Single-palette PNG container 强制 merge 12288 → 256 cluster pool
- merge loss > R6 headroom → strict-gate fail
- 视觉上 perceptually-invisible(p167 sampled output 跟 TinyPNG visually equivalent)

**这是 paper 的关键 negative finding 转 positive motivation**:
- 强化 Cycle 111 R6 ★★★★★ kernel:"算法可行,但 container 是 bottleneck"
- 暗示 ship-able R6 必须 leave standard PNG container(.nupic tile-aware format / WebP-AVIF transcoder)
- 或者放宽 gate semantics 到 "visually indistinguishable" 而不是 strict DSSIM ≤

**Evidence**:
- `assets/png-bench/cycle112/path_b{,_v2}.tsv` — 6 张 fixture × R6+K256 hybrid pipeline(d=0.3 + d=0)
- `assets/png-bench/cycle112/*.png` — 6 张 hybrid output PNG visual eye(p167 macbook + table 砖墙 clean)
- `assets/png-bench/cycle111/r6_probe_v2.tsv` — R6-only DSSIM baseline 对比

**论文化价值**:
- 强化 Cycle 111 R6 paper 的 commercial motivation(R6 不仅算法可行,还需 container 改造)
- Cycle 106-112 完整 arc 6 cycle 全部 evidence

**目的地**:同 Cycle 111 R6 paper(DCC / IEEE TIP)合并

**风险 / 待补**:
- 视觉等价 vs strict metric 的 gate semantics 是 user-side 决定;paper 应该呈现 both
- Path A `.nupic` container 工程未做,paper 可能需要 prototype 才有完整 production realizability 论据

---

## [Cycle 111 · ★★★★★] Spatial-aware quantization breaks the single-global-palette DSSIM ceiling

**Claim**:Cycle 106-110 established that 6 fixtures(p115, p125, p167, p175, p214, p274 — all Picsum HD photo)are **infeasible under any global palette K ∈ {64..256} × dither × lossless 范式**(DSSIM > tiny_dssim 一律 fail)。**8×8 tile × K=192 per-tile imagequant 在 6/6 fixture 上 PASS DSSIM(margins -0.00072 to -0.00825 — comfortable visual-indistinguishable headroom)**。

**机制**:single-global-palette 必须用一组 256 个 OKLab cluster 覆盖整图所有 chromatic region。当图跨多 region(海 + 沙 + 天 + 植物 + 阴影),256 cluster 必有 region 欠拟合。Per-tile palette **allow 不同 region 独立 cluster**,总等价 64 × 192 = 12288 cluster pool spatially distributed。

**Evidence**(三 cycle progression):
- `assets/png-bench/cycle106-r4/pile_a_grid.tsv` — Cycle 106 oracle K∈{64..256}×d∈{0,0.3,0.6} 6 张 fixture 全 fail DSSIM
- `assets/png-bench/cycle110/full_verify_v3.tsv` + Cycle 110 lossless probe — 同 6 张 lossless ratio 1.36-1.95× tiny
- `assets/png-bench/cycle111/r6_probe_v2.tsv` — 8×8 K=192 PASS 6/6,unanimous winning config

**论文化价值**:
- Cycle 106 + 107 + 108 + 109 + 110 + 111 是连续 6-cycle 完整 paper 主线
- 三 finding:K-monotonicity break(Cycle 106)+ cohort routing methodology(Cycle 107-108-109)+ spatial-aware ceiling break(Cycle 111)
- 第三 finding 是 ★★★★★ kernel — 第一个 cohort-level external-reference 驱动的 spatial quantization motivation paper

**目的地**:
- **DCC full paper / IEEE TIP main paper / ICIP**(三 finding 合并)
- 第三 finding 单独 sufficient for short paper at PCS / IS&T Imaging

**风险 / 待补**:
- Cycle 111 仅测 reconstruction DSSIM,**没测 encoder size** — 64 tile × 192 color = 12288 unique 超 PNG palette 256 ceiling 48×,production 不可 ship 标准 PNG container
- Cycle 112 Path B(R6 → K=256 re-quantize hybrid)将测量 R6 优势能否 survive 到 single-palette 输出 — 这是 production-realizable 路径的核心问题
- 真 R6 production wiring 需要 tile-aware container(.nupic file format),工程量大

---

## [Cycle 108 · ★★★] Input-only features hit a ceiling on cohort routing — true discriminator requires 2-pass

**Claim**:对 photo PNG quantize 的 K 选择问题,**任何 input-only feature(n_pixels / bits-per-pixel / image entropy / luma / chroma)都无法 cleanly 区分"K=224 救得了"vs"K=224 救不了"**。真正区分器是 baseline output size(production-side 需 2-pass routing)。

**Evidence**:
- Cycle 108 rule v3 在全 corpus-500 上 PASS pile 99.1% retention(105/106),**仅 1 张退化 p244**
- p244 vs 11 张 wins 在 bpp_in (5.20 vs 1.62-5.14)、bpp_v128 (1.48 vs 0.97-3.63)、n_pixels (9.83 MP 同) 等 input-only feature 上**全部重叠 / 无法 clean separation**
- 真正 discriminator: p244 的 v1.2.8 baseline output ratio 0.791× ≤ 0.80 cap(已 PASS,不需要救),而 11 张 wins 都 > 1.0×(必须救)

**机制**:
- Cycle 107 已证明 "K=224 single config" 让 16-25% PASS pile 退化
- Cycle 108 尝试用 input-feature classifier 缩小退化(99.1% achievable)
- 但 **input-feature 路径有结构性 ceiling**(p244-class fixture 跟 wins 在 input-feature 空间不可分)
- 2-pass routing(先 K=128 看 size,再决定升 K)是已知唯一 100% retention 路径
- 这是 RD theory 里 "rate-distortion 函数依赖 source distribution" 的实证 —— 单 image features 无法替代 measured RD curve

**论文化价值**:
- 跟 [Cycle 107 "Per-image RD optimum doesn't transfer"] 那条形成**双胞胎 finding** —— Cycle 107 证明 "single config 不行"、Cycle 108 证明 "input-feature classifier 也不够"
- 一起组成 "**cohort routing 必须 measured(2-pass)而非 predicted(features-only)**" 的核心论证
- 跟 cohort headroom methodology paper 合并,作其第三章实验

**目的地**:
- DCC / IEEE TIP methodology paper(跟 Cycle 106 + Cycle 107 一起)
- 升级为 paper 主体之一

**风险 / 待补**:
- 需要 Cycle 109 2-pass production wiring 真做出来,在 corpus-500 全测拿到 100% retention 数据才能立住 claim
- p244 还可能有更隐微的 input feature(如 chroma covariance, FFT spectral content)能区分 —— 没全部尝试。但即使能,工程复杂度肯定不如 2-pass 简洁

---

## [Cycle 107 · ★★★] Per-image RD optimum doesn't transfer to cohort routing

**Claim**:Cycle 106 Pile A oracle 选出的"中心赢家 config"(K=224 d=0.3 p=6,7/23 winners)**当作 cohort-wide production default 反而让原 PASS pile 退化 16-25%**。Per-fixture oracle 跟 production-side single-config routing 之间存在结构性 gap — 必须有 input-feature classifier 才能落地。

**机制**:四个 Pile 对 K 有相互矛盾的偏好:
- PASS pile(mi/synthetic/small):K=128 已够,K=224 增加 palette overhead 无视觉收益
- Pile A head(tiny_dssim ≥ 0.005):K=224 大胜
- Pile A tail(tiny_dssim ≤ 0.002):K=224 不够紧
- Pile B(size pass,DSSIM 微退):K=224 让 size 越界
- Pile C(双轴微退):K=224 同时让 size 越界 + DSSIM 仍不够

**Evidence**:
- `assets/png-bench/cycle107/pile_classification.tsv` — corpus-500 506 张二轴分类
- `assets/png-bench/cycle107/single_config_sample.tsv` — 100 张 stratified sample,K=224 d=0.3 → PASS 22/100(其中 PASS pile 退化 4/25,Pile B/C 几乎无收益)
- 32-quick-bench 复现同趋势(PASS 退 2/8 = 25%)

**论文化价值**:
- 是 Cycle 106 "Cohort headroom-mapped Pareto methodology" 那篇的**第二章实验** — 给"为什么需要 per-pile routing 而不是 single oracle config"提供数据
- 也是 "Per-image RD curve vs cohort gate" 的 negative-result short paper

**目的地**:
- 跟 Cycle 106 methodology paper 合并(同 venue,DCC / IEEE TIP)
- Standalone short note 不够独立,但作 cohort routing 设计 protocol 的 case study 极有价值

**风险 / 待补**:
- 需要 Cycle 108 input-feature classifier 真实验证"per-image routing 能反超 single config"才能立住这条 finding
- 若 Cycle 108 classifier 也救不了 PASS pile,这条 finding 升级为"routing 范式 fundamental ceiling"(更尖锐的 paper)

---

## [Cycle 106 · ★★★] Palette-size monotonicity break in indexed PNG

**Claim**:对 photo-class PNG 内容,quantize 时 **K=192-256 经常生成比 K=128 更小的文件**(palette overhead 反而被 filter-chain entropy 收益超过)— 跟 "more palette → larger file" 的朴素直觉相反。

**Evidence**:
- `assets/png-bench/cycle106-r4/pile_a_grid.tsv` — Pile A 31 fixture × K∈{64,96,128,160,192,224,256} × d∈{0,0.3,0.6} = 651 行 per-config (size_B, dssim) 数据
- Pile A winners 中心:K=224 + d=0.3 + p=6,5 张 K=256 winner、8 张 K=224、5 张 K=192
- 23 winners cohort 总 size 0.59× TinyPNG(竞品基线)

**机制假说**(待论文化):
- K=128 → palette 覆盖不足 photo gradient → 量化残差变 spatial high-freq artifact → PNG row filter(Paeth/Up/Sub)预测残差大 → DEFLATE 后字节多
- K=192-256 → palette 充分 → 量化残差小 → 像素邻域差分小 → filter 残差低熵 → DEFLATE 压紧
- 净效应:**额外 palette overhead < filter chain 字节收益**

**目的地**:
- PCS(Picture Coding Symposium)short paper
- ACM TOG short note
- DCC(Data Compression Conference)poster

**风险 / 待补**:
- 需要在 Pile A 之外的 fixture 验证 K-non-monotonic 普适性(目前 Pile A 是"size 浪费"特定子集,可能有偏差)
- 需要 mechanism ablation:把 PNG filter chain 关掉看 K 单调性是否恢复(目前是假说,不是证明)
- Cycle 107 给的 Pile B/C oracle headroom 数据将提供 broader 验证

---

## [Cycle 106 · ★★★★] Cohort headroom-mapped Pareto methodology

**Claim**:提出"cohort-level oracle PASS rate Pareto sweep"作为压缩 codec 路由表设计的**新方法论** — 不是新算法,是新 **protocol**。

**核心 protocol**(从 Cycle 102 - 106 演化出来):
1. 对 corpus 按 (size_axis × quality_axis) 二轴分类:PASS / Pile-A(quality-win, size-loss)/ Pile-B(size-win, quality-loss)/ Pile-C(both-loss)
2. 对每个 Pile 跑 per-fixture oracle sweep(K × d × p × filter × … )找最优可达 PASS 点
3. 把"oracle PASS 上限"投影回 cohort 得到 routing-table 设计的**理论天花板**
4. 用此天花板决定:
   - 写 production routing predicate(GREEN)
   - R4 微调(YELLOW)
   - 切换范式 R6 / R3(RED)

**跟现有 work 区别**:
- 不是 single-image RD curve(经典 Shoham-Gersho 1988 系列),是 **cohort 级 oracle PASS rate**
- 用 **production binary 实际输出** + **第三方竞品 baseline**(TinyPNG)做 gate,不用合成数据
- 是 **codec design protocol**,不是新 codec

**Evidence**:
- 5 个 cycle 累积数据:Cycle 102-106 完整 protocol 演化记录(`docs/research/png/04ggg` … `04kkk`)
- `assets/png-bench/corpus-500-*.tsv` — 506 fixture × 多 metric 数据集
- Cycle 106 是首个完整跑通"二轴分类 → Pile oracle sweep → cohort 投影 → 决策"的 cycle

**目的地**:
- DCC full paper
- IEEE TIP methodology note
- ACM Compression Symposium

**风险 / 待补**:
- Cycle 107 必须完成 Pile B/C 同款 sweep,否则 methodology 只有一个 Pile 的应用证据(weak)
- 需要在非 PNG codec 上 reproduce(JPEG / WebP / AVIF)证明 protocol 通用性,这是 Cycle 200+ 远景

---

## [Cycle 106 · ★★] DSSIM 主指标 + SSIMULACRA2 alpha-floor short note

**Claim**:SSIMULACRA2 在 transparent fixture(alpha 通道有内容)上出现 −492 floor 现象,不能作单一质量 gate;DSSIM 才是 PNG quantize bench 的可信主指标。

**Evidence**:
- `assets/png-bench/corpus-500-dssim.tsv` vs `corpus-500-ssim.tsv` 数据对比
- 03-wikipedia-logo(透明背景)SSIMULACRA2 = −63.72(floor),但 DSSIM = 0.0006(实际质量极高)— 反例校验
- TinyPNG 在 01/02/03 透明 fixture 上 DSSIM 0.22 / 0.018 / 0.13,实际肉眼可见的损失,但 SSIMULACRA2 floor 显示不出

**目的地**:
- VCIP short paper
- 短篇 IEEE Signal Processing Letters note(很可能 reject 因为太 incremental,但数据可作 reproducibility appendix)

**风险 / 待补**:
- 这只是 PNG quantize 场景的 metric reliability finding,可能 SSIMULACRA2 paper 原作者已经知道 alpha-handling 限制;需要 lit-review 确认 novelty

---

## [Cycle 108+ ★★★★★ kernel 远景] Spatially-aware quantization for hard-DSSIM-infeasible images

**Claim**(Cycle 106 给出的 motivation 数据,待 Cycle 108+ 验证):
- Pile A 中 6 张 fixture(p125 p274 p214 p115 p175 p167)**任何全局 palette(K ≤ 256)都无法达到 tiny_dssim**
- 这些都是高频 Picsum photo,内容跨多 chromatic region(海 + 沙 + 天 + 植物 + …)
- **single-global-palette 范式有理论上界**,需 multi-tile / VQ-VAE-style spatially-adaptive quantization 才能突破

**Evidence(已有)**:
- `assets/png-bench/cycle106-r4/pile_a_grid.tsv` 中 6 张 fixture 在 K=64..256 × d=0..0.6 共 21 个配置全部 DSSIM > tiny_dssim
- 这些 fixture 的 input 尺寸(Picsum HD 1920x1080 / 2400x1600 / 3840x2560)和高频内容跟 R6 multi-tile 文献的典型 motivation 一致

**目的地**(远景):
- ICIP / VCIP full paper
- IEEE TIP main paper
- 若 Cycle 110+ R6/R3 spike 真做出 70%+ cohort PASS,可冲 ★★★★★ top venue(SIGGRAPH / CVPR / ICCV — 看 quantization 那块的 paper 接收偏好)

**风险 / 待补**:
- 不是 Cycle 106 直接产物,需 Cycle 108+ R6 spike 验证假说
- 即使 spatially-aware 也未必能拿下全部 6 张(可能有些就是 quantization-infeasible,只能 lossless fallback)
- VQ-VAE 在 PNG 上的实用性未经验证(latency / training corpus / 部署 cost 都未知)
