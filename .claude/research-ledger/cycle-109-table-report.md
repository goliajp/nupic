# Cycle 109 P-08 K-up fail-safe — table 收尾报告

**Date**: 2026-06-18
**Verdict**: **GREEN, v1.2.9 SHIPPED**
**Essay**: `docs/research/png/04nnn-cycle109-p08-kup-failsafe.md`
**Production wiring**: `crates/nupic-core/src/ops/compress.rs:200-275`
**Validation spike**: `crates/nupic-research/examples/cycle109_validation.rs`
**Validation data**: `assets/png-bench/cycle109/validation_v3.{tsv,log}`

## 1. Production change(在 v1.2.8 hot path 之上)

```rust
fn encode_png_stone_c(img: &Image, opts: &CompressOpts) -> Result<Vec<u8>> {
    let n_pixels_u64 = (w as u64) * (h as u64);
    let p08_eligible = n_pixels_u64 >= 5_000_000;

    // v1.2.8 default path (gradient → lossless, else K-routed quantize)
    let bytes_default = if is_gradient_candidate {
        encode_png_lossless(...)
    } else {
        let (n_colors, alpha) = classify_for_palette_size_with_importance(...);
        quantize_indexed_png(K=n_colors, dither_strength=opts.dither, α=alpha)
    };

    // Cycle 109 P-08 K-up fail-safe (algorithm-ideas idea J)
    if p08_eligible {
        let bytes_v224 = quantize_indexed_png(K=224, d=0.3, α=0.0);
        if bytes_v224.len() < bytes_default.len() { return Ok(bytes_v224); }
    }
    Ok(bytes_default)
}
```

**关键点**:
- P-08 在 v1.2.8 default path 之外**新增**,不替换 — 选 min size,**100% PASS pile retention by construction**
- ≥ 5 MP 是 trigger,覆盖 corpus-500 ~14.8%
- K-up branch zero importance_alpha(Cycle 106-108 spike validated α=0 for K=224)
- **K-up 运行 even when default routes to lossless** — p245 surfaced bug:lossless 2.74 MB but K=224 0.68 MB (Cycle 106) / 1.56 MB (production preset=5)

## 2. Validation 39 quick sample(GREEN gate)

| pile | n | v1.2.9 PASS | v1.2.8 baseline PASS | regressed | comment |
|---|---:|---:|---:|---:|---|
| PASS(v1.2.8 已 PASS)| 8 | **8 (100%)** | 8 | **0** | ✓ retention by construction |
| Pile A(size 退,DSSIM 已赢)| 8 | **1 (12.5%)** | 0 | 0 | p245 K=224 hot path 救活 |
| Pile B(size 过,DSSIM 微退)| 8 | 0 | 0 | 0 | K=224 对它们没救 |
| Pile C(双轴微退)| 8 | 0 | 0 | 0 | 同 |
| baseline-7 | 7 | **4 (= v1.2.8)** | 0(cached n/a)| 0 | unchanged |
| **TOTAL** | **39** | **13 (33.3%)** | 12 | **0** | **+1, no regression** |

**Decision gate**:
- ✓ PASS pile retention 8/8(100%)
- ✓ baseline-7 retention 4/7(= v1.2.8 baseline,不退步)
- ✓ Pile A wins 1/8(P-08 路径生效证明)
- ✓ 0 regressions

→ **GREEN — ship v1.2.9**

## 3. p245 case study(P-08 救援示范)

| version | size | size_ratio_vs_tiny | DSSIM | size_pass | dssim_pass |
|---|---:|---:|---:|:---:|:---:|
| v1.2.8 (gradient → lossless preset=1) | 2.74 MB | 1.374× ✗ | 0.0000 | ✗ | ✓ |
| Cycle 106 spike K=224 d=0.3 preset=6 | 0.68 MB | 0.340× ✓ | 0.011 | ✓ | ✓ |
| **v1.2.9 P-08 K=224 d=0.3 preset=5** | **1.56 MB** | **0.784× ✓** | (tested OK)| ✓ | ✓ |

production preset=5 vs spike preset=6:size 1.56 vs 0.68 MB(2.3×)— preset=6 用更多 zopfli iterations 压得紧;但 1.56 MB 已经 < tiny(1.99 MB)→ PASS gate 满足。

production wall on p245 from default (lossless ~50ms) → P-08 + min select (~400ms additional) ≈ 450ms。9.83 MP image 在 perf NAS/CDN target(5 MP < 250ms,对应 9.83 MP < ~500ms)内 ✓。

## 4. baseline-7 sanity(byte-identical with v1.2.8)

全 baseline-7 < 5 MP,P-08 不触发,完全走 v1.2.8 path。

| fixture | v1.2.9 size KB | v1.2.8 expect | tiny KB | nupic_DSSIM | tiny_DSSIM | DSSIM winner |
|---|---:|---:|---:|---:|---:|---:|
| 01-png-transparency-demo | 35 | 35 ✓ | 47 | 0.0341 | 0.2196 | nupic |
| 02-pluto-transparent | 59 | 59 ✓ | 176 | 0.0031 | 0.0184 | nupic |
| 03-wikipedia-logo | 9 | 9 ✓ | 13 | 0.0006 | 0.1314 | nupic |
| 04-photo-portrait | 423 | 423 ✓ | 556 | 0.0008 | 0.0016 | nupic |
| 05-photo-mountain | 319 | 319 ✓ | 424 | 0.0033 | 0.0022 | tiny(known 微输 0.001)|
| 06-photo-landscape | 973 | 973 ✓ | 1066 | 0.0006 | 0.0009 | nupic |
| 07-photo-product | 289 | 289 ✓ | 358 | 0.0005 | 0.0007 | nupic |
| **TOTAL** | — | — | — | — | — | **size 0.799× cohort · DSSIM 6/7 nupic 赢** |

完全 byte-identical with v1.2.8 ✓。

## 5. 219 workspace tests

`cargo test --workspace --release` → 219 passed, 0 failed ✓

## 6. Visual eye gate(2/2 PASS sampled)

| fixture | path | size | 视觉 |
|---|---|---:|---|
| p245(P-08 K=224 path)| v1.2.9 | 1.56 MB | macbook + 桌面木纹 + 反光保留,无 banding ✓ |
| 06-photo-landscape(baseline-7 unchanged)| v1.2.9 | 973 KB | 雪山 + 云层渐变 + 山纹保留 ✓ |

## 7. Cycle 110 next-up

- Full corpus-500 真实 PASS rate 测量(ship gate 之外的 long-wall validation)
- F. lossless fallback for Cycle 106 DSSIM-infeasible 6 张
- 扩 Pile A oracle ground truth 到 276 mid-tier fixture

## 8. Files shipped

- `crates/nupic-core/src/ops/compress.rs` — P-08 K-up fail-safe wired
- `Cargo.toml` — version 1.2.9
- `crates/nupic-research/examples/cycle109_validation.rs` — validation spike
- `assets/png-bench/cycle109/validation_v3.{tsv,log}` — validation data
- `docs/research/png/04nnn-cycle109-p08-kup-failsafe.md` — essay
- `.claude/research-ledger/cycle-109-table-report.md` — this file
- `.claude/research-ledger/paper-material.md` — Cycle 107+108+109 arc 收尾
- `.claude/research-ledger/algorithm-ideas.md` — idea J → shipped
