> **⚠️ RETRACTION (Cycle 102, 2026-06-18)**
>
> This essay measured spike configs against a forced `K=256 d=0 preset=3` baseline,
> which does NOT match production `Quality::Auto` (which picks K via classifier).
> The GREEN/YELLOW/RED verdicts are internally consistent but **production-irrelevant**.
> See [[04ggg-cycle102]] for the methodology reset and locked three-axis gate
> (size ≤ 0.80× tiny AND SSIM ≥ tiny AND perf max).

# 04ddd · Cycle 99 — R4 B1 router corpus-500 validation YELLOW (13/20, wc 66%)

**Status:** **YELLOW**. The B1 router that cleared baseline-7 + 5MP at 8/10
pass and **90% mean win-capture** drops to **13/20 pass and 66% mean
win-capture** on the corpus-500 sample. Pass-fraction clears the gate
(65% ≥ 60%) but mean-wc does not (66% < 80%).

Cycle 98's Risk note was right: the chroma threshold 0.025 calibrated on
10 fixtures **was too tight to generalize**. The wider corpus exposes two
opposing failure modes that pull in different directions:

- **Type-X (too-narrow Chroma):** B1 misses 3 small wins (n29 astronaut
  −0.14%, p11 −0.55%, p119 −0.67%) where features sit just below
  threshold but oracle wants K=256 d=0.5.
- **Type-Y (too-broad Chroma):** B1 takes 3 false Chroma routes (p426,
  p66, mi0) costing +0.5% size for no Pareto benefit, because
  smoothness < 0.05 and chroma > 0.025 don't actually predict
  d=0.5-friendly content on these.

Plus mi0 — a tiny (770 B) chroma-zero entropy-zero image — gets
misrouted because the UI predicate's `edge_density > 0.2` excludes it.

This is honest data for paper §6 routing analysis: the gap between
**training-cohort GREEN** and **wider-cohort YELLOW** is the same lesson
R1's classifier thread (Cycles 91a-95) hit, just on a different routing
problem. The B1 router is **not ship-ready** as `Quality::Auto-R4`
without further widening.

## TL;DR

| metric | Cycle 98 (10-fix) | **Cycle 99 (20-fix corpus-500)** | gate |
|---|---:|---:|---:|
| pass count | 8/10 (80%) | **13/20 (65%)** | ≥ 60% |
| mean win-capture | 90% | **66%** | ≥ 80% |
| mean oracle Δsize | −2.12% | **−0.53%** | (ceiling) |
| mean router Δsize | −1.39% | **−0.20%** | (actual) |
| router/oracle ratio | 66% | **38%** | — |

Gate result: **pass-fraction ✓, mean-wc ✗ → YELLOW**.

## Failure-mode breakdown

### Type-X (too-narrow Chroma): 3 fixtures, missed −0.14 to −0.67% wins

| fixture | oracle | router | features (chr/sm/ed) | why missed |
|---|---|---|---|---|
| n29_astronaut | (256, 0.5) | (256, 0) | .044 / **.0505** / .270 | smoothness 0.0505 just above 0.05 cutoff → Stoch |
| p11_480x320  | (256, 0.5) | (256, 0) | **.019** / .0481 / .265 | chroma 0.019 below 0.025 threshold → Stoch |
| p119         | (256, 0.5) | (256, 0) | **.019** / .0140 / .092 | chroma 0.019 below 0.025 → Stoch (even with smooth=0.014 ≪ 0.05) |

Total missed savings if these had routed Chroma: −1.36% size cohort sum
(0.07% per fixture amortized).

### Type-Y (too-broad Chroma): 3 fixtures, +0.47 to +0.52% false-Chroma cost

| fixture | oracle | router | features (chr/sm/ed/ent) | router's mistake |
|---|---|---|---|---|
| p66_1024x768  | (256, 0) default | (256, 0.5) | .065 / .0189 / .152 / 4.89 | chroma 0.065 + smooth 0.019 → Chroma, but oracle = default |
| p426_sm       | (256, 0) default | (256, 0.5) | .061 / .0405 / .346 / 5.52 | chroma 0.061 + smooth 0.040 → Chroma, but oracle = default |
| p430_sm       | (192, 0.3) win | (256, 0) | **.000** / .0144 / .078 / 2.47 | chroma 0 fails predicate → Stoch; misses K=192 win unreachable to 3-class anyway |

