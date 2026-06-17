# 04a — Cycle 34: tier-1c/2c uniq-bucket peak-d split (v0.5.45)

## Motivation

Cycle 33 (03z) closed all 12 tier-4 corpus fixtures to peak-d. Audit
of the remaining tiers found tier-1c and tier-2c (sharp-mask
transparency) underserved — Cycle 28 hard-coded `d=0.5` for both,
based on N=2 fixtures (02-pluto, 21-earth-hemi at the time). New
round-2 fixtures (22-tree, 23-statue) and a fine-grid sweep
revealed peak-d for these tiers is NOT 0.5 across the board.

## Sweep — peak-d monotonic in opaque-region uniq

```
fixture        tier   opq    a_part  uniq   peak d   peak SSIM   gap vs 0.5
02 pluto       2c     0.78   0.008   19 K   0.50     80.87       0
22 tree-trans  1c     0.30   0.052   26 K   0.70     66.99       +0.25
23 statue      1c     0.16   0.001   43 K   0.80     80.73       +0.10
21 earth-hemi  2c     0.60   0.046   86 K   0.85     67.43       +1.01
```

Sort by uniq:

```
19 K  → 0.50
26 K  → 0.70
43 K  → 0.80
86 K  → 0.85
```

**Clean monotonic** — uniq is the discriminator. Intuition: a tiny
palette (02-pluto's 19 K) covers its colour space densely enough
that dither residual is small; high-uniq sharp-masks like 21 have
sparse palette coverage, so strong dither (0.85) helps fill in
between palette entries.

No other tier in the corpus has this signal — the previous
classifier branches all routed to `d ∈ {0.0, 0.25, 0.5, 0.7}`.
Cycle 34 introduces `d=0.85` as a new bucket value.

## Implementation — 3-way split, same for tier-1c & 2c

```rust
if opaque_ratio < 0.95 {
    let n_partial = n_total - n_opaque - n_zero_alpha;
    if (n_partial as f64 / n_total as f64) >= 0.10 {
        // Smooth-gradient transparency, unchanged from pre-C34.
        return if opaque_ratio < 0.50 { 0.0 } else { 0.35 };
    }
    // Sharp-mask: split by opaque-region uniq color count.
    let mut uniq = HashSet::with_capacity(60_500);
    /* count opaque-pixel unique colors, capped at 60_001 */
    if uniq.len() > 60_000 { return 0.85; } // tier-1c/2c-h
    if uniq.len() > 20_000 { return 0.7; }  // tier-1c/2c-m
    return 0.5;                              // tier-1c/2c-l
}
```

The tier-1c and tier-2c branches are now unified — same uniq logic.
This consolidates two near-duplicate code paths from Cycle 28 into
one. Cost is one HashSet insert pass over opaque pixels (cap 60 001
to early-break on high-uniq fixtures), only when the sharp-mask
condition (`a_partial < 0.10`) is met.

Thresholds {20 K, 60 K} chosen to hit peak on 02 / 22 / 21 exactly
and 23 within 0.02 of peak. Friendlier round numbers than the
fitted optima {22 K, 60 K}.

## Bench wins

```
              Cycle 33 d=0.5         Cycle 34 routed       Δ SSIM   Δ size
02 pluto      d=0.5 / 80.87          d=0.5  / 80.87       0        0
22 tree       d=0.5 / 66.73          d=0.7  / 66.99       +0.25    +22 KB
23 statue     d=0.5 / 80.63          d=0.7  / 80.71       +0.08    +11 KB
21 earth-hemi d=0.5 / 66.42          d=0.85 / 67.43       +1.01    +148 KB
                                                          ------
                                                          +1.34    +181 KB
```

23 doesn't hit its exact peak (d=0.8 → 80.73) because it routes
into the 0.7 bucket; residual 0.02 SSIM. Adding a 4th split
({20 K, 40 K, 60 K}) would close it but overfits N=1 to a third
threshold. Deferred.

## Full-corpus verification

`probe_real_corpus` post-Cycle 34 — only 3 fixtures changed routing:

| fixture | C33 d | C34 d |
|---|---:|---:|
| 21 earth-hemi | 0.50 | 0.85 |
| 22 tree-trans | 0.50 | 0.70 |
| 23 statue | 0.50 | 0.70 |
| (26 others) | unchanged | unchanged |

All workspace tests pass. tier-4 fixtures bit-exact identical.

## Open backlog

1. **23-statue residual** 0.02 SSIM if a future fixture wants
   d=0.8 at uniq ∈ [40 K, 60 K), justify a 4-bucket split.
2. **01-trans-demo / 14-soft-trans** (tier-1 smooth-gradient) —
   not swept yet. May have peak-d gaps similar to tier-1c. Quick
   peak-d sweep before declaring tier-1 closed.

## Files touched

- `crates/nupic-quantize/src/lib.rs::classify_for_auto_dither`
  (tier-1c + tier-2c branches consolidated, 3-bucket uniq split)
- `Cargo.toml` workspace version 0.5.44 → 0.5.45
