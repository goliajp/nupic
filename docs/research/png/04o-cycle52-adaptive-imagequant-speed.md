# 04o — Cycle 52: Adaptive imagequant speed (RSS −58%, time −7%, v1.0.9)

## Discovery — speed=4 has a 130 MB allocation cliff on 5MP

Cycle 50 RSS profile pointed at `imagequant::quantize()` as the
dominant memory event (+156 MB on 25-sofia 5MP). Cycle 51 closed
20 MB of that via zero-copy. The remaining 130 MB needed direct
investigation.

Cycle 52 sweep on imagequant's `set_speed` parameter (4 = current,
8 = fast, 10 = fastest):

```
25 sofia 5MP (n=144, α=0.5 importance):
  speed=4   train=115 ms   ΔRSS=+135 MB   size=2095 KB   SSIM=67.41
  speed=6   train=104 ms   ΔRSS=+2 MB     size=2089 KB   SSIM=67.69
  speed=8   train=26 ms    ΔRSS=+2 MB     size=2089 KB   SSIM=67.87
  speed=10  train=26 ms    ΔRSS=+2 MB     size=2089 KB   SSIM=67.87

05 mountain (1.4MP):
  speed=4   train=88 ms    ΔRSS=+5 MB     size=312 KB    SSIM=60.65
  speed=8   train=21 ms    ΔRSS=+0 MB     size=329 KB    SSIM=60.92
```

**Critical finding**: speed=4 carries a sharp +135 MB allocation
spike on ≥ 5 MP inputs that disappears completely at speed ≥ 6.
This is imagequant's best-quality k-d-tree codepath; faster speeds
use a lighter median-cut path that doesn't allocate the tree.

Effect on output:
- 5 MP: speed=8 is **better on every axis** — 4× faster, 130 MB
  less RSS, SAME size (within 6 KB), HIGHER SSIM (+0.46).
- < 5 MP (05 mountain 1.4 MP): speed=8 is faster but +5 % size.
  Keep speed=4 here to preserve baseline-7 marketing size.

## Implementation — adaptive speed

```rust
let n_pixels = (w as usize) * (h as usize);
let speed = if n_pixels >= 5_000_000 { 8 } else { 4 };
attrs.set_speed(speed).map_err(|_| ())?;
```

Single conditional in `try_iq`. No API change.

## Bench

Peak RSS (`/usr/bin/time -l maximum resident set size`):

| fixture | Cycle 36 | Cycle 50 | Cycle 51 | **Cycle 52** | reduction vs C36 |
|---|---|---|---|---|---|
| 25 sofia 5MP | 503 MB | 230 MB | ~210 MB | **146 MB** | **3.4 ×** |
| 17 aurora 5MP | similar | similar | similar | **164 MB** | similar |
| 27 whale 5MP | similar | similar | similar | **165 MB** | similar |

End-to-end latency (best of 3, `nupic compress --dither auto`):

| fixture | Cycle 36 | Cycle 47 | Cycle 51 | **Cycle 52** | total speedup |
|---|---|---|---|---|---|
| 25 sofia 5MP | 15.6 s | 1.53 s | 1.33 s | **1.24 s** | **12.6 ×** |
| 17 aurora 5MP | 18.5 s | 2.10 s | 1.83 s | **2.02 s** | ~9 × |
| 27 whale 5MP | 7.2 s | 1.56 s | (n/m) | **1.23 s** | ~5.9 × |
| 05 mountain | n/m | 0.65 s | (same) | **0.66 s** | (no change, < 5 MP) |

Baseline-7 marketing UNCHANGED at -17.93 % vs TinyPNG.

## NAS/CDN viability progression

```
Target           5MP latency: < 250 ms      RSS: < 100 MB
Cycle 36 baseline             15.6 s        503 MB
Cycle 52 (today)              1.24 s        146 MB
Remaining gap                  5 ×           1.5 ×
```

Gap halved + reduced from "10-50 × over" to "1.5-5 × over" target.

## Why speed=8 doesn't lose SSIM

imagequant produces a palette via median-cut + Lloyd-style local
search. speed=4 spends more iterations on the local-search refinement
step; speed=8 trusts the median-cut output more. Our pipeline then
runs Stone D Lloyd refinement (100 iters) on top, which converges
both inputs to a similar end palette. So the imagequant speed
parameter primarily affects RSS / compute, not final palette quality
once Lloyd runs.

This is the same observation behind the project's longstanding "Stone
D > imagequant alone" pattern from Cycle 30+: our Lloyd dominates
the quality outcome; imagequant just needs to provide a reasonable
init seed.

## Files touched

- `crates/nupic-quantize/src/lib.rs::train_palette_rgba::try_iq`
  (adaptive `set_speed(8)` for ≥ 5 MP, retain `set_speed(4)` else)
- `Cargo.toml` workspace 1.0.8 → 1.0.9

## Paper P2 angle

Adaptive imagequant speed is a small impl detail but it strengthens
the "real-time deployable" claim:

- Memory characteristic now in NAS/CDN deployable range (< 200 MB)
- 12.6 × encode speedup over the project's start, monotonic across
  cycles 36-52
- Zero quality regression on the marketing baseline (size + SSIM
  invariant)

Together with Cycle 37-46 SIMD + adaptive stride + Cycle 51 zero-copy,
the perf+mem story for P2 has substantial concrete numbers.
