# 03a-bis — Stone A perf attack:LUT / Halley / SIMD,数据翻车

> Continuation of [`03a-oklab-design.md`](03a-oklab-design.md) — drilling
> the perf-ceiling rungs on Stone A. **Stone A 的 next phase 由实测数据
> 决定方向,不由直觉决定**。
>
> Backing experiment:
> `cargo run --release -p nupic-research --example oklab_simd_bench`
> → `target/research-out/03a-bis-oklab-simd-bench.{csv,md}`.

---

## 1. 测量目标(perf 优先)

按 03a essay §1.3 attack plan 排:
- **A0** naive scalar f32 — 已测 8.18 ms(03a)
- **A1a** scalar + `fast-srgb8` LUT 替换 sRGB transfer
- **A1b** A1a + Halley 2-iter cbrt 替换 `libm.cbrtf`
- **A2** SIMD f32x4 via `wide` crate(LUT + Halley SIMD + matmul SIMD)

每个 impl 必须满足:diff vs oklab crate v1.1.2 oracle ≤ 1e-5 per channel
(f32 epsilon 范围)。**只测 forward**(stone A roundtrip 已在 03a 验证)。

---

## 2. 实测数据(M2 / release / 7-run median)

完整表 5 个 impl × 3 个 image:

### 02-pluto(400K px,6.4 MB streaming IO)

| impl | median ms | bw GB/s | diff(L,a,b) | distance to bw ceiling(0.06 ms)|
|---|---:|---:|---|---:|
| A0 naive scalar | 7.99 | 0.80 | 0,0,0 | 133× |
| A1a LUT-srgb | 3.16 | 2.02 | 0,0,0 | 53× |
| A1b LUT + Halley | 2.59 | 2.47 | 0,0,0 | 43× |
| A2 wide SIMD-f32x4 | 3.83 | 1.67 | 0,0,0 | 64× |
| **A3a FMA + Lagny** | **0.66** | **9.74** | **0,0,0** | **11×** |
| (oracle ref)`oklab` crate v1.1.2 | 1.88 (from oklab_bench)| 3.41 | — | 31× |

### 04-photo-portrait(960K px,15.4 MB IO)

| impl | median ms | bw GB/s |
|---|---:|---:|
| A0 | 20.38 | 0.75 |
| A1a | 8.20 | 1.87 |
| A1b | 6.92 | 2.22 |
| A2 | 9.41 | 1.63 |
| **A3a** | **1.58** | **9.72** |

### 06-photo-landscape(1.44M px,23 MB IO)

| impl | median ms | bw GB/s |
|---|---:|---:|
| A0 | 32.85 | 0.70 |
| A1a | 12.24 | 1.88 |
| A1b | 10.65 | 2.16 |
| A2 | 13.64 | 1.69 |
| **A3a** | **2.37** | **9.71** |

---

**Stone A perf graduation 达成**(< 1 ms / 02-pluto threshold,A3a = 0.66 ms)。
A3a 是当前最佳;A0–A2 / oklab crate 都被 A3a 超过。

---

## 3. 四个翻车的预设

### 3.1 ❌ "SIMD 会单调快过 scalar"

**真相**:A2(`wide::f32x4` portable SIMD)**比 A1b scalar 慢 48%**。

原因(分析,not 实证):
- LLVM 在 arm M2 上对 scalar A1b 已经 auto-vectorize(NEON f32x4 自动
  打包)— 手写 SIMD 没有 free 额外加速
- `cbrt_halley_simd` 用 `std::mem::transmute` 把 `f32x4` 拆 `u32` 做
  bit-trick 初值估计,这阻碍了 LLVM 的 register coalescing
- `wide::f32x4` 在 arm 上的 codegen 比 LLVM auto-vec 多了几层 ABI 包装
- LUT lookup 是 random access,SIMD gather 不一定友好;A2 内仍是 scalar
  `fast_srgb8::srgb8_to_f32` 一个一个 lookup

