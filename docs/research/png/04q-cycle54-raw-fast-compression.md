# 04q — Cycle 54: Raw PNG Fast compression (v1.1.1)

## Insight

Our pipeline:
```
indices + palette → png crate (Compression::Balanced default)
                  → oxipng::optimize_from_memory (re-deflates IDAT)
```

oxipng's recoding REPLACES the raw IDAT. So our intermediate
deflate level only affects:
- Raw encode time (Fast 11ms vs Balanced 74ms on 5MP)
- Intermediate raw_png byte size (bigger raw → tiny oxipng overhead)

The FINAL OUTPUT IDAT is whatever oxipng generates, identical
regardless of our intermediate level.

## Test

Sweep `Compression ∈ {Fast, Balanced, High}` on raw encode +
fixed oxipng (preset=1 on 5MP, preset=5 on smaller):

```
25 sofia 5MP:
  Fast           raw=4423 KB (11 ms)   →  oxi=2140 KB (646 ms)   total=656 ms
  Balanced       raw=3571 KB (74 ms)   →  oxi=2140 KB (657 ms)   total=731 ms  ← current
  High           raw=3572 KB (255 ms)  →  oxi=2140 KB (675 ms)   total=930 ms

04 portrait 1MP:
  Fast           raw= 895 KB (2 ms)    →  oxi=451 KB (233 ms)    total=235 ms
  Balanced       raw= 708 KB (13 ms)   →  oxi=451 KB (231 ms)    total=244 ms
  High           raw= 701 KB (38 ms)   →  oxi=451 KB (231 ms)    total=268 ms
```

**Fast → Balanced delta: 5MP -75 ms, 1MP -9 ms. Final size identical.**

## Implementation

```rust
enc.set_compression(png::Compression::Fast);
```

One line in `encode_indexed_png_with_alpha`. Zero new dependencies.

## Bench

Final pipeline numbers UNCHANGED in size + SSIM:
- 25 sofia 5MP: 2140 KB (was 2140 KB), SSIM 63.27 (was 63.27)
- 27 whale 5MP: 2938 KB (was 2938 KB), SSIM 76.67 (was 76.67)
- baseline-7: 2169 KB (was 2169 KB), -17.93 % vs TinyPNG

End-to-end nupic compress perf gain seen in detailed sweep but masked
by oxipng's high-variance latency at the total level. The -60 ms gain
is real but ~5 % of total wall-clock.

## Why intermediate level doesn't matter

oxipng's default `idat_recoding=true` (which we keep per Cycle 53
ablation) decodes the input IDAT to raw pixels then re-encodes with
libdeflate at its preset level. Our raw IDAT compression level only
affects (a) decode-time of our raw stream by oxipng (tiny) and
(b) intermediate raw_png byte volume in RAM (transient ~3 MB).

## Files touched

- `crates/nupic-quantize/src/lib.rs::encode_indexed_png_with_alpha`
  (set_compression(Fast))
- `Cargo.toml` workspace 1.1.0 → 1.1.1
