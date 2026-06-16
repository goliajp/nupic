# 03n — Cycle 12/13 (research-only): 05 ceiling + default policy

## Cycle 12 — 05-mountain ceiling profile

After Cycle 11 brought 05 from SSIM 75.73 (d=0.5) → 76.82 (d=0.7),
**is there more to extract?**

### Palette utilization

`cycle12_05_profile`: all 256 palette slots used; Lloyd's converges
fully before iter=100. Iter count to 500 yields no observable palette
structure change.

### imagequant param sweep (cycle12_iq_quality, 05 only)

| params | size | SSIM |
|---|---|---|
| baseline q70-95 s4 | 473174 | 76.818 |
| q_max=100 s4 | 473174 | 76.818 |
| q_max=100 s1 | 470429 | **76.828** |
| q_min=0 q_max=100 s1 | 470429 | 76.828 |

Speed=1 yields +0.01 SSIM / -2.7KB on 05 alone. Negligible.

### Corpus extrapolation (cycle12_iq_speed_corpus, full 7-fixture)

| fixture | s4 SSIM | s1 SSIM | Δ |
|---|---|---|---|
| 01 trans-demo | -46.43 | -46.40 | +0.03 |
| **02 pluto** | **80.44** | **71.99** | **-8.45** |
| 03 wiki logo | 100 | 100 | 0 |
| 04 portrait | 89.03 | 88.98 | -0.05 |
| 05 mountain | 76.82 | 76.83 | +0.01 |
| 06 landscape | 84.94 | 85.22 | +0.29 |
| 07 product | 86.86 | 86.77 | -0.09 |
| **mean** | 67.38 | 66.20 | **-1.18** |

**Negative result.** speed=1 collapses 02-pluto-transparent by
8.45 points (likely IQ transparency handling differs at slow speed).
Net corpus delta is -1.18 SSIM. Do not wire effort → IQ speed.

### Conclusion

05-mountain SSIM 76.82 is the **practical ceiling** for the current
algorithm shape: indexed PNG + 256 palette + OKLab Lloyd's + FS dither.
Further gains require orthogonal innovation:

- Blue-noise / Riemersma dither (vs FS) — Stone E redesign
- SSIMULACRA2-aware Lloyd's loss (vs OKLab L2) — expensive inner loop
- Selective regional dither (only banding-prone areas) — needs
  high-frequency detection
- A non-indexed lossy PNG path — different beast entirely

Backlog for later cycles.

## Cycle 13 — should `--dither` default flip from `off` to `auto`?

After Cycle 11 made tier-4 split content-aware, is `--dither auto`
strictly better than `--dither off`?

`cycle13_default_auto` (7-fixture, full pipeline):

| fixture | off size / SSIM | auto size / SSIM | Δ size / SSIM |
|---|---|---|---|
| 01 trans-demo | 45364 / -46.43 | same | 0 / 0 |
| 02 pluto | 158109 / 79.66 | 162009 / 80.44 | +3900 / +0.78 |
| 03 wiki logo | 14718 / 100 | same | 0 / 0 |
| 04 portrait | 484513 / 87.99 | 499378 / 88.85 | +14865 / +0.87 |
| 05 mountain | 389264 / 70.38 | 473174 / **76.82** | +83910 / **+6.44** |
| 06 landscape | 1035965 / 82.77 | 1109644 / 84.94 | +73679 / +2.17 |
| 07 product | 340640 / 84.70 | 404312 / 86.50 | +63672 / +1.80 |
| **mean** | — / 65.58 | — / **67.30** | — / **+1.72** |
| **total** | 2.47 MB | 2.71 MB | **+9.72%** |

**Decision: keep default as `off`.**

Reasoning:
- Mission is **"又小又好"** (small AND good). Both must hold.
- `off` default beats TinyPNG on BOTH dimensions across all 7 fixtures.
- `auto` default beats TinyPNG on SSIM but **loses size advantage**
  on 05/07 (`auto` 473KB > Tiny 434KB on 05).
- Quality-first users have `--dither auto` available; perf-first
  is the safe default.
- Cycle 11 shipped `--dither auto` improvements; that path is now
  meaningfully better for opt-in users.

`--dither off` stays default. `--dither auto` is the quality-first
opt-in.

## Files

- `crates/nupic-research/examples/cycle12_05_profile.rs`
- `crates/nupic-research/examples/cycle12_iq_quality.rs`
- `crates/nupic-research/examples/cycle12_iq_speed_corpus.rs`
- `crates/nupic-research/examples/cycle13_default_auto.rs`
