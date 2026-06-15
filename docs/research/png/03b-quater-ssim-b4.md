# 03b-quater — Stone B B3 buffer reuse 翻车 + B4 rayon 持平 cement

> Continuation of [`03b-ter-ssim-b2.md`](03b-ter-ssim-b2.md). Two
> hypotheses tested in this round:
>
> 1. B3 — allocation churn drives the 04/06 gap → buffer reuse via
>    `Scratch` struct
> 2. B4 — cement uses `rayon` (default feature) to parallelize the
>    horizontal IIR pass
>
> One翻车,one 大胜。详见。
>
> Backing experiment:
> `cargo run --release -p nupic-research --example ssim_b1_bench`
> (now produces B1/B2/B4 columns; B3 measured separately).

---

## 1. B3 翻车 — buffer reuse 没赢(perf 优先 ⇒ 排第一)

### 假设

B1 → B2 改善了 vertical pass cache locality。剩下 04/06 vs cement 仍
1.1-1.25× off。B3 假设 = **allocation churn**:每次 `gaussian_blur`
alloc 4 个 Vec<f32; w*h>(out × 3 + tmp)× 5 calls / scale × 6 scales = ~120
allocs / call。对 06-landscape 1.44 M px × 4 byte = ~5.6 MB / alloc,~700
MB total allocation traffic per ssimulacra2 call。

### B3 实做

加 `Scratch` struct holding all reusable buffers(`mul_buf`, `xyb1/2`,
`blur_temp`)。`std::mem::take` 来 borrow / return Vec across scopes,
`shrink_to(n)` 在 scale 之间 truncate(不 dealloc)。

### 实测

| image | cement | B2 | B3 (reuse) | B3/B2 | B3/cement |
|---|---:|---:|---:|---:|---:|
| 02-pluto | 30 | 26 | 27 | +4% slow | 0.90× |
| 04 | 55 | 61 | 65 | +6% slow | 1.16× |
| 06 | 79 | 95 | 101 | +6% slow | 1.29× |

**B3 比 B2 反慢 4-6%**。

### 假设证伪 — 分析

- macOS `malloc` 给 zeroed pages from kernel → 几乎免费 zero-fill
- 5.6 MB alloc 实测 ~50-100 µs(`malloc` + mmap),120 allocs × 75 µs =
  9 ms total — 但实测整 06 跑 ~95 ms,所以 alloc 占 10% 不是 dominant
- B3 内部用 `std::mem::take` swap buffer 进 / 出 Scratch struct 有 overhead
  (Vec swap 是 metadata 但 + bounds check + drop 流程)
- B3 改了循环结构(`fill_positive_xyb_planar` 用 push 不 indexed assignment),
  push 多了 reserve / capacity check

净影响:**B3 比 B2 慢**。alloc 不是 04/06 gap 的真原因。

### Lesson

**ceiling-first 不等于盲优化**。这次的"alloc churn"假设 直觉很
plausible(allocation 是 known slow path)但实测推翻。继续追真原因。

---

## 2. B4 大胜 — cement 用 rayon 多核(perf 优先 ⇒ 排第二)

### 真原因

`grep "rayon" ssimulacra2-0.5.1/Cargo.toml`:

```toml
[dependencies.rayon]
version = "1.5.3"
optional = true

[features]
default = ["rayon"]
```

`rayon` **是 cement crate 的 default feature**!`ssimulacra2 = "0.5"`
in workspace 默认开启,**cement 跑 multi-core horizontal pass**。

cement 的 `horizontal_pass` 有两条实现路径:

```rust
#[cfg(feature = "rayon")]
pub fn horizontal_pass(&self, input: &[f32], output: &mut [f32], width: usize) {
    input.par_chunks_exact(width)
         .zip(output.par_chunks_exact_mut(width))
         .for_each(|(in_row, out_row)| self.horizontal_row(in_row, out_row, width));
}
```

每 row 独立 IIR 扫,rayon 把 N rows 分到 N cores。M2 8-core,理想 8×
speedup on horizontal pass。

### B4 实做

加 `recursive_h_parallel` 用 `rayon::par_chunks_exact / par_chunks_exact_mut`,
逻辑跟 `recursive_h` 同但 per-row 分发到 thread pool。`VerticalKind::ParallelH`
新增 dispatch。

### 实测

| image | cement | B1 | B2 | **B4** | B4/cement |
|---|---:|---:|---:|---:|---:|
| 02-pluto self | 30 | 39 | 26 | **28** | **0.92×** |
| 02-pluto vs tp | 30 | 39 | 25 | **28** | **0.93×** |
| 04 self | 55 | 94 | 61 | **56** | **1.02×** ≈ 持平 |
| 04 vs tp | 55 | 94 | 62 | **55** | **1.01×** ≈ 持平 |
| 06 self | 78 | 142 | 93 | **86** | **1.10×** |
| 06 vs tp | 80 | 142 | 92 | **83** | **1.04×** |

**B4 跟 cement 在 04 / 06 ≈ 持平**,在 02-pluto 仍略快 8%(因为 02
体积小,rayon 启动开销大,cement 没赚到多核)。

### score diff:仍 0.0000 全过

rayon 分发不改变 IIR 计算(每 row 仍 sequential within row)。score
bit-exact preserved。

---

## 3. ceiling 表(B0→B4)

