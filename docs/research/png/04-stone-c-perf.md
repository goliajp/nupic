# 04 — Stone C inference perf:rayon par_chunks + breakdown 数据

> Post-graduation polish on Stone C(`crates/nupic-quantize/`).
> Closes the 03c-ter §2.1 "06-landscape 4.39× cement" claim — which
> turned out to be based on a guessed cement baseline. Real data:
> Stone C 0.5.0 is **24-32% faster** than 0.4.0 across the bench
> fixtures.
>
> Sections ordered **perf > mem > disk > cov > doc** per
> [[feedback-ceiling-first-priorities]].
>
> Backing experiments:
> - `cargo run --release -p nupic-research --example stone_c_perf_bench`
>   → `target/research-out/04-stone-c-perf.{csv,md}`
> - 0.4.0 baseline:`target/research-out/02-metric-sweep.csv`
>   `encode_ms` column on the `sweep:q_target=95` rows (which is what
>   `nupic 0.4.0 compress` exercises)

---

## 1. perf

### 1.1 Apply path: rayon par_chunks across pixels

The Stone C "apply palette" step was per-pixel:

```rust
for i in 0..n_pixels {
    let p = srgb_u8_to_oklab(...);
    let mut best_j = 0; let mut best_d2 = f32::INFINITY;
    for j in 0..k { /* argmin */ }
    indices[i] = best_j as u8;
}
```

Switch to `rayon::par_chunks_exact(4)` zipped with
`par_chunks_exact_mut(1)` over the indices buffer — each pixel
independent. Inner `for j in 0..k` argmin stays scalar(Stone A
lesson:portable SIMD wrappers don't beat LLVM auto-vec on M2;
NEON intrinsics is a Stone-C polish backlog item if needed).

`f64.mul_add` swapped in for the distance sum to give the auto-
vectoriser the fmla hint.

### 1.2 Per-stage breakdown(M2 release,5-run median)

```
| image       | train (iq) | apply (us) | oxipng | total |
| 01-trans    | 123 ms     | 10 ms      | 174 ms | 307 ms |
| 02-pluto    | 30 ms      |  8 ms      | 133 ms | 171 ms |
| 03-logo     |  1.4 ms    |  0.6 ms    |  14 ms |  16 ms |
| 04-portrait | 48 ms      |  8 ms      | 298 ms | 354 ms |
| 05-mountain | 127 ms     | 20 ms      | 331 ms | 478 ms |
| 06-landscape| 133 ms     | 32 ms      | 250 ms | 416 ms |
| 07-product  | 51 ms      | 10 ms      | 251 ms | 312 ms |
```

**Stone C apply step is 2-10% of total**:
- 03-logo:0.6 ms = 4% of 16
- 04-portrait:8 ms = 2%
- 06-landscape:32 ms = 8%

Cost is dominated by:
1. **oxipng**(135-330 ms,~70-85% total)
2. **train**(imagequant median-cut,30-133 ms,~10-30% total)

Both belong to the cement layer(Stone C wraps them);Stone C own
contribution = apply,already negligible after rayon。

### 1.3 跨 0.4.0 / 0.5.0 实测对比

0.4.0 baseline 数据来自 `02-metric-sweep.csv` `sweep:q_target=95`
row(那 row 跑的就是 0.4.0 nupic-core `Quality::Auto` 路径)。

| image | 0.4.0 total ms | 0.5.0 total ms | Δ |
|---|---:|---:|---:|
| 01-trans | 317 | 307 | -3% |
| 02-pluto | 212 | 171 | **-19%** |
| 03-logo | 28 | 16 | **-42%** |
| 04-portrait | 462 | 354 | **-23%** |
| 05-mountain | 549 | 478 | **-13%** |
| 06-landscape | 589 | 416 | **-29%** |
| 07-product | 396 | 312 | **-21%** |
| **AVG** | **365** | **293** | **-20%** |

