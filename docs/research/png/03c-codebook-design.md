# 03c — Stone C 设计:SSIMULACRA2-driven differentiable codebook

> Anchor 篇 for Stone C — the ceiling-breaker for 02-pluto SSIMULACRA2
> -65 → high quality 路径。03 essay §3 §C 长版 + 实测 dependency
> grounding。
>
> Sub-essay of [`03-perceptual-stone.md`](03-perceptual-stone.md).
> Sections ordered **perf > mem > disk > cov > doc**.

---

## 0. Stone C 是 PNG research thread 的 climax

整个 PNG research thread 的核心 motivating problem:

- 02-pluto (RGBA gradient with alpha) 在 cement imagequant + oxipng 上
  SSIMULACRA2 score **-65**(灾难 quality)
- [02 essay](02-perceptual-metrics.md) 证明:imagequant 的 metric +
  algorithm 是 algorithmic ceiling — 任何 (dither, quality_min, quality_target)
  组合都 ceiling 在 -65 ± 0.5(60+ configs swept)
- [03 essay §1.4](03-perceptual-stone.md) 估:**Stone C 是脱离这条
  ceiling 的唯一 path**。 02 essay 预测 stone C 后 02-pluto SSIMULACRA2
  → ≥ 30+(low/medium quality 带)

Stone A(OKLab,graduated)+ Stone B(SSIMULACRA2,graduated)是 Stone C
的 dependency:
- Stone A 提供 perceptual color space for palette 表示
- Stone B 提供 differentiable-relaxation 的 loss function

Stone C 实做时这两个 stone 是 runtime deps,**首次让 perceptual metric
直接 drive palette choice**,而不是 cement 用 Lab L2 做 internal metric。

---

## 1. perf — training + inference ceiling

### 1.1 算法 sketch

参考 [`docs/png-pipeline.md` §1 Layer C](../../png-pipeline.md) 概述,
具体到 02-pluto 的:

```
INPUT: src_rgba_u8 (RGBA8 packed) of size N = W × H
TARGET: indexed PNG with palette P ∈ R^{K × 3} (K=256 colors in OKLab),
        + alpha-per-palette tRNS,
        minimising SSIMULACRA2(src, decode(quantize(src, P)))

INIT:
  - Convert src to OKLab via nupic-color (Stone A)
  - Initialise P with imagequant default (cement baseline,
    SSIMULACRA2 -65 on 02-pluto)
  - Initialise dither pattern D ∈ R^{tile × tile} (e.g. 16×16,
    blue-noise mask seed)

ITERATE (T iterations, target T ≈ 500-1000):
  1. forward — soft-assignment:
       for each pixel x, compute weights w_{x,k} = softmax(-‖x - P_k‖² / τ)
       reconstructed pixel: x̂ = Σ_k w_{x,k} · P_k
       (τ = temperature schedule, anneals from large → small)
  2. forward — render:
       distorted = decode(quantize_indexed(x̂))
       — apply dither D modulated by w
  3. loss = SSIMULACRA2(src, distorted)
       (Stone B in OKLab-space variant or XYB; need to decide §1.3)
  4. backward via Straight-Through Estimator:
       ∂loss/∂P_k = Σ_x w_{x,k} · ∂loss/∂x̂  +  STE for hard-quantise step
       ∂loss/∂D = similar via STE for dither modulation
  5. Adam step: P ← P - α · m̂_P / (√v̂_P + ε), same for D
  6. anneal τ; check early-stop if loss plateau

OUTPUT: final P (256 colors), indexed PNG bytes (oxipng pass after).
```

### 1.2 perf ceiling 估算

每 iter 的 cost(02-pluto N=400K pixels, K=256):

| step | ops / pixel / iter | total ops / iter | ceiling note |
|---|---:|---:|---|
| forward soft-assignment | K × 3 mul + K exp | 200M + 100M | dominant cost |
| forward render | K × 3 (lookup + weight) | 300M | |
| SSIMULACRA2 forward | ~600M | 600M | (B5 = 20 ms / call) |
| SSIMULACRA2 backward | ~600M | 600M | (autograd-like, ~ same as forward) |
| Adam update | K × 3 × 4 | 3 K = neg | tiny |