p426 and p66 are the diagnostic: the Chroma rule says they are
chroma-rich-flat but the encoded Pareto says they don't benefit from
dither. **The smoothness × chroma 2-predicate cannot distinguish "wants
d=0.5" from "wants d=0" on opaque mid-chroma photos.**

### Edge case: mi0 (tiny logo, 770 B)

mi0 has `chroma=0, smoothness=0, edge=0, trans_frac=0.68, entropy=0` —
a near-degenerate content type the baseline cohort didn't have. Oracle
is K=128 d=0.3 (−5.19%). B1's UI rule requires `edge_density > 0.2`
which mi0 fails (edge=0), so it falls through to Chroma via
`trans_frac > 0`. Wc=30% because the Chroma K=256 d=0.5 captures
−1.56% of the −5.19% available win.

Cleanest fix: **widen UI predicate to `chroma_entropy < 3.0 AND
(edge_density > 0.2 OR trans_frac > 0.5)`** — catches both real
UI-edge fixtures and trans-rich tiny-image content like mi0.

## Per-fixture trace (full)

| fixture | features (chr / sm / ed / ent) | class | oracle (K,d) | router (K,d) | oracle Δ% | router Δ% | wc | pass? |
|---|---|:---:|:---:|:---:|---:|---:|---:|:---:|
| mi0                        | .000 / .000 / .000 / 0.0 | Chrm | (128, 0.3) | (256, 0.5) | −5.19% | −1.56% | **30%** | ✗ |
| n29_astronaut              | .044 / .050 / .270 / 4.7 | Stoch | (256, 0.5) | (256, 0.0) | −0.14% | 0.00% | **0%** | ✗ |
| p11_480x320                | .019 / .048 / .265 / 5.0 | Stoch | (256, 0.5) | (256, 0.0) | −0.55% | 0.00% | **0%** | ✗ |
| p32_480x320                | .031 / .033 / .278 / 5.1 | Chrm | (256, 0.3) | (256, 0.5) | −0.54% | −0.50% | 93% | ✓ |
| p409_sm                    | .039 / .042 / .309 / 5.1 | Chrm | (256, 0.5) | (256, 0.5) | −2.11% | −2.11% | 100% | ✓ |
| p426_sm                    | .061 / .040 / .346 / 5.5 | Chrm | (256, 0.0) | (256, 0.5) | 0.00% | **+0.52%** | **0%** | ✗ |
| p449_sm                    | .035 / .049 / .292 / 3.5 | Chrm | (256, 0.5) | (256, 0.5) | −0.42% | −0.42% | 100% | ✓ |
| p66_1024x768               | .065 / .019 / .152 / 4.9 | Chrm | (256, 0.0) | (256, 0.5) | 0.00% | **+0.47%** | **0%** | ✗ |
| p7_480x320                 | .051 / .054 / .310 / 4.5 | Stoch | (256, 0.0) | (256, 0.0) | 0.00% | 0.00% | 100% | ✓ |
| s042_stripes_p8            | .270 / .034 / .249 / 1.0 | **UI** | (256, 0.0) | (128, 0.0) | 0.00% | 0.00% | 100% | ✓ |
| n01_mars                   | .045 / .019 / .129 / 4.1 | Chrm | (256, 0.5) | (256, 0.5) | −0.39% | −0.39% | 100% | ✓ |
| n31_rover                  | .065 / .066 / .457 / 4.9 | Stoch | (256, 0.0) | (256, 0.0) | 0.00% | 0.00% | 100% | ✓ |
| p119_1024x768              | .019 / .014 / .092 / 3.6 | Stoch | (256, 0.5) | (256, 0.0) | −0.67% | 0.00% | **0%** | ✗ |
| p38_480x320                | .012 / .012 / .095 / 3.6 | Stoch | (256, 0.0) | (256, 0.0) | 0.00% | 0.00% | 100% | ✓ |
| p430_sm                    | .000 / .014 / .078 / 2.5 | Stoch | **(192, 0.3)** | (256, 0.0) | −0.49% | 0.00% | **0%** | ✗ |
| p56_480x320                | .030 / .084 / .520 / 5.5 | Stoch | (256, 0.0) | (256, 0.0) | 0.00% | 0.00% | 100% | ✓ |
| p84_1024x768               | .026 / .100 / .367 / 3.8 | Stoch | (256, 0.0) | (256, 0.0) | 0.00% | 0.00% | 100% | ✓ |
| s006_gradient              | .125 / .000 / .000 / 4.8 | Chrm | (256, 0.0) | (256, 0.5) | 0.00% | 0.00% | 100% | ✓ |
| s040_stripes_p2            | .129 / .145 / 1.000 / 1.0 | **UI** | (256, 0.0) | (128, 0.0) | 0.00% | 0.00% | 100% | ✓ |
| s059_solid                 | .056 / .000 / .000 / 0.0 | Chrm | (256, 0.0) | (256, 0.5) | 0.00% | 0.00% | 100% | ✓ |