**Lesson**:portable SIMD wrapper(`wide`)在 LLVM-friendly scalar 代码上
**没有自动收益**。要真正打 SIMD ceiling 必须 arm NEON / x86 AVX2 **specific
intrinsics**(03a essay §1.3 A3 phase),且要从 LUT 路径开始重新设计。

### 3.2 ❌ "Halley cbrt 比 libm cbrtf 慢很多(因为 2 iter × 4 ops/iter > 1 libm call)"

**真相**:Halley 2-iter cbrt **比 libm cbrtf 还快**(A1b 2.53 vs A1a 3.19 ms,
**26% 加速**)。libm cbrtf 是带分支 + 通用 fallback 的函数调用,Halley 是
straight-line + bit-trick init,**对 LLVM auto-vec 友好得多**。

**Lesson**:数学上 cbrt 是 "expensive" 操作,但工程上 inlined Halley
> libm function call。这跟 03 essay 预设(cbrt 是 perf bottleneck)需要
revise — cbrt 不再是 bottleneck after A1b。

### 3.3 ❌ "我们最终能持平 oklab crate v1.1.2 (1.88 ms / 02-pluto)"

**真相**:A1b 比 oklab crate 慢 25%。深挖 oklab crate source 找到 4 个
codegen 优势:

1. **`f32::mul_add` (FMA)** 替代 `a * b + c * d`:arm NEON `vfmla` /
   x86 `vfmadd` 单条指令,2× throughput
2. **Lagny rational cbrt approximation**:1 iter,几个 mul + 2 div(我的
   Halley 2 iter × 4 ops 是它 2 倍)
3. **`#[inline(always)]`** 在 hot path
4. **`rgb::Rgb<u8>` struct 传值** 而不是 `(u8, u8, u8)` tuple — LLVM
   把 3 通道在同一寄存器组里 layout

合并这 4 点 = **A3a**。实测:

| 02-pluto | A1b | A3a | speedup |
|---|---:|---:|---:|
| median ms | 2.59 | **0.66** | **3.9×** |

A3a 不仅持平 oklab crate,**还快 2.85×**(0.66 vs 1.88 ms,从 oklab_bench
测得 oklab crate row)。**stone A perf 已超过 graduation 阈值**。

### 3.4 ❌ "Stone A 的 perf attack 已经接近 scalar codegen 上限"

(03a-bis §3.1 原文)**真相**:错。LLVM 在 arm M2 上能 emit FMA + Lagny
的 codegen 是 8-12× 比 naive scalar 快,**远超 LLVM auto-vec scalar
A0**。"scalar codegen ceiling" 实际位置是 ~10 GB/s effective bandwidth
on M2(限制可能是 LUT load + Lagny 的 2 div),仍距 memory bandwidth
ceiling (30+ GB/s) 3× off。

**Lesson**:LLVM auto-vec 在 naive scalar 上工作,但**给它 FMA hint 才
能进入下一档**。`mul_add` 是 perf-critical 代码必备 idiom。

---

## 3.5 ✅ 验证后的预设

- **数值精度仍 0 diff**:A3a 跟 oracle 的 max(L, a, b) diff = 0 at f32
  epsilon(同 A0/A1a/A1b)— Lagny approximation 在 ≤ 1e-7 范围
- **跨 image 一致**:A3a 在 02 / 04 / 06 都 ~9.7 GB/s bandwidth,**说明
  perf bound 是 streaming + compute 平衡**,不是 cache thrash

---

## 4. ceiling 表(实测后更新)

| phase | what | 02-pluto ms | bw GB/s | 距 ceiling(0.06 ms / 100 GB/s 实际可达 ~30 GB/s)|
|---|---|---:|---:|---:|
| A0 naive scalar | 03a baseline | 7.99 | 0.80 | 133× |
| A1a LUT srgb | 本 essay | 3.16 | 2.02 | 53× |
| A1b LUT + Halley | 本 essay | 2.59 | 2.47 | 43× |
| A2 wide SIMD | 翻车 | 3.83 | 1.67 | 64× |
| `oklab` crate ref | oklab_bench | 1.88 | 3.41 | 31× |
| **A3a FMA + Lagny + inline-always** | **本 essay 当前最佳** | **0.66** | **9.74** | **11×** |
| A3b arm NEON intrinsics | 未实施 | < 0.3 估 | > 20 估 | < 5× |
| A4 + tile/prefetch + streaming | 未实施 | ~0.21 估 | ~30 | ~3× |
| A∞ bandwidth ceiling | M2 streaming peak ~30 GB/s | 0.21 | 30 | 1× |