Per-iter total ≈ 1.5G ops. At M2 ~600 GFLOP/s peak f32 = **2.5 ms / iter**
理论。

For 1000-iter training: 2.5 s理论 ceiling per 02-pluto。

Cement(imagequant median cut + k-means refine)~80 ms — but cement
不 perceptual-driven。stone C target 是 ~10× slower 而 quality 大幅提升。
**training time 不是 graduation 阻塞维度**(per-image one-time);**inference 
time = cement-comparable** 是 graduation 阻塞维度。

Inference(给定 trained palette 量化新图):
- per pixel K-NN argmin = K × 3 mul = 800M ops on 400K px
- SIMD ceiling ~30 ms / 02-pluto with NEON
- Cement imagequant inference ~80 ms / 02-pluto
- Stone C inference target ≤ cement(< 80 ms)= achievable

### 1.3 SSIMULACRA2 forward in 训练 loop — Stone B 路径 vs differentiable surrogate

Stone B 的 `ssimulacra2_score` 返回 f64 score,**不是 differentiable**
(里面有 cube root + abs + max(0, x) + powi non-smooth)。直接拿 it 做
loss → gradient = 0 (numerical) 或 chain rule break。

两条路:
- **A) Differentiable SSIMULACRA2 reimpl**: Stone B 内部 expose
  intermediate gradients(per-pixel d_ssim/d_pixel array)。需要 substantial
  扩 Stone B API
- **B) STE-style surrogate**: 用 L2 in OKLab 当 differentiable proxy
  loss for soft-assignment phase, 用 Stone B 当 final selection metric
  for **discrete** step. 标 SOTA 但 surrogate ≠ target

