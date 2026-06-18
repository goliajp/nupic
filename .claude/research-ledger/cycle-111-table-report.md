# Cycle 111 R6 multi-tile feasibility — table 收尾报告

**Date**: 2026-06-18
**Verdict**: **GREEN(algorithm-level paper kernel)**;**encoder integration deferred to Cycle 112+**
**Essay**: `docs/research/png/04ppp-cycle111-r6-multitile-greenlight.md`
**Spike**: `crates/nupic-research/examples/cycle111_r6_multitile_probe.rs`
**Data**: `assets/png-bench/cycle111/r6_probe{_v2}.{tsv,log}`
**Ship**: 不 ship,v1.2.9 仍 production。

## 1. Per-fixture R6 PASS verdict

8×8 tile × K=192 是统一 winning config(6/6 fixture 一致)。

| fixture | tile_n × K winner | R6 DSSIM | tiny_DSSIM | margin |
|---|---|---:|---:|---:|
| p115_1024x768 | **8×8 × 192** | 0.000214 | 0.001970 | **-0.001756** |
| p125_1920x1080 | **8×8 × 192** | 0.001514 | 0.009766 | **-0.008252** |
| p167_1920x1080 | **8×8 × 192** | 0.000161 | 0.000880 | **-0.000719** |
| p175_1920x1080 | **8×8 × 192** | 0.000303 | 0.001966 | **-0.001663** |
| p214_2400x1600 | **8×8 × 192** | 0.001116 | 0.002845 | **-0.001729** |
| p274_3840x2560 | **8×8 × 192** | 0.001568 | 0.003084 | **-0.001516** |

**R6 PASS 6/6 vs Cycle 106-110 single-palette PASS 0/6**。

## 2. Tile coarseness vs DSSIM margin(spike sweep)

| tile_N | K=64 PASS / 6 | K=128 PASS / 6 | K=192 PASS / 6 |
|---:|---:|---:|---:|
| 2 | 0 | 1 | n/a |
| 3 | 0 | 4 | n/a |
| 4 | 1 | 5 | n/a |
| 6 | n/a | 6 | n/a |
| 8 | n/a | 6 | **6 ✓** |

8×8 K=192 是单点 sweet spot — coarser tiles 在 high-frequency content 上 quantization 不够 spatial-aware,K=64 总 palette 太小。

## 3. Workflow speed

| metric | result |
|---|---:|
| spike file | `cycle111_r6_multitile_probe.rs` |
| jobs | 54(9 configs × 6 fixtures)|
| wall | **9 s** (4-core via `bench_pool`)|
| 平均 per-job | ~0.7 s |

完美 ≤ workflow target,远低于 Cycle 110 的 12 min full-corpus wall。

## 4. Cycle 106-110-111 完整 arc

| cycle | finding | role |
|---|---|---|
| 106 | 6 张 DSSIM-infeasible 在 K∈{64..256}×d×p oracle 全 fail | ceiling 诊断 |
| 110 | 同 6 张 lossless fallback ratio 1.36-1.95× tiny | fallback 路径死 |
| **111** | 8×8 tile × K=192 R6 PASS **6/6**, margins -0.00072 to -0.00825 | **范式突破** |

这 3 个 cycle 形成 paper 主线核心 evidence。

## 5. Cycle 112+ encoder integration paths

R6 8×8 K=192 reconstruction 总 unique colors = 64 tile × 192 = 12288,**超 PNG palette 256 ceiling 48×**。Production ship 必须:

- **Path A: tile-aware container**(.nupic / .npc 文件格式)— 工程量大,paper-faithful
- **Path B: R6 → re-quantize K=256 hybrid** — 用 R6 reconstruction 当 imagequant K=256 starting point,看 R6 优势能保留多少 → Cycle 112 直接测,**最可能 ship**
- **Path C: WebP / AVIF transcoder for R6 cohort** — 这些 format 原生支持 spatial color variation,但跳出 PNG codec scope

**Cycle 112 推荐 Path B**(cheapest,ships inside PNG)。

## 6. Decision

- **No v1.2.10 ship**:R6 算法 GREEN 但 encoder 未做
- **Paper material captured**:Cycle 111 R6 ceiling-break 是 ★★★★★ paper kernel data
- Cycle 112 next-up:Path B re-quantize hybrid spike

## 7. Files

- `docs/research/png/04ppp-cycle111-r6-multitile-greenlight.md` — essay
- `crates/nupic-research/examples/cycle111_r6_multitile_probe.rs` — R6 spike
- `assets/png-bench/cycle111/r6_probe{,_v2}.{tsv,log}` — feasibility data
- `.claude/research-ledger/cycle-111-table-report.md` — this file
- `.claude/research-ledger/paper-material.md` — R6 finding 新增
- `.claude/research-ledger/algorithm-ideas.md` — idea E status → algorithm-GREEN
