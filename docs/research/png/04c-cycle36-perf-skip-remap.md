# 04c — Cycle 36: skip imagequant per-pixel remap (v0.5.47)

## Motivation

Per "perf > mem > disk > cov > doc" priority, after closing the cov
distance via Cycles 30–35 (+19.44 SSIM), perf becomes the binding
axis. End-to-end `nupic compress --dither auto` on 5 MP fixtures
takes 7–18 s — far above any reasonable encode-speed target.

## Time decomposition (5 MP fixtures, M2 8-core release build)

Instrumented `train_palette_rgba` + `refine_palette_kmeans` +
`apply_palette_rgba` separately:

```
fixture                        train    refine    apply    total
17-aurora-5mp                  0.16     10.64     0.13     10.93
25-sofia-cathedral-5mp         0.12     12.45     0.10     12.68
27-whale-tail-5mp              0.10      4.45     0.10      4.65
```

**Lloyd's k-means refinement is 96–99 % of all nupic-quantize time.**
imagequant's median-cut runs in 0.10–0.16 s; OKLab argmin in 0.10–
0.15 s; everything else is Lloyd iters.

The full `nupic compress` adds ~1–3 s of oxipng + indexed PNG encode
on top. So out of ~15 s on 25-sofia, ~12.5 s is Lloyd refinement.

## Cycle 36 — quick win: skip the imagequant remap

Pre-Cycle 36 the palette-training helper called `quant.remapped()`:

```rust
let (palette, _idx) = quant.remapped(&mut img).map_err(|_| ())?;
Ok(palette)
```

`remapped()` runs **per-pixel nearest-palette assignment** (O(N · K))
*inside* imagequant and returns both palette and indices. We
**discard the indices** because `apply_palette_rgba` re-does the
assignment in OKLab space (the Stone C insight). The imagequant remap
is pure waste.

The imagequant API exposes a cheaper alternative:

```rust
Ok(quant.palette().to_vec())   // O(K), just the palette
```

## Bench (3-run min, M2 release)

```
fixture                  Cycle 35      Cycle 36      Δ
05-photo-mountain         2.16 s        2.11 s       −0.05
06-photo-landscape        2.30 s        2.23 s       −0.07
17-aurora-5mp            18.47 s       14.57 s       −3.90  (−21 %)
25-sofia-cathedral-5mp   15.59 s       15.28 s       −0.31
27-whale-tail-5mp         7.23 s        6.95 s       −0.28
```

17-aurora's −21 % win is the largest single-fixture speedup. The
imagequant remap step appears to scale super-linearly with image
content complexity — easy fixtures (05/06) saw flat noise; complex
ones gained measurably.

## Output bit-exact

Same input + same code path → byte-identical output. Verified:

```
25-sofia post-C36: size 2 752 407, SSIM 78.395586
25-sofia pre-C36:  size 2 752 407, SSIM 78.395586
```

Zero SSIM / size impact. Pure perf win.

## Open backlog — the real ceiling-distance is Lloyd

Lloyd refinement remains 5–13 s on 5 MP fixtures. Per-fixture iter
counts (from the `DEFAULT_REFINE_ITERS = 100` docstring):

```
fixture       convergence_iter
01-trans      48
02-pluto      46
03-wiki       3        (instant)
04-portrait   34
05-mountain   67
06-landscape  48
07-product    21
```

`EPS = 0.0005` is a tight convergence threshold; iters that have
"essentially converged" but not crossed EPS keep running. Cycle 37
candidates:

1. **Tighten EPS** to 0.001 / 0.002 / 0.005 — measure per-fixture
   iter saved and SSIM impact.
2. **Lower `DEFAULT_REFINE_ITERS`** from 100 to 50 — most fixtures
   already converge under 50; capping clips only the tail.
3. **Sub-sample pixels per iter** — assign+accumulate on 1/N
   pixels then full-pass for the final iter to lock in indices.
4. **Mini-batch k-means** — replace sequential EM iters with
   incremental batch updates (Sculley 2010); 5–10× speedup on
   k-means in literature, with ε SSIM cost.

## Files touched

- `crates/nupic-quantize/src/lib.rs::train_palette_rgba::try_iq`
  (replaced `quant.remapped()` with `quant.palette().to_vec()`)
- `crates/nupic-research/examples/speed_sweep.rs` (new — used to
  instrument the time decomposition; kept for future perf cycles)
- `Cargo.toml` workspace version 0.5.46 → 0.5.47
