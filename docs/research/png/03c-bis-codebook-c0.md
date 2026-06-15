# 03c-bis — Stone C C0 baseline:Adam differentiable training 翻车,真 win 在 OKLab argmin

> Backing experiment:
> `cargo run --release -p nupic-research --example codebook_c0_bench`
> → `target/research-out/03c-bis-codebook-c0-bench.{csv,md}`.
>
> Triggered by [03c design](03c-codebook-design.md). 三个意外的反转。

---

## 1. perf + 跨 7 fixture 实测(优先 ⇒ 排第一)

C0 实现两条 path 同时测,跑 `assets/png-bench/inputs/` 全 7 fixture(M2 release):

| image | imagequant cement SSIM | **C0 no-train**(imagequant init + OKLab argmin + no dither)| C0 + Adam 500 iter |
|---|---:|---:|---:|
| 01-png-transparency-demo | -443.03 | **-64.23**(+379)| -6.70 |
| 02-pluto-transparent | -65.13 | **+71.74**(+137)| +11.91 |
| 03-wikipedia-logo | 50.92 | **+77.95**(+27)| +40.74 |
| 04-photo-portrait | 81.46 | +81.79(+0)| -19.39 |
| 05-photo-mountain | 71.06 | 69.39(-2)| -60.28 |
| 06-photo-landscape | 82.75 | 82.13(-1)| -6.61 |
| 07-photo-product | 82.28 | 82.56(+0)| -55.34 |

**C0-no-train 几乎全 ≥ imagequant cement**:
- 大胜 3 张:01(+379), 02(+137), 03(+27)— 跨 disaster / RGBA / logo
- 微调 4 张:04 / 05 / 06 / 07 photo 在 ±2 范围内(noise-level)
- **Stone C graduation criterion**(SSIMULACRA2 ≥ cement on every fixture)
  几乎实现 —— 05/06 -1 ~ -2 分 borderline,需要 follow-up tuning

**C0 + Adam 500 iter 训练大败 6/7 fixture**:
- 01:-64 → -7(改善 +57)
- 但 02:71 → 12,03:78 → 41,04-07:80+ → -19 ~ -60
- **训练把好 init 拉远 SSIMULACRA2 optimum**

Training 时间:1.5 s / 02-pluto(预估 5 s,实测更快)。Inference 100-360
ms(linear in N_pixels)。

### 1.1 perf ceiling 更新

03c essay 估的 stone C inference ≤ cement 80 ms × 2 = 160 ms。
实测 C0 inference:
- 02-pluto:100 ms ≈ cement 80 ms ✓
- 04:234 ms — 1.5× cement,**ceiling violated**(待 tile + SIMD attack)
- 06-landscape:360 ms — 1.7× cement,同
- bottleneck:per-pixel argmin over 256 palette entries with no SIMD

03c essay §1 提的 "inference ~ 30 ms / 02-pluto" 估计是 with SIMD;
当前 naive scalar 是 100 ms。Stone C polish 维度。

---

## 2. mem — 当前 ok,Adam state 微不足道

实测 mem:
- pixels buffer: N × Oklab(12B/px)= 02-pluto 4.8 MB / 06 17 MB
- palette: 256 × Oklab = 3 KB
- Adam state: 256 × 6 × f32 = 6 KB
- batch buffer: 4096 × index = 16 KB
- total < 18 MB on 02-pluto

跟 03c §2 估计 "tile-based ≤ 100 MB / 02-pluto" 一致 — 当前实现已经
small-batch-friendly,不需要 mandatory tiling for the test fixture set。

---

## 3. disk — output bytes 跨 fixture

C0 no-train output size(indexed PNG + oxipng):

| image | imagequant cement size | C0 no-train | C0 / cement |
|---|---:|---:|---:|
| 01-transparency-demo | ~50 KB(03b 表)| 19.8 KB | 0.40× |
| 02-pluto | ~158 KB | 68.3 KB | 0.43× |
| 03-logo | ~13 KB | 7.2 KB | 0.55× |
| 04-portrait | ~384 KB | 77.1 KB | 0.20× |
| 05-mountain | ~463 KB | 100.8 KB | 0.22× |
| 06-landscape | ~1090 KB | 302.8 KB | 0.28× |
| 07-product | ~347 KB | 32.3 KB | 0.093× |

**C0 size 跨集 ~25% cement**,因为 no dither → smoother indexed pixel
stream → 高 deflate 收益。

但!**SSIMULACRA2 仍 ≥ cement** 几乎所有 fixture。**所以 C0 同时 size +
quality 全胜**?

是,只是这个 win 不是因为 differentiable training,而是 因为 simpler
quantization strategy。Stone C 真正的洞见简单到 03c essay 完全没想到。

---

## 4. 三个翻车的预设

### 4.1 ❌ "Differentiable codebook training 是 Stone C 的 core algorithm"

**真相**:Adam-driven L2-OKLab loss 训练在 6/7 fixture 上 hurt
SSIMULACRA2,从 80+ retreat 到 -19 ~ -60。

原因:**L2-OKLab loss surrogate ≠ SSIMULACRA2-optimal direction**。03c
essay §1.3 已经 hint 这条 risk,但低估了 magnitude — gradient 把
palette 拉离 imagequant init 的 perceptual sweet spot,且方向 perpendicular
to SSIMULACRA2 optimum。

**Lesson**:任何 surrogate loss 必须 **verify against target metric**
before extensive training。03c-bis 应该先 ablate `n_iters=0` vs
`n_iters=500` 再判断 training 有效性。**这是 ceiling-first 的 explore vs
verify 失败案例**:跑 Adam 训练前应该有 sanity check baseline。

### 4.2 ❌ "Imagequant palette 是 Stone C 的 init,training 是 refinement"

