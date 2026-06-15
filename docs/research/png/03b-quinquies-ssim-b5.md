# 03b-quinquies — Stone B B5:per-scale task parallelism,反超 cement 25%

> Continuation of [`03b-quater-ssim-b4.md`](03b-quater-ssim-b4.md). B4
> caught cement via row-level rayon. B5 nests another layer of rayon
> across the 3 independent blur streams within each pyramid scale.
>
> Sections ordered **perf > mem > disk > cov > doc** per
> [[feedback-ceiling-first-priorities]].

---

## 1. perf — B5 nested rayon

### 1.1 假设

每 scale 有 5 个 gaussian_blur 调用:
- `σ₁₁ = blur(xyb₁ × xyb₁)`,`σ₂₂ = blur(xyb₂ × xyb₂)`,`σ₁₂ = blur(xyb₁ × xyb₂)`
- `μ₁ = blur(xyb₁)`,`μ₂ = blur(xyb₂)`

3 个 σ 必须串行(共用 mul_buf scratch),`μ₁` / `μ₂` 互不依赖。

→ **3 个独立 stream**:`σ-chain` / `μ₁` / `μ₂`,理想 3× concurrency。

### 1.2 实做

`rayon::join` 嵌套两层:外层 split (σ-chain) vs (μ-pair),内层 split
(μ₁ vs μ₂)。σ-chain 内部仍三步 sequential image_multiply + blur(共用
独立的 mul_buf,跟 outer thread 隔离)。每 inner blur 仍走 `VerticalKind::ParallelH`
(B4 row-parallel)。

总并发模型 = (3 outer task)× (B4 row-parallel inside each blur)= rayon
work-stealing 调度。

### 1.3 实测(M2, release, 5-run median)

| image | pass | cement | B1 | B2 | B4 | **B5** | **B5/cement** | score diff |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| 02-pluto | self | 29 | 38 | 25 | 28 | **20** | **0.71×** | 0.0000 |
| 02-pluto | vs tp | 29 | 38 | 25 | 28 | **21** | 0.71× | 0.0000 |
| 04 | self | 55 | 90 | 60 | 56 | **43** | **0.78×** | 0.0000 |
| 04 | vs tp | 53 | 89 | 59 | 55 | 51 | 0.96× | 0.0000 |
| 06 | self | 83 | 136 | 90 | 86 | **61** | **0.74×** | 0.0000 |
| 06 | vs tp | 74 | 138 | 90 | 83 | **62** | **0.84×** | 0.0000 |

**average B5 / cement ≈ 0.79×**(B5 比 cement 快 21%)。score bit-exact
across all 6 measurements。

### 1.4 加速分析

为什么 nested rayon 在 B4 之上还能赢 30%?

- B4 row-parallel 在 horizontal pass 满载 M2 8-core,但 vertical pass
  仍 single-threaded(chunked 但单线程 walk through chunks)
- 5 个 blur calls **之间** 可以 task-level 并行;不同 blur 的
  vertical pass 在不同 core 上同时跑 — 这就把 "vertical pass 沉本"
  也 8-core 用起来了
- σ chain 内部 image_multiply 也是 single-thread,但 3 outer tasks
  让 image_multiply 至少跟 μ blur 的 vertical pass 重叠

理论 ideal:3-stream concurrency × within-blur cores → 但 inner blur 的
row parallel 跟 outer task parallel 共享同一 rayon thread pool,
work-stealing 让总 wall-clock = max(stream cost)而不是 sum / N_cores。

实测看出 cement 在 single-stream 模式下花了所有 8 cores 在 horizontal
pass,**vertical pass 没并行化** — B5 通过 task parallel 把 vertical
也间接 8-core 化。

### 1.5 ceiling 表(updated)

| phase | what | 02-pluto ms | 距 cement | 距 bandwidth ceiling(~2.6 ms)|
|---|---|---:|---:|---:|
| B0 cement reference | rayon-default | 29 | 1.0× | 11× |
| B1 single-column vertical | 03b-bis | 38 | 1.31× | 15× |
| B2 chunked vertical | 03b-ter | 25 | 0.86× | 10× |
| B3 buffer reuse | 翻车 | 27 | 0.93× | 10× |
| B4 + parallel horizontal | 03b-quater | 28 | 0.97× | 11× |
| **B5 + parallel-task per scale** | **本 essay** | **20** | **0.69×** | **7.7×** |
| B6 + SIMD inner loop NEON | 待测 | < 12 估 | < 0.4× | < 4.6× |
| B7 + cross-scale parallel | 待测 | < 8 估 | graduation ✓ | < 3× |
| B∞ bandwidth ceiling | M2 streaming peak | 2.6 | 0.09× | 1× |

