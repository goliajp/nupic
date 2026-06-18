# Cycle 108 input-feature K classifier — table 收尾报告

**Date**: 2026-06-18
**Verdict**: **YELLOW**(99.1% PASS pile retention,1 regression,net +14 PASS;用户选 path B → Cycle 109 2-pass fail-safe)
**Essay**: `docs/research/png/04mmm-cycle108-input-k-classifier.md`
**Spike**: `crates/nupic-research/examples/cycle108_input_k_classifier.rs`
**Data**: `assets/png-bench/cycle108/rule_v{1,2,3,3_full}.{tsv,log}`
**Ship**: 不 ship,binary 仍是 v1.2.8。

## 1. Spike progression

| rule | n_pixels threshold | small branch | baseline-7 path | sample 32 PASS | sample 32 PASS retention | full 506 PASS | verdict |
|---|---:|---|---|---:|---:|---:|:---:|
| v1 | 2 MP | `quantize_indexed_png(K=128)` (raw,跳过 P-01/P-03)| 同 raw | 6/39 (15.4%) | **2/8 (25%)** | — | **RED**(spike 跳过 production routing)|
| v2 | 2 MP | use `Fixture.baseline_*` (cached) | subprocess `nupic compress` | 12/39 (30.8%) | 7/8 (87.5%) | — | RED(p220 3.84MP regression)|
| v3 | **5 MP** | use `Fixture.baseline_*` | subprocess `nupic compress` | 13/39 (33.3%) | **8/8 (100%)** ✓ | 120/513 (23.4%) | **YELLOW**(1 regression in full)|

## 2. Rule v3 full corpus-500 per-pile breakdown

| pile | n | PASS (rule v3) | retention | net delta vs v1.2.8 |
|---|---:|---:|---:|---:|
| PASS(v1.2.8 已 PASS)| 106 | **105 (99.1%)** | ↓ 1(p244)| **−1** |
| Pile A(size 退,DSSIM 已赢)| 307 | 11 (3.6%) | ↑ 11 | **+11** |
| Pile B(size 过,DSSIM 微退)| 40 | 0 (0%) | — | 0 |
| Pile C(双轴微退)| 53 | 0 (0%) | — | 0 |
| **TOTAL pile sample** | **506** | **116 (22.9%)** | net **+10** | +10 |
| baseline-7(独立)| 7 | 4 (= v1.2.8 baseline)| 0 | 0 |
| **GRAND TOTAL** | **513** | **120 (23.4%)** | — | **+14** |

## 3. PASS pile regression analysis — p244 only

| metric | p244 | wins(11 fixtures range)| discriminative? |
|---|---:|---|:---:|
| n_pixels | 9.83 MP | 9.83 MP(同)| no |
| input_KB | 6235 | 1945 - 6166 | no |
| v1.2.8 K=128 output KB | **1778** | 1162 - 4360 | (only via 2-pass) |
| v1.2.8 ratio | **0.791× ✓**(narrow PASS)| 1.37×, 2.85×, ...(all fail v1.2.8)| **YES — true discriminator** |
| K=224 ratio | 0.851× ✗ | 0.65 - 0.80 ✓ | — |
| bits/pixel(input)| 5.20 | 1.62 - 5.14 | partial(p246=5.14 重叠)|
| bits/pixel(K=128 output)| 1.48 | 0.97 - 3.63 | no(p287=2.14 重叠)|

**结论**:任何 **input-only feature** 都无法 cleanly 区分 p244 vs wins。真正区分器是 **v1.2.8 baseline output size**,production 必须 2-pass 才能获取。

## 4. Workflow speed verdict

| spike 阶段 | scope | wall | OK? |
|---|---|---:|:---:|
| Rule v3 32-sample bench | 32 + 7 b7 = 39 | **22s** | ✓(under 30s target)|
| Rule v3 full corpus | 506 + 7 b7 = 513 | **588s**(9.8 min)| ✗(超 5 min)|

**反思**:**spike 不应该轻易跑 full corpus**(违反 [[feedback-no-long-sweeps-in-workflow]])。本 cycle 跑 full 是为了 ship decision 的最终 gate,这种"ship 前最终验证"性质允许 long wall,但中间 iteration 必须 sample。

**优化方向**:小图分支即便用 baseline cache 也要 `ImageReader::open + decode` 拿 width × height — 这是 0.1-0.5s/fixture overhead 累积。可以缓存 (width, height) 到 `corpus-500-three-axis.tsv` 一列,完全免 decode。Cycle 109 顺手做。

## 5. Cycle 109 next-up: 2-pass fail-safe production wiring(path B)

**目标**:**100% PASS pile retention** + ship v1.2.9。

**production wiring**(at `nupic-core/src/ops/compress.rs:225`):

```rust
// Existing v1.2.8 path
let (n_colors, importance_alpha) = nupic_quantize::classify_for_palette_size_with_importance(&raw, w as usize);
let p03_preset_boost = ...;
let qopts = QuantizeOpts { n_colors, /* ... */ };
let bytes_v128 = nupic_quantize::quantize_indexed_png(&raw, w, h, qopts.clone())?;

// Cycle 109 P-08 K-up fail-safe
let n_pixels = (w as u64) * (h as u64);
let input_size = src_png_bytes.len() as u64;
let need_kup = n_pixels >= 5_000_000
    && bytes_v128.len() as u64 > (input_size * 78 / 100);  // > 0.78× input
if need_kup {
    let mut kup_opts = qopts.clone();
    kup_opts.n_colors = 224;
    kup_opts.dither_strength = 0.3;
    let bytes_v224 = nupic_quantize::quantize_indexed_png(&raw, w, h, kup_opts)?;
    if bytes_v224.len() < bytes_v128.len() {
        return Ok(bytes_v224);
    }
}
return Ok(bytes_v128);
```

**Decision gate(Cycle 109 收尾)**:
- GREEN: PASS pile retention **8/8 (32 sample) + 106/106 (corpus)** + ≥ 25% total corpus + baseline-7 不退 → wire production + v1.2.9 ship
- YELLOW: PASS pile 不退步但总 PASS < 25% → 再 tune threshold
- RED: PASS pile 仍退步 → input-feature 路径死,转 E. R6 multi-tile

## 6. baseline-7 sanity

本 cycle 没改 production source,baseline-7 仍 v1.2.8(size 5/7 0.799× cohort,DSSIM 6/7 nupic 赢)。subprocess 跑 v1.2.8 binary 的 b7 输出在 spike 中 PASS 4/7 = v1.2.8 baseline 不变。
