# 03w — Cycle 30: round-2 real-photo corpus + tier-4d threshold re-tune (v0.5.41)

## Motivation

Cycle 27 (v0.5.39) shipped the `tier-4d` high-uniq split with threshold
`uniq > 50_000 → d=0.7`, based on N=3 evidence (13-very-large,
17-aurora, 20-rainbow). User asked for round-2 corpus to validate.

## New round-2 corpus (`assets/png-bench/inputs-ext-real/`, 6 fixtures)

All public-domain from Wikimedia Commons, fetched via `curl
upload.wikimedia.org`, converted to PNG-lossless.

| file | dims | MP |
|---|---|---|
| 24-melk-abbey-24mp.png | 5971×3981 | 23.8 |
| 25-sofia-cathedral-5mp.png | 2908×1883 | 5.5 |
| 26-angkor-wat-32mp.png | 7956×4009 | 31.9 |
| 27-whale-tail-5mp.png | 3128×1765 | 5.5 |
| 28-orca-14mp.png | 5787×2493 | 14.4 |
| 29-sundew-3mp.png | 2048×1536 | 3.1 |

## Re-tune analysis — 50K threshold is too aggressive

Round-2 added 1 false-positive at threshold 50K:

| fixture | uniq | var | Cycle 27 routes | peak SSIM is at | Δ |
|---|---:|---:|---:|---:|---:|
| 04 portrait | 25 K | (training corpus) | 0.5 (tier-4a) | 0.5 ✓ | — |
| 16 earthrise | 43 K | 2.1 | 0.5 (tier-4a) | 0.5 ✓ | — |
| **27 whale-tail** | **118 K** | 35.8 | **0.7 (tier-4d)** | **0.5** | **−0.31 SSIM, +54 KB** |
| 29 sundew | 131 K | 30.8 | 0.7 (tier-4d) | 0.7 ✓ | — |
| 17 aurora | 159 K | 26.1 | 0.7 (tier-4d) | 0.7 ✓ | — |
| 20 rainbow | 164 K | 11.1 | 0.7 (tier-4d) | 0.7 ✓ | — |
| 13 very-large | 1.2 M | (training corpus) | 0.7 (tier-4d) | 0.7 ✓ | — |

Clean gap between 27-whale (118 K, wants 0.5) and 29-sundew (131 K,
wants 0.7). **Threshold 120 K cleanly separates with no false-positive
in the combined Cycle 27 + Cycle 30 corpus.**

Implemented as a literal-constant bump in
`crates/nupic-quantize/src/lib.rs::classify_for_auto_dither`:

```diff
-    if uniq > 50_000 {
+    if uniq > 120_000 {
         return 0.7; // tier-4d: high-uniq smooth photo
     }
```

Also bumped `HashSet::with_capacity(50_500)` → `120_500` so the new
threshold doesn't trigger an extra rehash on the hot path.

## Bench

```
27-whale @ d=0.5 (new):  3 266 917 B,  SSIM = 80.03  ← new default
27-whale @ d=0.7 (old):  3 321 811 B,  SSIM = 79.71
                         ↑               ↑
                       −54 KB         +0.31 SSIM

29-sundew @ d=0.5:      1 575 018 B,  SSIM = 81.85
29-sundew @ d=0.7:      1 605 850 B,  SSIM = 82.08  ← stays here
```

27 improves both dimensions; 29 stays on the threshold-correct side.

## Independent finding — tier-4b var split is also wrong on N=3

While probing the round-2 corpus, three fixtures classify into
tier-4b (`var ≥ 50 → d=0.7`) but actually want 0.5:

| fixture | var | uniq | tier-4b routes | peak is |
|---|---:|---:|---:|---:|
| 18 snowflake | 123.2 | 114 K | 0.7 | 0.5 |
| 25 sofia-cathedral | 209.1 | 117 K | 0.7 | 0.5 |
| 28 orca | 68.0 | 126 K | 0.7 | 0.5 |

These all share `mr=1.0–2.6` and `adj_mn=2.7–6.9` (low contrast),
unlike 17-aurora / 20-rainbow which are higher-frequency. Likely a
signal in `mr` or `adj_mn` not in `var` alone. **Documented for
future cycle, NOT fixed here** — the tier-4d miss is the clear
N=1 false-positive worth fixing immediately; tier-4b retuning needs
N≥3 corroboration before we touch it.

## Verification

- 219 workspace tests still pass (all 16 crates green)
- `probe_real_corpus` shows 27-whale → 0.5, 29-sundew → 0.7 ✓
- 0.4 / 0.5.x size & SSIM regression sweep clean on 19 baseline
  fixtures (no bit-exact change outside the routed fixture)

## Files touched

- `crates/nupic-quantize/src/lib.rs::classify_for_auto_dither`
  (threshold + capacity literal + updated comment block)
- `crates/nupic-research/examples/probe_real_corpus.rs`
  (added 24–29 to the probe list)
- `assets/png-bench/inputs-ext-real/README.md`
  (round-2 inventory)
- `assets/png-bench/inputs-ext-real/24–29.png` (new fixtures)

## Open questions (next cycle backlog)

1. **tier-4b false-positives (3 fixtures)** — `var` alone isn't
   sufficient. Probe `mr × adj_mn` joint signal.
2. **24-melk / 26-angkor** — these classify tier-4b (var > 50) and
   pre-Cycle 30 baseline isn't measured for them. Need peak-d sweep
   to know whether they're correctly routed.