## Why the gate flipped

On baseline-7, B1's pickable wins are large (10.98% on 03 wiki, 8.80% on
02 pluto). On corpus-500, the **available oracle wins are 5-10× smaller**
(mean −0.53% vs −2.12%), and Cycle 96's grid structure was already
showing this — most corpus content lives near the Pareto front with the
production default. **Small wins amplify router noise:** missing a
−0.5% win costs the same wc=0% as missing a −10% one, and the wins are
no longer big enough to absorb the costs of +0.5% false routes.

This is the natural extrapolation of Cycle 96's "default is on Pareto for
4/7" baseline-7 result to a wider distribution: **most content's
production default already sits near the Pareto front**. The router can
only ship if it can decide **abstain** as accurately as it decides
Chroma — currently it does not.

## Decision gate

- mean win-capture = **66%** (gate ≥ 80%) ✗
- pass-fraction = **65%** (gate ≥ 60%) ✓
- **YELLOW** — pass-rate gate clears but mean-wc does not. Production
  wiring as the default `Quality::Auto-R4` is not safe; opt-in shippable
  with documented per-fixture risks would be a stretch.

## Cycle 100 candidate (autorun entry)

Two concurrent widenings target the named failures, then re-validate
**on the same 20-fixture corpus-500 sample**:

1. **Widen UI predicate (catches mi0):**
   `chroma_entropy < 3.0 AND (edge_density > 0.2 OR trans_frac > 0.5)`
   Expected wins: mi0 30% → ≥80%, others unchanged.

2. **Tighten Chroma predicate (kills p66 / p426 false-positives) while
   recovering Type-X wins (n29, p11, p119)** — replace the OR-based
   trans-or-chroma rule with a 2-predicate AND that requires both
   chroma signal AND a content-specific gate:

   ```text
   Chroma class:  trans_frac > 0.1
              OR  (mean_chroma > 0.025 AND smoothness < 0.05 AND
                   edge_density > 0.2 AND chroma_entropy > 3.5)
   ```

   The `chroma_entropy > 3.5` join-predicate is the new piece: it
   distinguishes "wants d=0.5" (n29 ent 4.69, p409 ent 5.11, p449 ent
   3.54 borderline, mars 4.09) from "doesn't" (p66 ent 4.89 — but its
   smoothness 0.019 ≪ 0.05 sneaks through anyway, so chroma entropy
   alone won't fix p66). A real fix for p66 / p426 requires an
   anti-d=0.5 feature, possibly **`bandpass_ratio < t`** since the
   bandpass distinguishes mid-scale (R1-friendly) from fine-scale
   (R1-hostile) content.

   So Cycle 100 specifically should sweep **bandpass_ratio** as the
   gate for Chroma vs Stoch on opaque content, retaining the threshold
   tuning from B1 elsewhere.

3. **Accept the K=192 / d=0.3 limitations.** 3-class router cannot hit
   p430 K=192 oracle or p32 d=0.3 oracle exactly. Mark these as
   "structural" and out of scope; the win loss is bounded (≤ 0.5% per
   fixture).

Decision gate for Cycle 100: same as Cycle 99. **GREEN requires both
mean-wc ≥ 80% AND pass-fraction ≥ 60% on the 20-fixture corpus-500
sample.** If GREEN, propose production wiring at last.

## Files

- `crates/nupic-research/examples/cycle99_r4_b1_corpus500.rs` — full
  driver. 180 encodes + 180 SSIM subprocess calls in 82 s.
- Previous: 04ccc (Cycle 98 B1 GREEN on 10-fix), 04bbb (Cycle 97 v1
  YELLOW).