**Stone A perf graduation 达成**:
- 03a §6 设的 perf 阈值 < 1 ms / 02-pluto,**A3a 0.66 ms 通过**
- distance to bandwidth ceiling 11×(从 03a estimate 16× 提升)
- A3b / A4 是 future polish — 不阻塞 graduation,**可以开 stone B**

未 done 项(graduation 完整条件,03a §6):
- [x] perf < 1 ms / 02-pluto ✓ (A3a = 0.66 ms)
- [ ] mem tile-based,working set < 64 KB(M1 phase)
- [x] disk N/A
- [ ] cov ≥ 50 props + 5 fixture roundtrip
- [ ] `crates/nupic-color/` skeleton + 公共 API
- [x] doc:本 essay + 03a essay + bench markdown

剩 mem / cov / skeleton 三项,**接 03a-ter sub-essay 推完,推完 stone A
graduate 进 `crates/nupic-color/`,然后开 stone B**。

---

## 5. mem(无变化)

A1b / A2 working set 跟 A0 一致:RGBA8 输入 + OKLab f32 输出。tile-based
设计(M1 phase)仍是 stone A 内部 future work。本 essay 不动 mem 轴。

---

## 6. disk(无变化)

不写盘。stone A 是 in-memory perceptual 工作空间。

---

## 7. cov(扩展)

本 essay 加 3 个新 impl(A1a / A1b / A2),每个都跟 oracle 对照 diff = 0。
property test 设计跟 03a §4.1 一致 — 50 props 测在 stone A graduation 时
落到 `crates/nupic-color/tests/`,不在 research crate 写测。

bench harness 本身的 contract:不同 impl 在同一 input 上输出 byte-for-byte
一致(within f32 epsilon)。本 essay 用 diff_max 跟 oracle 校验,实测全
0。

---

## 8. doc(更新 cross-ref)

- A1a 引用:[`fast-srgb8` v1.0.0](https://docs.rs/fast-srgb8/1.0.0/)
- A1b 引用:Halley method on cube root(numerical analysis textbook;
  bit-trick init from Quake III fast inverse square root, adapted)
- A2 引用:[`wide` v0.7](https://docs.rs/wide/) — portable SIMD wrapper
- A3 计划引用:`std::arch::aarch64::*` / `std::arch::x86_64::*` intrinsics

---

## 9. 下一步

stone A perf 已经 graduate(A3a 0.66 ms / 02-pluto < 1 ms 阈值)。剩 mem
+ cov + crate skeleton 三项。

下一 essay:**`03a-ter-oklab-graduation.md`** — 处理 mem(tile-based +
streaming verify on 4K)+ cov(50 props + 5 fixture roundtrip)+
`crates/nupic-color/` skeleton + 最终 graduation 宣告。完了开 stone B
sub-essay。

A3b(arm NEON specific intrinsics)+ A4(tile prefetch)作为 stone A
graduation **之后** 的 polish,不阻塞 stone B 推进。

---

## 10. 验收材料

- 实验代码:[`crates/nupic-research/examples/oklab_simd_bench.rs`](../../../crates/nupic-research/examples/oklab_simd_bench.rs)
- raw bench:`target/research-out/03a-bis-oklab-simd-bench.{csv,md}`(generated;not committed)
- 上游 essay:[`03a-oklab-design.md`](03a-oklab-design.md)
- ceiling-first feedback:[`feedback_ceiling_first_priorities.md`](../../../../../.claude-profile-1/projects/-Users-doracawl-workspace-labs-lab29-nupic/memory/feedback_ceiling_first_priorities.md)
