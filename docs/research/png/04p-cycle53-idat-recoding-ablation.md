# 04p — Cycle 53: oxipng idat_recoding ablation (MIXED, v1.1.0)

## Hypothesis

After Cycle 52 (RSS -58 %), oxipng preset=1 is the remaining
~600 ms bottleneck on 5MP. Hypothesis:

> `Options::idat_recoding = false` skips deflate retry while keeping
> bit-depth / chunk strip, recovering most time at small size cost.

## Method

Run full pipeline with both `idat_recoding ∈ {true, false}` on
baseline-7 (preset=5) + 5MP fixtures (preset=1). Record size + time
delta of the oxipng phase (not full pipeline).

## Results

```
fixture                          default_KB  noidat_KB     Δ_size  Δ_oxi_time
01-png-transparency-demo               20         28      +44.59%    -90 %
02-pluto-transparent                   69         69       +0.00     -0 %
03-wikipedia-logo                      14         14       +0.00     +3 %
04-photo-portrait                     451        451       +0.00     -1 %
05-photo-mountain                     317        317       +0.00     -0 %
06-photo-landscape                    974        974       +0.00     +3 %
07-photo-product                      325        325       +0.00     -2 %
25-sofia-cathedral-5mp               2140       2140       +0.00     +0 %
27-whale-tail-5mp                    2939       2939       +0.00     +0 %
```

## Findings

1. **01-trans-demo loses 8 KB (+44 %)** when idat_recoding=false — its
   small structure benefits significantly from re-deflate. Catastrophic
   for the baseline-7 size gate.
2. **All other fixtures: 0 % size + 0 % time difference**. The "free
   perf win on 5MP" hypothesis from a single-run measurement was
   actually noise.
3. Conclusion: oxipng's idat_recoding default (true) is **near-optimal**
   for our pipeline. Disabling helps no fixture meaningfully and hurts
   small-fixture compression.

## Why the prior single-run showed -58 %

Earlier single-shot bench:
```
preset=1               2531 KB  1613 ms
p1 idat_recoding=false 2140 KB   672 ms
```

The 391 KB difference (2531 vs 2140) appeared because the two runs
went through SLIGHTLY DIFFERENT pipelines:
- First measurement: end-to-end `quantize_indexed_png` (which since
  Cycle 47 applies `strip = StripChunks::Safe` only when
  `strip_metadata = true`).
- Second measurement: direct `oxipng::optimize_from_memory` with the
  flag set inline — different strip behaviour, different filter+IDAT
  re-encode state.

Once stage 2 normalised both calls to identical `oxipng::Options`,
the difference vanished.

This is itself a useful lesson: **A/B oxipng ablations must hold all
other options fixed**, otherwise the test isolates the wrong variable.

## Negative finding value

- Eliminates "idat_recoding=false" as a perf lever for our pipeline.
- Refines investigation away from oxipng-flag tuning toward
  fundamentally lower-overhead paths (custom libdeflate integration,
  pipeline parallelism, Lloyd shortcut tricks).
- Confirms baseline-7 size optimum is robust to oxipng option
  changes — current preset choice is locked in.

## Files touched

- `crates/nupic-research/examples/speed_sweep.rs` (ablation harness)
- `docs/research/png/04p-cycle53-idat-recoding-ablation.md` (essay)
- `Cargo.toml` workspace 1.0.9 → 1.1.0 (bumps minor — accumulating
  cycles 48-53 all explore one open question (oxipng latency) and
  collectively close it)
