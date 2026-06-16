# 03r — Cycle 21: `--effort 7-10` unlocks oxipng Zopfli deflater (v0.5.35)

## Motivation

After Cycles 14-15 squeezed Lloyd's perf, Cycle 18 confirmed Path B
gap is deflate-bound (not filter-selection), and Cycles 16/19
exhausted easy quality/perf wins, the remaining size ceiling on
Path A (default `oxipng` deflate) is the deflater quality itself.
`oxipng` 9.x ships with two deflater backends:

- **Libdeflater** (default, preset 5): fast, well-balanced
- **Zopfli**: 100×-class slower, ~0.3-1% smaller output, used as the
  reference "lossless asymptote" by image compression community

Mission "又小又好" benefits from any zero-quality-regression size
reduction. Question: does Zopfli give a meaningful net win when
exposed as a high-effort opt-in?

## Probe (cycle21_zopfli_probe)

Full 7-fixture corpus, `--dither auto`, oxipng preset 5 with
Libdeflater(compression=12) vs Zopfli(iterations=15):

| fixture | libdeflater | zopfli | Δ size | Δ SSIM | lib_ms | zop_ms |
|---|---|---|---|---|---|---|
| 01 trans-demo | 45364 | 44856 | **-508** | 0 | 886 | 2770 |
| 02 pluto | 163674 | 162787 | **-887** | 0 | 744 | 1950 |
| 03 wiki logo | 14718 | 14695 | **-23** | 0 | 87 | 260 |
| 04 portrait | 499378 | 497965 | **-1413** | 0 | 1162 | 2784 |
| 05 mountain | 473174 | 470678 | **-2496** | 0 | 2514 | 5968 |
| 06 landscape | 1109644 | 1108643 | **-1001** | 0 | 2484 | 4395 |
| 07 product | 404312 | 401926 | **-2386** | 0 | 1013 | 4450 |
| **total** | 2710264 | 2701550 | **-8714 (-0.32%)** | **0** | 8890 | 22577 |

**Zero SSIM regression on every fixture** (zopfli is lossless deflate
variation — same decoded output, smaller compressed stream).

Wall time: ~2.7× slower median.

## Wiring

Currently `--effort 5-6` map to oxipng preset 5/6, effort > 6 was
capped (no-op). After Cycle 21:

```rust
if opts.oxipng_preset >= 7 {
    let iters = ((opts.oxipng_preset - 6) as u8 * 5).min(30).max(1);
    oxipng_opts.deflate = oxipng::Deflaters::Zopfli {
        iterations: NonZeroU8::new(iters).unwrap(),
    };
}
```

Effort gradient:

| --effort | deflater | iterations | wall time vs e5 |
|---|---|---|---|
| 0-6 | Libdeflater | n/a | baseline |
| **7** | Zopfli |  5 | ~2× |
| **8** | Zopfli | 10 | ~2.5× |
| **9** | Zopfli | 15 | ~2.7× |
| **10** | Zopfli | 20 | ~3× |

Backward compatible: default `--effort 5` unchanged.

## Full-corpus ship validation (effort 5 vs 10, `--dither off` default)

```
fixture                    e5 size    e10 size    Δ
01-png-transparency-demo    45364      44855    -509
02-pluto-transparent       158109     157178    -931
03-wikipedia-logo           14718      14688     -30
04-photo-portrait          484513     482850   -1663
05-photo-mountain          389264     387598   -1666
06-photo-landscape        1035965    1035079    -886
07-photo-product           340640     338734   -1906
TOTAL                     2468573    2460982   -7591 (-0.31%)
```

SSIM bit-exact identical on all 7 fixtures (zopfli is lossless).

## vs TinyPNG corpus position

- pre-Cycle-21 (effort 5): nupic 2468 KB vs Tiny 2706 KB = **-8.8%**
- post-Cycle-21 (effort 10): nupic 2461 KB vs Tiny 2706 KB = **-9.1%**

Marginal but mission-aligned: every byte counts, no quality cost.

## Workspace

219 tests pass. Cargo.toml workspace deps: `oxipng` gains `zopfli`
feature.

## Files

- `Cargo.toml` — oxipng feature += zopfli
- `crates/nupic-quantize/src/lib.rs` — effort ≥ 7 → Zopfli iters
- `crates/nupic-core/src/ops/compress.rs` — pass full effort 0-10 through
- `crates/nupic-research/examples/cycle21_zopfli_probe.rs` — probe bench
