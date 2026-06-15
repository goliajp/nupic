# 03b-ter — Stone B B2:vertical pass chunked,02-pluto 反超 cement

> Continuation of [`03b-bis-ssim-b1.md`](03b-bis-ssim-b1.md). B1
> matched cement score bit-exactly but ran 1.3-1.9× slower due to
> single-column vertical IIR. B2 attacks that — adds chunked vertical
> pass.
>
> Sections ordered **perf > mem > disk > cov > doc** per
> [[feedback-ceiling-first-priorities]].
>
> Backing experiment:
> `cargo run --release -p nupic-research --example ssim_b1_bench`
> → `target/research-out/03b-bis-ssim-b1-bench.{csv,md}` (extended to
> include B2 columns).

---

## 1. perf — B2 实测

### 1.1 实测数据(M2,release,5-run median)

| image | pass | cement_ms | B1_ms | **B2_ms** | B1/cement | **B2/cement** | score diff |
|---|---|---:|---:|---:|---:|---:|---:|
| 02-pluto | self | 30.77 | 38.98 | **26.05** | 1.27× | **0.85×** ✓ | 0.0000 |
| 02-pluto | vs-tp | 30.02 | 38.80 | **25.77** | 1.29× | **0.86×** ✓ | 0.0000 |
| 04-portrait | self | 54.31 | 94.85 | **61.33** | 1.75× | **1.13×** | 0.0000 |
| 04-portrait | vs-tp | 55.18 | 94.15 | **61.63** | 1.71× | **1.12×** | 0.0000 |
| 06-landscape | self | 76.78 | 142.17 | **94.52** | 1.85× | **1.23×** | 0.0000 |
| 06-landscape | vs-tp | 76.67 | 140.30 | **95.60** | 1.83× | **1.25×** | 0.0000 |

**02-pluto 反超 cement 15%。** 04 / 06 仍 1.1-1.25× cement(non-fatal,
仍要追)。score 全 0 diff(算法仍 bit-exact)。

### 1.2 反超 cement 的原因 + 04/06 留差距的原因

**为什么 02 反超 cement 而 04 / 06 仍落后?**

cement 的 `vertical_pass_chunked::<128, 32>` 给定 width 选 chunk size:
- 02-pluto width=632 → 4×128 chunk + 120 余 = 4 chunks of 32 + 24 余 …
  cement 实际跑 4 个 J=128 chunks + 余 32-aligned + 残 1-stride
- 04 / 06 width=1200/1600 → 更多 chunks,但 cement 还有内部 SIMD 包装
  (`safe_arch` transitive via `wide` crate),我 B2 是纯 scalar IIR + FMA

cement 的优势 = SIMD-friendly memory access pattern + likely SIMD math
on the 3-pole state vector through `wide::f32x8`(或者 LLVM auto-vec
on the COLUMNS=128 inner loop)。我 B2 没明示 SIMD,LLVM auto-vec
跨 width=128 的 inner loop **可能** 出 NEON f32x4 但效果跟 cement 比有 gap。

**B3 attack target**: hand-emit f32x4 NEON intrinsics on the chunked
inner loop, get to 04 / 06 reverse-pass cement。

### 1.3 ceiling 表(updated)

| phase | what | 02-pluto ms | 距 cement | 距 bandwidth ceiling(~2.6 ms)|
|---|---|---:|---:|---:|
| B0 cement reference | 已测 | 30 | 1.0× | 12× |
| B1 scalar reimpl single-column vertical | 03b-bis | 38 | 1.27× | 15× |
| **B2 chunked vertical(本 essay)** | | **26** | **0.85×** ✓ | **10×** |
| B3 SIMD f32x4 NEON inner loop | 待 03b-quater | < 15 | < 0.5× | < 6× |
| B4 tile + prefetch + 4K-safe | 待 03b-quinquies | < 10 graduation ✓ | < 0.35× | < 4× |
| B∞ bandwidth ceiling | M2 streaming peak | 2.6 | 0.09× | 1× |

02-pluto B2 ≈ 0.85× cement — graduation 仍要 < 10 ms,差 2.6×。

---

## 2. mem — 状态向量 expansion

B2 用 `[f32; 3 * 128]` 作 prev / prev2 / out 状态(stack-allocated,
fixed-size array)。COLUMNS=128 chunk × 3 pole = 384 × f32 = **1.5 KB
per state vector**,3 vectors = **4.5 KB 总 stack 状态**。L1 友好。

切换到 COLUMNS=32(fallback)或 COLUMNS=1(残 tail)用同 stack array,
slice off 前 N 个 — 浪费 mem 但不 alloc。

这跟 cement 实现策略一致(cement 用 `Vec<f32>` heap alloc per call,
B2 用 stack array)。B2 在 alloc 压力上比 cement 略 cleaner;cement 的
`Vec` 重用是 borrowed `&mut self.temp`,也 OK。

**mem 不退步**,B1 → B2 改动只是改算法结构,不改 pyramid 总 working
set。

---

## 3. disk — n/a 沿用

---

## 4. cov — score 仍 bit-exact

B1 → B2 改的只是 vertical pass 的内部循环结构(单 column striped scan vs
chunked column window)。代数上等价(IIR filter 是 linear,顺序无关只
要 state vector 维持)。

实测 cement score diff = 0.0000 跨 6 个测试点。**这是 cement-equivalence
contract 的最强证据**:从 B0 → B1 → B2,score 路径一直 bit-exact。

---

## 5. doc — cache locality 是独立 ceiling 维度

B1 → B2 的 1.5× speedup 完全来自 vertical pass 的 cache locality。
**算法没改;只改了 traversal pattern**。这是 codec 优化里反复出现的
phenomena:同样 ops,不同 memory access pattern,可以差 2-3×。

写进 03 essay 的 [`Stone A polish 中的 A4 phase`](03a-ter-oklab-graduation.md#6-open--stone-a-polish-after-graduation)
已经有这个 hint(`tile + prefetch`),但 stone B 是第一次实测它。

**Stone-layer essay 模板要更新**:每个 stone 的 perf attack plan 都要
显式包含 "memory access ladder" — 不只是 SIMD / FMA ladder。

---

## 6. cross-link

- 上游 essay:[03b-bis B1 baseline](03b-bis-ssim-b1.md)
- cement reference:`ssimulacra2-0.5.1/src/blur/gaussian.rs::vertical_pass_chunked`
- B2 实现:`crates/nupic-research/src/ssim_b1.rs::recursive_v_chunked` +
  `recursive_v_cols<const COLUMNS: usize>`

---

## 7. 下一步

按 dependency graph,**B3 SIMD attack**。目标:04 / 06 反超 cement,
02 进一步压低到 < 15 ms。

essay **`03b-quater-ssim-b3.md`** 待写。

---

## 8. 验收材料

- 模块 update:[`crates/nupic-research/src/ssim_b1.rs`](../../../crates/nupic-research/src/ssim_b1.rs)
- bench update:[`crates/nupic-research/examples/ssim_b1_bench.rs`](../../../crates/nupic-research/examples/ssim_b1_bench.rs)
- 价值观:[[feedback-ceiling-first-priorities]] / [[feedback-no-cost-thinking]]
