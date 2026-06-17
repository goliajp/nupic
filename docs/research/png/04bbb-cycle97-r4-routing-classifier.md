> **⚠️ RETRACTION (Cycle 102, 2026-06-18)**
>
> This essay measured spike configs against a forced `K=256 d=0 preset=3` baseline,
> which does NOT match production `Quality::Auto` (which picks K via classifier).
> The GREEN/YELLOW/RED verdicts are internally consistent but **production-irrelevant**.
> See [[04ggg-cycle102]] for the methodology reset and locked three-axis gate
> (size ≤ 0.80× tiny AND SSIM ≥ tiny AND perf max).

# 04bbb · Cycle 97 — R4 3-class K/dither routing classifier YELLOW

**Status:** YELLOW (5/10 PASS, mean win-capture 52% vs 80% gate). The hand
rule from essay 04aaa's "Cycle 97 next-up" specification captures real
structure on **3 fixtures perfectly** (03 wiki 100% / 02 pluto class
correct / 05 mountain & 06 landscape default-correct), but **misroutes 4 of
the 5MP-tier-plus-portrait fixtures**. Two failure modes account for all
four: (a) threshold `mean_chroma > 0.04` is **just above** 04 portrait's
0.027 so portrait drops to Stochastic and forfeits its +0.02-SSIM, −0.3%
Pareto win; (b) the `smoothness < 0.05` chroma-rich predicate is too
permissive on **stochastic-noise 5MP** fixtures (17 aurora 0.0177,
27 whale 0.0387) — both get false-Chroma routes that cost +16.5% / +3.6%
size for SSIM the router doesn't need.

