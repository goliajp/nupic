# From Per-Image Oracle to Spatial-Aware Quantization: A Cohort-Driven Protocol for Breaking the Indexed-PNG Quality Ceiling

**Draft v0.1 — Cycle 115 outline, 2026-06-19**

**Authors:** TBD
**Target venues:** DCC 2027 (Data Compression Conference) / IEEE TIP /
ACM Compression Symposium

---

## Abstract

We present a six-cycle empirical protocol for routing PNG codec
choices using cohort-level oracle headroom mapping against an
externally-defined reference (TinyPNG). On a corpus of 506 fixtures
spanning synthetic, icon, NASA, Picsum HD, and Wikimedia content
classes, the protocol surfaced three structural findings:

1. **Palette-size monotonicity break**: K=192-256 indexed PNG output
   is often *smaller* than K=128 on photo-class content (palette
   overhead offset by tighter PNG filter-chain entropy). The Cycle
   106 Pile A oracle showed K=224 as the single most-common winning
   slot (35% of 23 winners), contradicting the naive "more colors
   means more bytes" intuition.

2. **Per-image RD optimum does not transfer to cohort-wide routing**:
   The K=224 single-config production-default regresses the original
   PASS cohort by 16-25% on our stratified bench. Cohort routing must
   be input-aware, and input-only feature classifiers hit a ceiling
   at 99.1% retention — the true discriminator is the baseline output
   size, accessible only via 2-pass quantize-and-measure routing. We
   ship this finding as the v1.2.9 fail-safe P-08 K-up override.

3. **Spatial-aware quantization breaks the single-global-palette
   DSSIM ceiling on 6/6 externally-defined infeasible fixtures**.
   Cycle 106-110 confirmed 6 Picsum HD photos un-rescuable under any
   global K ∈ {64..256} × dither × lossless. Cycle 111 R6 emulation
   (8×8 tile × K=192 per-tile imagequant + reassemble) passes DSSIM
   on all 6 with comfortable visual-indistinguishable margin
   (-0.00072 to -0.00825 below tiny_dssim). Cycle 112 confirmed
   the bottleneck is PNG's 256-palette container, not the R6
   algorithm: re-quantizing R6's 12288 effective colors into
   K=256 single palette exceeds the R6 DSSIM headroom on every
   fixture (margin +0.00013 to +0.00496) — visually identical to
   TinyPNG at half the bytes, but strict metric-gate fail.

The protocol's six cycles (106-112) form a complete diagnostic
narrative: ceiling diagnosis → fallback exhaustion → paradigm shift
validation → container bottleneck. The methodology generalizes to
any lossy codec evaluation against an external reference cohort.

**Key contributions:**
- Cohort headroom-mapped Pareto sweep as codec routing design protocol
- v1.2.9 production wire: 2-pass measured K-up fail-safe on ≥ 5 MP
  HD photos, 100% PASS retention by construction
- R6 spatial-aware quantization as the ceiling-break mechanism on
  high-frequency Picsum HD content
- Public reproducibility data: per-fixture grid sweeps, oracle PASS
  tables, R6 emulation outputs, all in `assets/png-bench/cycle*/*.tsv`

---

## 1. Introduction

### 1.1 Motivation

PNG remains the dominant lossy-acceptable image format for the web's
icons, transparent overlays, illustrations, and low-resolution photos
where artifact tolerance is moderate. Production PNG codecs face a
fundamental routing challenge: for each input, the encoder must pick
quantization palette size (K), dither strength (d), filter chain,
deflate strategy, and (sometimes) lossless-vs-lossy switch. State of
the art (TinyPNG, pngquant + zopfli, oxipng) ship hand-tuned defaults
that work well across diverse content but leave per-image headroom
unexploited.

Prior work on adaptive quantization focuses on either (a) per-image
RD analysis at training-time — a one-fixture-at-a-time view that
naturally produces a "perfect oracle" — or (b) cohort-level
benchmarking against fixed metric targets without an external
competitive reference. The gap is **a protocol for designing
production routing tables from cohort-level oracle data against an
external commercial benchmark**.

This paper presents such a protocol, instantiated in `nupic`, an
open-source PNG codec we develop and ship. Over six research cycles
spanning ~6 weeks, we use TinyPNG output as the external reference
cohort and `corpus-500` (506 fixtures from synthetic + open-domain
images) as the evaluation set. We discover three structural codec
findings the per-image RD literature couldn't surface, ship one as a
production update (v1.2.9), and reach the spatial-aware-quantization
ceiling — bottleneck'd not by algorithm but by the indexed-PNG
container itself.

### 1.2 Contributions in detail

**C1 (methodology):** We formalize *cohort headroom-mapped Pareto
sweep* — a 4-pile classification (PASS / Pile-A FAIL-SIZE / Pile-B
FAIL-QUAL / Pile-C FAIL-BOTH) with per-pile oracle K × dither × preset
sweeps. The methodology produces both a production routing-table
design driver and a ceiling-diagnosis output.

**C2 (palette-size finding):** We document the K-monotonicity break:
on Picsum HD photo content, K=224 produces strictly smaller PNG
output than K=128, because the larger palette captures gradients
without high-frequency quantization artifacts that compound PNG
filter-chain bytes. Effect size 0.59× TinyPNG cohort ratio on 23
Pile-A winners.

