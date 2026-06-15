# 03b — Stone B 设计:`nupic-ssimulacra` / SSIMULACRA2 self-built

> Sub-essay of [`03-perceptual-stone.md`](03-perceptual-stone.md). Sections
> ordered **perf > mem > disk > cov > doc** per
> [[feedback-ceiling-first-priorities]].
>
> Backing experiment:
> `cargo run --release -p nupic-research --example ssim_cement_bench`
> → `target/research-out/03b-ssim-cement-bench.{csv,md}`.

---

## 0. 修正 03 essay 的 dependency 错误

03 essay §3 Stone B 描述 "在 OKLab 空间工作"。**错误**:SSIMULACRA2 实
际用 **XYB 色彩空间**(JPEG XL 团队的 perceptual space),不是 OKLab。
[cloudinary/ssimulacra2 README](https://github.com/cloudinary/ssimulacra2)
明确"XYB color space (rescaled to 0..1 range with B-Y component)"。

含义:
- Stone B 不消耗 [`nupic-color::Oklab`];直接消耗 sRGB → 走自己的 XYB
  pipeline(`yuvxyb` crate 或自研)
- `nupic-color` 仍是 Stone C(codebook learner)需要的;Stone B 跟 A
  在依赖图上**并联**而非串联
- 03 essay §4 的 dependency graph `A → B → C` 修正为:
  - `A` (OKLab) 独立 graduate ✓
  - `B` (SSIMULACRA2 / XYB) 独立 graduate(本 essay)
  - `C` (codebook) 需要 A (OKLab perceptual ground for palette) **和** B (perceptual loss in training)
  - `D` (dither) + `E` (filter) 仍依赖 C

不影响最终架构,只是 A 和 B 可以并行做。

---

## 1. perf — cement baseline 实测 + ceiling 推算

### 1.1 实测 cement 数据(`ssim_cement_bench`,M2, 5-run median, release)

| image | n_pixels | self-vs-self ms | vs-tinypng ms | tinypng score |
|---|---:|---:|---:|---:|
| 02-pluto | 399 424 | **31.85** | 32.03 | -59.98 |
| 04-portrait | 960 000 | 55.16 | 54.93 | 85.86 |
| 06-landscape | 1 440 000 | 78.69 | 75.86 | 79.77 |

Scaling: 跨 image 大小近线性 ~55 ns/pixel,跟 algorithm O(n) 预测一致。

**03 essay 当时估"cement ~100 ms"是 over-conservative**:实测 32 ms
on 02-pluto(metric 单独跑,无 quantize / oxipng 开销)。

### 1.2 perf ceiling 推算

SSIMULACRA2 algorithm 每 pixel streaming bytes(naive 估):
- 6 scales × pyramid factor 1.33(1 + 1/4 + 1/16 + ...)
- 每 scale × 3 channels × 2 images × 4 byte = 24 byte
- 每 scale × 5 intermediate planes(mu1, mu2, σ1², σ2², σ12)× 4 byte = 20 byte
- 共 ~(24 + 20) × 1.33 ≈ **58 byte/pixel** streaming

更激进估算(考虑 Gaussian blur 重读):~**192 byte/pixel**。

02-pluto 400 K px × 192 B = 77 MB streaming traffic。
- 实测 32 ms → effective bandwidth **2.4 GB/s**
- M2 streaming peak ~30 GB/s
- 距 bandwidth ceiling **12.5×**

### 1.3 Stone B perf attack 阶梯

| phase | what | 02-pluto ms 目标 | 距 ceiling |
|---|---|---:|---:|
| B0 cement reference | 已测 | 32 | 12× |
| B1 baseline reimpl scalar | match cement within ±10% | 32 | 12× |
| B2 FMA-aware blur kernel | `f32::mul_add` + Lagny-style fast paths | ~20 | 8× |
| B3 portable SIMD pyramid | `wide::f32x4` for blur passes(verify whether wide loses again here)| ~10 | 4× |
| B4 arm NEON intrinsics | hand-emit `vfmla` + `vld4q` | ~5 | 2× |
| B5 tile + prefetch | L2-friendly pyramid build | ~3 | 1.2× |
| B∞ bandwidth ceiling | M2 streaming peak | 2.6 | 1× |

**Stone B graduation 阈值**(03 essay §3):**< 10 ms / 02-pluto**(B3 level)。
B4 / B5 是 post-graduation polish。

cement-vs-stone-graduation gap = **3.2×**;Stone A 上 A0 → A3a 是 12×。
SSIMULACRA2 比 OKLab 难压(更多 dependent kernel + pyramid level),但
Stone A 的 codegen lessons(FMA + Lagny + struct-pass)直接适用 blur
kernels。

---

## 2. mem — pyramid + sliding-window ceiling

### 2.1 计算 ceiling

每 scale:
- ref + dist LinearRgb (3 channels f32) — 2 × 12 B/px
- ref + dist XYB (planar 3 channels) — 2 × 12 B/px
- 5 mid-buffers(mu1, mu2, σ1², σ2², σ12)— 5 × 4 B/px
- 共 ~32 + 20 = 52 B/px **per scale**

6-scale pyramid sum:1 + 1/4 + 1/16 + ... ≈ 1.33

**02-pluto 400 K px × 52 × 1.33 = 27.6 MB 全 pyramid working set**。
4K image (8 M px) → 553 MB,**必须 streaming**。

### 2.2 cement crate 实际 mem

`compute_frame_ssimulacra2` 用 `Vec<f32>` 一把 allocate 全 pyramid。
For 4K 这会爆掉。我们的 stone 必须 **tile-aware**(类比 nupic-color 的
`RECOMMENDED_TILE_PIXELS`),提供 streaming pyramid build。

但 SSIMULACRA2 跟 OKLab 不同 — SSIMULACRA2 的 blur kernel **跨 row
依赖**(Gaussian blur 是 2D),不能简单切 1D chunk。需要 2D tile + halo
border。

### 2.3 Stone B mem attack 阶梯

| phase | what | working set / 02-pluto | 4K |
|---|---|---:|---:|
| M0 cement-style | full pyramid in memory | 28 MB | 553 MB ✗ |
| M1 per-scale streaming | only one scale alive at a time | 22 MB | 415 MB ✗ |
| M2 ref+dist interleave only | swap memcpy with on-the-fly recompute | 14 MB | 256 MB ✗ |
| M3 2D tile with halo | 256×256 tile + 8 px halo | < 1 MB | < 1 MB ✓ |
| ceiling | one scanline pair × pyramid scales | ~ 50 KB | ~ 50 KB |

graduation 要求 ≤ M3 (tile-aware,4K-safe);ceiling M3 polish to ~100 KB
working set 是 post-graduation。

---

## 3. disk — 间接

Stone B 不写盘。SSIMULACRA2 score 是 Stone C 训练 loop 的 driver。**直
接 disk impact = 0**。

间接 impact:SSIMULACRA2-driven quantization(via Stone C)会让 02-pluto
从 SSIMULACRA2 -60 → ?(待 Stone C 测,03 essay 估 ≥ +30)。

---

## 4. cov — algorithm fidelity + property + reference

### 4.1 cement-equivalence contract

Stone B 输出必须跟 `ssimulacra2` crate v0.5.1 score **diff < 0.5 分**
(Sneyers §6.2 给的 metric 自身 noise floor)。

实现 reference:`ssim_cement_bench` 上的 6 行(02 / 04 / 06 × self+tinypng)
是 stone 必须 match 的 ground truth。

### 4.2 cross-tool oracle

跟 cement crate 比仍嫌单边。再加:
- [`ssimulacra2_rs`](https://crates.io/crates/ssimulacra2_rs) CLI(同
  rust-av port,但 binary)— sanity check 我们的 cement timing
- (optional)Cloudinary 原 C++ `ssimulacra2_main`(若 CI 装得动)
  在 5 fixture 上跑,跟 stone 输出比 < 0.5 分

### 4.3 reference fixture(graduation 必须覆盖)

03 essay §3 Stone B 估 "20 reference fixture from JPEG XL CFP"。具体:
- 10 张 [JPEG XL CFP test set](https://github.com/cloudinary/ssimulacra2#correlations)
  里的图,带原 ssimulacra2 reference 分(30 / 50 / 70 / 90 各档至少 1 张)
- 我们 7 张 `assets/png-bench/inputs/` PNG 也跑
- 共 17+ fixture

### 4.4 property tests(graduation 阈值 30+)

| 类别 | 例子 | 数量 |
|---|---|---|
| 完美一致 | self-vs-self score = 100.000(实测 cement 已确认)| 5 |
| 对称 | score(A, B) == score(B, A) | 5 |
| Monotonic in distortion | mild blur < strong blur(score 单调下降)| 8 |
| 分辨率不变 | 上/下 sample 2× 后 score 应在 ±2 内 | 5 |
| 边界 | 8×8 最小尺寸 / 1×1 错误返回 / dimension mismatch 错误返回 | 7 |

总 30+ prop。**不**测内部 blur 系数 / scale 数。

### 4.5 不测什么

- Gaussian blur kernel 内部 coefficients(implementation detail)
- 6 scale 数量(post-graduation 可改成 5 或 7 看 ceiling distance)
- 衡量 Sneyers 修正 SSIM 公式 vs 标准 SSIM 的内部 diff
- bench timings(在 nupic-research,不进 stone tests)

---

## 5. doc — algorithm + cross-link

### 5.1 算法分解(Cloudinary README + ssimulacra2 v0.5.1 source 复制 reproduction)

```
INPUT: source (sRGB Rgb<f32> or u8), distorted (same)

Step 1: sRGB → linear sRGB (per Rec.709 transfer or sRGB 2.4 gamma)
Step 2: linear sRGB → XYB
  X = mixed red-green opponent
  Y = green-yellow (luminance-like)
  B = blue
  rescale to [0, 1] via add 0.55 / 0.42 / 0.01 offsets (Sneyers v2.1)
Step 3: for scale in 0..6:
  if scale > 0:
    downscale_by_2 (linear-light average over 2×2 blocks)
  convert to planar [Vec<f32>; 3]
  mu1 = gaussian_blur(img1)
  mu2 = gaussian_blur(img2)
  sigma1_sq = gaussian_blur(img1 * img1)
  sigma2_sq = gaussian_blur(img2 * img2)
  sigma12 = gaussian_blur(img1 * img2)
  ssim = ssim_map(width, height, mu1, mu2, sigma1_sq, sigma2_sq, sigma12)
  edge_diff = edge_diff_map(img1, mu1, img2, mu2)   # blockiness + smoothness
  per-scale (avg_ssim, avg_edgediff) → MsssimScale
Step 4: aggregate per-scale → 1-norm + 4-norm × 6 scales × 3 channels = 108 values
Step 5: weighted sum + polynomial remap → 0..=100 score

OUTPUT: f64 score in [-∞, 100], where 100 = identical
```

### 5.2 算法步骤的 ceiling 分布

| step | cost(02-pluto)| ceiling | 距 ceiling |
|---|---:|---:|---:|
| 1. sRGB → linear | ~2 ms | 0.06 ms (bw) | 30× |
| 2. linear → XYB | ~3 ms | 0.1 ms (bw) | 30× |
| 3a. downscale_by_2 | ~2 ms × 5 scales | 0.5 ms | 20× |
| 3b. Gaussian blur(5 calls × 6 scales = 30)| ~12 ms 总 | 1 ms | 12× |
| 3c. ssim_map + edge_diff_map | ~10 ms 总 | 1 ms | 10× |
| 4. aggregation | ~2 ms | 0.1 ms | 20× |
| 5. polynomial remap | < 0.1 ms | 0.01 ms | 10× |
| **total**(实测合 32 ms,各项加总粗一致) |  |  |  |

最大单步成本 = Gaussian blur(30 次)+ ssim_map computation。这两步是
SIMD attack 主目标。

### 5.3 cross-link

- [`docs/png-pipeline.md` §1 Layer A](../../png-pipeline.md) — Stone B
  作为 codebook driver 的角色
- [`docs/roadmap.md` 阶段 4](../../roadmap.md) — self-built SSIMULACRA2 排队
- [cloudinary/ssimulacra2 README](https://github.com/cloudinary/ssimulacra2)
- [rust-av/ssimulacra2 v0.5.1 source](https://docs.rs/ssimulacra2/0.5.1/)
- [yuvxyb v0.4.2](https://docs.rs/yuvxyb/0.4.2/) — color-space transitive,
  可选 dep 或自研 reimplementation
- [Wang et al. 2003, *Multi-scale Structural Similarity*](https://ieeexplore.ieee.org/document/1292216)

---

## 6. Stone B graduation criteria

按 ceiling-first 优先级:

- [ ] **perf**:< 10 ms / 02-pluto(B3 SIMD pyramid)— **gap from B0: 3.2×**
- [ ] **mem**:M3 tile-aware,4K-safe(working set < 1 MB)
- [ ] **disk**:n/a
- [ ] **cov**:30+ property + 17 reference fixtures + cement-crate score
  agreement within 0.5 分
- [ ] **API**:`crates/nupic-ssimulacra/` 公共:
  - `pub fn ssimulacra2_score(reference_srgb_rgba: &[u8], distorted_srgb_rgba: &[u8], width: u32, height: u32) -> f64`
  - `pub fn ssimulacra2_score_xyb(reference_xyb_planar: &[Vec<f32>; 3], distorted_xyb_planar: &[Vec<f32>; 3], width: u32, height: u32) -> f64`(给 Stone C 复用,跳过 XYB conversion)
  - `pub struct Ssimulacra2Score(f64)`(后续可 expose per-scale breakdown)
- [ ] **doc**:本 essay + crate-level rustdoc + cross-link

---

## 7. Stone B 攻击规划(sub-essay 顺序)

按 ceiling-first 优先级(perf 先):

| seq | sub-essay | focus | 当前预期 02-pluto ms |
|---|---|---|---:|
| 03b | 本篇 | design + cement baseline + algorithm reproduction | 32 (cement)|
| 03b-bis | scalar B1 reimpl + FMA / Lagny pass | match cement within 10% | ~32 |
| 03b-ter | B2 / B3 SIMD blur kernel attack | < 15 ms | < 15 |
| 03b-quater | B5 tile + 4K safety + cov | < 10 ms graduation | < 10 |

(命名沿用 Stone A 的 `bis / ter / quater` 后缀 pattern。)

---

## 8. Open questions

1. **yuvxyb crate 是否要 dep**?它做 sRGB → linear → XYB conversion,~400
   lines,跟 `nupic-color` 的 OKLab 类似 scope。可以 dep 它先 ground
   stone B,然后单独把它 fork 成 `nupic-color` 的 XYB module(数学上 OKLab
   和 XYB 都是 LMS-based perceptual space,共享部分 LUT)。或直接 reimpl
   in stone B。
2. **Polynomial remap weights** 跟 cement crate hard-coded — Sneyers v2.1
   tuned via Nelder-Mead on subjective scores。我们不 retrain;直接 copy
   weights,作为 stone implementation 的常量。这跟自研 stone 哲学是
   compatible 还是 trade-off?(争论:weights 是 algorithm constants 不是
   独立设计点)
3. **跟 Cloudinary 原 C++ score 一致性**:rust-av port 跟 C++ score 差
   多少?Sneyers §6.2 说 noise floor 0.5 分,但 port 内部 implementation
   choices 可能漂 1-2 分。**graduation cov 阈值要先 calibrate**。
4. **6-scale vs 5/7-scale**:cement hardcodes NUM_SCALES = 6。post-graduation
   可探索 reduced scale 在 small image 上是否仍 calibrated。
5. **Stone C 是否需要 stone B 的 per-scale 输出 + gradient**(用于
   differentiable codebook training)?目前 cement crate 只 expose 总
   score f64。Stone B 公共 API 要不要 expose `(scale, plane, map) → value`
   structured output?**第二期 essay 决定**(03c-codebook-design 时再回看)。

---

## 9. 验收材料

- Cement baseline 实测:[`crates/nupic-research/examples/ssim_cement_bench.rs`](../../../crates/nupic-research/examples/ssim_cement_bench.rs)
- raw bench output:`target/research-out/03b-ssim-cement-bench.{csv,md}`
- 上游 essay:
  - [03 Stone B 描述](03-perceptual-stone.md) — 本 essay 修正其 OKLab dependency 错误
  - [03a graduation 的 codegen lessons](03a-bis-oklab-simd.md) — FMA + Lagny + inline-always 直接适用 stone B 的 blur kernel
- 上游 reference:
  - [cloudinary/ssimulacra2](https://github.com/cloudinary/ssimulacra2)
  - [rust-av/ssimulacra2 v0.5.1](https://docs.rs/ssimulacra2/0.5.1/)
- 价值观:
  - [[feedback-ceiling-first-priorities]]
  - [[feedback-metric-over-human-eye]]
  - [[feedback-no-cost-thinking]]
