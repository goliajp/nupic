# 04sss · Cycle 119 — effort=9 zopfli slow-tier validation (YELLOW, doc-only)

**Status:** **YELLOW, doc-only cycle, no version bump.** Cycle 21 (v0.3.x) already wired
oxipng Zopfli into the `--effort ≥ 7` path with iters `(effort-6) × 5`.
Cycle 119 audited the existing path against the cycle 102-118 corpus-500
DSSIM-primary gate, confirmed it flips 16/50 size-edge fixtures from
fail-PASS, and probed whether stepping iters from 15 → 30 would flip more.
It does not — iters ≥ 15 is fully saturated. The Cycle 21 mapping ships
unchanged.

## TL;DR

| metric | value |
|---|---|
| `--effort 9` already wired (Cycle 21) | iters=15, zero-arg cost |
| baseline-7 ratio with `--effort 9` vs `--effort 5` | 0.795× vs 0.799× cohort (-0.51%) |
| baseline-7 per-fixture regression | **0/7, strictly tighter or byte-identical** |
| corpus-500 size-edge 0.80-0.85× cohort PASS flip | **14/25 (56%)** |
| corpus-500 0.85-0.95× cohort PASS flip | **2/25 (8%)** |
| p245/p274 9.83 MP wall at `--effort 9` | **218 s / 245 s** (over 60 s KPI 3.6-4.1×) |
| iters 15 → 20 (effort 9 vs 10) marginal Δsize on 5 fixtures | **-0.00% to -0.12%** |
| iters 15 → 30 marginal Δsize on 5 NO fixtures (offline patch) | **0.00% to 0.05%, 0/5 flip** |
| 219 workspace tests | pass |
| Production code change | **none** (mapping reverted after spike) |

Decision: opt-in slow tier survives as **documented behavior** for power
users with `--effort 9`. No CLI surface change, no version bump.

## Why no code change

The Cycle 119 kickoff hypothesized `--effort 9 → Deflaters::Zopfli`
needed wiring. Grep first:

```text
crates/nupic-quantize/src/lib.rs:411 → already wired
crates/nupic-core/src/ops/compress.rs:426 → already wired
```

Cycle 21 (Phase 3.5) wired both in 2025. The mapping
`(effort-6) × 5` capped at 30 means effort=7→5, 8→10, 9→15, 10→20.

The interesting Cycle 119 question shifted to: **does iters > 15 still
save bytes?** The cycle 21 essay never re-measured after iters=15.

## Saturation probe

Hand-patched `nupic_quantize:412` to:

```rust
let iters: u8 = match opts.oxipng_preset {
    7 => 5,
    8 => 15,
    9 => 30,
    _ => 30,
};
```

Rebuilt, re-ran the 5 fixtures that effort=9 (iters=15) couldn't flip
into PASS (0.81-0.82× tiny):

| fixture | tiny KB | e9 (iters=15) KB | e9 (iters=30) KB | Δ% | flip? |
|---|---:|---:|---:|---:|---:|
| p436_sm_460x260 | 54 | 43 | 43 | 0.01 | no |
| p36_480x320 | 68 | 55 | 55 | 0.05 | no |
| p445_sm_380x320 | 50 | 40 | 40 | 0.02 | no |
| p410_sm_380x380 | 55 | 44 | 44 | 0.00 | no |
| p70_1024x768 | 333 | 270 | 270 | 0.00 | no |

**5/5 saturate at iters=15.** The 15→30 step costs ~2× wall and returns
nothing. Mapping reverted.

Cross-validation via existing CLI: effort=9 (iters=15) vs effort=10
(iters=20) on the *passing* edge cohort:

| fixture | e9 bytes | e10 bytes | Δ% |
|---|---:|---:|---:|
| p121_1920x1080 | 404856 | 404811 | -0.01 |
| p35_480x320 | 67279 | 67278 | -0.00 |
| p131_1920x1080 | 582210 | 581531 | -0.12 |
| p144_1920x1080 | 842063 | 842033 | -0.00 |
| p153_1920x1080 | 885727 | 885591 | -0.02 |

Saturation confirmed by two independent probes.

## Size-edge flip data

### 0.80-0.85× cohort (25 fixtures, the easiest PASS-edge band)

14/25 flip to PASS via `--effort 9`. Sample:

| fixture | e5 ratio | e9 ratio | flip |
|---|---:|---:|---:|
| p121_1920x1080 | 0.8005 | 0.7463 | YES |
| p0_480x320 | 0.8060 | 0.7780 | YES |
| p68_1024x768 | 0.8074 | 0.7872 | YES |
| p204_2400x1600 | 0.8089 | 0.7696 | YES |
| p144_1920x1080 | 0.8169 | 0.7860 | YES |
| p181_2400x1600 | 0.8206 | 0.7581 | YES |
| p131_1920x1080 | 0.8251 | 0.7749 | YES |
| p153_1920x1080 | 0.8252 | 0.7434 | YES |
| n20_moon | 0.8084 | 0.8036 | no |
| p70_1024x768 | 0.8198 | 0.8092 | no |
| p87_1024x768 | 0.8225 | 0.8198 | no |

Pattern: large palette-quantized photos (1920×1080+) flip cleanly,
small icons with palette-saturated content (p70, p87, p20) don't —
their bottleneck is the 256-palette ceiling, not the deflate.

### 0.85-0.95× cohort (25 fixtures, the harder band)

Only **2/25 flip** to PASS (p128_1920x1080: 0.8724 → 0.7915 ; p130_1920x1080:
0.8903 → 0.7868). The rest get a -3 to -8% size reduction but stay above
the 0.80× gate.

Pattern: 0.85-0.95 ratio means the v1.2.11 default is already losing to
TinyPNG on bytes (despite winning DSSIM); Zopfli alone doesn't close the
gap. These need the Cycle 116-118 transcoder rescue (WebP/AVIF), not
deflate tuning.

## Wall on 9.83 MP

```
p245_3840x2560.png  e5=1561897B 5.68s  ratio=0.7846
                    e9=1323358B 245.43s ratio=0.6648  (-15%)
p274_3840x2560.png  e5=1862494B 5.31s  ratio=0.7443
                    e9=1618572B 218.36s ratio=0.6468  (-13%)
```

Both fixtures **already PASS at effort=5**. effort=9 just tightens by
-13% to -15% bytes, costing **218-245 s** vs the 60 s slow-tier KPI
(3.6-4.1× over). Not viable as a default.

## Production posture

- `--effort 9` and `--effort 10` remain available but undocumented in
  the user-facing help beyond "0 (fastest) to 10 (slowest)". This essay
  is the load-bearing doc that captures (a) when they help (1920×1080+
  palette-quantized photos already in the 0.80-0.85× band) and (b) when
  they don't (small icons, 0.85+ ratio cohort, perf-bounded large photos).
- No new flag. The cycle 116-118 transcoder rescue path
  (`--photo-rescue-webp` / `--photo-rescue-avif`) covers the 0.85+ band
  by switching codec entirely; effort=9 only meaningfully helps the
  narrow 0.80-0.85× band where PNG is already close.
- No version bump.

## What this closes / opens

- **Closes:** Cycle 21 zopfli iters tuning thread — empirically saturated
  at iters=15 across two probe rounds; the linear mapping `(effort-6)×5`
  shipped in Cycle 21 was retroactively the right call.
- **Opens:** the small-icon-in-0.81-0.82× band (p70/p87/p20-style)
  remains the genuine PNG-ceiling cohort with no obvious next move
  beyond palette work — punted to Cycle 120+.
