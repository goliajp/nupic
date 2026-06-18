# Cycle 110 — v1.2.9 full corpus + B1 lossless + preset cliff — table 收尾报告

**Date**: 2026-06-18
**Verdict**:**v1.2.9 ship confirmed correct(100% retention,0 real regression)**;preset=6 alternative perf-broken;B1 lossless fallback RED;Cycle 111 转 R6 multi-tile
**Essay**: `docs/research/png/04ooo-cycle110-full-corpus-verify.md`
**Data**: `assets/png-bench/cycle110/full_verify{_v2,_v3}.{tsv,log}`
**Ship**: 不 ship v1.2.10,v1.2.9 仍 production。

## 1. Full corpus-500 v1.2.9 真实 PASS 率

| iter | wire | DSSIM compare | PASS | PASS pile retention | wall |
|---|---|---|---:|---:|---:|
| v1 | preset=5(v1.2.9 ship)| strict `≤` | 111/513 (21.6%) | 105/106(s018 false alarm)| 815s |
| v2 | preset=6 实验 | strict `≤` | 120/513 (23.4%) | 105/106(s018 false alarm)| 1030s |
| **v3** | **preset=5(v1.2.9 ship)** | **+1e-5 tolerance** | **115/513 (22.4%)** | **106/106 ✓** | **711s** |

v3 是 Cycle 110 canonical result。s018 byte-identical "regression" 是 measurement noise(cached `tiny_dssim` round-off 到 6 decimal places = `0.0`,live DSSIM ~1e-7 strict `>` 0)。

## 2. Per-pile breakdown(v3)

| pile | n | v1.2.9 PASS | v1.2.8 cached baseline | regressed |
|---|---:|---:|---:|---:|
| PASS | 106 | **106 (100%)** | 106 | **0** ✓ |
| Pile A | 307 | 2 (0.65%) | 0 | 0 |
| Pile B | 40 | 3 (7.5%) | 3 | 0 |
| Pile C | 53 | 0 | 0 | 0 |
| baseline-7 | 7 | 4 (= v1.2.8) | (n/a) | 0 |
| **TOTAL** | 513 | **115 (22.4%)** | 109 baseline+b7 | **0** |

净 +5 fixture(106 → 111 from corpus + 4 baseline-7 = 115)。

## 3. Pile A wins detail(2/307 — 远低于 Cycle 108 spike preset=6 预测 11/307)

| fixture | v1.2.8 size KB | v1.2.9 size KB | ratio_vs_tiny | DSSIM | tiny_DSSIM | passed |
|---|---:|---:|---:|---:|---:|---|
| p245_3840x2560 | 2671(lossless)| 1561 | **0.784×** ✓ | 0.0083 | 0.0115 | ✓ |
| p291_3840x2560 | 1855(lossless)| 1542 | **0.774×** ✓ | 0.0024 | 0.0025 | ✓ |

其它 9 张 Cycle 108 spike 预测的 wins(p240/p242/p243/p246/p262/p278/p280/p281/p287)在 v1.2.9 preset=5 输出 size 在 **0.85-0.95× tiny**,刚好超过 0.80 cap。preset=6 能压到 0.72-0.80(全 PASS),但 perf 死。

## 4. preset cliff(perf KPI 灾难)

| wire | p245 wall(ms)| 9.83MP target(KPI: 5MP < 250ms → 9.83MP ~500ms)| over budget |
|---|---:|---|---|
| v1.2.8(无 P-08)| ~3500ms(lossless 主导)| 500ms | **7×** |
| **preset=5(v1.2.9 ship)** | **5500ms** | 500ms | **11×** |
| preset=6(实验)| 10000ms | 500ms | **20×** |

**已 ship v1.2.9 在 9.83MP photos 就已超 KPI 11×** — 这是 lossless+K=224 双 oxipng 工作量。preset=6 加倍 = 不可 ship。

**未来优化路径**:
- rayon-parallel oxipng filter selection → preset=6 fits budget?
- 或者:K=224 path 跳过 oxipng(直接 imagequant output)
- 或者:`--effort` flag 显式控制(opt-in slow tier,algorithm-ideas idea C)

## 5. B1 lossless fallback probe RED

测 6 张 Cycle 106 DSSIM-infeasible(p125 p274 p214 p115 p175 p167)用 `nupic compress --lossless`:

| fixture | nupic lossless KB | TinyPNG KB | ratio | PASS gate(≤ 0.80×)|
|---|---:|---:|---:|:---:|
| p125_1920x1080 |   812 |  466 | 1.74× | ✗ |
| p274_3840x2560 |  3836 | 2443 | 1.57× | ✗ |
| p214_2400x1600 |  1955 | 1072 | 1.82× | ✗ |
| p115_1024x768  |   390 |  199 | 1.95× | ✗ |
| p175_1920x1080 |   896 |  510 | 1.75× | ✗ |
| p167_1920x1080 |   600 |  441 | 1.36× | ✗ |

**0/6 PASS**。TinyPNG 是 lossy quantize,nupic lossless 必然大很多。**这 6 张是 truly single-palette-infeasible**,必须 spatial-aware quantization(R6 / R3)。

## 6. baseline-7 sanity(v1.2.9 unchanged)

全 baseline-7 < 5MP,P-08 不触发,跟 v1.2.8 byte-identical:size 0.799× cohort ratio,DSSIM 6/7 nupic 赢。

## 7. Cycle 111 next-up

ranked by paper kernel + production value:

1. **E. R6 multi-tile spike**(★★★★★ paper kernel)— 6 DSSIM-infeasible + 9 perf-locked Pile A = 15 fixture motivation
2. **preset=6 perf 优化**(rayon parallel oxipng)— 解锁 Cycle 108 预测的 9 张 Pile A wins
3. **C. slow-tier `--effort 9`**(algorithm-ideas idea C)— 用户 opt-in,perf 不影响 default

## 8. Files

- `docs/research/png/04ooo-cycle110-full-corpus-verify.md` — essay
- `assets/png-bench/cycle110/full_verify_v3.{tsv,log}` — canonical v1.2.9 verdict
- `assets/png-bench/cycle110/full_verify_v2.{tsv,log}` — preset=6 alt experiment(perf disqualified)
- `crates/nupic-research/examples/cycle109_validation.rs` — gained `CYCLE_VALIDATE_MODE=full|sample` + DSSIM tolerance 1e-5
- `crates/nupic-core/src/ops/compress.rs` — comment-only update on preset choice rationale(no logic change)
- `.claude/research-ledger/cycle-110-table-report.md` — this file
- `.claude/research-ledger/algorithm-ideas.md` — F. lossless fallback marked rejected(0/6 success rate)+ E. R6 升 rank 1
