# 04ii — Cycle 78: Lloyd parameter sweep on 5MP+ ceiling (negative, no ship)

## TL;DR

Cycle 77 closed most of the corpus tail but residuals remain at
SSIM 56-61 (p244 picsum 4K, p235/p183 2.4 MP, NASA-class).
Hypothesis: tighter Lloyd parameters (iter cap, stride) recover
some of the gap. Sweep showed BOTH already near-optimal — the
algorithmic ceiling is the L2-in-OKLab+α + n=256 PNG palette
combination, not a tunable parameter.

Negative finding. No code change.

## Iter cap sweep (5MP+ outliers)

```
p244_3840x2560 (9.8MP, n=256):
  cap=10  size=1548 KB  ssim=59.08
  cap=20  size=1535 KB  ssim=60.86  ← current default
  cap=30  size=1534 KB  ssim=61.00  (+0.14)
  cap=50  size=1532 KB  ssim=60.99  (flat)
  cap=100 size=1532 KB  ssim=60.99

p235_2400x1600 (3.8MP):
  cap=20  → 49.26
  cap=50  → 49.62  (+0.36)

p183_2400x1600 (3.8MP):
  cap=20  → 57.05
  cap=50  → 57.56  (+0.51)
```

Lloyd convergence plateaus by cap=30-50. The Cycle 55 cap=20 for
5MP+ is leaving ≤0.5 SSIM on the table per outlier — marginal even
if recovered, and not the dominant SSIM loss.

## Stride sweep (sample density)

```
p244 (5MP+):
  stride= 4  ssim=60.99  t=2.57s
  stride= 8  ssim=60.90  t=2.02s
  stride=16  ssim=60.86  t=1.74s   ← current default for 5MP+
  stride=32  ssim=59.39  t=1.63s

p235/p183 (under 5MP, stride=8 default):
  stride=4  ~equal to stride=8
  stride=8  current
  stride=16 marginally worse
```

Going from stride=16 → stride=4 on p244 gains +0.13 SSIM at +0.83s
wall time. The +47 % time cost for +0.13 SSIM is a clear bad trade
under the perf KPI (5MP < 250ms target).

Stride is also at its optimum.

## Where the loss is

For p244 SSIM 61 at n=256: the source has ~143K unique colors. The
palette can hold 256. Per-pixel L2-OKLab+α assigns each source
pixel to its nearest centroid. The reconstruction error per pixel
is fundamentally bounded by `min over palette entries d(pixel, p)`.

For high-uniq photos this floor is significant: the average pixel
is ~0.5 OKLab units from its nearest palette entry. SSIMULACRA2
penalises these displacements multiplicatively across spatial
scales.

No amount of Lloyd iteration tightens this floor below what 256
optimally-placed centroids can cover. The Cycle 71 joint anneal
helps marginally by re-balancing palette around perceptually
important regions, but the n=256 PNG limit is the wall.

## What WOULD push further

Per Cycle 76 essay, three directions:

1. **Vector quantisation / VQ-VAE** — replace L2-OKLab+α with
   a learned distance that better aligns with SSIMULACRA2's
   spatial bandpass response. Research scope.

2. **Multi-pass tile encoding** — independent palettes per tile,
   stitched via index map. Breaks single-image PNG; would need
   a wrapping container.

3. **Lossless RGBA route gated by per-image lossless/quantize
   size estimate** — currently `is_gradient_candidate` uses
   adj_mn<1.0 + uniq>1K. Could extend to "estimated lossless KB
   < 1.4 × estimated quantize KB → prefer lossless". Requires
   two-pass encoding or a regression model. Not free.

## Files touched

- Research-only `examples/c78_iter_sweep.rs` + `c78_stride.rs`
  (not committed)
- `docs/research/png/04ii-cycle78-lloyd-ceiling.md` (this essay)
- No Cargo bump (no behaviour change)

## Paper material

This negative finding is high-value for P3:

- We ruled out two obvious "just iterate more / sample more"
  improvements via direct empirical sweep on the corpus tail
- The remaining gap is established as ALGORITHMIC (Lloyd-on-L2-
  OKLab + n=256 ceiling), motivating §4 (joint MRF) and §5
  (importance Lloyd) contributions
- VQ-VAE positioned as the principled extension, not an
  arbitrary alternative

Without Cycle 78's negative ruling, reviewers would ask
"have you tried tighter Lloyd?" Now we cite §X.Y.
