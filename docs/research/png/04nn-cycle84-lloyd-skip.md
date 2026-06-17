# 04nn — Cycle 84: blanket Lloyd skip for 5MP+ ruled out (negative)

## TL;DR

Cycle 83 essay suggested skipping Lloyd entirely (cap=0) for ≥5MP
content as a path to < 250 ms KPI. Empirical test shows cap=0 IS
fast enough (17 aurora 0.24 s, 25 sofia 0.23 s — both inside the
gate!) but loses 2-13 SSIM uniformly. 16 earthrise particularly
drops 83.4 → 69.9 (-13.5 SSIM). Not a shipping trade.

## Per-fixture (cap=0 vs cap=10 at 5MP+)

```
fixture           cap=0                  cap=10                 Δsize  ΔSSIM
                  size_KB  t      SSIM    size_KB  t      SSIM
17 aurora 5.9MP    1584    0.24s  47.6     1551   0.31s  51.1    +33    -3.5
25 sofia 5.5MP     2777    0.23s  72.7     2736   0.30s  74.8    +41    -2.1
27 whale 5.5MP     3275    0.23s  76.6     3266   0.30s  78.5     +9    -1.9
28 orca 14MP      10414    0.52s  75.1    10351   0.68s  77.9    +63    -2.8
18 snow 17MP       4815    0.67s  80.8     5271   0.87s  83.0   -456    -2.2   ← anomaly
16 earthrise 25MP  8713    0.96s  69.9    13997   1.11s  83.4  -5284   -13.5   ← big anomaly
```

## Anomalies

18 snow and 16 earthrise show SMALLER output at cap=0. Lloyd
should optimise palette for content fidelity, so output should be
SMALLER post-Lloyd, not larger. The reverse here means:

- For stochastic content (16 earthrise has heavy texture), Lloyd
  centroids drift toward dense color clusters in the data. This
  distributes palette indices more uniformly across the image,
  which DISRUPTS the deflate LZ77 patterns vs. the post-imagequant
  median-cut palette which often has more "color regions" that
  deflate exploits.
- The result is: Lloyd improves PIXEL fidelity but harms COMPRESSION
  density. For high-frequency content the net is +size.

This is a Cycle 76 type finding repeated: "the existing pipeline
already does its job; the obvious 'optimisation' has unforeseen
second-order effects".

## Why we can't just ship cap=0

Even though 17 aurora cap=0 = 0.24 s HITS the 250 ms KPI, the
SSIM drop of 3.5 makes content qualitatively worse:

- 17 aurora SSIM 47.6 vs 51.1: ~7 % perceived quality reduction
- 16 earthrise SSIM 69.9 vs 83.4: ~16 % perceived quality drop

For the perf-vs-quality trade-off there's no obvious win — the
SSIM drop is real, and the size win on 18/16 is content-specific
(not a general property to ship behind).

## Routing alternatives ruled in/out

Could we ship `cap=0 if var > threshold` (stochastic-only skip)?

```
fixture          var       cap=10 SSIM  cap=0 SSIM  Δ
17 aurora        ?         51.1         47.6        -3.5
16 earthrise     stochastic 83.4        69.9       -13.5   ← STOCHASTIC HURTS MOST
```

So the obvious "stochastic content benefits least from Lloyd
→ skip it" is FALSE empirically. 16 earthrise (likely high-var
stochastic landscape) actually NEEDS Lloyd to recover SSIM.

The relationship between content statistics and Lloyd's effect is
not monotonic in var. No clean routing signal emerges.

## Conclusion

Cycle 84 = negative finding. The Lloyd cap=10 (Cycle 79) is the
right operating point for 5MP+. To reach < 250 ms 5MP requires
non-Lloyd improvements:

1. **Apply skip when palette unchanged** (Cycle 83 backlog) —
   estimated ~55 ms saved on 5MP if palette converged early.
   Quality-neutral if implemented correctly.

2. **Direct libdeflate** — Cycle 81 ruled out current nupic-deflate
   Level::Best. Could implement Level::Fast in nupic-deflate or
   bind to system libdeflate directly. Months of work.

3. **GPU palette quantisation** — would skip CPU pipeline for ≥5MP.
   Massive integration cost.

4. **Two-pass pipeline overlap** — start oxipng while encoding
   intermediate PNG bytes in parallel. ~50 ms savings on 5MP.

Best ship-feasible: (1) apply skip. (4) requires rayon/tokio
restructuring.

No Cargo bump. Doc-only.

## Files touched

- Research `examples/c84_lloyd_skip.rs` (not committed)
- `docs/research/png/04nn-cycle84-lloyd-skip.md` (this essay)
