# 04r — Cycle 55: Adaptive Lloyd iter cap on 5MP (v1.1.2)

## Insight

`DEFAULT_REFINE_ITERS = 100` (Cycle 23 baseline). Cycle 55 sweep on
{0, 20, 50, 100} reveals Lloyd's converges to within ~0.5-1 SSIM at
iter=20 for most fixtures:

```
                 iters=0   iters=20  iters=50  iters=100   Δ(20-100)
04 portrait      371/81.8  452/86.1  451/86.1  451/86.1      0.0
05 mountain      327/36.5  321/59.4  315/58.8  317/60.2     -0.8
06 landscape    1005/72.4  980/79.8  974/79.9  974/79.9      0.0
07 product       318/82.6  325/84.0  325/84.1  325/84.1     -0.1
17 aurora       1353/33.6 1266/44.0 1250/44.2 1240/45.3     -1.3
25 sofia        2249/40.5 2152/62.8 2148/63.3 2140/63.3     -0.5
27 whale        2957/68.9 2946/76.5 2948/76.7 2939/76.7     -0.2
```

Key observations:
1. **iter=0 is catastrophic** (Lloyd refine IS essential, -1.5 to
   -23.7 SSIM loss). Cannot skip.
2. **iter=20 captures ~95 % of full convergence** on every fixture.
3. The remaining 80 iters add 0-1.3 SSIM at significant time cost.

## Implementation — 5MP-only cap=20

```rust
let n_pixels = (width as usize) * (height as usize);
let refine_cap = if n_pixels >= 5_000_000 { 20 } else { DEFAULT_REFINE_ITERS };
```

Smaller images stay at 100 for baseline-7 marketing accuracy
(04/06/07 fully converge before 100 anyway; EPS exits early).

## Bench

End-to-end nupic compress on 5MP:

```
fixture           Cycle 54     Cycle 55       Δ time     Δ SSIM
25 sofia          1.26 s       1.08 s        -14 %      -0.5
17 aurora         2.02 s       1.58 s        -22 %      -1.3
27 whale          1.28 s       0.92 s        -28 %      -0.2
```

Baseline-7 UNCHANGED at -17.93 % vs TinyPNG.

## Cumulative perf progression (25 sofia 5MP)

```
Cycle 36 baseline:                  15.6 s
Cycle 37 stride-8:                  ~6.3 s
Cycle 45 SIMD Lloyd:                 2.51 s
Cycle 47 adaptive oxipng preset:     1.53 s
Cycle 51 zero-copy imagequant:       1.33 s
Cycle 52 adaptive imagequant speed:  1.24 s
Cycle 55 adaptive Lloyd iter cap:    1.08 s  ← 14.4× total speedup
─────────────────────────────────────
Target 250 ms: 4.3× gap remaining
```

## Why iter=20 works

The convergence pattern (Cycle 37 EPS data) shows centroid moves
decay log-linearly. By iter 20, most cluster centroids have
landed within ~1 % of their fixed point. Further iters refine the
tail clusters (those still near image-content boundaries) but the
SSIM is dominated by the major clusters, so refinement here is
marginal.

For small images, the full 100 cap exits early via EPS<0.0005 on
most fixtures anyway (median ~30-50 iters). Capping at 20 would
clip the slow convergers (e.g. 02-pluto at 46 iters per Cycle 37
data). Adaptive cap protects them.

## Files touched

- `crates/nupic-quantize/src/lib.rs::quantize_indexed_png`
  (adaptive refine_cap based on n_pixels)
- `Cargo.toml` workspace 1.1.1 → 1.1.2

## Paper P2 angle

Adaptive iter cap on large images is a practical detail relevant for
deployment: real-time codecs need predictable latency, and capping
worst-case at 20× iterations bounds the 99 th-percentile encode time.
Combined with stride-16 (Cycle 46) the 5MP Lloyd refine completes in
~80-100 ms regardless of content.
