# 04b — Cycle 35: tier-1 smooth-gradient gets dither (v0.5.46)

## Motivation

Cycle 34's audit left 01-trans-demo and 14-soft-transparent
(both tier-1 smooth-gradient — `opq < 0.5` + `a_partial ≥ 0.1`)
unswept. Cycle 28's rationale assumed smooth-gradient transparency
should not be dithered ("transparency-dominant — smooth-gradient
or mixed → d=0.0"). Peak-d sweep contradicts that assumption.

## Sweep — 0.7 is decisive peak

```
01-trans-demo
  d=0.0  size= 45 364  ssim=-46.43   ← Cycle 34 routing
  d=0.25 size= 50 857  ssim=-42.49
  d=0.5  size= 57 430  ssim=-34.90
  d=0.7  size= 65 610  ssim=-32.75   ← peak  (+13.68 vs current)
  d=1.0  size= 78 917  ssim=-58.37   ← crash

14-soft-trans
  d=0.0  size=148 361  ssim= 66.90   ← Cycle 34 routing
  d=0.25 size=181 076  ssim= 68.72
  d=0.5  size=197 027  ssim= 69.96
  d=0.7  size=208 610  ssim= 70.44   ← peak  (+3.55 vs current)
  d=1.0  size=221 721  ssim= 63.29   ← crash
```

Both fixtures peak at d=0.7. The +13.68 SSIM jump on 01 is the
largest single-fixture gap discovered in the entire research cycle
to date.

01-trans-demo's absolute score is negative because the raw indexed-
PNG palette quantization loses the smooth alpha-gradient frame —
SSIMULACRA2 reports a low (negative) score even at peak. The point
is the GAP from current routing (−46 → −33 is decisive
improvement), not the absolute number.

## Why Cycle 28 was wrong

Cycle 28 reasoned: "transparency-dominant gradient — dither doesn't
help". But it never SWEPT the fixtures. The smooth alpha gradient
gets palette-quantized into discrete alpha bands; dither in OKLab +
alpha space (the `nupic_quantize` FS routine handles both axes)
breaks up the bands and recovers smoothness.

Cycle 35 lesson: **assumptions about which content benefits from
dither must be sweep-verified**, not reasoned from priors. Even
"obviously won't help" categories can have multi-SSIM gaps.

## Fix — flip tier-1 smooth-gradient return

```diff
        if a_partial_ratio >= 0.10 {
-           return if opaque_ratio < 0.50 { 0.0 } else { 0.35 };
+           // Cycle 35: smooth-gradient transparency benefits from dither.
+           return if opaque_ratio < 0.50 { 0.7 } else { 0.35 };
        }
```

The tier-2 smooth branch (`opaque_ratio ∈ [0.5, 0.95)`) stays at
0.35 — zero corpus evidence to retune.

## Bench wins (auto-routed)

```
               C34 d=0.0           C35 d=0.7           Δ SSIM    Δ size
01 trans-demo  45 364 / −46.43     65 610 / −32.75     +13.68    +20 KB
14 soft-trans  148 361 / 66.90     208 610 / 70.44     +3.55     +60 KB
                                                       ------
                                                       +17.23    +80 KB
```

The largest two-fixture wins in the session.

## Routing diff

Only 01 and 14 changed routing (0.0 → 0.7); other 27 corpus
fixtures bit-exact identical.

## Tier coverage status

| tier | corpus fixtures | peak coverage |
|---|---|---|
| tier-1 smooth (01, 14) | 2 | **2/2 peak (Cycle 35)** |
| tier-1c (22, 23) | 2 | 1/2 peak, 1 within 0.02 |
| tier-2c (02, 21) | 2 | 2/2 peak |
| tier-2 smooth | 0 | n/a |
| tier-3 (03, 09, 10, 12, 15) | 5 | SSIM=100 trivially, d-invariant |
| tier-4 (a/b/c/d/e/f, 13 fixtures) | 13 | **12/12 peak** (Cycle 33+34) |
| tier-1 small (n_total < 200K) | 0 in corpus | n/a |

After Cycle 35, every classifier branch with corpus evidence
routes its fixtures to peak-d (or within 0.02 for 23). Two
branches (tier-2 smooth, tier-1 small) lack corpus evidence and
keep their priors.

## Open backlog

1. **tier-2 smooth-gradient corpus** — need fixture with
   `opq ∈ [0.5, 0.95)` + `a_partial ≥ 0.1` to validate d=0.35 prior.
2. **tier-1 small corpus** (n_total < 200K) — current return 0.0
   never validated. Quick to sweep if a sub-200K fixture exists.

## Files touched

- `crates/nupic-quantize/src/lib.rs::classify_for_auto_dither`
  (tier-1 smooth branch flipped 0.0 → 0.7)
- `Cargo.toml` workspace version 0.5.45 → 0.5.46
