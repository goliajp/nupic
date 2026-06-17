# 03y — Cycle 32: tier-4e narrow-band fix for Cycle 31 misroute (v0.5.43)

## Motivation

Cycle 31 (03x) shipped `var > 50 + adj_mn > 5.0 → 0.5` based on a
5-fixture probe (19, 24, 26, 25, 28). It did NOT sweep the baseline-7
or `inputs-ext/` corpus to confirm no regression. Cycle 32 ran a
full-corpus probe (29 fixtures) and discovered the rule silently
flipped THREE high-adj_mn fixtures from peak `d=0.7` to off-peak
`d=0.5`:

| fixture | var | adj_mn | pre-C31 d (peak) | C31 routes |
|---|---:|---:|---:|---:|
| 05 mountain | 320 | 9.44 | 0.7 (76.82) | 0.5 (75.73) **−1.09** |
| 06 landscape | 663 | 21.68 | 0.7 (84.94) | 0.5 (84.53) **−0.41** |
| 11 noisy | 297 | 12.68 | 0.7 (81.47) | 0.5 (80.99) **−0.48** |

Net Cycle 31 effect: +0.40 SSIM on 25/28 — **−1.98 SSIM on 05/06/11**
= **net −1.58 SSIM** across the corpus. The "baseline 7-fixture SSIM
bit-exact identical" claim in essay 03x was wrong: 05 and 06 are part
of the baseline-7 set, and both regressed.

## Root cause — adj_mn ↔ peak-d is NON-monotonic

Full peak-d sweep across all 9 tier-4 textured fixtures (var > 50):

| fixture | var | adj_mn | peak d | peak SSIM |
|---|---:|---:|---:|---:|
| 26 angkor | 58 | **3.71** | 0.7 | 75.59 |
| 19 iceberg | 52 | 3.80 | 0.7 | 83.23 |
| 07 product | 85 | 4.13 | 0.7 | 86.50 |
| 24 melk | 63 | 4.41 | 0.7 | 77.18 |
| 28 orca | 68 | **6.78** | **0.5** | 81.47 |
| 25 sofia | 209 | **6.86** | **0.5** | 78.40 |
| 05 mountain | 320 | 9.44 | 0.7 | 76.82 |
| 11 noisy | 297 | 12.68 | 0.7 | 81.47 |
| 06 landscape | 663 | **21.68** | 0.7 | 84.94 |

Peak-d's shape is **U-curve in adj_mn**: 0.7 on both ends, 0.5 in the
middle band `adj_mn ∈ (5, 7.5]`. Intuition — mid-contrast textures
hit FS dither's sweet spot; low-contrast smooths benefit from
stronger dither (banding suppression), and high-contrast already has
distinct palette entries so extra diffusion adds noise.

## Fix — narrow band in classifier

```diff
     if var > 50.0 {
-        if mean > 5.0 {
-            return 0.5; // tier-4e (Cycle 31): coarse-texture photo
+        if mean > 5.0 && mean <= 7.5 {
+            return 0.5; // tier-4e: coarse-texture band (peak shifts to 0.5)
         }
-        return 0.7; // tier-4b: textured photo
+        return 0.7; // tier-4b: textured photo (peak 0.7 below/above the band)
     }
```

Threshold 7.5 is the midpoint of the gap (6.86, 9.44) leaning
conservative — moves marginal new fixtures into 0.7-bucket (the
majority class) rather than 0.5.

## Bench wins (Cycle 32 vs Cycle 31)

```
                       C31 SSIM    C32 SSIM    Δ
05 mountain            75.73       76.82       +1.09  (recovered)
06 landscape           84.53       84.94       +0.41  (recovered)
11 noisy               80.99       81.47       +0.48  (recovered)
25 sofia               78.40       78.40       0      (Cycle 31 win kept)
28 orca                81.47       81.47       0      (Cycle 31 win kept)
                                              ------
                                              +1.98
```

Size cost: routing 05/06/11 back to d=0.7 adds ~7% size on each
(per peak-d sweep), ~+47 KB total across the three.

## All tier-4 fixtures now hit peak

11 out of 12 tier-4 corpus fixtures land on the peak-d under the
classifier — 18 snowflake (adj_mn=2.66, peak d=0.25) is the lone
remaining 0.16 SSIM gap, still deferred pending uniq × adj_mn 2D
rule (N=1 evidence not enough for retune).

## Verification

- All workspace tests pass (no test relies on the d=0.5 vs d=0.7
  routing of 05/06/11)
- 9 tier-4 fixtures re-bench at peak SSIM exactly
- baseline tier-4a (04 portrait, 16 earthrise) and tier-4d
  (13/17/20/29) untouched
- 07 product (var=85, adj_mn=4.13): stays at peak 0.7 (tier-4b
  retained, below the band)

## Files touched

- `crates/nupic-quantize/src/lib.rs::classify_for_auto_dither`
  (one-line `mean <= 7.5` upper bound + comment block)
- `Cargo.toml` workspace version 0.5.42 → 0.5.43

## Process lesson — full-corpus sweep before shipping classifier change

Cycle 31 trusted a 5-fixture probe (the new round-2 corpus). The
adj_mn signal sliced cleanly within that subset but flipped sign on
the baseline-7 + `inputs-ext/` fixtures that weren't re-tested. From
Cycle 32 forward, any classifier change runs `probe_real_corpus`
(now covering all 29 fixtures) followed by full peak-d sweep on any
fixture whose routing branch changed. The sweep is ~5 minutes; the
debug cycle for shipping a regression is ~30 minutes.

## Open backlog (next cycle candidates)

1. **18-snowflake** (adj_mn=2.66, want 0.25-0.5) — needs uniq ×
   adj_mn 2D rule or fresh signal. N=1 evidence; deferred.
2. **Confidence on 7.5 upper bound** — N=2 evidence in the band
   (25, 28); next mid-contrast (adj_mn 5-7.5) fixture could tighten
   or widen the threshold. Watch for it on next corpus expansion.
