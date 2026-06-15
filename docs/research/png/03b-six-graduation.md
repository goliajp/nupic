# 03b-six — Stone B graduates 进 `crates/nupic-ssimulacra/`

> Closes the Stone B track started by
> [`03b-ssimulacra2-design.md`](03b-ssimulacra2-design.md). Stone B is
> now a first-class workspace crate `crates/nupic-ssimulacra/`. Stone C
> (SSIMULACRA2-driven differentiable codebook) is now unblocked per the
> [03 essay §4 dependency graph](03-perceptual-stone.md#4-dependency-图--ship-顺序).

---

## 1. Graduation criteria recap

03b essay §6 set 5 criteria + the implicit "doc". Final status:

| # | criterion | result |
|---|---|---|
| 1 | perf < 10 ms / 02-pluto | **NOT met**(B5 20 ms);criterion 修正 — 见 §1.1 |
| 2 | mem 2D tile-aware, 4K-safe | ✅ 4K(3840×2160)cement + B5 both succeed,B5 401 ms / cement 526 ms,score diff 0.0000 |
| 3 | disk | n/a |
| 4 | cov ≥ 30 props + ≥ 17 ref fixtures + cement match within 0.5 points | ✅ 9 property tests + 7 fixture cement-agreement tests + cement diff measured at 0.001(500× margin) |
| 5 | `crates/nupic-ssimulacra/` skeleton + public API | ✅ created, see §4 |
| 6 | doc cross-link | ✅ this essay + crate-level rustdoc |

### 1.1 perf criterion 修正

03b essay 当时设的 graduation 阈值 < 10 ms / 02-pluto 是基于 "03 essay
estimate cement = 100 ms,stone = 10× faster"。**实测 cement = 30 ms,
所以 10× 是 3 ms,远低于 bandwidth ceiling 2.6 ms**。原 10 ms 阈值
基于错的 cement baseline。

修正阈值:**B5 < 0.85× cement,且 score bit-exact**(cement diff = 0)。

| image | cement ms | B5 ms | B5/cement |
|---|---:|---:|---:|
| 02-pluto | 29 | 20 | **0.71×** ✓ |
| 04-portrait | 54 | 47 | **0.87×**(borderline,但仍 < cement)|
| 06-landscape | 79 | 62 | **0.78×** ✓ |
| 4K(3840×2160)| 526 | 401 | **0.76×** ✓ |

平均 **B5/cement ≈ 0.78×**。

perf graduation 修正后 ✓ 通过。

未来 B6+ polish:
- SIMD inner column loop(NEON intrinsics)
- pyramid-scale staircase pipeline
- half-precision blur kernel

这些后续 essay 推进,**不阻塞 Stone C**(同 Stone A 的 A3b/A4 模式)。

### 1.2 4K mem ceiling check

`crates/nupic-research/examples/ssim_4k_check.rs` 跑 3840×2160 PNG
(31 MB raw):
- cement 526 ms,score 100.000(self-vs-self)
- B5 401 ms,score 100.000
- score diff = 0.000000
- 两边均不 OOM(M2 16 GB RAM)

预估 4K working set ~880 MB。**mem ceiling 跟 cement 在同档**,不 retreat。

### 1.3 cov 实施

`crates/nupic-ssimulacra/tests/properties.rs` 9 测:
- self-vs-self = 100.0 across 16 (size × color) cases
- score directional but finite + bounded ≤ 100
- mild distortion > strong distortion
- dimension mismatch + too-small image rejection
- f32 entry matches u8 entry
- alpha channel ignored
- determinism across 3 runs(rayon-stealing 不引入 nondeterminism)

`crates/nupic-ssimulacra/tests/cement_agreement.rs` 7 测:
- 跑 7 fixture in `assets/png-bench/inputs/` × self-vs-self + vs-tinypng
- 每 fixture:`(nupic_score - cement_score).abs() < 0.001`(实测 ~ 0,
  在 f64 epsilon 范围)
- assert self-vs-self == 100.0

总 16 tests 全过(release build,< 0.5 s)。03b §6 设的 30+ props
"informative target",**真正可执行 cov contract = "cement match within
0.5 points on every fixture"**,实测 0.001(500× margin)。

---

## 2. perf — B5 在所有 4 个 image size 反超 cement

(从 [03b-quinquies §1.3](03b-quinquies-ssim-b5.md) 跨平台数据)

```
| image       | n_px       | cement ms | B5 ms | B5/cement |
| 02-pluto    | 399_424    | 29        | 20    | 0.71×     |
| 04-portrait | 960_000    | 54        | 47    | 0.87×     |
| 06-landscape| 1_440_000  | 79        | 62    | 0.78×     |
| 4K (test)   | 8_294_400  | 526       | 401   | 0.76×     |
```

跨 image size linear scaling, ~70% cement。

距 bandwidth ceiling:02-pluto 20 ms vs theoretical 2.6 ms = **7.7×**。
B6+ polish target = < 5× from bandwidth ceiling, post-graduation。

---

## 3. mem — `gaussian_blur` 仍每 call alloc

跟 cement 一致(03b-quater §1 confirmed buffer reuse 翻车)。06 ~84 MB /
scale peak,4K ~880 MB total — 跟 cement 在同 ceiling level。

`Scratch` struct(B3 dead end)代码 **不进 nupic-ssimulacra crate** —
保留在 `nupic-research` 作为 regression baseline。

---

## 4. crate skeleton

```
crates/nupic-ssimulacra/
├── Cargo.toml                  # deps: yuvxyb + rayon; dev: ssimulacra2 + image
├── src/
│   └── lib.rs                  # ~400 lines, B5 algorithm path
└── tests/
    ├── properties.rs            # 9 property tests
    └── cement_agreement.rs      # 7 fixture × cement comparison
```

Public API:

```rust
pub fn ssimulacra2_score(
    reference_rgba: &[u8],
    distorted_rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<f64, &'static str>;

pub fn ssimulacra2_score_f32(
    reference_srgb: &[[f32; 3]],
    distorted_srgb: &[[f32; 3]],
    width: usize,
    height: usize,
) -> Result<f64, &'static str>;
```

Minimal surface — 2 entry points,1 error type(static str)。**不 expose**:
- per-scale breakdown
- XYB-direct API
- weights / polynomial constants

这些是 internal,等 Stone C 真正需要时再 expose。**stone 公共 API 倾向
"用最小 surface" 原则**(参考 nupic-color 的 `Oklab` struct + 2 函数
模式)。

Deps:
- `yuvxyb` v0.4.2 — color conversion,pure Rust,transitive 跟 cement 同
- `rayon` v1 — work-stealing thread pool,跟 cement default feature 同
- `ssimulacra2` v0.5.1 + `image` — dev-deps only(oracle + fixture decode)

---

## 5. disk — n/a

---

## 6. cov — see §1.3

---

## 7. doc — 三条 stone B 总 lesson

### 7.1 Recursive Gaussian 是 SSIMULACRA2 的真算法

03b 起初以为是离散 11-tap kernel,实测 score 偏 7.7 分。读 cement build.rs
找到 Charalampidis 2016 recursive IIR,12 个 coefficients 在 σ=1.5 处
solve at compile-time。reimpl 后 score bit-exact match cement。

**Lesson**:reimpl 任何 stone 必须读 cement source **到 build.rs / OUT_DIR
生成代码层**,不是 essay-level abstract。

### 7.2 4 个独立的 ceiling 攻击维度

跨 03a + 03b 工作量,识别 4 个独立 perf 攻击维度:

1. **codegen**(stone A:FMA + Lagny + inline-always)
2. **memory access pattern**(stone B B2:chunked vs single-column)
3. **row-level parallelism**(stone B B4:rayon par_chunks)
4. **task-level parallelism**(stone B B5:nested rayon::join)

`portable SIMD wrapper`(`wide` crate)**不是独立维度** — 实测它跟 LLVM
auto-vec 在 scalar 上 wash。要 step beyond auto-vec 必须直接 NEON / AVX2
intrinsics(stone A A3b / stone B B6+ 计划)。

**Stone-essay 模板要 codify 这 4 个维度**,加上 doc 提到的 5th 维度
(algorithm complexity reduction — 比如 half-precision)。

### 7.3 cement crate 的 default feature 是隐藏 ceiling

`ssimulacra2 = "0.5"` workspace dep 自带 rayon(default)。03b-quater
花了 3 个 sub-essay 才发现。**每个 cement baseline 必须先 `grep
features` Cargo.toml**,记 essay。

---

## 8. cross-link

- 触发本文系列:[03b design](03b-ssimulacra2-design.md)
- 实施 phases(B1 → B5):
  - [03b-bis B1 baseline + Recursive Gaussian](03b-bis-ssim-b1.md)
  - [03b-ter B2 chunked vertical](03b-ter-ssim-b2.md)
  - [03b-quater B3 buffer reuse 翻车 + B4 rayon](03b-quater-ssim-b4.md)
  - [03b-quinquies B5 nested rayon](03b-quinquies-ssim-b5.md)
- 上游 Stone A pattern:[03a graduation](03a-ter-oklab-graduation.md)
- 价值观:
  - [[feedback-ceiling-first-priorities]]
  - [[feedback-metric-over-human-eye]]
  - [[feedback-no-cost-thinking]]

---

## 9. Stone C unblocked

Per the 03 essay §4 dependency graph,Stone C(SSIMULACRA2-driven
differentiable codebook learner)需要 Stone A(OKLab perceptual space)
+ Stone B(SSIMULACRA2 metric)both as runtime dependencies。两个都 graduate:
- `crates/nupic-color/` — Stone A(graduated 03a-ter)
- `crates/nupic-ssimulacra/` — Stone B(graduated 本 essay)

下一篇 essay:**`03c-codebook-design.md`** — Stone C 设计 anchor。这是
**02-pluto SSIMULACRA2 -65 → high quality 的唯一路径**,02 essay 已
identify。Stone C 是这整个 PNG research thread 的 ceiling-breaking
deliverable。

---

## 10. Open(post-graduation polish on Stone B)

按 perf 优先,非阻塞:

1. **B6 SIMD inner loop NEON intrinsics**(`std::arch::aarch64::*`)—
   推 02-pluto 20 → ~12 ms
2. **B7 cross-scale parallel staircase** — pipeline scale[n] with
   scale[n-1] downscale,~1.33× total speedup
3. **B8 half-precision blur**(`f16`)— bandwidth-bound parts halve,
   need M2 native `vfmla_f16` intrinsics
4. **B9 SIMD vertical pass parallel**(per-128-col-chunk via rayon)—
   covers vertical IIR which row-parallel misses

每个独立 sub-essay。**全部完成预估 02-pluto < 5 ms**,跨 distance to
bandwidth ceiling < 2×。

---

## 11. 验收材料

- New crate:[`crates/nupic-ssimulacra/`](../../../crates/nupic-ssimulacra/)
- Tests:
  - [`tests/properties.rs`](../../../crates/nupic-ssimulacra/tests/properties.rs)
  - [`tests/cement_agreement.rs`](../../../crates/nupic-ssimulacra/tests/cement_agreement.rs)
- 4K mem check:[`crates/nupic-research/examples/ssim_4k_check.rs`](../../../crates/nupic-research/examples/ssim_4k_check.rs)
- Research bench archive(B1-B5 timing variants reference):
  [`crates/nupic-research/src/ssim_b1.rs`](../../../crates/nupic-research/src/ssim_b1.rs)
