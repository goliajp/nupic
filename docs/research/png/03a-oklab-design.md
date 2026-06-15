# 03a — Stone A 设计:`nupic-color` / OKLab pipeline

> Sub-essay of [`03-perceptual-stone.md`](03-perceptual-stone.md). First
> stone-design essay under [[feedback-ceiling-first-priorities]] —
> sections ordered **perf > mem > disk > cov > doc**.
>
> Backing experiment:
> `cargo run --release -p nupic-research --example oklab_bench`
> → `target/research-out/03a-oklab-bench.{csv,md}`.

---

## 0. Why now

Stone B (SSIMULACRA2)、Stone C (codebook) 都在 OKLab 空间里工作。OKLab
是 03 essay 5-stone 依赖图的 **dependency root**。这一篇 land 它就解锁
后续所有 stone 子 essay。

输入 = sRGB u8 RGBA 像素(`assets/png-bench/inputs/*.png` decode 后);
输出 = OKLab f32 三通道 + 反向转回 sRGB u8。math 跟 Ottosson 2020(updated
2021-01-25)逐字对齐。

---

## 1. perf —— 当前距离 ceiling 多远(实测)

### 1.1 计算 ceiling 估算

| 项 | 公式 / 数字 |
|---|---|
| 每像素 OKLab forward | 2 × matmul(3×3, 3) + 3 × cube root + 1 × sRGB transfer per channel ≈ 50 FLOP + 3 transcendental |
| pixel arithmetic ceiling | 50 FLOP / cycle / lane × 4 lanes (NEON) × 3 GHz = ~600 GFLOP/s ≈ 0.08 ns/pixel = **24 µs / 02-pluto** |
| bandwidth (Apple M2, real-world streaming) | ~100 GB/s peak;02-pluto = 4 B read + 12 B write per pixel = 6.4 MB;**0.06 ms / 02-pluto** |
| 物理 ceiling(取 max) | **bandwidth-bound**, 0.06 ms / 02-pluto |

cube root 是慢操作:scalar f32 cube root ~30 cycles;LUT(`fast-srgb8`)
~3 cycles。SIMD `vsqrtq_f32` × Newton iter 或 `polynomial cbrt` 可以 4
路并行,~8 cycles/lane。

### 1.2 实测(M2,release build,7-run median)

`oklab_bench.rs` 给的数:

| Image | n_pixels | naive scalar f32 | `oklab` crate v1.1.2 (LUT) | bandwidth GB/s ratio |
|---|---:|---:|---:|---:|
| 02-pluto | 399,424 | **8.18 ms** | **1.88 ms** | 0.78 / 3.41 |
| 04-portrait | 960,000 | 21.15 ms | 4.40 ms | 0.73 / 3.49 |
| 06-landscape | 1,440,000 | 34.31 ms | 6.68 ms | 0.67 / 3.45 |
| roundtrip(forward+back, naive)| 02-pluto | 4.61 ms(只回路)| — | 1.30 |

**距 ceiling**:
- naive scalar:**136× off** (bandwidth ceiling 0.06 ms vs 实测 8.18 ms)
- oklab crate (LUT):31× off(1.88 ms / 0.06 ms)
- 距 SIMD theoretical(0.5–1 ms 估计):naive **8–16×** off,oklab crate **2–3×** off
- bandwidth utilization:naive 0.7%(scalar-bound,bottleneck = cube root);
  oklab crate 3.5%(LUT-bound,fast-srgb8 仍 scalar)

跟 03 essay 估计的对照:

| | 03 essay 估 | 实测 |
|---|---:|---:|
| naive scalar | 8 ms | **8.18 ms** ✓ 精准 |
| SIMD target | 2 ms | 待 03a-bis(`std::simd` 实现) |
| bandwidth ceiling | 0.1 ms | 0.06 ms(M2 measured peak)|

### 1.3 attack plan

按 perf 优先级硬序:

| 阶段 | what | 期望 02-pluto 时间 | 距 ceiling | 实施 cost |
|---|---|---:|---:|---|
| **A0**(本 essay)| naive scalar Rust + oracle match | 8.18 ms ✓ | 136× | done |
| **A1** | `fast-srgb8` LUT 接入(参考 oklab crate)| ~2 ms | 30× | 0.5 周 |
| **A2** | `std::simd` 4×f32 portable SIMD | 0.5–1 ms | 8× | 1 周 |
| **A3** | arm NEON / x86 AVX2 specific intrinsics + cbrt polynomial | < 0.3 ms | 5× | 1 周 |
| **A4** | tile-based with L2 fit + prefetch | < 0.15 ms | 2.5× | 0.5 周 |

A0 已落地。A1 是 cheapest 路径;A2 是 portability ceiling;A3 是 native
SIMD ceiling;A4 是 cache ceiling。**stone A graduation 阈值 = A2(< 1 ms /
02-pluto)**;A3 / A4 是 nice-to-have polish。

`fast-srgb8` 已经在 `oklab` crate 的 transitive 里;我们 stone 也接
(crate `fast-srgb8` v1.0.0,1 file pure Rust LUT,acceptable)。

---

## 2. mem —— working set 和 streaming ceiling

### 2.1 计算 ceiling

| 项 | 数字 |
|---|---|
| 单像素 OKLab buf | 3 × f32 = 12 B |
| 02-pluto 全图 OKLab | 400K × 12 = **4.8 MB**(超 L1,接近 M2 L2 8 MB)|
| 06-landscape 全图 | 1.44M × 12 = **17 MB**(超 L2,落 L3)|
| 4K 全图(3840 × 2160 = 8.3M px)| 100 MB(L3 也超,**必须 streaming**)|

### 2.2 实测内存峰值

`oklab_bench.rs` 用 2 buffers per image(input RGBA8 + output OKLab f32):
- 02-pluto:1.6 MB + 4.8 MB = 6.4 MB working
- 06-landscape:5.8 MB + 17 MB = 23 MB working

未优化,L3 范围内可承受。但 4K 上必须 tile。

### 2.3 attack plan

按 mem 优先级:

| 阶段 | what | working set / 02-pluto | 4K extrapolation |
|---|---|---:|---:|
| **M0**(本 essay)| full-buffer scalar | 6.4 MB | 100 MB |
| **M1** | tile-based 64×64 块(下游 stone B/C 也 tile)| < 64 KB | < 64 KB |
| **M2** | streaming(in-place where alpha 不需要;double-buffer one tile)| < 32 KB | < 32 KB |

stone A graduation 阈值 = **M1(L2 friendly)**。

注意 stone A 的 mem 设计需要跟 stone B / C 协同 tile size,不能各自选不
同。设为常量 `TILE = 64`(64 × 64 pixel = 16K px = 192 KB OKLab buf;放 L2
舒服),或者 feature-flag 可调。decision 进 stone graduation PR 时定。

---

## 3. disk —— 间接影响

OKLab 本身不写盘 — 它是 in-memory perceptual 工作空间,给 stone B / C
用。**对 PNG 输出 size 的贡献全部通过 stone C palette 选择体现**。

约束:OKLab → RGB 反向 roundtrip 必须 **per-channel u8 误差 ≤ 1**(由
sRGB 8-bit 量化 ceiling 决定)。`oklab_bench` 实测:naive impl 在 02 / 04 /
06 上 max(dR, dG, dB) = 0(diff_u8 函数显示 0,实际上是 f32 epsilon ≤ 0.5
被 round 吸收)。**这条 ceiling 已达**。

---

## 4. cov —— property + reference fixture 设计

按 [[feedback-not-rotting-tests]],测契约,不测内部:

### 4.1 Property tests(目标 ~50 props)

