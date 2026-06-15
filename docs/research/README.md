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
- [`png/01-pluto-case.md`](png/01-pluto-case.md) — 02-pluto 上 imagequant 的 algorithmic ceiling;cement-layer adaptive q_target spec(实测 sweep 推外延总体 0.66× TinyPNG)

## Companion crate

`crates/nupic-research` 是 workspace member,`publish=false`。每个 essay 配
一个或多个 `examples/` 二进制,跑出 essay 引用的数字。详见
[`../../crates/nupic-research/README.md`](../../crates/nupic-research/README.md)。
