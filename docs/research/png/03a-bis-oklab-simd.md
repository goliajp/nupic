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

```
[oklab_simd_bench] done 02-pluto-transparent.png (399424 px)
[oklab_simd_bench] done 04-photo-portrait.png   (960000 px)
[oklab_simd_bench] done 06-photo-landscape.png  (1440000 px)
```

完整表(`03a-bis-oklab-simd-bench.md`)三档 image × 4 impl:

### 02-pluto(400K px,6.4 MB streaming IO)

| impl | median ms | bw GB/s | diff(L,a,b) | distance to bw ceiling(0.06 ms)|
|---|---:|---:|---|---:|
| A0 naive scalar | 7.77 | 0.82 | 0,0,0 | 130× |
| A1a LUT-srgb | 3.19 | 2.00 | 0,0,0 | 53× |
| **A1b LUT + Halley** | **2.53** | **2.53** | **0,0,0** | **42×** |
| A2 SIMD-f32x4 | 3.75 | 1.71 | 0,0,0 | 63× |
| (reference) `oklab` crate v1.1.2 | 1.88 | 3.41 | — | 31× |

### 04-photo-portrait(960K px,15.4 MB IO)

| impl | median ms | bw GB/s |
|---|---:|---:|
| A0 | 20.54 | 0.75 |
| A1a | 8.44 | 1.82 |
| **A1b** | **6.91** | **2.22** |
| A2 | 9.14 | 1.68 |

### 06-photo-landscape(1.44M px,23 MB IO)

| impl | median ms | bw GB/s |
|---|---:|---:|
| A0 | 32.79 | 0.70 |
| A1a | 12.64 | 1.82 |
| **A1b** | **10.34** | **2.23** |
| A2 | 13.81 | 1.67 |

---

## 3. 三个翻车的预设

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

**真相**:A1b 2.53 ms 仍比 oklab crate 1.88 ms 慢 **25%**。两边都用 LUT,
都是 scalar Rust,数学完全一致(diff = 0)— **gap 不在算法,在 codegen**。

可能差异源(待后续 essay 实证):
- oklab crate `srgb_to_oklab` 函数签名以 `RGB8` 直接传值,LLVM 把 3 通
  道 inline 在一个寄存器组里;我 A1b 用 `(r, g, b): (u8, u8, u8)` 三个
  独立参数,可能阻碍寄存器分配
- oklab crate 用 `cube` 路径(forward 是 cbrt,可能用了更短的 polynomial
  approx)— 待查 source
- oklab crate 可能有 `#[inline(always)]` 而我的 A1b 是 default `#[inline]`

**Lesson**:**ceiling 比想象的更难触及**,即使所有 macro 优化(LUT +
Halley)都做了。下一步必须深挖 oklab crate source 还原它的 codegen 优势。

---

## 4. ceiling 表(实测后更新)

| phase | what | 02-pluto ms | bw GB/s | 距 ceiling(0.06 ms / 100 GB/s 实际可达 ~30 GB/s)|
|---|---|---:|---:|---:|
| A0 naive scalar | 03a baseline | 8.18 → 7.77 (本 bench) | 0.78 → 0.82 | 130× |
| A1a LUT srgb | 本 essay | 3.19 | 2.00 | 53× |
| **A1b LUT + Halley** | **本 essay 当前最佳** | **2.53** | **2.53** | **42×** |
| A2 wide SIMD | 本 essay,**翻车** | 3.75 | 1.71 | 63× |
| `oklab` crate ref | 外部参考 | 1.88 | 3.41 | 31× |
| A3 arm NEON intrinsics | 待 03a-ter | <1 估 | >6 估 | <16× |
| A4 + tile/prefetch | 待 03a-quater | <0.5 估 | >12 估 | <8× |
| A∞ bandwidth ceiling | M2 streaming peak ~30 GB/s | 0.21 | 30 | 1× |

**stone A 真正 graduation 阈值修正**:
- 03a 估的 "< 1 ms / 02-pluto" 不是 random — 它对应 ~6× over bandwidth
  ceiling,跟 oklab crate 同档(或更好)
- 当前 best A1b 2.53 ms,**还差 2.5×**
- 下一 attack:A3 arm NEON specific intrinsics,**目标 < 1 ms**

graduation 阈值不变。**未达 graduation 之前,不开 stone B/C 子 essay**。

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

按 dependency graph,stone A graduation 前不开 stone B。下一 essay:
**`03a-ter-oklab-neon.md`** — arm NEON specific intrinsics,目标 < 1 ms /
02-pluto + 跨 x86 AVX2 一致输出。

x86 AVX2 路径在 CI 上 calibrate;本机 arm 跑 NEON。两个平台都要 graduation
前测。

---

## 10. 验收材料

- 实验代码:[`crates/nupic-research/examples/oklab_simd_bench.rs`](../../../crates/nupic-research/examples/oklab_simd_bench.rs)
- raw bench:`target/research-out/03a-bis-oklab-simd-bench.{csv,md}`(generated;not committed)
- 上游 essay:[`03a-oklab-design.md`](03a-oklab-design.md)
- ceiling-first feedback:[`feedback_ceiling_first_priorities.md`](../../../../../.claude-profile-1/projects/-Users-doracawl-workspace-labs-lab29-nupic/memory/feedback_ceiling_first_priorities.md)