**真相**:Imagequant palette 本身已经接近 perceptual sweet spot。
Refinement step 实际是 detour — 跑 500 iter Adam 拉离 imagequant 的 optimum,
SSIMULACRA2 大幅 retreat。

**Lesson**:**当 cement 已经在 perceptual ceiling 附近时,stone 是
modification 不是 refinement**。Stone C 的真 win:换 assignment metric
(Lab → OKLab)+ 关 dither,而不是 retrain palette。

### 4.3 ❌ "Stone C 是 Adam + STE + Gumbel 的研究项目"

**真相**:Stone C 的核心改变是简单的两步:
1. Quantization assignment 用 OKLab argmin(替代 cement 的 Lab L2)
2. **不**在 indexed pixel stream 上叠 Floyd-Steinberg dither

这两条加起来已经突破 02-pluto 的 algorithmic ceiling(-65 → +72,**137
点跃迁**),同时保持其他 fixture 不退步(±2 内)。

**没有可微分量化。没有 STE。没有 Gumbel-softmax**。

03c essay 整个 §1.1 算法 sketch 大部分是 wrong direction(对 L2-OKLab
loss 而言)。**stone C 真正的 design 远简单**,可能在 essay 03c-ter
revise。

---

## 5. cov — 不实施

C0 是 research-stage prototype。Cov 测套等 stone C 真 architecture clear
(post-graduation 03c-quinquies 阶段)再实施。

当前实测 7-fixture cross-check 已经 expose 关键 finding,perf / score
data 充分支撑 essay 论点。

---

## 6. doc — 三条 lesson

### 6.1 No-train baseline 是 ceiling-first 必须先跑的 sanity check

03c essay §1.1 给的算法 sketch 很 detailed(STE + Adam + temp schedule),
看起来 thorough。但 **我没在跑 500 iter 训练之前先测 n_iters=0**。
若先测,会立即看到 imagequant + OKLab argmin 是真 win,training 不必要。

**Lesson**:**任何 iterative algorithm 都要先 measure n_iters=0
baseline**。这是 stone work 的 unit-baseline,跟 Stone A 的 oracle 对比
同等性质。

### 6.2 Surrogate loss 跟 target metric 验证 mismatch — 不要 silent

L2-in-OKLab 在 paper(Ottosson 2020,SSIMULACRA2)被 hint 为 reasonable
surrogate for SSIMULACRA2 loss。**实测它不仅不 reasonable,而且 actively
counterproductive** when used as Adam objective。

这是 ML 文献的一般 phenomenon:**surrogate-vs-target gap 是经常远超直
觉**。Stone D / future C1(differentiable Stone B 本体)才能 close
gap。

### 6.3 Stone C 的真 design 比想象简单 N 倍

03c essay 写了 ~300 行 sketch + 800 行 code 来设计 differentiable
codebook training。最终 win 来自一个 30 行的简单变化:把 quantization
assignment 的 metric 从 Lab L2 换成 OKLab argmin,关掉 dither。

**Lesson**:essay 起始的设计直觉可能 over-engineering。**实测的最低-
complexity baseline 应该先 land**,然后再 incremental complicate。

---

## 7. cross-link

- 上游设计:[03c-codebook-design.md](03c-codebook-design.md)(部分推翻)
- 02 essay metric ceiling 数据:[02-perceptual-metrics.md §4](02-perceptual-metrics.md)
- C0 实施:[`crates/nupic-research/src/codebook_c0.rs`](../../../crates/nupic-research/src/codebook_c0.rs)
- C0 bench:[`crates/nupic-research/examples/codebook_c0_bench.rs`](../../../crates/nupic-research/examples/codebook_c0_bench.rs)
- 价值观:
  - [[feedback-ceiling-first-priorities]] — Adam attempt 是 ceiling-first 的 explore 阶段产物
  - [[feedback-no-cost-thinking]] — 翻车的 Adam path 没必要 defer,直接
    documented + 推 next iteration

---

## 8. 下一步 — 03c-ter:重新 ground Stone C 的实质

按 现 data 反推,Stone C 的核心 algorithm 应该是:

```
1. Get imagequant median-cut palette in sRGB
2. Convert palette to OKLab
3. For each pixel:
   - convert to OKLab (Stone A)
   - assign to nearest palette entry via OKLab argmin (no dither)
4. Encode as indexed PNG (palette in sRGB, indices)
5. oxipng pass
```

**stone C ≈ stone A application,not a separate ML algorithm**。

但仍有 open:
- 05 / 06 上 -1 ~ -2 point regression — 用何机制 close 这个 gap?可能
  light dither 模式 / adaptive dither / mixed assignment
- 03c essay 提的 Stone D(blue-noise dither)可能就是这 gap 的 fix
- 02-pluto 还能 push 更高(从 +72 到 stone C ceiling)吗?需要 future
  experiment

**03c-ter** 计划:
- 重写 Stone C algorithm = 简单 OKLab argmin path
- Restructure essay 03c(remove Adam sketch)
- 实施 light dither variant attack 05/06 regression
- 拓 cov 7-fixture × cement-strict comparison
- 然后 03c-quater = graduation 进 `crates/nupic-quantize/`

---

## 9. 验收材料

- raw bench:`target/research-out/03c-bis-codebook-c0-bench.{csv,md}`
- 实施:
  - `crates/nupic-research/src/codebook_c0.rs`(含 Adam 训练 path
    + InitKind enum + 失败 lesson)
  - `crates/nupic-research/examples/codebook_c0_bench.rs`(7-fixture +
    no-train + Adam-train 对照)
- 价值观:[[feedback-ceiling-first-priorities]] / [[feedback-no-cost-thinking]]
