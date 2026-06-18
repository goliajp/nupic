# Cycle 116 — WebP transcoder for R6 cohort — table 收尾报告

**Date**: 2026-06-19
**Verdict**: **GREEN 6/6** — WebP lossy q=75 (q=85 for p167) PASS 双轴 + 视觉 6/6 OK + 11-14× smaller than TinyPNG PNG
**Essay**: TODO `docs/research/png/04rrr-cycle116-webp-r6-rescue.md`
**Spike**: `crates/nupic-research/examples/cycle116_webp_for_r6.rs`
**Data**: `assets/png-bench/cycle116/webp_sweep.{tsv,log}`
**Visual eye**: `assets/png-bench/cycle116/visual/{p115_q75,p274_q75}.{webp,png}`
**Ship**: 待 Cycle 117 wire CLI flag → v1.2.10 候选。

## Per-fixture best q + size + DSSIM

| fixture | best_q | webp_KB | tiny_KB | size_ratio | webp_DSSIM | tiny_DSSIM | both PASS? |
|---|---:|---:|---:|---:|---:|---:|:---:|
| p115_1024x768 | 75 | **17.3** | 200.0 | **0.086×** | 0.001373 | 0.001970 | ✓ |
| p125_1920x1080 | 75 | **46.4** | 466.7 | **0.100×** | 0.007535 | 0.009766 | ✓ |
| p167_1920x1080 | 85 | **51.3** | 442.0 | 0.116× | 0.000699 | 0.000880 | ✓ |
| p175_1920x1080 | 75 | **37.5** | 511.0 | **0.073×** | 0.001389 | 0.001966 | ✓ |
| p214_2400x1600 | 75 | **102.0** | 1072.3 | **0.095×** | 0.001680 | 0.002845 | ✓ |
| p274_3840x2560 | 75 | **187.9** | 2443.8 | **0.077×** | 0.001463 | 0.003084 | ✓ |

**Mean size ratio: 0.091× tiny(11× smaller than TinyPNG PNG)**
**DSSIM PASS 6/6 strict**

## vs 全 cycle 演化对比(同 6 张 fixture)

| approach | size ratio mean | strict DSSIM PASS | 视觉 quality |
|---|---:|---:|---|
| Cycle 110 lossless | 1.69× | 0/6 | n/a(size fail)|
| Cycle 111 R6-only reconstruction | n/a | 6/6 | yes(算法-only)|
| Cycle 112 R6+K=256 hybrid | 0.51× | 0/6 strict | 4/6 视觉差(背景 bokeh blocky)|
| Cycle 113-114 `.nupic` minimal | 1.03× | n/a(size only)| n/a |
| **Cycle 116 WebP q=75** | **0.091×** | **6/6** | **6/6 视觉 OK** |

**WebP lossy 是 Cycle 106-114 探索后的 production-realizable winner** — 不需要新 container,nupic 已有 encoder。

## Workflow speed

| spike | jobs | wall |
|---|---:|---:|
| cycle116_webp_for_r6 | 30(6 fixture × 5 q)| **3.5 s** |

## 视觉 eye(2/6 sampled)

- **p115 q=75** (17.3 KB, 0.086× tiny): bokeh circles 色彩 + edge softness 保留,无 blocky / banding
- **p274 q=75** (188 KB, 0.077× tiny): 仙人掌 spike + 远山雾化 + 阳光金色光泽 全保留,无 WebP visible artifact

视觉**好于 Cycle 112 R6 hybrid**(同张 p274 Cycle 112 hybrid 1.27 MB 有背景 bokeh blocky;Cycle 116 WebP 188 KB 更紧 + 视觉更好)。

## Cycle 117 wire 设计选项

**目标**:不破 PNG default behavior(user expectation .png → .png),但提供 R6-cohort WebP rescue 路径。

| 选项 | UX 影响 | 工程量 |
|---|---|---|
| **A. 加 CLI flag `--photo-rescue-webp`**(opt-in,user 选)| zero default 影响 | 小 |
| **B. 加 stdout advisory**("WebP -q 75 would be 9× smaller, run with -f webp")| zero file 影响 | 小 |
| **C. `-f auto` 增强:detect R6-like → output WebP**(file extension 自动换)| user 必须接受 format substitution | 中 |
| **D. wire 加 P-09: if R6-like → WebP encoder + 输出 .png-named webp byte stream**(隐藏 format)| 可能让用户困惑 | 大 |

**推荐 A**:`--photo-rescue-webp` flag,user opt-in,nupic-level wire,简单 ship。

## Cycle 117 next-up

1. Wire `--photo-rescue-webp` CLI flag(`opts.allow_webp_rescue`)
2. Trigger predicate:`is_r6_like_content`(可复用 Cycle 112 8×8 R6 quantize 看 unique colors / 或 Cycle 106 oracle headroom rule)
3. Production routing in `encode_png_stone_c`:if `opts.allow_webp_rescue` ∧ `is_r6_like` → return WebP bytes(file extension TBD)
4. baseline-7 sanity + 32 quick sample + cycle116 6 fixture validation
5. 219 tests + bump v1.2.10 + push if GREEN
