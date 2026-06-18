# Cycle 121 — paper Section 5 Findings (C2 + C3) writeup

**Date**: 2026-06-19
**Verdict**: paper-only cycle, no ship, no binary change
**Cycle type**: writeup (跟 Cycle 115 / 120 同款)
**Files touched**: `docs/research/paper/draft.md` only
**Production binary**: nupic 1.2.11 (unchanged)

## 1. Paper progress snapshot

| section | status before | status after | source cycles |
|---|---|---|---|
| Abstract | done | done | 106-112 |
| 1 Introduction | done | done | 106-112 |
| 2 Related Work | outline | outline | (Cycle 125) |
| 3 Corpus + Metric | done | done | 107 |
| 4 Methodology | done (Cycle 120) | done | 106-110 |
| **5 Findings C2/C3** | **TODO** | **done (this cycle)** | 106-110 |
| 6 Finding C4 + Container | TODO | TODO | next (Cycle 122) |
| 7 Discussion | TODO | TODO | Cycle 123 |
| 8 Conclusion + Figures | TODO | TODO | Cycle 124 |
| References | partial | partial (+ Cycle 110 + 3 tsv + 1 code path) | populate Cycle 125 |

**Draft size**: 338 → 482 行(+144 行,~7800 → ~10500 字)。

## 2. Section 5 内容结构

3 subsection,prose + table + code snippet + evidence reference,zero new spike / zero new data。

| subsection | content | primary source |
|---|---|---|
| 5.1 Finding C2 — palette-size break | winner histogram table(K=64..256 mean ratio)+ DSSIM bucket cross-tab + filter-chain entropy 机制解释 | Cycle 106 ledger §3-4 |
| 5.2 Finding C3 — production wire 4-step path | 5.2.1 single-config RED → 5.2.2 input-feature ceiling p244 → 5.2.3 P-08 wire snippet → 5.2.4 verify table | Cycle 107/108/109/110 ledger |
| 5.3 Combined narrative | C2+C3 共生论证 | (synthesis) |

## 3. References stub 增量

- Cycle 110 ledger 加入(本 cycle 引用新增的 5.2.4 verify table)
- 3 个 tsv 数据路径加入(cycle107 single_config_sample / cycle108 rule_v3_full / cycle110 full_verify_v3)
- 1 个 code path 加入(`compress.rs:209-273` P-08 wire)
- 04ooo essay 加入(Cycle 110 essay)

## 4. Cycle 119/120 教训复用 self-check

✓ 无 code 改动(P-08 source 只 grep 验证 line range,不 edit)
✓ 无 spike(全用 cycle 106-110 已有数据)
✓ 无 ship(paper-only)
✓ Section 5 长度(+144 行)对齐 Section 4(+110 行)风格,每 subsection 25-45 行
✓ P-08 code snippet 8 行简化(cycle109 ledger §1 是 18 行完整版),保留 spirit 不重复 inline 全 compress.rs

## 5. Cycle 122 next-up

- Section 6 Finding C4(R6 8×8 tile × K=192 spatial-aware quantization)+ C5(PNG 256-palette container bottleneck)
- 数据来源:Cycle 111(R6 algorithm GREEN 6/6)+ Cycle 112(Path B size GREEN + strict DSSIM RED)+ Cycle 113-114(`.nupic` minimal container probe)
- 同款 writeup-only cycle,no ship

## 6. baseline-7 sanity(skip)

paper-only,binary 不动,baseline-7 跟 v1.2.11 commit `d28cd0b` byte-identical(不重测)。
