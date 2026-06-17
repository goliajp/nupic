# 04w — Cycle 61-62: 506-corpus bench + Auto-gradient lossless latency fix (v1.1.8)

## Cycle 61 — full corpus latency profile

After Cycle 51-60 ships, the corpus bench (last refreshed in Cycle
50, before SIMD/RSS work landed) needed a refresh. Cycle 61 runs all
506 fixtures through v1.1.7 and records latency / size / SSIM.

### Latency distribution (paper Table 2 candidate)

```
megapixel band     n     p50      p90      p99      max
< 1MP             269    184 ms   590 ms   938 ms   1192 ms
1-3 MP             98   1122 ms  1878 ms  2514 ms   2514 ms
3-10 MP           126   2033 ms  9996 ms 12016 ms  13916 ms  ← p99 outliers
> 10 MP            13   3329 ms  4401 ms  5128 ms   5128 ms
─────────────────────────────────────────────────────
overall                  603 ms  2514 ms 10454 ms  13916 ms
```

### Size + SSIM

```
total       1276.6 MB → 449.9 MB (ratio 0.352)
SSIM        min=54.1   p10=66.1   median=82.0   mean=83.2   p90=100.0
```

### Bench finding — slowest fixtures are 3840×2560 (9.8 MP) gradient

The 5 slowest fixtures (12-14 s each) are all 3840×2560 picsum
photos that the `is_gradient_candidate` detector routes to the
lossless PNG path. All have SSIM = 100 — Auto correctly picks
lossless on smooth-gradient content.

But the lossless path on a 9.8 MP image runs oxipng preset=5, which
takes 4-12 s on M2. **The Cycle 47 adaptive-preset (5 MP → preset=1)
was added only to the palette path; the lossless path was missed.**

## Cycle 62 — fix

Where: `encode_png_stone_c` in `nupic-core/src/ops/compress.rs`.

When `is_gradient_candidate` routes Quality::Auto to lossless on
≥ 5 MP images, downgrade `opts.effort` 5 → 1 for the oxipng call.
Crucially, **explicit `Quality::Lossless` is untouched** — those
users requested smallest-size encoding, not lowest latency.

```rust
if nupic_quantize::is_gradient_candidate(&raw, w) {
    let n_pixels = (w as usize) * (h as usize);
    if n_pixels >= 5_000_000 && opts.effort == 5 {
        let mut auto_opts = opts.clone();
        auto_opts.effort = 1;
        return encode_png_lossless(img, &auto_opts);
    }
    return encode_png_lossless(img, opts);
}
```

## Bench (p292 9.8 MP, was slowest at 13.9 s)

```
                    Cycle 61         Cycle 62        Δ
Auto (gradient)     13.92 s/1819 KB   4.05 s/2099 KB  -71 % time, +15 % size
Explicit Lossless   ~13.67 s/1819 KB  same            unchanged (preserve size-priority)
Baseline-7          -17.93 % vs TPNG  -17.94 % ✓      unchanged
```

## Trade-off analysis

For Auto-gradient on big images:
- −71 % latency: 14 s → 4 s, deployment-relevant
- +15 % size: 1.8 MB → 2.1 MB, modest absolute cost

The +15 % is a real loss for "size purist" workloads. Mitigation:
users who need smallest-size on large content can pass
`--lossless --effort 5` or equivalent. The Auto path now prioritises
latency on the lossless-routed branch, matching the CDN/NAS dogfood
target.

## Paper P2 + P4 material

Adds Table 2 (latency distribution by megapixel band) to the
evidence pool. The p99 outlier identification + targeted fix
demonstrates the project's empirical engineering practice: real
deployment latency dominated by edge cases that wouldn't surface
on baseline-7 benchmarks alone.

## Open backlog (next cycles)

1. 1-3 MP band still p50 = 1122 ms. Need to attack ~50 % of latency.
   Lloyd refine + apply still ~600 ms here even after Cycle 47-55.
2. SSIM < 60 outliers (worst 54.1): four are noise/aurora content
   where palette quantize fundamentally can't do well. One Wikimedia
   fixture (4543×3176) gates at n=208 with SSIM 54 — may need a
   "high-detail huge" tier with n=256.
3. Variance in 3-10 MP latency (p50 2 s, p90 10 s): the 5-10 s mid-
   band suggests other adaptive thresholds not catching all cases.

## Files touched

- `assets/png-bench/corpus-500-results-v117.tsv` (Cycle 61 raw data)
- `crates/nupic-core/src/ops/compress.rs::encode_png_stone_c`
  (Auto-gradient downgrade to preset=1 for ≥ 5 MP)
- `docs/research/png/04w-cycle61-62-corpus-bench-and-lossless-fix.md`
- `Cargo.toml` workspace 1.1.7 → 1.1.8