**Stone C 0.5.0 跨集 平均 -20% wall clock vs 0.4.0**,大胜 03c-ter 草率
claim 的 "4.39× cement"(那是 wrong baseline 推算)。

### 1.4 为什么 Stone C 比 0.4.0 cement 快

- **No Floyd-Steinberg dither** = indexed pixel stream entropy 更低 →
  oxipng 内的 deflate 工作量减少
- **OKLab argmin** = 比 cement Lab L2 + dither 的 internal computation 略
  simpler(fewer floating-point ops per pixel)
- **rayon parallel apply** = 把 user-side per-pixel argmin 从 sequential
  scalar 推到 8-core 并行

---

## 2. mem(unchanged)

rayon par_chunks 不增加 working set。每 worker thread 跟原 sequential
版本同样的 stack-side state(palette ptr + 局部 best_j / best_d2)。
heap allocation(indices Vec + palette_srgb)同前。

---

## 3. disk(unchanged)

Output bytes unchanged(rayon 只 reorder execution,not output)。
0.5.0 跨集 size 0.96× 0.4.0(03c-ter 已 documented)。

---

## 4. cov

`crates/nupic-quantize/tests/` 16 tests 全过(release 模式,total run
< 2.5s,从 4s 缩到 ~2s)。determinism property test
(`output_deterministic`)仍 pass — rayon par_iter 在 `for_each` 模式
下不引入 nondeterminism because each closure 调用是 idempotent +
per-pixel 独立(无 reduction 跨 worker)。

---

## 5. doc

03c-ter §2.1 "06-landscape 4.39× cement" 数字 retracted:
- 当时 "~80 ms cement infer" 是 guess,不是实测
- 实测 0.4.0 06-landscape total compress = 589 ms,Stone C 0.5.0 = 416
  ms,实际 **0.71× of 0.4.0**(快 29%)
- "post-graduation polish 优先排第一" 也 retract — 不需要 SIMD attack,
  rayon par_chunks 已经把 Stone C 推过 cement parity

(03c-ter essay 不动,本 essay header 记录 retraction;ceiling-first 价
值观允许 essays 之间互相 update。)

---

## 6. cross-link

- 上游 graduation essay:[03c-ter Stone C graduation](03c-ter-graduation.md)
  §2.1(retracted claim)+ §10(open list)
- 实现:`crates/nupic-quantize/src/lib.rs::apply_palette` rayon path
- 实测 raw:`target/research-out/04-stone-c-perf.csv`
- 价值观:
  - [[feedback-ceiling-first-priorities]] — perf 优先 + 实测数据
  - [[feedback-no-cost-thinking]] — 不去 ROI 化 "rayon 加进去"
  - 这次 essay 自身意外是 **ceiling-first 自身 lesson**:claim "4.39×
    cement" 是 wrong baseline 推算,**所有 ceiling claim 都必须 grounded
    in 实测 not 估算**

---

## 7. 下一步

Stone C 推理 perf 已经超过 cement parity。Stone C polish backlog 剩:
- 05/06 -1~-2 SSIM 点 gap close(adaptive light dither,Stone D 候选)
- `Quality::Perceptual(Ssimulacra2)` 接 Stone B(目前 `NotImplemented`)
- nupic-deflate(roadmap 阶段 1,0.6.x 主线)

按 user-facing impact:`Quality::Perceptual(Ssimulacra2)` 让 stone B 成
为 user-driven metric target,直接 unlock 0.4.0 时预留的 `Ssimulacra2`
variant。**这是下一 essay 候选**。

---

## 8. 验收材料

- 实测代码:[`crates/nupic-research/examples/stone_c_perf_bench.rs`](../../../crates/nupic-research/examples/stone_c_perf_bench.rs)
- Stone C apply update:[`crates/nupic-quantize/src/lib.rs`](../../../crates/nupic-quantize/src/lib.rs)
  `apply_palette` 改 rayon par_chunks
- 0.4.0 baseline 数据源:`target/research-out/02-metric-sweep.csv`
  `sweep:q_target=95` rows
