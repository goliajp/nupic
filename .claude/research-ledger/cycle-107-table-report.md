# Cycle 107 single-config K=224 — table 收尾报告

**Date**: 2026-06-18
**Verdict**: **RED for single-config production routing**(K=224 让 PASS pile 退化 16-25%)
**Essay**: `docs/research/png/04lll-cycle107-single-config-dead.md`
**Data**:
- `assets/png-bench/cycle107/pile_classification.tsv` — corpus-500 506 张 per-pile 分类
- `assets/png-bench/cycle107/single_config_sample.{tsv,log}` — 100-sample 单 config
- `/tmp/c107q.tsv` — 32-quick-bench(未落盘,见 essay 内嵌数据)

**Spike**:
- `crates/nupic-research/examples/cycle107_single_config_sample.rs`
- `crates/nupic-research/examples/cycle107_quick_single.rs`(新工具链 demo)

**Workflow tooling delivered**:
- `crates/nupic-research/src/bench.rs` — `Fixture` + `load_corpus_500_with_baseline` + `pile_sample_24` + `bench_pool`

**Ship**: 不 ship,binary 仍是 v1.2.8。

## 1. corpus-500 二轴分类(基础数据)

| Pile | 定义 | n | % |
|---|---|---:|---:|
| PASS | size ≤ 0.80× tiny ∧ DSSIM ≤ tiny | 106 | 20.9% |
| Pile A | size > 0.80× tiny ∧ DSSIM ≤ tiny | **307** | **60.7%** |
| Pile B | size ≤ 0.80× tiny ∧ DSSIM > tiny | 40 | 7.9% |
| Pile C | size > 0.80× tiny ∧ DSSIM > tiny | 53 | 10.5% |

**关键修正**:Cycle 106 的"Pile A"是 `corpus-500-pile-a.tsv` 31 张极端尾(top 10% size 浪费),真实 Pile A 容量 307。Cycle 106 攻面利用率只有 31/307 = **10%**。

## 2. Pile B / Pile C 退步分布

| Δdss bucket | Pile B (n=40) | Pile C (n=53) |
|---|---:|---:|
| < 0.001 (视觉不可分辨) | **30 (75%)** | **48 (90.6%)** |
| 0.001 - 0.005 | 10 (25%) | 5 (9.4%) |
| 0.005 - 0.020 | 0 | 0 |
| ≥ 0.020 | 0 | 0 |

Pile B/C 退步都极轻 — 理论上"轻微 K 调整就能反超"。**但 Cycle 107 测出 single-config K=224 无法实现**。

## 3. Single-config K=224 d=0.3 p=6 全 sample 表

### 100-fixture stratified sample(25 per pile)

| pile | n | both_PASS | size_pass | dssim_pass |
|---|---:|---:|---:|---:|
| PASS(原 v1.2.8 过) | 25 | 21 (84%) | 23 (92%) | 23 (92%) |
| Pile A | 25 | **0 (0%)** | 2 (8%) | 23 (92%) |
| Pile B | 25 | 1 (4%) | 20 (80%) | 5 (20%) |
| Pile C | 25 | **0 (0%)** | 2 (8%) | 11 (44%) |
| **TOTAL** | **100** | **22 (22.0%)** | 47 (47%) | 62 (62%) |

**红灯**:**原 PASS pile 退化 4/25 = 16%** — 任何 production 替换都不能接受让现有 PASS pile 退步。

### 32-quick-bench(8 per pile)— 工具链 demo

| pile | n | PASS | 备注 |
|---|---:|---:|---|
| PASS | 8 | 6 | **退化 2/8 = 25%** |
| Pile A | 8 | 1 | 12.5% |
| Pile B | 8 | 0 | 0% |
| Pile C | 8 | 0 | 0% |
| **TOTAL** | **32** | **7 (21.9%)** | 跟 100-sample 趋势一致 |

## 4. Workflow tooling 速度对照

| 维度 | 老 cycle 102-105(SSIM 时代) | 新 cycle 107(本 cycle)| 备注 |
|---|---|---|---|
| sample size | 7(baseline-7) | 32(pile stratified 8×4) | 4.5× 数据量 |
| metric | SSIMULACRA2 × 1(subprocess) | DSSIM × 1(in-process,tinypng 已 cache) | DSSIM 比 SSIM2 快 3-5× |
| wall total | ~12 秒 | **54 秒** | 4.5× sample → 4.5× wall(per-fixture 持平 ~1.7s) |
| CPU 占用 | 单核 | 4 核(`NUPIC_BENCH_THREADS=4`) | 留 9 核给系统 UI |
| 数据复用 | 无 | tinypng_size / tinypng_dssim / pile 全 cache | 0 ms baseline 数据 |

**关键修复 — 不再卡机**:Cycle 107 第一刀错用 `par_iter` 默认 13 核全占(1304% CPU),user UI freeze。bench_pool 限 4 核后机器响应正常,wall 也合理。

**关键修复 — 不再重算 baseline**:之前每个 spike 都 `dssim_of_path(tiny)` 重算 TinyPNG DSSIM,纯浪费 50% 工作量。bench module 把 baseline 全 pre-load 到 `Fixture` struct,inner loop 只算 nupic 输出 DSSIM。

## 5. Decision gate matrix

| 选项 | 决策 |
|---|---|
| GREEN(production wire,v1.2.9 ship) | ✗ |
| YELLOW(R4 微调,有进步但不 ship) | ✗ |
| **RED(single-config 路径死路)** | ✓ — 下 cycle 必须 input feature classifier |

## 6. Cycle 108 next-up entry(自然延续)

- **Spike**: `cycle108_input_k_classifier.rs`
- **训练集**: `assets/png-bench/cycle106-r4/pile_a_grid.tsv` 31 fixture × 21 config 的 per-fixture optimal (K, d) ground truth
- **Features**: opaque_fraction / palette_pre_cluster_spread / luma_gradient_entropy / chroma_variance(候选,Cycle 102 P-01/P-03 的 feature pool 直接复用)
- **Validation**: 32 quick-bench sample(8 each pile)— 必须保 PASS pile 不退步
- **Decision gate**:
  - GREEN: 32 sample PASS ≥ 50%(↑ 显著 vs baseline 21.9%)AND PASS pile 不退 → 设计 v1.2.9 routing
  - YELLOW: 32 sample PASS 30-50% AND PASS pile 不退 → 续 Cycle 109 tune
  - RED: PASS pile 退步 OR < 30% → input classifier 也救不了,转 R6 multi-tile / lossless fallback

## 7. baseline-7 sanity

本 cycle 没改 production source,跳详细 sanity 表(Cycle 106 commit 后未改动)。
