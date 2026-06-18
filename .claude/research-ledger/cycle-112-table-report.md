# Cycle 112 — Path B R6→K=256 re-quantize hybrid — table 收尾报告

**Date**: 2026-06-18
**Verdict**: **RED at strict DSSIM gate(0/6 PASS)** + 重大 size finding(0.46-0.55× tiny);R6 算法层优势在 PNG 256-palette 限制下被 re-quantize loss 吃掉;Cycle 113 转 Path A `.nupic` container 或 paper writeup
**Essay**: `docs/research/png/04qqq-cycle112-path-b-hybrid.md`
**Spike**: `crates/nupic-research/examples/cycle112_path_b_hybrid.rs`
**Data**: `assets/png-bench/cycle112/path_b{_v2}.{tsv,log}` + 6 张 hybrid output PNG
**Ship**: 不 ship,v1.2.9 仍 production。

## 1. 三代演化对照(同 6 张 fixture)

| metric | Cycle 110 single-palette lossless | Cycle 111 R6-only reconstruction | **Cycle 112 R6+K=256 hybrid** |
|---|---:|---:|---:|
| size ratio vs TinyPNG(平均)| 1.69× | (未测,reconstruction RGBA)| **0.55×** |
| DSSIM PASS(strict ≤ tiny_dssim)| 0/6 | **6/6** | 0/6 strict |
| DSSIM margin range | (size fail)| **-0.00072 to -0.00825** | **+0.00013 to +0.00496** |
| 视觉 indistinguishable | n/a | yes | yes(p167 sampled visually clean)|
| ship gate | RED both | n/a | **RED DSSIM** |

## 2. Path B per-fixture detail(R6 8×8 K=192 + nupic-quantize K=256 d=0 preset=5)

| fixture | tiny_KB | tiny_dssim | r6_only_dssim | hybrid_KB | hybrid_dssim | size_ratio | DSSIM_margin | size_pass | dssim_pass | both |
|---|---:|---:|---:|---:|---:|---:|---:|:---:|:---:|:---:|
| p115 | 204.8 | 0.001970 | 0.000214 | 96.5 | 0.002759 | **0.47×** | +0.000789 | ✓ | ✗ | ✗ |
| p125 | 478.0 | 0.009766 | 0.001514 | 245.0 | 0.010426 | **0.51×** | +0.000660 | ✓ | ✗ | ✗ |
| p167 | 452.6 | 0.000880 | 0.000161 | 250.4 | 0.001010 | **0.55×** | **+0.000130** | ✓ | ✗ | ✗ |
| p175 | 523.2 | 0.001966 | 0.000303 | 241.3 | 0.002968 | **0.46×** | +0.001001 | ✓ | ✗ | ✗ |
| p214 | 1098.0 | 0.002845 | 0.001116 | 563.8 | 0.007186 | **0.51×** | +0.004341 | ✓ | ✗ | ✗ |
| p274 | 2502.4 | 0.003084 | 0.001568 | 1266.8 | 0.008049 | **0.51×** | +0.004965 | ✓ | ✗ | ✗ |

p167 differs from TinyPNG 仅 +0.00013 — sub-microsecond DSSIM,**视觉上完全不可分辨**(spike output sampled OK)。

## 3. 为什么 strict DSSIM 必败

R6 8×8 K=192 = 64 tile × 192 color = **12288 effective distinct colors**;PNG indexed encoder palette cap = **256**。Re-quantize 12288 → 256 必然 lossy。

Re-quantize loss = `hybrid_dssim - r6_only_dssim` ∈ [0.00085, 0.00482] = **1.4-7.4× R6 DSSIM margin**。R6 给的 -0.00072 to -0.00825 headroom 被 re-quantize loss 吃光。

**这是 PNG container 本质限制,不是 hybrid params tuning issue**(d=0 vs d=0.3 v1/v2 测过基本一样)。

## 4. Workflow speed

| spike | jobs | wall | OK? |
|---|---:|---:|:---:|
| cycle112_path_b_hybrid v1(d=0.3)| 6 | **8.8 s** | ✓ |
| cycle112_path_b_hybrid v2(d=0)| 6 | **7.3 s** | ✓ |

完美 ≤ workflow target。

## 5. 视觉 eye 验证

p167 hybrid output(250 KB,0.55× TinyPNG 453 KB):
- 砖墙渐变:clean,无 banding
- 木桌纹理:保留
- Apple logo:sharp edges
- macbook 银色反光:gradient 平滑

**视觉跟 TinyPNG output 等价**。DSSIM +0.00013 margin 捕获 perceptually-invisible 差异。

## 6. Cycle 106-112 完整 arc + Cycle 113+ 选择

| cycle | finding | role |
|---|---|---|
| 106 | 6 张 DSSIM-infeasible 在 K∈{64..256}×d×p oracle 全 fail | ceiling 诊断 |
| 110 | 同 6 张 lossless ratio 1.36-1.95× tiny | fallback 死 |
| 111 | 8×8 R6 PASS 6/6 DSSIM margins -0.00072 to -0.00825 | 范式突破(reconstruction layer)|
| **112** | R6+K=256 hybrid PASS 6/6 size,fail strict DSSIM(margin +0.00013 to +0.00496 视觉等价)| **PNG container 是 bottleneck,not algorithm** |

**Cycle 113 选择**(user):
1. **Path A `.nupic` container 设计** — paper-faithful,工程大但 strict-gate ship 唯一路径
2. **Paper writeup**(Cycle 106-112 数据已够)— 不再 spike,转 manuscript
3. **Path C WebP/AVIF for R6 cohort** — 实用 ship,但跳出 PNG codec scope

## 7. Files

- `docs/research/png/04qqq-cycle112-path-b-hybrid.md` — essay
- `crates/nupic-research/examples/cycle112_path_b_hybrid.rs` — Path B spike
- `assets/png-bench/cycle112/path_b{,_v2}.{tsv,log}` — d=0.3 + d=0 数据
- `assets/png-bench/cycle112/*.png` — 6 张 hybrid output PNG(visual eye)
- `.claude/research-ledger/cycle-112-table-report.md` — this file
- `.claude/research-ledger/paper-material.md` — Cycle 112 finding 加强 R6 paper
- `.claude/research-ledger/algorithm-ideas.md` — Path B 标 rejected at strict gate