03c 起手用 **B**(STE)— 工程量小,且 paper 数据(Sneyers 文里有提及
"a perceptually motivated loss can be approximated by L2 in a
perceptual space")支持 OKLab L2 是 reasonable surrogate。

Stone C 训练 iter ≈ 1000 × 2.5 ms = 2.5 s。比 A 路径(每 iter 调 Stone B
~20 ms × 2 forward+backward = 40 ms × 1000 = 40 s)快 16×。

但 **paper 数据指出 SSIMULACRA2 比 L2-OKLab 显著好 quality**(`docs/png-pipeline.md`
§1 Layer A 量化 +0.5-1.5 SSIMULACRA2 分 in same color count)。所以 STE
surrogate 起步,**Stone D / 03d sub-essay 再考虑 differentiable Stone B**。

### 1.4 ceiling 表(estimated)

| phase | what | 02-pluto training s | 02-pluto inference ms | SSIMULACRA2 expected |
|---|---|---:|---:|---:|
| cement imagequant | baseline | 0.08 (one-shot) | 80 | -65 |
| **C0**: STE + L2-OKLab + 1000 iter Adam | 03c-bis | ~5 s | ~80 ms | ≥ 30(估)|
| C1: + SSIMULACRA2-differentiable forward | 03d 候选 | ~30 s | ~80 ms | ≥ 50(估)|
| C2: + GPU(Metal Performance Shaders)| post-graduation | ~0.5 s | ~10 ms | same |
| C∞ Voronoi-optimal w.r.t. SSIMULACRA2(NP-hard)| unreachable | — | — | absolute max |

**Stone C 当前 graduation target**:
- 02-pluto SSIMULACRA2 ≥ 30(从 -65 跃迁 95 分)
- inference time ≤ 2× cement 80 ms(160 ms 上限)
- training time per image ≤ 10 s(post-process,not real-time)
- score 跨平台一致(M2 / x86 / arm-linux),浮点 epsilon 范围

---

## 2. mem — soft-assignment buffer is the problem

Naive soft-assignment(每 pixel 对 K palette 的 weight)= **N × K × f32**:
- 02-pluto:400K × 256 × 4 = **400 MB**
- 06-landscape:1.44M × 256 × 4 = 1.4 GB ✗
- 4K:8M × 256 × 4 = 8 GB ✗ infeasible

**必须 tile-based training**。每 tile 1024 px × 256 × 4 = 1 MB per
iter,total working set ~50 MB(palette + Adam state + tile soft-assignment +
Stone B pyramid for tile)— acceptable。

或者 sparse soft-assignment:每 pixel 只 keep top-K' palette entries
(K' = 8 or 16),N × K' × f32 = 25 MB on 02-pluto。Stone D candidate
optimisation。

C0 起步用 **dense tile-based**,K' = K(无 sparse approximation)。

03 essay §3 Stone C mem ceiling 表 prediction:**50 MB working set per
02-pluto** — 兑现。

---

## 3. disk — Stone C 是 disk-side ceiling 突破

跟 Stone A/B 不同,**Stone C 直接驱动 disk output**:trained palette
+ quantized image = final PNG bytes。

02-pluto 当前:
- nupic 0.4 default(imagequant + oxipng): 158 KB / SSIMULACRA2 -65
- tinypng baseline: 180 KB / -60

Stone C 后(C0 estimate):
- size 跟 cement comparable(palette + indexed pixels + oxipng)≈ 150-200 KB
- SSIMULACRA2 跃 ≥ 30 — **disaster → low quality 带**

跨图全集预估:
- nupic 0.4 size = 0.92× tinypng
- stone C 后 size = ~0.95× tinypng(微涨 因 palette 更细)
- nupic SSIMULACRA2 跨图 = 5/7 胜 → **7/7 胜**(02 翻盘)

---

## 4. cov — differentiable 训练的 property + reference 测

Stone A / B 都是 deterministic algorithm。Stone C 含 Adam stochastic
optim(random init,gradient noise)。Cov design 要变:

### 4.1 Property tests(30+ target)

| 类别 | 例子 | 数量 |
|---|---|---|
| 训练 loss 单调 | last_iter_loss < first_iter_loss(every fixture)| 5 |
| 早停 robustness | run 不同 seed 10×,SSIMULACRA2 std < 1 分 | 5 |
| Output 形态 | 256-color palette + 长度 = N indexed bytes | 3 |
| 跨平台一致 | arm64-darwin / x86_64-linux output diff < 2 分 | 7 |
| Adam state 数值稳定 | NaN / Inf 不出现 | 3 |
| Imagequant baseline ≥ | Stone C SSIMULACRA2(02) > -65(strict ceiling 突破)| 7 |

### 4.2 Reference oracle 测

- 跑 cement imagequant on `assets/png-bench/inputs/` 7 张,SSIMULACRA2 baseline
- Stone C output SSIMULACRA2 must be **≥ cement SSIMULACRA2 + 5** for each
  fixture(否则 stone C 没真改善)
- ≥ 5 fixture cross-check

---

## 5. doc — 算法源 + cross-link

- [`docs/png-pipeline.md` §1 Layer C](../../png-pipeline.md) — 算法 sketch
- [`docs/roadmap.md` 阶段 5](../../roadmap.md) — k-means++ / DPSO /
  differentiable codebook 路线
- references for the differentiable quantization technique:
  - Bengio et al. 2013, *Estimating or Propagating Gradients Through
    Stochastic Neurons* — STE
  - Jang, Gu, Poole 2016, *Categorical Reparameterization with
    Gumbel-Softmax* — soft-assignment alternative
  - Adam:Kingma & Ba 2014

---

## 6. Stone C graduation criteria(初稿)

按 ceiling-first 硬序:

- [ ] **perf**:training ≤ 10 s / 02-pluto;inference ≤ 2× cement 80 ms
- [ ] **mem**:tile-based,working set ≤ 100 MB / 02-pluto,4K-safe
- [ ] **disk**:02-pluto SSIMULACRA2 跃迁 ≥ 30(从 -65);跨 fixture
  set 总 size ≤ 1.1× cement;跨 SSIMULACRA2 ≥ cement baseline + 5 分
- [ ] **cov**:30+ property + 5+ fixture + imagequant baseline diff
  ≥ 5 SSIMULACRA2 分
- [ ] **API**:`crates/nupic-quantize/` 公共:
  - `pub fn quantize_to_palette_png(src_rgba: &[u8], width: u32, height: u32, opts: QuantizeOpts) -> Result<Vec<u8>, ...>`
    — full pipeline,one-shot
  - `pub fn train_palette(src_rgba: &[u8], width: u32, height: u32, n_colors: u8, opts: TrainOpts) -> Result<Palette, ...>`
  - `pub fn apply_palette(src_rgba: &[u8], width: u32, height: u32, palette: &Palette) -> Result<Vec<u8>, ...>` — indexed PNG bytes
  - `pub struct Palette { rgba: Vec<Rgba8>, alphas: Vec<u8> }`
- [ ] **doc**:本 essay + sub-essay sequence + crate-level rustdoc

---

## 7. sub-essay roadmap

按 perf 优先:

| seq | sub-essay | focus | 预计 02-pluto SSIMULACRA2 |
|---|---|---|---:|
| 03c | 本篇 | design + ceiling table + dependency grounding | — |
| 03c-bis | C0 STE + L2-OKLab + Adam baseline | 数据 grounding 跨 imagequant | ≥ -50 估 |
| 03c-ter | tile-based training perf + 4K safety | < 10 s training | same |
| 03c-quater | 跨平台 reproducibility + cov 30+ props | tests | same |
| 03c-quinquies | graduation 进 `crates/nupic-quantize/` | shipping | same |
| 03d 候选 | differentiable Stone B(true SSIMULACRA2 loss)| ceiling extension | ≥ 30 估 |
| 03e 候选 | GPU(MPS / wgpu)backend | post-graduation | training 0.5 s |

03c 之后 5 个 sub-essay 推进。**预期工作量 在所有现有 stone work 之
上**(差异化 codebook 本身就是研究级算法,STE / Adam 数值稳定性 / 跨
平台 reproducibility 都是 sub-essay scale 工作)。

---

## 8. Open questions

1. **STE 的 sticky problem**:hard quantise step 在 forward 是 离散,
   backward 直接 pass-through gradient — 可能 gradient 方向 mismatch
   real loss gradient,training 不收敛。Sneyers et al. 没具体 propose
   palette-quantise 的 STE 形态。需要 try Gumbel-softmax 作为 fallback
2. **K=256 hard cap** for indexed PNG。stone C learn 出更少 colors 是否
   OK?— stone-after-stone-C 可能要做 palette-size adaptive(降低 K 拿
   smaller size)
3. **Dither 跟 codebook 联合训练** vs 分阶段。03 essay §3 Stone C 描的
   是联合;但 stone D(blue-noise dither)单独 essay 可能更清晰
4. **trained palette 跨 image 复用**?如果 stone C 一次 training cost
   ~5 s,缓存 trained palette 给 类似图 复用,inference 摊销 → 0
5. **Score vs cement on simple images**(03-logo,04-portrait):cement
   SSIMULACRA2 已经 80+ 高质量。stone C 不应 retreat。fixture cross-check
   严格 enforce ≥ cement on every fixture

---

## 9. 验收材料

- 上游 stones:
  - `crates/nupic-color/`(graduated 03a-ter)— OKLab dep
  - `crates/nupic-ssimulacra/`(graduated 03b-six)— SSIMULACRA2 dep
- 价值观:
  - [[feedback-ceiling-first-priorities]] — Stone C 是 ceiling-breaker for
    02-pluto disaster zone
  - [[feedback-metric-over-human-eye]] — Stone C 训练 loss 完全 metric-driven,
    不需要 human-eye calibration
  - [[feedback-no-cost-thinking]] — STE 训练 5+ 秒 / fixture 不是 cost
    阻塞点,research-grade 投入正常
- 算法 anchor:
  - [`docs/png-pipeline.md` §1 Layer C](../../png-pipeline.md)
  - [`docs/roadmap.md` 阶段 5](../../roadmap.md)
- 关键 reference:Sneyers 2023, Bengio 2013, Jang 2016, Kingma 2014