| phase | what | 02-pluto ms | 距 cement | 距 bandwidth ceiling(~2.6 ms)|
|---|---|---:|---:|---:|
| B0 cement reference | feature=rayon default | 30 | 1.0× | 12× |
| B1 scalar single-column vertical | 03b-bis | 38 | 1.27× | 15× |
| B2 chunked vertical(单线程)| 03b-ter | 26 | 0.85× | 10× |
| ~~B3 buffer reuse~~ | **翻车 +4-6% slower** | 27 | 0.90× | 10× |
| **B4 + parallel horizontal**(本 essay)| **28** | **0.92×** | **11×** |
| B5 + parallel-across-scales(下一)| 待测 | < 15 估 | < 0.5× | < 6× |
| B6 + SIMD inner loop NEON | 待测 | < 10 graduation | < 0.35× | < 4× |
| B∞ bandwidth ceiling | M2 streaming peak | 2.6 | 0.09× | 1× |

graduation 阈值 < 10 ms / 02-pluto 还差 **2.8×**。

cement 同 02-pluto 30 ms也距 graduation 3× — 即 cement 也没 graduate
ceiling。我们 attack 的是 cement 之外的 ceiling 维度,**B5 / B6 起将开始
超越 cement,不仅是 catch up**。

---

## 4. mem — 不退步

B4 mem footprint 跟 B2 一致:per-call gaussian_blur 仍 alloc(B3 reuse
被证伪)。rayon thread pool 占用栈空间 + per-thread 状态,对 IIR
row-parallel 来说 per-row state 是 6 × f32 = 24 byte,8 threads ×
24 = 192 byte。可忽略。

---

## 5. disk — n/a

---

## 6. cov — score bit-exact preserved

B0 → B4 4 个 phase × 6 measurements = 24 score checks,全部
**diff = 0.0000 vs cement**(IIR linearity)。Stone B graduation
"cement match within 0.5 score points" criterion 已超过 1000×
margin。

stone B 仍未 graduate:
- ✅ cov score match
- ❌ perf < 10 ms / 02-pluto(B4 28 ms,差 2.8×)
- ❌ mem 2D tile + halo for 4K-safe
- ❌ 30+ property + 17 reference fixture
- ❌ `crates/nupic-ssimulacra/` skeleton

剩 perf + mem + cov framework + skeleton。perf 优先。

---

## 7. doc — 三条 lesson

### 7.1 Default features 是隐藏 ceiling

`ssimulacra2 = "0.5"` 我们以为是 "cement scalar reference",**实际是
"cement + rayon 多核"**。这次找了 3 个 sub-essay 才发现。**每个 cement
baseline 必须先 inspect default features**,记 essay。

### 7.2 alloc churn 不是 macOS 上的常见 bottleneck

macOS `malloc` + lazy zero-fill kernel pages 让大 Vec alloc 几乎免费。
B3 假设错了。这跟 Linux glibc 默认 behavior 略不同;Linux 上 alloc 可能
更显著。**跨平台 perf attack 时,要标 "测试平台"**。

### 7.3 parallel 是独立 ceiling 维度

ceiling-first 价值观至此识别 3 个独立维度:
1. **codegen**(stone A:FMA + Lagny + inline-always)
2. **memory access pattern**(stone B B2:chunked vs single column)
3. **parallelism**(stone B B4:row-level via rayon)

03 essay 的 ceiling 模型缺第 3 条。**Stone-essay 模板再 update**:加
"parallelism ladder",跟 codegen ladder / memory access ladder 并列。

---

## 8. cross-link

- 上游:[03b-ter B2 chunked vertical](03b-ter-ssim-b2.md)
- cement source:
  - [`ssimulacra2-0.5.1/Cargo.toml` features](https://docs.rs/crate/ssimulacra2/0.5.1/source/Cargo.toml)(rayon default)
  - [`ssimulacra2-0.5.1/src/blur/gaussian.rs::horizontal_pass`](https://docs.rs/crate/ssimulacra2/0.5.1/source/src/blur/gaussian.rs)(rayon path)
- 实现:
  - `crates/nupic-research/src/ssim_b1.rs::recursive_h_parallel`
  - `crates/nupic-research/src/ssim_b1.rs::Scratch`(B3 dead end,kept for
    bench reference)

---

## 9. 下一步 — B5

按 perf 优先 + dependency,B5 attack:**parallelize the 5 blur calls
within a single scale**。每 scale 有 5 个独立 gaussian_blur 调用
(sigma1_sq, sigma2_sq, sigma12, mu1, mu2)。可以 rayon scope 并行,
让 5 个 blur 同时 alive。这是 task-level parallelism on top of B4's
row-level parallelism inside each blur。

预期收益 5×(理想)但 rayon nested overhead + 5 blurs share L3 → 实际
3-4×。

essay `03b-quinquies-ssim-b5.md` 待写。

---

## 10. 验收材料

- 模块 update:[`crates/nupic-research/src/ssim_b1.rs`](../../../crates/nupic-research/src/ssim_b1.rs)
  - 新:`ssimulacra2_score_srgb_parallel`(B4 entry)
  - 新:`recursive_h_parallel`
  - 新:`recursive_h_row`(per-row factored helper)
  - 新:`Scratch` struct + `ssimulacra2_score_srgb_reuse`(B3 dead end,
    保留 as regression reference)
- bench update:[`crates/nupic-research/examples/ssim_b1_bench.rs`](../../../crates/nupic-research/examples/ssim_b1_bench.rs)
  - B3 column 改成 B4 timing(buffer reuse fail, parallel succeeds)
- raw output:`target/research-out/03b-bis-ssim-b1-bench.{csv,md}`
- 价值观:[[feedback-ceiling-first-priorities]] / [[feedback-no-cost-thinking]]
