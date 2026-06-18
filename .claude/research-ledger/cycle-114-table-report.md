# Cycle 114 — K-high probe + zopfli replacement — table 收尾报告

**Date**: 2026-06-19
**Verdict**: **YELLOW(同 Cycle 113)**;**Pivot Cycle 115 → paper writeup**
**Spike**:
- `crates/nupic-research/examples/cycle114_k_high_probe.rs`(K-high RED — imagequant 256 hard ceiling)
- `crates/nupic-research/examples/cycle113_nupic_size_estimate.rs`(env `CYCLE114_ZOPFLI=1` zopfli switch)
**Data**: `assets/png-bench/cycle114/{k_high.log, nupic_zopfli.tsv}`

## Test 1: K-high global probe(RED, imagequant ceiling)

试 K∈{256, 512, 1024, 4096} global imagequant on 6 张 fixture。

- imagequant `set_max_colors(k)` panics `ValueOutOfRange` for K>256
- **imagequant 256 是 hard library ceiling** — K-high 全图 quantize 路径不可走
- 真测 K=512+ 需要自己实现 k-means / median-cut — Cycle 115+ 工程

## Test 2: zopfli 替换 zlib(改善 ~8% 但 1/6 PASS 不变)

| fixture | tiny_KB | zlib_KB | zopfli_KB | reduction | zlib_ratio | zopfli_ratio | PASS |
|---|---:|---:|---:|---:|---:|---:|:---:|
| p115_1024x768 | 200.0 | 297.5 | **276.8** | -7.0% | 1.49× | 1.38× | ✗ |
| p125_1920x1080 | 466.7 | 539.5 | **492.6** | -8.7% | 1.16× | 1.06× | ✗ |
| p167_1920x1080 | 442.0 | 539.6 | **497.5** | -7.8% | 1.22× | 1.13× | ✗ |
| p175_1920x1080 | 511.0 | 628.2 | **575.6** | -8.4% | 1.23× | 1.13× | ✗ |
| p214_2400x1600 | 1072.3 | 954.3 | **887.9** | -7.0% | 0.89× | 0.83× | ✗(close)|
| p274_3840x2560 | 2443.8 | 1724.2 | **1579.4** | -8.4% | 0.71× | 0.65× | **✓** |

**Mean ratio**:zlib 1.11× → zopfli 1.03× tiny(减 8%)
**PASS rate**:**1/6 unchanged**(p274)
**Wall**:zlib 1.9s → zopfli 21s(11×)— acceptable for size budget check,not production

## 关键 finding:小图 size-cap 本质不可达

`.nupic` 8×8 K=192 minimal:
- **Palette overhead 47 KB fixed**(64 tile × 192 × 4)
- **Per-pixel index** 8 bit raw → zopfli 30-35% retention

对 p115(tiny 200 KB):
- Even with palette sharing(降到 15 KB)+ zopfli index(230 KB):total 250 KB / 200 KB = 1.25× tiny → FAIL
- TinyPNG 在小图上是 K=64-128 lossy quantize,**nupic 任何 K=192+ structure 都 structurally bigger**

**R6 multi-tile ship 路径只 viable on 大图**(p274 0.65× tiny ✓,p214 close-but-fail 0.83×)。

## Pivot decision

| Cycle 115+ 候选 | 评估 |
|---|---|
| **Paper writeup**(Cycle 106-112 数据 sufficient)| ★★★★★ kernel 已捕获;.nupic 工程 multi-cycle 但 marginal commercial value(只大图可救)→ **推荐** |
| Palette sharing engineering | 减 32 KB 但救不了 p115 / p125 / p167 / p175 |
| K=512+ global quantize(需自己写 k-means)| 工程深;**不一定**比 R6 更紧(因为 single-palette ceiling on DSSIM 仍存在)|
| LZMA preset 9 替换 zopfli | 可能再减 5-10%,仍救不了小图 palette floor |

**Cycle 115 = Paper writeup**(Section 1 abstract + intro + related work outline,可在一个 cycle 完成)。
