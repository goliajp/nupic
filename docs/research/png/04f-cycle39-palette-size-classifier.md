# 04f — Cycle 39: adaptive palette size for -16% size vs TinyPNG (v0.5.50)

## Motivation

Cycle 38 flattened the dither classifier (`d=0`) and reached
**-8.27 %** vs TinyPNG on the 7-fixture marketing baseline.
User raised the size gate to **-15 %** with the SSIM constraint
"strictly ≥ TinyPNG per fixture".

## Lever — adaptive `n_colors` (palette size)

Sweep on the 4 photo-class fixtures revealed substantial size at
sub-256 palette sizes, but the SSIM-vs-gate cutoff is content-
specific:

```
            ── n=256 ──  ── n=208 ──  ── n=192 ──
            size  SSIM   size  SSIM   size  SSIM   gate
04 portrait 478   88.11  451   86.07  440   84.78  85.86  → 208 (buffer +0.21)
05 mountain 380   70.37  ?     ?      341   65.33  59.40  → 192 (buffer +5.92)
06 landscape 1013 83.07  974   79.93  969   77.76  79.76  → 208 (buffer +0.17)
07 product  340   85.16  ?     ?      321   83.63  80.32  → 208 (also passes 192)
```

(`?` means the n=208 row wasn't directly measured in the sweep but
sits between the bracketing rows; both 05/07 chose lowest n that
passes their gate.)

The transparency tier (01, 02) is even more forgiving — both stay
above their (negative) TinyPNG gates at n=64:

```
01 trans-demo  n=256 / -46.2  →  n=64 / -63.7   gate -492.6 → buffer +428.9
02 pluto-trans n=256 /  79.5  →  n=64 /  73.1   gate  -60.0 → buffer +133.1
```

## Rule (3-branch signal-based classifier)

```rust
pub fn classify_for_palette_size(src_rgba: &[u8]) -> usize {
    let opq = opaque_ratio(src_rgba);
    if opq < 0.95 {
        return 64;        // tier-1/1c/2c sharp-mask transparency
    }
    let uniq = unique_opaque_colors(src_rgba, cap: 100_000);
    if uniq > 100_000 {
        return 192;       // high-uniq stochastic photo (palette noise hidden)
    }
    208                   // smooth photo (gate-critical gradient quality)
}
```

Two signals only — `opq` (cheap O(N) opaque-pixel count, already
computed by the dither classifier's legacy form) and `uniq`
(O(N) HashSet insert with early-exit at 100 001).

## Bench — gate ✓ hit at -16.28 %

```
fixture                  TinyPNG          nupic Cycle 39   Δsize    Δssim
01 trans-demo            47 KB / -492.6   27 KB / -63.7    0.581x   +428.9
02 pluto-trans           176 / -60.0      84 /  73.1       0.481x   +133.1
03 wiki-logo             13 / -63.7       9 /  77.7        0.751x   +141.4
04 portrait              556 / 85.9       450 / 86.1       0.810x   +0.21
05 mountain              424 / 59.4       340 / 65.3       0.804x   +5.92
06 landscape             1066 / 79.8      973 / 79.9       0.913x   +0.16
07 product               358 / 80.3       324 / 84.1       0.905x   +3.75
─────────────────────────────────────────
TOTAL                    2643 KB          2213 KB         0.837x    all + ≥0
                                                          -16.28 %
```

Hits the user-set **-15 % gate** with a **+1.28 pp buffer**.
SSIMULACRA2 ≥ TinyPNG on every fixture, with min buffer +0.16 (06)
and median buffer +5.9.

## Files touched

- `crates/nupic-quantize/src/lib.rs`
  - new `classify_for_palette_size` (signal-based, 3 buckets)
- `crates/nupic-core/src/ops/compress.rs`
  - `encode_png_stone_c` now reads palette size from the classifier
    instead of hard-coding 256
- `Cargo.toml` workspace version 0.5.49 → 0.5.50

## Open backlog (path to -20 % gate)

Current -16.28 % buys 4 pp of headroom toward the -20 % goal.
Remaining candidates:

1. **oxipng effort=10 (zopfli) on photo path** — Cycle 21 measured
   -0.3 % on un-dithered streams; with our cleaner d=0 IDAT, the
   zopfli win may be larger. Slow but global.
2. **n_colors=192 for "moderate uniq" branch** — current rule
   routes 06 (uniq=52 K) to n=208 (buffer +0.17); a 4-bucket split
   (e.g. uniq > 50 K → 208 floor, uniq > 100 K → 192, uniq > 200 K
   → 176) might squeeze 06 lower without breaking gate. N=1
   evidence, risky.
3. **Stone D refine_iters per-fixture** — bench (Cycle 39 prep)
   showed 04 needs iter ≥ 20 to clear gate (iter=0 → SSIM 81.79
   below 85.86), but 05/06/07 are flat after iter ≥ 20. Adaptive
   refine cap could save 0.5-1 s per fixture without size cost
   (perf win); not a size lever.
4. **Algorithmic — SSIMULACRA2-aware quantization** — current Lloyd
   minimises OKLab+α L2. SSIMULACRA2's perceptual loss is non-L2
   (luma×chroma channel weighting + multi-scale). A loss-aware
   k-means might pack the same SSIM into fewer palette entries.
   Research cycle.
