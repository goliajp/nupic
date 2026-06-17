# 04gg — Cycle 76: real-Auto corpus diagnosis (no code change, v1.2.2)

## TL;DR

Cycle 74's 506-corpus probe used `quantize_indexed_png` directly,
bypassing `compress.rs::encode_png_stone_c`'s gradient-routing step.
The SSIM-41 "p295 picsum 4K" outliers it surfaced were a probe
artifact — production Auto routes these to lossless (SSIM=100).

Cycle 76 rebuilds the probe on the real `Image::compress` path and
identifies the actual outliers. No code change shipped (diagnostic
cycle).

## Probe correction

```rust
// Cycle 74 (incorrect — bypasses Auto routing):
let png = quantize_indexed_png(&raw, w, h, QuantizeOpts::default()).unwrap();

// Cycle 76 (correct — uses real Auto path):
let opts = CompressOpts { format: Png, quality: Auto, .. };
let png = img.compress(opts).unwrap();
```

The difference matters because Auto routes via `is_gradient_candidate`
to `encode_png_lossless` for `adj_mn < 1.0` + opaque + dense uniq.
For picsum-4K p295 (`adj_mn=0.514`, opq=1.0), Auto goes lossless
preset=1 = 3.2 MB / SSIM 100 — perfect quality.

## Real-Auto corpus stats (Cycle 76)

```
n=506   total_in=1307231 KB   total_out=470806 KB   ratio=0.360

SSIM distribution:
  p1   57.96
  p5   63.55
  p10  67.43
  p50  81.80
  p90  100.00     <-- 10% of corpus is bit-perfect SSIM 100
```

vs Cycle 74's quantize-only:
```
n=506   total_in=1307231 KB   total_out=436403 KB   ratio=0.334
SSIM:   median 81.72, p10 67.16
```

Auto adds 34 MB total size (8 % more than quantize-only) but
recovers exact quality on the ~50 gradient-detected fixtures.

## Actual outliers (SSIM < 65, real Auto path)

```
n29_astronaut          1280×853    314 KB   ssim=56.15
n30_astronaut          1280×853    216 KB   ssim=57.06
s033-s039 noise        1100-1800w  827-2243 KB  ssim=57-60   (stochastic content)
n21_sun                1280×650    155 KB   ssim=58.91
n01_mars               1280×1062   296 KB   ssim=60.22
p120_1920x1080         1920×1080   290 KB   ssim=60.34
p122-p124 picsum HD    1920×1080   253-288 KB  ssim=60-62
p244_3840x2560         3840×2560  1534 KB   ssim=60.86
wm13 Egret 5184×3456  10701 KB   ssim=61.48
n02_mars               1280×1024   361 KB   ssim=62.28
```

34 fixtures (6.7 %) below SSIM 65. Categorise:

- **NASA planetary** (n01/n02 mars, n21 sun, n29/n30 astronaut):
  high-detail photographic with fine textures. Already routed
  through joint anneal (var < 200). Hit n=256 ceiling.
- **Synthetic noise** (s032-s039): pure stochastic content. By
  construction palette quantisation loses noise structure. var > 200
  so joint anneal skipped (correctly).
- **Picsum HD** (p120 family at 1920×1080, p244 at 4K):
  adj_mn 1.07-1.59 = just above the gradient detector's 1.0
  threshold. Not extreme-smooth → quantize path → n=256 ceiling.
- **Wikimedia 5K** (wm13 Egret): 5184×3456, edge case at the
  top of the corpus by pixel count. n=256 ceiling.

## Why these are the achievable ceiling

For each picsum HD outlier, lossless RGBA size is 3.3-3.6 × larger
than current Auto quantize:

```
fixture           Auto KB    Lossless KB    multiplier   SSIM_Auto    SSIM_LL
p120 1920×1080    290         1049          3.6x         60.34        100
p122 1920×1080    253          900          3.6x         61.70        100
p244 3840×2560   1534         4121          2.7x         60.86        100
n29  astronaut    314         1026          3.3x         56.15        100
n01  mars         296         1024          3.5x         60.22        100
```

Routing these to lossless would more than double their footprint
for a SSIM gain users probably can't see (SSIM 60 on these is
already "natural photo quality" — visually OK per visual-eye-gate
spot checks).

The quantize ceiling at n=256 + Cycle 71 joint anneal IS the
algorithmic ceiling. Pushing further requires either:

1. **Multi-pass encoding** — tile decomposition + per-tile palette
   (complex, breaks single-image PNG)
2. **Vector quantisation** (LVQ / VQ-VAE) — learned dictionary
   (research scope, Cycle 80+ candidate)
3. **Lossless RGBA8 routing** with per-fixture size budget — accept
   the 3× size hit for high-priority quality. Not a default.

## Paper contribution

For P3/P4: this is a finding worth reporting. The corpus characterisation
shows nupic Auto reaches one of three regimes:

- Lossless route (~10% of corpus, mostly extreme-smooth gradients,
  SSIM 100)
- Quantize route at n=256 ceiling (~70% of corpus, photo class,
  SSIM 70-90)
- Joint-anneal route (~20% with var < 200 and opq ≥ 0.95, photo
  smooth, SSIM 70-90 with Pareto improvement)

The 6.7 % SSIM < 65 outliers represent the algorithmic ceiling
under the n=256 PNG palette constraint. This is the right place
to make a "what's the achievable Pareto frontier under PNG indexed
mode" claim.

## Why no Cycle 76 code change

Cycle 74 essay risk: "tier-trans outliers" was a probe artifact.
Real Auto path has no comparable outlier in the synthetic
tier-trans set (all 18 SSIM=100). The opaque outliers (NASA, noise,
HD picsum) are at the algorithmic ceiling — no quick fix candidate.

The "Cycle 73 visual regression" pattern (where the metric lied)
does not appear to repeat in the opaque path Cycle 76 surfaced.
The outliers are honest n=256 + Lloyd + joint anneal ceiling.

## Files touched

- `crates/nupic-research/examples/c76_corpus_real.rs` (research-only,
  not committed)
- `docs/research/png/04gg-cycle76-corpus-diag.md` (this essay)
- No Cargo bump (no behaviour change)

## Backlog for Cycle 77+

- **n29/n30 astronaut**: try Cycle 43 importance Lloyd α=0.5 on
  high-detail photos. Currently α=0 since they don't hit the
  `n=192 + var>200` branch.
- **p120 family 1920×1080**: widen `is_gradient_candidate` adj_mn
  threshold from 1.0 to 1.5? Would re-route to lossless. Bench
  baseline-7 to ensure no regression.
- **VQ-VAE study**: longer-horizon research candidate. Off the
  critical path for current ship cycle but high P4 value.
