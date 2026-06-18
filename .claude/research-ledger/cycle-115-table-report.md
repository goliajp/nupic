# Cycle 115 — paper writeup Section 1-3 — table 收尾报告

**Date**: 2026-06-19
**Verdict**: paper draft v0.1 created with abstract + intro(C1-C5 contributions)+ related work outline + corpus/metric framework
**Output**: `docs/research/paper/draft.md`(新文件,5 sections + TODO + references stub)
**Ship**: 不 ship 代码,本 cycle 全是 paper writing。

## Paper structure 入库

| section | status | content |
|---|---|---|
| Title + Authors | done | "From Per-Image Oracle to Spatial-Aware Quantization: A Cohort-Driven Protocol for Breaking the Indexed-PNG Quality Ceiling" |
| Abstract | done | 5-paragraph summary covering 3 findings + production wire + container bottleneck |
| 1. Introduction | done | 3 subsections(motivation / 5 contributions C1-C5 / paper organization)|
| 2. Related Work | outline | 5 subsections placeholder(PNG codec opt / RD analysis / VQ / metrics / multi-tile)|
| 3. Corpus + Metric | done | corpus-500 composition + DSSIM primary + 3-axis gate |
| 4. Methodology | TODO Cycle 116 | cohort headroom-mapped Pareto sweep protocol |
| 5. Findings C2/C3 | TODO Cycle 116 | palette-size break + production wire |
| 6. Finding C4 + Container | TODO Cycle 117 | R6 ceiling break + bottleneck |
| 7. Discussion | TODO Cycle 117 | `.nupic` container + WebP transcoder paths |
| 8. Conclusion | TODO Cycle 118 | future work + reproducibility |
| Figures | TODO Cycle 118 | heatmaps / cohort histograms / R6 tile boundary viz |
| References | stub | populate Cycle 117-118 |

## 5 named contributions in abstract

- **C1**(methodology)cohort headroom-mapped Pareto sweep protocol
- **C2**(palette-size)K=192-256 < K=128 on photo,Cycle 106 0.59× tiny on 23 Pile A winners
- **C3**(production wire)v1.2.9 P-08 K-up fail-safe,100% retention,+1.5pp PASS
- **C4**(R6 spatial-aware)8×8 K=192 PASS 6/6 DSSIM margin -0.00072 to -0.00825
- **C5**(container bottleneck)Cycle 112 PNG 256-palette caps R6 to size 0.46-0.55× tiny but strict DSSIM fail

## Workflow speed

| activity | wall | OK? |
|---|---:|:---:|
| paper draft v0.1 writing | one cycle | ✓ |

paper writeup 一个 cycle 完成 abstract + intro + 1 section + outline rest,合理 pace。

## Cycle 116 next-up

- Section 4 Methodology(detailed protocol pseudocode + 4-pile classification example)
- Section 5 Findings C2 + C3(K-monotonicity break data + v1.2.9 wire experiment results)
- estimated 1 cycle