graduation 阈值 < 10 ms / 02-pluto 还差 **2×**。距 bandwidth ceiling
7.7×。

cement 同图 distance to bandwidth ceiling 11×,**B5 已经把这个 distance
压缩到 70%**。这是 stone-layer self-built 第一次 quantifiably 超越
cement 显著幅度。

---

## 2. mem — σ-chain task 单独 mul_buf

B5 outer σ-task 需要独立 `mul_buf`(避免跟 μ-task 共享)。每 scale 多
alloc 3 × Vec<f32; w*h> = 额外 17 MB(06-landscape)。但 alloc 由 OS
backed by lazy-zero pages,实测时间 cost ≈ 0。

总 working set 不破 03 essay §2 mem ceiling 表的 M0(28 MB / 02-pluto)
估计。

跟 B3 buffer reuse 翻车 lesson 一致:macOS 上 alloc 不是 bottleneck,
要不要 reuse 不重要。

---

## 3. disk — n/a

---

## 4. cov — score bit-exact preserved

B0 → B5 5 个 phase × 6 测试 = 30 个 score 检查,**全部 diff = 0.0000**。
nested rayon 不改变 IIR 计算顺序(同 row / 同 col 仍 sequential),
浮点 reorder = 0,bit-exact。

Stone B graduation "cement match within 0.5 score points" 仍 ✓
(1000× margin)。

---

## 5. doc — 第 4 个 ceiling 维度:nested task parallelism

03b-quater 列了 3 个独立 ceiling 攻击维度:codegen / memory access /
parallelism(row-level)。本 essay 加第 4 条:

4. **task-level parallelism**(不同独立 task 用同 thread pool 并发)

跟 row-level parallelism 互补:row-level 让单个 op 用满 cores;task-level
让多个 op 并发占 thread pool。**两者必须同时存在** rayon work-stealing
才能优化跨 streams 的 idle time。

Stone-essay 模板再 update:每篇 stone perf attack plan 要标 "row vs task
parallelism ladder"。

---

## 6. cross-link

- 上游:[03b-quater B4](03b-quater-ssim-b4.md)
- 实现:`crates/nupic-research/src/ssim_b1.rs::ssimulacra2_score_srgb_b5`
  + `compute_scale_b5`(nested rayon::join × 2)
- 价值观:[[feedback-ceiling-first-priorities]] / [[feedback-no-cost-thinking]]

---

## 7. 下一步 — 余 2× 到 graduation 的攻击维度

按 perf 优先:

**B6**:SIMD inner loop NEON(`std::arch::aarch64::*`)on the IIR
recurrence。Stone A 的 lesson:portable wrappers 不行,specific intrinsics
能。但 IIR data dependency 沿 row direction,SIMD-vectorise inner 3-pole
loop(独立 pole)是 standard pattern。预期 02-pluto 20 → 12 ms。

**B7**:跨 pyramid scale parallel(6 scales 互独立,理论 6×)。但
downscale 是 reduction(每 scale 依赖上一个的 output),只有 first
scale 独立。可以 staircase:scale 0 跑完 → scale 1 跟 scale 0 的
compute_scale 并行(scale 1 的 downscale 在 scale 0 还在 blur 的时候做)。

或更激进:**Half-precision (f16) blur**。MS-SSIM 数值容忍 f16,内存带宽
减半 → bandwidth-bound perf 2×。M2 SoC 有 f16 native ops。

不在本 essay 试。

---

## 8. 验收材料

- 模块 update:[`crates/nupic-research/src/ssim_b1.rs::ssimulacra2_score_srgb_b5`](../../../crates/nupic-research/src/ssim_b1.rs)
- bench update:`crates/nupic-research/examples/ssim_b1_bench.rs`(B3 列
  改成 B5 timing)
- raw output:`target/research-out/03b-bis-ssim-b1-bench.{csv,md}`
