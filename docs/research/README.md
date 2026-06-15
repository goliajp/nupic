# nupic research

Research thread backing the codec work — long-form essays + measurable
experiments. Each essay must:

1. Quote numbers that come from a tool we can re-run (`nupic bench`,
   a script in `crates/nupic-research/examples/`, or a one-line shell
   command pinned in the essay)
2. State its **mathematical / physical ceiling** and how far we are from
   it (Shannon entropy, Voronoi-optimal palette, perceptual metric
   bound, …)
3. List its **open questions** — what could falsify the conclusion, what
   we haven't measured yet
4. Reference upstream paper / source / docs by URL or commit-hash

Essays that don't meet these bars get rewritten, not merged.

**Since [03-perceptual-stone.md](png/03-perceptual-stone.md) (2026-06-15)
all essays additionally follow the ceiling-first priorities**: each
section quotes a perf / mem / disk / cov / doc ceiling and the current
distance from it, in that priority order. See
[`feedback_ceiling_first_priorities.md`](../../../../.claude-profile-1/projects/-Users-doracawl-workspace-labs-lab29-nupic/memory/feedback_ceiling_first_priorities.md)
in memory for the rule.

## Layout

```
docs/research/
  README.md              ─ this file
  png/
    00-attack-surface.md ─ v0.4.0 站点视角下的 PNG 攻击面 + top-5 排序
    01-...               ─ deep dives, one per attack point
crates/nupic-research/   ─ experiments backing the essays
```

## Companion docs (pre-research, kept as theory anchors)

- [`../png-pipeline.md`](../png-pipeline.md) — PNG pipeline 数学松弛分析(pre-v0.3,理论)
- [`../roadmap.md`](../roadmap.md) — 8-阶段 self-built codec roadmap
- [`../references.md`](../references.md) — paper / source crate / 行业链接
- [`../requirements.md`](../requirements.md) — 项目宪法约束(0 deps、跨平台、PNG 优先)

## Current essays

- [`png/00-attack-surface.md`](png/00-attack-surface.md) — anchor 篇
- [`png/01-pluto-case.md`](png/01-pluto-case.md) — 02-pluto 上 imagequant 的 algorithmic ceiling;cement-layer adaptive q_target spec(基于 DSSIM,已被 02 supersede)
- [`png/02-perceptual-metrics.md`](png/02-perceptual-metrics.md) — 用 SSIMULACRA2 替代肉眼判断;7 图 metric 对照得 nupic 0.4 在 SSIMULACRA2 上 5/7 胜 TinyPNG;给出 metric-grounded 0.4.1 cement-fix spec(总体 0.95× TinyPNG,质量不退化)
- [`png/03-perceptual-stone.md`](png/03-perceptual-stone.md) — stone-layer 设计 anchor;5 stones (OKLab / SSIMULACRA2 / codebook / dither / filter-search) 每个的 perf/mem/disk/cov/doc ceiling 数字 + 依赖图 + 子 essay roadmap。Ceiling-first 价值观的首次落地。
- [`png/03a-oklab-design.md`](png/03a-oklab-design.md) — Stone A 详细设计 + 实测;naive scalar 8.18 ms / 02-pluto 精准命中 03 估计;oklab crate (LUT) 1.88 ms;距离 bandwidth ceiling 0.06 ms 还有 31×;给 stone A0→A4 attack plan。
- [`png/03a-bis-oklab-simd.md`](png/03a-bis-oklab-simd.md) — Stone A perf 推进:LUT + Halley cbrt 2.53 ms;`wide` portable SIMD 翻车;A3a FMA + Lagny scalar 0.66 ms / 02-pluto(穿过 graduation 阈值 < 1 ms)。
- [`png/03a-ter-oklab-graduation.md`](png/03a-ter-oklab-graduation.md) — Stone A graduates 进 `crates/nupic-color/`。6 项 graduation criteria 全过(perf 0.66 ms / mem `RECOMMENDED_TILE_PIXELS` + tiled API / cov 9 props + 5 fixture + 32 K oracle assertions / doc cross-link)。Stone B unblocked。
- [`png/03b-ssimulacra2-design.md`](png/03b-ssimulacra2-design.md) — Stone B 设计 anchor。修正 03 essay 的 OKLab 误判(SSIMULACRA2 用 XYB,跟 Stone A 并联非串联);cement baseline 实测 32 ms / 02-pluto(vs 03 估 100 ms over-conservative);bandwidth ceiling 2.6 ms;graduation target < 10 ms(B3 SIMD)。
- [`png/03b-bis-ssim-b1.md`](png/03b-bis-ssim-b1.md) — Stone B B1 baseline reimpl。三轮迭代发现 cement 用 Recursive Gaussian(Charalampidis 2016),不是离散 11-tap;reimpl 后 **score bit-exact match cement (diff = 0.0000)**;timing B1 1.3-1.9× cement 因为 single-column vertical scan;B2 = chunked vertical pass。
- [`png/03b-ter-ssim-b2.md`](png/03b-ter-ssim-b2.md) — Stone B B2 vertical pass chunked。02-pluto 26 ms / **B2 0.85× cement 反超**;04/06 仍 1.1-1.25× cement(待 B3 SIMD);score 仍 bit-exact。perf ladder 加 "memory access" 维度。
- [`png/03b-quater-ssim-b4.md`](png/03b-quater-ssim-b4.md) — Stone B B3/B4。B3 buffer reuse hypothesis 翻车(+4-6% slow on M2);**B4 rayon parallel horizontal 跟 cement ≈ 持平**(02 0.92×, 04 1.01×, 06 1.04×);score 仍 0.0000 diff;graduation 还差 2.8×;新攻击维度 "parallelism ladder" 加进 stone-essay 模板。

## Companion crate

`crates/nupic-research` 是 workspace member,`publish=false`。每个 essay 配
一个或多个 `examples/` 二进制,跑出 essay 引用的数字。详见
[`../../crates/nupic-research/README.md`](../../crates/nupic-research/README.md)。
