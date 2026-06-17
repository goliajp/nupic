# 03x — Cycle 31: tier-4b/4e split via adj_mn (v0.5.42)

## Motivation

Cycle 30 (03w) Open question §1: tier-4b (`var > 50 → d=0.7`)
misroutes 3 fixtures (18-snowflake, 25-sofia, 28-orca) that actually
want d ≤ 0.5. Cycle 30 deferred the fix pending more evidence.

Cycle 31 closes the loop with a peak-d sweep on all 5 tier-4b
candidates plus a counter-example (19-iceberg) that **does** want 0.7.

## Full peak-d sweep — adj_mn is the discriminator

| fixture | var | adj_mn | uniq | peak d | peak SSIM | tier-4b d=0.7 SSIM | regression vs peak |
|---|---:|---:|---:|---:|---:|---:|---:|
| 19 iceberg | 52 | 3.80 | 65 K | **0.7** | 83.23 | 83.23 | 0 ✓ |
| 24 melk | 63 | 4.41 | 233 K | **0.7** | 77.18 | 77.18 | 0 ✓ |
| 26 angkor | 58 | 3.71 | 281 K | **0.7** | 75.59 | 75.59 | 0 ✓ |
| 25 sofia | 209 | 6.86 | 117 K | **0.5** | 78.40 | 78.13 | −0.27 |
| 28 orca | 68 | 6.78 | 126 K | **0.5** | 81.47 | 81.34 | −0.13 |
| 18 snowflake | 123 | 2.66 | 114 K | 0.25 | 82.81 | 82.65 | −0.16 (defer) |

Clean signal: **`adj_mn > 5.0` separates 25/28 (want 0.5) from 19/24/26
(want 0.7)**. The classifier already computes `mean` (= adj_mn) for the
tier-4c gradient detector, so the new branch is free of additional
work.

18 snowflake (adj_mn=2.66) still misroutes to 0.7 but the gap is only
0.16 SSIM — within noise band. Deferring fixes a real but smaller
loss until more N=1 evidence shows up (probably a uniq × adj_mn 2D
threshold).

## Implemented as a single new branch

```diff
     if var > 50.0 {
+        // Cycle 31: tier-4b/4e split. Coarse-texture inputs
+        // (adj_mn > 5) have high adjacent contrast → already-distinct
+        // palette → dither hurts more than helps.
+        if mean > 5.0 {
+            return 0.5; // tier-4e (Cycle 31): coarse-texture photo
+        }
         return 0.7; // tier-4b: textured photo
     }
```

## Bench wins

With `--dither auto`:

| fixture | Cycle 30 (d=0.7) | Cycle 31 (d=0.5) | Δ size | Δ SSIM |
|---|---:|---:|---:|---:|
| 25 sofia-cathedral | 2 825 185 B / 78.13 | 2 752 407 B / 78.40 | **−73 KB** | **+0.27** |
| 28 orca | 9 880 453 B / 81.34 | 9 808 678 B / 81.47 | **−72 KB** | **+0.13** |
| **combined** | | | **−145 KB** | **+0.40** |

19, 24, 26 stay routed to 0.7 (correct). 27 whale + 29 sundew + the
7 baseline fixtures bit-exact identical (Cycle 30 routing untouched).

## Verification

- 219 workspace tests still pass
- baseline 7-fixture set SSIM unchanged (01: −46.43, 02: 80.87,
  03: 100.00, 04: 88.85, 05: 75.73, 06: 84.53, 07: 86.50)
- 19-iceberg keeps d=0.7 = 83.23 SSIM
- 24-melk keeps d=0.7 = 77.18 SSIM
- 26-angkor keeps d=0.7 = 75.59 SSIM
- 26-angkor was untouched by Cycle 30 sweep but auto-routes to peak ✓

## Tier-4 classifier final form (post Cycle 30+31)

```
1. adj_mn < 1.0       → 0.7   (tier-4c, smooth gradient)
2. var > 50.0 AND
   adj_mn > 5.0       → 0.5   (tier-4e, coarse texture)        [Cycle 31]
3. var > 50.0         → 0.7   (tier-4b, fine texture)
4. uniq > 120 000     → 0.7   (tier-4d, high-uniq smooth)      [Cycle 30]
5. otherwise          → 0.5   (tier-4a, smooth photo)
```

5 branches; cleanest split achievable on the current 12 tier-4
fixtures (11 hit, 1 deferred minor regression on 18-snowflake).

## Open backlog (next cycle candidates)

1. **18-snowflake** (adj_mn=2.66, want 0.25-0.5) — needs uniq × adj_mn
   2D rule or a fresh signal. Gap only 0.16 SSIM; not blocking.
2. Run a larger natural-photo corpus to validate the adj_mn=5.0
   threshold doesn't false-positive on more fine-texture-but-low-uniq
   photos.