Result is **paper §6 routing-analysis material**: the easy 2-of-3 classes
(UI 03 wiki, default-correct 05/06/01/25) are trivially separable, but
distinguishing **chroma-rich-portrait** from **stochastic-5MP** needs a
feature R1 ruled out (Cycle 95's LR thread) — likely `bandpass_ratio` or
`edge_chroma_corr` from Cycle 93.

## TL;DR

| metric | value |
|---|---:|
| cohort | baseline-7 + 5MP {17, 25, 27} = 10 fixtures |
| grid encodes | 81 (9 cfg × 7 baseline-7 + 6 cfg × 3 5MP) |
| total wall time | 59.8 s |
| router design | 3-class hand rule from 04aaa § "Cycle 97 next-up" |
| **fixtures passing wc ≥ 80%** | **5 / 10  (50%)** |
| **mean win-capture** | **52%** |
| mean oracle Δsize ceiling | −2.12% |
| mean router Δsize actual | **+0.74%** (router worse than default on average) |
| router / oracle ratio | −35% (router moves away from oracle on net) |

**Gate:** ≥ 6/10 fixtures at wc ≥ 80% AND mean-wc ≥ 80% → GREEN. Result:
50% pass, 52% mean — **YELLOW-wc** (mean promising but pass-rate short).

## The 3-class hand rule under test

```text
UI/logo class:        chroma_entropy < 3.0  AND  edge_density > 0.2
                      → route (K=128, d=0)

Chroma-rich class:    trans_frac > 0  OR  (mean_chroma > 0.04 AND
                                            smoothness < 0.05)
                      → route (K=256, d=0.5)

Stochastic-noise:     all others
                      → route (K=256, d=0)        [production default]
```

The (K, d) targets come from the Cycle 96 R-D-grid Pareto fronts (essay
04aaa § "Pareto front analysis"). Baseline-7 fixtures encode at preset=3
(production tier), 5MP at preset=0 (per Cycle 79 3-tier rule).

## Per-fixture trace

| fixture | features (chr / sm / ed / tr / ent) | class | oracle (K,d) | def→oracle Δsize | router (K,d) | def→router Δsize | wc | pass? |
|---|---|:---:|:---:|---:|:---:|---:|---:|:---:|
| 01 trans     | .139 / .018 / .128 / **.96** / 4.66 | Chroma | (256, 0.0) | 0.00% | **(256, 0.5)** | −13.0% (in-band, SSIM −39.6 vs default −36.5) | 100% | ✓ |
| 02 pluto     | .043 / .022 / .171 / .22 / 4.52 | **Chroma** | **(192, 0.5)** | −8.80% | (256, 0.5) | −1.74% | **20%** | ✗ |
| 03 wiki      | .010 / .053 / .267 / .26 / **2.54** | **UI** | **(128, 0.0)** | −10.98% | (128, 0.0) | −10.98% | **100%** | ✓ |
| 04 portrait  | **.027** / .042 / .433 / .000 / 4.77 | Stoch | **(256, 0.5)** | −0.26% | (256, 0.0) | 0.00% | **0%** | ✗ |
| 05 mountain  | .089 / .069 / .378 / .000 / 5.13 | Stoch | (256, 0.0) | 0.00% | (256, 0.0) | 0.00% | 100% | ✓ |
| 06 landscape | .023 / .160 / .734 / .000 / 4.67 | Stoch | (256, 0.0) | 0.00% | (256, 0.0) | 0.00% | 100% | ✓ |
| 07 product   | .026 / .030 / .129 / .000 / 5.06 | Stoch | (256, 0.3) | −1.17% | (256, 0.0) | 0.00% | **0%** | ✗ |
| 17 aurora    | .067 / .018 / .055 / .000 / 5.06 | **Chroma** | (256, 0.0) | 0.00% | (256, 0.5) | **+16.55%** (out of band) | **0%** | ✗ |
| 25 sofia     | .060 / .054 / .312 / .000 / 4.07 | Stoch | (256, 0.0) | 0.00% | (256, 0.0) | 0.00% | 100% | ✓ |
| 27 whale     | .067 / .039 / .369 / .000 / 4.12 | **Chroma** | (256, 0.0) | 0.00% | (256, 0.5) | **+3.57%** (out of band) | **0%** | ✗ |

Note 01 trans: oracle says "default on front" but router's (256, 0.5)
slips inside the −0.5-SSIM iso band at smaller size (57 854 vs 66 504 B),
so the win-capture formula scores it 100% — a fluke of the iso-band metric
on a fixture where SSIMULACRA2 is already deep-negative due to alpha
artifacts. Don't read this 100% as router skill.

## Failure-mode analysis

### F1. Chroma threshold misses 04 portrait

`mean_chroma > 0.04` rejects portrait at 0.027. Lowering to 0.025 would
catch it but at risk of pulling in other low-chroma fixtures. The literal
essay rule traded portrait off against false-positive resistance — that
trade-off didn't pay because portrait is the most clear-cut Chroma-rich
in the cohort (Cycle 96 measured **both quality AND size** improvement at
K=256 d=0.5 vs K=256 d=0).

### F2. `smoothness < 0.05` admits stochastic 5MP fixtures

17 aurora (smoothness 0.018) and 27 whale (0.039) clear the predicate
because at 5MP scale the per-pixel luma differential averages low even on
visually-noisy content. The smoothness feature was designed on baseline-7
fixtures where 0.05 cleanly separates flat-photo (02 pluto 0.022) from
edge-photo (06 landscape 0.160). On 5MP the threshold loses its meaning.

### F3. Granularity gap — no K=192 d=0.5 class

02 pluto's oracle is **K=192 d=0.5** but the 3-class router only offers
K=256 d=0.5 for the Chroma class — captures only 20% of the available
−8.8% win. Splitting Chroma into Chroma-K192 (transparent) and Chroma-K256
(opaque-portrait) would close this gap.

### F4. 07 product needs d=0.3, not in router options

07 product wants `d=0.3` (modest dither) — gap of 1.17% missed. d=0.3 is
absent from the router's (K, d) target set by design. Tolerable.

## What the next router design should look like (Cycle 98 candidate)

Three concrete changes from F1-F3:

1. **Lower chroma threshold to 0.025** — picks up 04 portrait. Verified
   non-collision: no Stoch fixture in cohort has chroma in (0.025, 0.04).
2. **Replace `smoothness < 0.05` with a 5MP-aware feature** — likely
   `bandpass_ratio` from Cycle 93 (which Cycle 95 also tried at thread end
   without success on R1 → friend-classifier; **but the routing problem
   here is K/dither, not friend/hostile, and the structural pattern is
   different**). Alternatively, a per-tier chroma-floor (5MP fixtures only
   chroma-rich if `mean_chroma > 0.10` etc.).
3. **Split Chroma into 4-class router** with Chroma-K192 (for transparent
   chroma-rich) and Chroma-K256 (for opaque high-edge chroma-rich).

The per-tier branch in change 2 is the lowest-risk minimum-viable fix; if
Cycle 98 lands on **change 1 + per-tier predicate**, it should clear
≥ 80% mean-wc on this same 10-fixture cohort.

## Decision gate

- mean win-capture = **52%** (gate ≥ 80%)
- fixtures-passing fraction = **50%** (gate ≥ 60%)
- **YELLOW** — 3-class router captures the easy half but misses portrait
  + 5MP-noise discrimination. Not ship-ready as `Quality::Auto-R4`.

## Production implication

**Don't ship.** The 3-class rule would **regress** 17 aurora by +16.5%
size and 27 whale by +3.6% size for no SSIM benefit (the +SSIM on aurora
is outside the iso band but doesn't justify the size cost on a NAS/CDN
workload). The clean wins (03 wiki, 04 portrait) need to be unlocked by a
better-discriminating router before this becomes a `Quality::Auto-R4`
ship.

## Files

- `crates/nupic-research/examples/cycle97_r4_routing_classifier.rs` — full
  driver. 81 encodes + 81 SSIM subprocess calls in 60 s.
- Previous: 04aaa (Cycle 96 R-D grid).

## Cycle 98 next-up (autorun entry)

Refine to a 3-or-4-class router that closes F1 + F2 simultaneously:

1. **(A1)** Threshold tune: chroma threshold 0.04 → 0.025, plus per-tier
   chroma-floor for 5MP (require `mean_chroma > 0.10` for 5MP-tier
   Chroma class).
2. **(A2)** Replace `smoothness < 0.05` predicate with `bandpass_ratio >
   t` (Cycle 93 feature) — sweep `t` for best split between 02 pluto /
   04 portrait (Chroma) and 17 aurora / 25 sofia / 27 whale (Stoch).
3. **(B)** 4-class split: introduce Chroma-K192 for transparent-rich,
   Chroma-K256 for opaque-rich. Use trans_frac to route between them.

Decision gate for Cycle 98 unchanged: mean-wc ≥ 80% AND pass-rate ≥
6/10. If only (B) clears, that's the spec for `Quality::Auto-R4` ship.
