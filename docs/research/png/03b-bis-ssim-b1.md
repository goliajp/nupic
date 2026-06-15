# 03b-bis — Stone B B1 baseline reimpl

> Continuation of [`03b-ssimulacra2-design.md`](03b-ssimulacra2-design.md).
> The first stone-B implementation phase: reproduce the SSIMULACRA2
> algorithm from scratch with Stone A codegen lessons applied, ground
> the score against cement crate, surface where the perf attack target
> moves next.
>
> Sections ordered **perf > mem > disk > cov > doc** per
> [[feedback-ceiling-first-priorities]].
>
> Backing experiment:
> `cargo run --release -p nupic-research --example ssim_b1_bench`
> → `target/research-out/03b-bis-ssim-b1-bench.{csv,md}`.

---

## 1. perf — 三轮迭代

### Round 1 — 用错算法

第一刀:沿用 03 essay 描的 "5-octave Gaussian pyramid",写了 11-tap
σ≈1.5 discrete Gaussian:

```rust
const GAUSS_TAPS: [f32; 11] = [...];
const RADIUS: usize = 5;
fn blur_horizontal(...) { /* 11 taps × N pixels */ }
fn blur_vertical(...)   { /* 11 taps × N pixels */ }
```

实测(M2,5-run median,vs cement crate v0.5.1):

| image | cement score | B1 score | diff | 评 |
|---|---:|---:|---:|---|
| 02-pluto self | 100.000 | 100.000 | 0 | trivial perfect |
| 02-pluto vs tp | -59.982 | -60.644 | **0.66** | 边界 |
| 04 vs tp | 85.861 | 89.222 | **3.36** | 偏 |
| 06 vs tp | 79.765 | 87.493 | **7.73** | 严重偏 |

7.73 分大幅 diverge — **cement 用的根本不是 11-tap discrete kernel**。

### Round 2 — 发现真算法

读 `cement::src/blur/gaussian.rs` 完整源码 + `cement::build.rs` 构建脚本:
SSIMULACRA2 cement 用 **Recursive Gaussian(Charalampidis 2016)** —
IIR 3-pole 滤波,O(1)/px(跟 σ 无关),不是离散 truncated tap。

`build.rs` 在编译时解算 Charalampidis 2016 §III 的方程系(eqs (33),
(37), (44), (50), (52), (53), (55), (56), (57)),emit 12 个 f32 常量 +
1 个 radius 进 `OUT_DIR/recursive_gaussian.rs`。

要 byte-exact reproduce 必须 reimpl 这套 const 生成 + IIR filter。

### Round 3 — 实做 + bit-exact match