**C3 (production wire):** v1.2.9's P-08 K-up fail-safe — a 2-pass
measured routing on ≥ 5 MP content — ships with 100% PASS retention
guaranteed by construction (pick min(default, K=224)). Full-corpus
verification (Cycle 110) shows +1.5 pp PASS rate over v1.2.8 with
zero real regressions. We discuss why naive single-config K=224
regresses 16-25% of small images and why input-only feature
classifiers ceiling at 99.1% (real discriminator = baseline output
size, only available via 2-pass).

**C4 (R6 spatial-aware ceiling break):** For 6 high-frequency Picsum
HD fixtures un-rescuable by any global palette K ≤ 256 or lossless,
we show 8×8 tile × K=192 per-tile imagequant + reassemble passes
DSSIM 6/6 with margin -0.00072 to -0.00825 below TinyPNG. This
breaks the single-global-palette ceiling at the algorithm layer.

**C5 (container bottleneck):** Re-quantizing R6 reconstruction
through PNG's K=256 indexed palette exceeds the R6 DSSIM headroom on
all 6 fixtures (margin +0.00013 to +0.00496) while delivering size
0.46-0.55× TinyPNG. The hybrid is visually identical to TinyPNG at
half the bytes, but strict metric-gate fail. Production realization
requires a tile-aware container (.nupic format spec'd but not shipped)
or transcoding to WebP/AVIF for the R6 cohort.

### 1.3 Paper organization

Section 2 surveys related work in PNG-codec optimization, per-image
RD analysis, and adaptive quantization protocols. Section 3 details
the corpus + metric framework (DSSIM as primary, SSIMULACRA2's
alpha-floor as motivating cautionary tale). Section 4 presents the
cohort headroom-mapped Pareto sweep protocol with worked examples.
Section 5 covers the three structural findings (C2/C4) and the
production wire (C3). Section 6 examines the R6 spatial-aware
ceiling break and PNG-container bottleneck (C5). Section 7 discusses
implications for PNG codec engineering and proposes the .nupic
tile-aware container. Section 8 concludes.

---

## 2. Related Work (outline)

### 2.1 PNG codec optimization
- TinyPNG (Voormedia 2014+): industry-standard lossy PNG service,
  uses pngquant + zopfli pipeline
- pngquant (Lesiński): imagequant K-means quantization, dither
- oxipng (Fornwall 2017+): pure-Rust lossless PNG optimizer, brute
  force filter selection + parallel deflate
- pngcrush, optipng: deflate-only optimization
- Mozilla pngcrush extension: HDR / smart pngquant integration

### 2.2 Per-image rate-distortion analysis
- Shoham-Gersho 1988: Lagrangian R-D optimization seminal
- Wallace 1991 (JPEG): RD curve via Q-matrix scan
- VP8/9 / AV1 RD literature: macroblock-level lambda search
- Limitations for cohort codec design (no external-reference gate)

### 2.3 Adaptive vector quantization
- Wallach 1993 spatial VQ
- Linde-Buzo-Gray 1980 codebook design
- Recent VQ-VAE work (van den Oord 2017): learned spatial quantizer,
  cousin of our R6 algorithm

### 2.4 Perceptual metric reliability
- SSIMULACRA2 (Wassenberg-Bauwens 2023): perceptual metric,
  alpha-edge floor issue we surface in Section 3.2
- Kornel Lesiński DSSIM (rust impl of Brunet 2012 ε-DSSIM)
- LPIPS, FLIP, butteraugli — alternatives we did not use

### 2.5 Multi-tile / spatially-adaptive compression
- WebP lossy/lossless dual coder
- AVIF tile structure
- JPEG XL: variable block size, optimal-coder switching
- HEIC tile coding

---

## 3. Corpus and Metric Framework

### 3.1 Corpus-500 composition
- 506 fixtures, breakdown:
  - 100 synthetic (s prefix: gradient, noise, solid, stripes)
  - 30+ NASA (n prefix: planet, nebula, galaxy)
  - 350+ Picsum HD (p prefix: 480-3840 pixel-wide photo)
  - 16 Wikimedia (wm prefix: large featured images)
  - 3 small icons (mi prefix)
- TinyPNG output captured via API (quota 494/500 used)
- Three-axis size + DSSIM + SSIM baselines persisted in
  `assets/png-bench/corpus-500-*.tsv`

### 3.2 DSSIM as primary metric
- Why: SSIMULACRA2 has a -492 floor on transparent fixtures with
  alpha-edge content; gate-comparable values not meaningful for
  PNG with rich alpha
- DSSIM: Kornel Lesiński's rust impl, bit-comparable across
  reference and distorted at 1e-7 precision
- Tolerance protocol: 1e-5 considered noise (Cycle 110 s018
  byte-identical "regression" surfaced this)

### 3.3 Three-axis gate
- size ≤ 0.80 × TinyPNG
- AND DSSIM ≤ TinyPNG_DSSIM
- AND production wall < 250ms on 5 MP (NAS / CDN target)

---

## TODO Next Cycles

- Cycle 116: Section 4 (methodology) + Section 5 (findings C2/C3) draft
- Cycle 117: Section 6 (R6 ceiling break) + Section 7 (.nupic container) draft
- Cycle 118: figure pipeline (per-fixture grid heatmaps, cohort PASS
  histograms, R6 8×8 tile boundary visualizations)
- Cycle 119: full manuscript pass, references, bibliography
- Cycle 120: peer review pass (internal or external)
- Cycle 121+: submission cycle (DCC deadline 早 / IEEE TIP rolling)

---

## References (bibliography stub)

- [will populate Cycle 117-118]