| # | property | 验证方法 |
|---|---|---|
| 1 | RGB(u8) → OKLab → RGB roundtrip,per-channel error ≤ 1 / 255 | sweep R/G/B ∈ {0, 32, 64, ..., 255} 全 6^3 组合 |
| 2 | L axis monotonic in luminance | per-hue sweep 灰阶,L 单调 |
| 3 | a axis ∝ 红–绿 contrast(纯红 a > 0,纯绿 a < 0)| 固定颜色断言 |
| 4 | b axis ∝ 黄–蓝 contrast | 固定颜色 |
| 5 | OKLab(0,0,0) = OKLab of pure black; OKLab(1,1,1) = OKLab of pure white | 固定 |
| 6 | sRGB primaries (R, G, B) OKLab values match Ottosson reference 表 | 跟 paper 数字对比 |
| 7+ | edge cases:LIN ≤ 0(黑)、LIN ≥ 1(白溢)、单通道 0 / 255 等 | 6-8 case |

50 prop bound = 6^3 grid + axis monotone + 边界共 ~250 个 sweep,远超 50。

### 4.2 Reference oracle 测

- `oklab` crate v1.1.2 forward/back:per-pixel L/a/b diff ≤ **1e-5**(float
  epsilon-acceptable)
- 跟 Ottosson 2020 原文表中 4 个固定 reference 颜色对比 ≤ **1e-4**
- 5 fixture(`assets/png-bench/inputs/` 三张 + 2 个 synthetic gradient)
  forward → back diff_u8 ≤ 1

### 4.3 不测什么

- 不测 LUT 内部数值
- 不测 SIMD 路径选择
- 不测 cube root 多项式系数
- 不测 perf 数字(放 bench 不放 test)

### 4.4 stone A cov graduation 阈值

≥ 50 property assertions + ≥ 5 fixture round-trip + 1:1 oracle match。
property 失败任何一条:不 graduate。

---

## 5. doc —— math 推导 + cross-link

### 5.1 OKLab math(Ottosson 2020,updated 2021-01-25)

```
INPUT: sRGB u8 (r, g, b) ∈ [0, 255]³

Step 1: sRGB → linear sRGB(IEC 61966-2-1 inverse transfer)
  for c ∈ {r, g, b}:
    v = c / 255
    lin = v / 12.92                       if v ≤ 0.04045
        = ((v + 0.055) / 1.055) ^ 2.4     otherwise

Step 2: linear sRGB → LMS(matrix M1,Ottosson §3)
  [L]     [0.4122214708  0.5363325363  0.0514459929]   [lin_r]
  [M] =   [0.2119034982  0.6806995451  0.1073969566] × [lin_g]
  [S]     [0.0883024619  0.2817188376  0.6299787005]   [lin_b]

Step 3: LMS → LMS'(per-channel cube root,perceptual nonlinearity)
  L' = cbrt(L), M' = cbrt(M), S' = cbrt(S)

Step 4: LMS' → OKLab(matrix M2)
  [L_okl]   [0.2104542553   0.7936177850  -0.0040720468]   [L']
  [a_okl] = [1.9779984951  -2.4285922050   0.4505937099] × [M']
  [b_okl]   [0.0259040371   0.7827717662  -0.8086757660]   [S']

OUTPUT: OKLab f32 (L_okl ∈ [0,1], a_okl, b_okl ∈ ~[-0.4, 0.4])
```

逆向:Step 4'inverse → cube → Step 2 inverse → Step 1 inverse。Matrices
M1_INV / M2_INV 已经在 `oklab_bench.rs` 中给出,Ottosson 没列(我们 compute
inverse via numpy / sympy,验证后写常量)。

### 5.2 工程映射

> 文中的 Step 1–4 = `rgb_to_oklab_naive` in
> `crates/nupic-research/examples/oklab_bench.rs`。逐行对应,无重构。

### 5.3 cross-link

- [`docs/png-pipeline.md` §1 Layer A](../../png-pipeline.md) — OKLab 作为
  CIELab 替代的理论 anchor
