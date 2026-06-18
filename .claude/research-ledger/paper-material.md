# 论文素材累积

跨 cycle 追加。每条带:
- **★ 等级**(★ 短 note → ★★ workshop → ★★★ 小 venue → ★★★★ 主流 venue → ★★★★★ top-tier)
- **来源 cycle**
- **核心 claim**
- **evidence 数据资产路径**
- **可能投稿目的地**
- **风险 / 待补**

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
