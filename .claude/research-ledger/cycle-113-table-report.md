# Cycle 113 — `.nupic` minimal container size estimate — table 收尾报告

**Date**: 2026-06-18
**Verdict**: **YELLOW**(1/6 size PASS;大图可行,小图 palette overhead 主导)
**Essay**: `docs/research/png/04rrr-cycle113-nupic-size-estimate.md`(pending)
**Spike**: `crates/nupic-research/examples/cycle113_nupic_size_estimate.rs`
**Data**: `assets/png-bench/cycle113/nupic_size.tsv`

## Per-fixture estimate

`.nupic` minimal: header(17B)+ 64 × (K_u8 + 192×4 palette) + zlib_best(concat all tile index streams)

| fixture | tiny_KB | palette_KB | index_raw_KB | index_zlib_KB | .nupic_KB | ratio_vs_tiny | PASS |
|---|---:|---:|---:|---:|---:|---:|:---:|
| p115_1024x768 | 200.0 | 46.9 | 768.0 | 250.6 | 297.5 | 1.49× | ✗ |
| p125_1920x1080 | 466.7 | 46.5 | 2025.0 | 493.0 | 539.5 | 1.16× | ✗ |
| p167_1920x1080 | 442.0 | 43.8 | 2025.0 | 495.7 | 539.6 | 1.22× | ✗ |
| p175_1920x1080 | 511.0 | 48.1 | 2025.0 | 580.1 | 628.2 | 1.23× | ✗ |
| p214_2400x1600 | 1072.3 | 46.7 | 3750.0 | 907.6 | 954.3 | 0.89× | ✗(close)|
| **p274_3840x2560** | 2443.8 | 45.6 | 9600.0 | 1678.6 | **1724.2** | **0.71×** | **✓** |

**1/6 PASS, mean ratio 1.11× tiny**。

## 关键 finding

- **Palette overhead 47 KB fixed**(64 tile × 192 color × 4 byte = 49152 bytes)
- **大图 palette 稀释**:p274 9.83 MP tiny 2.4 MB → palette 47 KB = 1.9% of budget → ratio 0.71×
- **小图 palette 主导**:p115 0.79 MP tiny 200 KB → palette 47 KB = 23% of budget → ratio 1.49×
- **index zlib 压缩率**:raw → zlib 大约 30% retention(p115 768 KB → 251 KB,p274 9600 KB → 1679 KB)

## Cycle 114 改善方向(待选)

1. **Palette sharing**(global super-palette K=512/1024 + per-tile selection bitmap)— palette overhead 从 47 KB 降到 ~2-4 KB,小图也可行
2. **Stronger entropy coder for index stream**(LZMA / zstd / arithmetic coding instead of zlib)— index_zlib 占大头(p115 251 KB / total 297 KB = 84%)
3. **Pivot to paper writeup**(Cycle 106-112 数据 sufficient,`.nupic` 仍需多 cycle 工程)

## Workflow speed

| spike | jobs | wall |
|---|---:|---:|
| cycle113_nupic_size_estimate | 6 fixture | **1.9 s** ✓ |

完美 ≤ workflow target。