- [`docs/roadmap.md`](../../roadmap.md) — 阶段 3:OKLab / ICtCp 色彩管线
- [Ottosson 2020 *A perceptual color space for image processing*](https://bottosson.github.io/posts/oklab/) — 原文
- [Ottosson 2021 update](https://bottosson.github.io/posts/oklab/#how-i-calculated-the-matrices) — 矩阵精度更新

---

## 6. Stone A — `nupic-color` 工程交付清单(graduation criteria)

按 ceiling-first 优先级,stone A 进入 `crates/nupic-color/` 的条件:

- [ ] **perf**:`bench` 子项目跑 02-pluto forward < 1 ms(stone A2 SIMD 阶段)
- [ ] **mem**:tile-based,working set per tile < 64 KB(stone A2 + M1)
- [ ] **disk**:N/A(间接)
- [ ] **cov**:≥ 50 property + 5 fixture roundtrip,oracle 1:1 match within 1e-5
- [ ] **doc**:本 essay + sub-bench markdown 在 `docs/research/png/`
- [ ] **公共 API surface**(`nupic-color/src/lib.rs`):
  - `pub fn srgb_u8_to_oklab(rgb: [u8; 3]) -> Oklab`
  - `pub fn oklab_to_srgb_u8(oklab: Oklab) -> [u8; 3]`
  - `pub fn srgb_u8_to_oklab_slice(rgba_in: &[u8], lab_out: &mut [Oklab])`
    (tile-aware bulk path)
  - `pub struct Oklab { pub l: f32, pub a: f32, pub b: f32 }`
- [ ] **跨平台 contract**:arm64-darwin / x86_64-linux 输出 bit-exact within
  1e-5(SIMD lane 顺序不同导致最后位差异 acceptable)

`crates/nupic-color/` skeleton 在 stone A 进入 graduation 时创建,**不
是本 essay 一步**。当前阶段 stone A 代码仍住在
`nupic-research/examples/oklab_bench.rs` + 后续 `oklab_simd_bench.rs`
(03a-bis sub-essay)。

---

## 7. Open questions / 下一步 sub-essay

1. **`fast-srgb8` LUT vs polynomial approximation** 的 perf 对照:
   `fast-srgb8` 用 65 KB LUT;poly approx 用 256 B 常量但多 mul。在
   cache-friendly workload 上谁更快?**03a-bis 实测**。
2. **`std::simd` portable 在 stone A2 阶段产出多少 GB/s**?如果接近
   oklab crate 已有的 LUT-scalar 3.5 GB/s,说明 SIMD 直接被 cube root
   主导成本;那 stone A2 要先做 cbrt SIMD polynomial 再 swallow LUT。
3. **跨平台数值精度**:arm NEON 跟 x86 AVX2 在 `cbrt_simd` 多项式末位
   bit 是否一致?如果不一致,property test 的 `1e-5` 阈值是否 robust?
4. **tile size 跟 stone B / C 协同**:stone B 5-octave pyramid 不同尺度
   tile size 不同。stone A 是否要支持参数化 tile size 而不是定 64×64?
5. **本 essay graduation 数字 vs 03 essay 估计的差距**:实测 naive
   8.18 ms 跟估计 8 ms 精准对齐,**但 SIMD 估计 2 ms vs oklab crate
   1.88 ms (LUT not SIMD)** — 说明 03 essay 的 SIMD 估计 over-conservative
   或者 ML cube root cost 估计 under-estimate。需要 stone A2 实测 nail
   down。

---

## 8. 验收材料

- 实验代码:[`crates/nupic-research/examples/oklab_bench.rs`](../../../crates/nupic-research/examples/oklab_bench.rs)
- raw bench output:`target/research-out/03a-oklab-bench.{csv,md}`(generated;not committed)
- 触发本篇:[`03-perceptual-stone.md` §3 Stone A](03-perceptual-stone.md)
- ceiling-first 价值观:[`feedback_ceiling_first_priorities.md`](../../../../../.claude-profile-1/projects/-Users-doracawl-workspace-labs-lab29-nupic/memory/feedback_ceiling_first_priorities.md)
- math 引用:Ottosson 2020;updated 2021-01-25 matrices
- 工具引用:[`oklab` v1.1.2](https://docs.rs/oklab/1.1.2/)、[`fast-srgb8`](https://docs.rs/fast-srgb8/)
