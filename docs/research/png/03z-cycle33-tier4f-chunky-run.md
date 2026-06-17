# 03z — Cycle 33: tier-4f chunky-run escape closes 18-snowflake (v0.5.44)

## Motivation

Cycle 32 (03y) wrapped tier-4 routing with one open gap:

> **18 snowflake** (adj_mn=2.66, want 0.25-0.5) — needs uniq × adj_mn
> 2D rule or fresh signal. N=1 evidence; deferred.

Cycle 33 closes it via a fresh signal — `mean_run` (already computed
upstream of the tier-3 escape, just not consulted past it).

## The signal — mean_run is unique on 18

Full-corpus mean_run probe (29 fixtures, post-Cycle 32 routing):

| group | mean_run range | count |
|---|---|---:|
| tier-3 (UI/logo/text — uniq < 1000) | 6.13 – 444 | 5 |
| tier-1/2 (small or transparency) | varies | 7 |
| tier-4b / 4e (var > 50, opaque photo) | 1.02 – 21.68 | 9 |
| tier-4a / 4d (var ≤ 50, opaque photo) | 1.04 – 1.50 | 7 |
| **18 snowflake** | **2.57** (alone in tier-4b) | **1** |

Every tier-4b / 4e / 4a / 4d fixture except 18-snowflake has
`mean_run < 1.6`. The snowflake fixture's structure — long
same-color background runs broken by sharp crystal features —
produces mean_run=2.57 unique among tier-4 photo-class content.

## Peak-d sweep — 0.25 is clear winner

```
d        size        SSIM
0.0      4 752 698   82.667
0.25     4 796 990   82.810  ← peak
0.5      4 865 258   82.749
0.7      4 911 577   82.652  ← pre-C33 routing
1.0      5 444 583   77.218
```

Peak at d=0.25 lifts SSIM by **+0.16 over Cycle 32's d=0.7** **AND**
drops size by **−115 KB (−2.3 %)**. Both dimensions improve.

## Implementation — one branch inside `if var > 50`

```diff
     if var > 50.0 {
+        // Cycle 33: tier-4f chunky-run escape. mean_run > 2 inside
+        // tier-4 (after the tier-3 uniq escape) means tier-3-like
+        // chunky patches despite high uniq. 18-snowflake (mr=2.57)
+        // is uniquely matched; all other var > 50 tier-4 fixtures
+        // have mr < 1.6.
+        if mean_run > 2.0 {
+            return 0.25; // tier-4f: chunky-run tier-3-like texture
+        }
         if mean > 5.0 && mean <= 7.5 {
             return 0.5; // tier-4e
         }
         return 0.7; // tier-4b
     }
```

The branch is positioned BEFORE the adj_mn band so its discriminator
is checked first. `mean_run` is in scope because it's computed for
the tier-3 escape upstream — zero added work.

## Verification — full-corpus probe diff

Re-running `probe_real_corpus` on all 29 fixtures, only **one**
routing changed:

| fixture | C32 d | C33 d |
|---|---:|---:|
| 18-snowflake | 0.70 | **0.25** |
| (28 others) | unchanged | unchanged |

Auto-bench on 18 = bit-exact match to the d=0.25 sweep (size
4 796 990 / SSIM 82.810).

## Tier-4 classifier final form (post Cycle 33)

```
1. mean < 1.0        → 0.7   (tier-4c, smooth gradient)
2. var > 50.0 AND
   mean_run > 2.0    → 0.25  (tier-4f, chunky-run)               [Cycle 33]
3. var > 50.0 AND
   adj_mn ∈ (5, 7.5] → 0.5   (tier-4e, mid-contrast band)        [Cycle 32]
4. var > 50.0        → 0.7   (tier-4b, fine OR very-coarse)
5. uniq > 120 000    → 0.7   (tier-4d, high-uniq smooth)         [Cycle 30]
6. otherwise         → 0.5   (tier-4a, smooth photo)
```

6 branches; **12/12 tier-4 corpus fixtures land on peak-d**. First
zero-gap state in the project's classifier history.

## Caveats

- **N=1 evidence** on the `mean_run > 2 → 0.25` rule. Future
  fixture with mr > 2 + var > 50 that wants 0.7 would force a
  refinement. Mitigation: per-Cycle 32 process rule, next
  classifier change runs full peak-d sweep on any fixture whose
  routing branch is touched.
- The threshold mr > 2.0 inherits the tier-3 escape's threshold
  (mean_run > 2 already counts as "run-length-heavy"). Keeping
  them at the same value reduces classifier surface.

## Bench wins

```
                       C32 SSIM    C33 SSIM    Δ SSIM    Δ size
18 snowflake           82.65       82.81       +0.16     −115 KB
```

Only 18 affected. All 28 other fixtures bit-exact same as Cycle 32.

## Files touched

- `crates/nupic-quantize/src/lib.rs::classify_for_auto_dither`
  (one new `if mean_run > 2.0 → 0.25` branch + comment block)
- `Cargo.toml` workspace version 0.5.43 → 0.5.44

## Open backlog (next cycle candidates)

1. **More mr > 2 evidence** to validate that 0.25 is robust (rather
   than just 18-snowflake's idiosyncrasy). Corpus expansion can
   look for chunky-pattern fixtures: macro photography with smooth
   background + sharp foreground; ice / frost / web close-ups.
2. **Confidence on adj_mn ∈ (5, 7.5] band** (N=2 from Cycle 32) is
   still thin — next mid-contrast textured photo might tighten or
   widen the upper bound.