实现 `src/ssim_b1.rs::consts::compute()` 重做 cement build.rs(`OnceLock`-
backed runtime init,3×3 Cramer's rule matrix solve)+ horizontal /
vertical recursive IIR passes,每条 line 跟 cement source 同步 + FMA
applied。

实测:

| image | pass | cement_ms | B1_ms | cement_score | B1_score | **score_diff** | B1 / cement timing |
|---|---|---:|---:|---:|---:|---:|---:|
| 02-pluto | self | 31.57 | 37.74 | 100.000 | 100.000 | **0.0000** | 1.20× |
| 02-pluto | vs tp | 29.67 | 38.01 | -59.982 | -59.982 | **0.0000** | 1.28× |
| 04-portrait | self | 56.80 | 92.00 | 100.000 | 100.000 | **0.0000** | 1.62× |
| 04-portrait | vs tp | 64.29 | 102.07 | 85.861 | 85.861 | **0.0000** | 1.59× |
| 06-landscape | self | 76.96 | 146.55 | 100.000 | 100.000 | **0.0000** | 1.90× |
| 06-landscape | vs tp | 76.60 | 140.69 | 79.765 | 79.765 | **0.0000** | 1.84× |

**score diff = 0.0000 across all 6 measurements**(score 是 f64,
0.0000 ≡ identical to cement)。

**timing**:B1 比 cement 慢 1.2–1.9×。**B1 baseline 实际反向命中** —
我们重写了同算法但效率没那么高。原因见 §1.4。

### 1.4 为什么 B1 timing 慢于 cement —— 已识别的 ceiling 攻击点

cement 的 `RecursiveGaussian::vertical_pass_chunked<128, 32>`
读 **128 columns 同时** 跨高度扫描,fall back 到 32 / 1 chunk size 当
width 不整除。这让 vertical pass 的 strided reads 跨 columns 合并 ≤ 1
cache miss per row。

B1 当前用 `recursive_v` 单 column 扫:对 632×632 02-pluto 来说,每个 column 是 632 个 striped reads through a 6.4 MB 工作集 — **cache miss
per row**。`width × height` columns × striped = the bottleneck。

cement 跟 B1 的 timing 差:**几乎全部来自单 column vs chunked**。算法
理论 ops/pixel 相同;cache locality 不同。

下一 phase **B2**:实现 vertical_pass_chunked,预期 B1 → B2 close 1.5×
gap。

### 1.5 perf 表(updated 后 Round 3)

| phase | what | 02-pluto ms | 距 cement | 距 bandwidth ceiling(~2.6 ms)|
|---|---|---:|---:|---:|
| B0 cement reference | 已测 | 30 | 1.0× | 12× |
| **B1 scalar reimpl + Recursive Gaussian + FMA** | **本 essay** | **38** | 1.27× | 15× |
| B2 vertical chunked (COLUMNS=128/32) | 待 03b-ter | < cement / ~25 | 0.83× | 10× |
| B3 SIMD blur f32x4 NEON | 待 03b-quater | < 15 | 0.5× | 6× |
| B4 tile + prefetch + 4K safety | 待 03b-quinquies | < 10 graduation | 0.33× | 4× |
| B∞ bandwidth ceiling | M2 streaming peak | 2.6 | 0.087× | 1× |

graduation 阈值 < 10 ms 还要从 B1 38 ms 走 4 倍;cement 30 ms 走 3 倍。
**未达 graduation 之前不开 stone C sub-essay**。

---

## 2. mem — 沿用 cement 的 5-buffer pyramid

B1 的 mem 形态跟 cement 一致:
- 每 scale 用 3 channel × Vec<f32>(w × h)= 3 个 buffer
- + `mul_buf` 重用 1 set × 3 channel
- + Gaussian temp 1 buffer
- + per-scale output for mu1/mu2/σ1²/σ2²/σ12

跟 03b §2 表对照:M0 cement-style full pyramid。02-pluto working set
~28 MB。

B1 没改 mem 模型 — 这是 B4 phase 的事。

---

## 3. disk — n/a

Stone B 不写盘。score 数据流向 Stone C training loop(待 03c)。

---

## 4. cov — bit-exact cement match 当前已满足"graduation cov"的最强项

03b §6 设的 Stone B graduation cov 阈值是 "30+ properties + 17 ref
fixtures + cement-crate score agreement within 0.5 分"。

B1 当前覆盖:
- ✅ cement score agreement:**diff = 0.0000 on all 3 lead images × 2
  passes**(self + vs tp),远超 0.5 阈值
- ❌ 30+ property tests(待 stone graduation 时落到 `crates/nupic-ssimulacra/tests/`)
- ❌ 17 ref fixtures(待 graduation 时,可包括 JPEG XL CFP)

B1 phase 不写测,bench rows 是 6(3 image × 2 pass)。properties +
fixtures 在 stone graduation phase 落地。

---

## 5. doc — 主要 lesson

### 5.1 不能从 essay 直接套实现

03 essay §5.2 的 ceiling-distribution 表估 "Gaussian blur 5 calls ×
6 scales = 30 次,~12 ms 总,distance to ceiling 12×"。**真实算法不是
11-tap discrete,是 Recursive Gaussian IIR**,ops/pixel 都不同。

只有读完 cement 源码 + build.rs **才能 ground 实际 algorithm**。Essay
里的 "5-octave Gaussian pyramid" 描述太抽象,工程实现细节差几个数量级。

**Lesson**:每个 stone reimpl 第一步必须 `find ~/.cargo/registry/src/<cement>` +
读源码到 build.rs 级别。

### 5.2 cement 内部 cache 优化是可移植的

`vertical_pass_chunked::<128, 32>` 是 cache locality 攻击,跟算法
correctness 解耦。Stone A 的 codegen recipe(FMA + inline-always +
struct-pass)适用所有 hot kernel,但 **cache layout decisions** 是
独立的攻击轴 — 给 stone-layer 后续 essay 加一条 ceiling 维度。

### 5.3 Charalampidis 2016 recursive Gaussian 是真正 ceiling

11-tap discrete kernel = O(R) per pixel(R=半径)。
Recursive Gaussian IIR = **O(1) per pixel regardless of σ**。
对 SSIMULACRA2 fixed σ=1.5 来说优势不大;**对 stone C 训练 loop**
(几百 iter 跑 SSIMULACRA2)需要 fast Gaussian 时,IIR 的 O(1) 是关键。

---

## 6. cross-link

- [03 Stone B 描述](03-perceptual-stone.md)
- [03b design + cement baseline](03b-ssimulacra2-design.md)
- 触发本文的 cement source:
  - [`ssimulacra2-0.5.1/src/lib.rs`](https://docs.rs/crate/ssimulacra2/0.5.1/source/src/lib.rs)
  - [`ssimulacra2-0.5.1/src/blur/gaussian.rs`](https://docs.rs/crate/ssimulacra2/0.5.1/source/src/blur/gaussian.rs)
  - [`ssimulacra2-0.5.1/build.rs`](https://docs.rs/crate/ssimulacra2/0.5.1/source/build.rs)
- 算法 reference:
  - Charalampidis 2016, *Recursive Implementation of the Gaussian Filter Using Truncated Cosine Functions* — `consts::compute` 全部推导出自此
  - Sneyers v2.1 polynomial remap(从 cement v0.5.1 §`Msssim::score` 直接 reproduce)

---

## 7. 下一步

按 perf 优先级硬序,**B2** 攻 `vertical_pass_chunked<128, 32>` cache
locality。预期把 B1 的 1.6× cement 拉到 < cement。score 不应改变(纯
cache-locality 改写,无算法改动)。

essay **`03b-ter-ssim-b2.md`** 接续。stone B 仍未 graduate;stone C
sub-essay 仍 blocked。

---

## 8. 验收材料

- 模块:[`crates/nupic-research/src/ssim_b1.rs`](../../../crates/nupic-research/src/ssim_b1.rs)
- bench:[`crates/nupic-research/examples/ssim_b1_bench.rs`](../../../crates/nupic-research/examples/ssim_b1_bench.rs)
- raw output:`target/research-out/03b-bis-ssim-b1-bench.{csv,md}`
- 价值观:
  - [[feedback-ceiling-first-priorities]] — 这次发现"discrete vs IIR
    Gaussian"差异是 ceiling-first 探索的直接产出
  - [[feedback-no-cost-thinking]] — 没因为 "Charalampidis recursive
    Gaussian 复杂" 退到 11-tap;直接攻 ceiling
