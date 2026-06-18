# Cycle 120 — paper Section 4 Methodology writeup

**Date**: 2026-06-19
**Verdict**: paper-only cycle, no ship, no binary change
**Cycle type**: writeup (跟 Cycle 115 同款)
**Files touched**: `docs/research/paper/draft.md` only
**Production binary**: nupic 1.2.11 (unchanged)

## 1. Paper progress snapshot

| section | status before | status after | source cycles |
|---|---|---|---|
| Abstract | done | done | 106-112 |
| 1 Introduction | done | done | 106-112 |
| 2 Related Work | outline | outline | (Cycle 125) |
| 3 Corpus + Metric | done | done | 107 |
| **4 Methodology** | **TODO** | **done (this cycle)** | 106-110 |
| 5 Findings C2/C3 | TODO | TODO | next (Cycle 121) |
| 6 Finding C4 + Container | TODO | TODO | Cycle 122 |
| 7 Discussion | TODO | TODO | Cycle 123 |
| 8 Conclusion + Figures | TODO | TODO | Cycle 124 |
| References | stub | partial (§4 internal refs) | populate Cycle 125 |

**Draft size**: 228 → 338 行(+110 行,~5300 字 → ~7800 字)。

## 2. Section 4 内容结构

5 subsection,每段 prose + evidence reference,zero new spike / zero new data。
全部 sourced from cycle 106-110 ledger / essay / tsv。

| subsection | content | primary source |
|---|---|---|
| 4.1 Cohort construction | 2-axis(size_ratio / dssim_delta)→ 4-pile decomp(106/307/40/53) | three-axis.tsv + Cycle 107 ledger |
| 4.2 Headroom-mapped oracle sweep | per-fixture K×d×preset grid + cohort histogram + DSSIM bucket | Cycle 106 ledger Table 3-4 |
| 4.3 Headroom-driven K selection | K monotonicity break(18/23 winners K≥192,8/23 K=224)+ 解释 | Cycle 106 ledger §4 + §3 bucket |
| 4.4 Per-input routing + fail-safe wire | single-config RED → input-feature ceiling → 2-pass min(default, K_up) | Cycle 107/108/109 ledger |
| 4.5 Generalizability | 3-role 解耦 + JPEG/AVIF/monotone-codec 推广 | (synthesis,无 new data) |

## 3. References stub 增量

Section 4 引用的 in-repo evidence 全部列入末尾 References:
- 5 个 cycle ledger(106/107/108/109/110)
- 4 个 essay(04kkk/04lll/04mmm/04nnn)
- 2 个 corpus tsv(three-axis + pile-a oracle grid)

External references(Section 1-3 用的 TinyPNG / pngquant / DSSIM / SSIMULACRA2 / VQ 文献)留 Cycle 125 populate。

## 4. Cycle 119 教训复用 self-check

✓ 无 code 改动(grep 之前先确认是 writeup cycle)
✓ 无 spike(全用现成 cycle 106-110 数据)
✓ 无 ship(paper-only)
✓ Section 4 长度对齐 kickoff 预算(~50 行预期,实际 95 行,但每段 8-15 行符合 Section 3 风格)

## 5. Cycle 121 next-up

- Section 5 Findings:
  - **C2**: palette-size monotonicity break(K=224 size 反而更小的 PNG filter entropy 解释)
  - **C3**: production wire(P-08 K-up fail-safe 详细 case study)
- 数据来源:Cycle 106 winner cluster + Cycle 109 p245 case study + Cycle 110 full-corpus 22.4% PASS
- 同款 writeup-only cycle,no ship

## 6. baseline-7 sanity(skip)

paper-only,binary 不动,baseline-7 跟 v1.2.11 commit `d28cd0b` byte-identical(不重测)。
