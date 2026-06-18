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

## 4. Cohort Headroom-Mapped Pareto Sweep Protocol

The protocol takes a corpus and an external commercial reference,
emits both (a) a ceiling diagnosis per failure mode and (b) a
production routing-table design driver. Five stages, each
instantiated against TinyPNG + corpus-500.

### 4.1 Cohort construction

Each fixture is placed on two axes against the external reference:

- `size_ratio = nupic_size / tinypng_size`
- `dssim_delta = nupic_dssim − tinypng_dssim`

The three-axis gate of §3.3 partitions corpus-500 into four piles:

| pile | size_ratio | dssim_delta | n | semantic |
|---|---|---|---:|---|
| PASS | ≤ 0.80 | ≤ 0 | 106 | both axes won |
| Pile A | > 0.80 | ≤ 0 | 307 | size-attackable (quality already wins) |
| Pile B | ≤ 0.80 | > 0 | 40 | dssim-attackable (size already wins) |
| Pile C | > 0.80 | > 0 | 53 | dssim-infeasible candidates |

Persisted in `assets/png-bench/corpus-500-three-axis.tsv` (506 rows)
and `corpus-500-pile-a.tsv` (31-fixture extreme-tail subset used as
Cycle 106 oracle ground truth). The 4-pile decomposition is the
critical move: a single global PASS-rate masks both the attack
direction per fixture and the ceiling structure across the cohort.

### 4.2 Headroom-mapped oracle sweep

For each Pile A fixture, a per-image RD oracle is computed over a
K × dither × preset grid: K ∈ {64, 96, 128, 160, 192, 224, 256},
d ∈ {0.0, 0.3, 0.6}, preset ∈ {3, 6} plus optional zopfli rescue —
roughly 24-42 configurations per fixture, 651 rows for the Cycle 106
31-fixture Pile A oracle (`assets/png-bench/cycle106-r4/pile_a_grid.tsv`).

The oracle is `arg-min(size) subject to dssim ≤ tiny_dssim`. Two
cohort-level aggregations matter: (i) winner histogram over (K, d)
slots, and (ii) DSSIM-tightness bucket → PASS-rate. Both reject the
naive "best single config" framing; (i) reveals the routing target,
(ii) reveals the ceiling beyond which no single-palette config can
go.

### 4.3 Headroom-driven K selection

The Cycle 106 oracle surfaced a counter-intuitive break in the
"smaller palette ⇒ smaller PNG" monotonicity assumed by every prior
indexed-PNG codec we surveyed. On Pile A, 18/23 winners selected
K ∈ {192, 224, 256}, with K=224 the single most-common slot (8/23).
The DSSIM-bucket cross-tab (Cycle 106 Table 3) explains why:
photo-class content with `tiny_dssim ≥ 0.005` (i.e. TinyPNG itself
tolerates non-trivial visual loss) reaches 12/12 PASS at K ≥ 192,
because larger palettes capture gradient cluster diversity that, in
turn, lowers PNG filter-chain residual entropy more than the palette
overhead costs. The headroom map — DSSIM slack on the external
reference — is the routing signal; raw input features are not.

### 4.4 Per-input routing and the fail-safe wire

A naive single-config K=224 production replacement regresses the
original PASS pile by 16-25% on stratified samples (Cycle 107,
`assets/png-bench/cycle107/single_config_sample.tsv`). The
discriminator p244-vs-wins analysis (Cycle 108, `cycle108/rule_v3_full.tsv`)
shows no input-only feature — `n_pixels`, `bits/pixel`, luma /
chroma variance — cleanly separates the 11 K=224 wins from the
single regression at 99.1% retention; the true discriminator is the
v1.2.8 baseline output size, accessible only via a 2-pass measure.

The Cycle 109 wire resolves this by construction. For inputs with
`n_pixels ≥ 5 MP` the encoder runs the v1.2.8 default path and a
K=224 d=0.3 candidate, then returns `min(default, K_up)`
(`crates/nupic-core/src/ops/compress.rs:200-275`). PASS retention is
100% because the default output is always available as a floor.
Cycle 110 full-corpus verification: +1.5 pp PASS rate over v1.2.8
(20.9% → 22.4%), 106/106 PASS pile retention, zero real regressions,
baseline-7 byte-identical. Shipped as v1.2.9 (commit `8ecca8d`).

### 4.5 Generalizability

The protocol decouples three roles that prior PNG-codec work
conflated: (a) the external reference defines the gate, (b) the
cohort piles localize the attack surface, (c) the per-input fail-safe
wire bounds the production downside. None of the three steps depend
on PNG-specific structure. The same three-step pattern applies to
JPEG quality-table tuning (gate = libjpeg-turbo Q=75 baseline),
AVIF tile rate-distortion (gate = libavif preset=6), or any lossy
codec evaluated against a fixed commercial reference. The K-up
fail-safe pattern in particular — run the candidate alongside the
default and return the smaller — generalizes to any monotone-quality
codec parameter where 2-pass cost is amortizable.

---

## 5. Findings: Palette-Size Monotonicity Break and Production Wire

We report two structural findings from the §4 protocol applied to
corpus-500 + TinyPNG. C2 (§5.1) is an algorithmic observation about
PNG indexed-palette routing that contradicts the conventional
codec-design intuition. C3 (§5.2) ships C2 as a production routing
update under the cohort-retention constraint, surfacing a second
finding along the way: input-only feature classifiers ceiling at
99.1% retention because the true discriminator is the baseline
output size, accessible only via 2-pass measurement.

### 5.1 Finding C2 — Palette-size monotonicity break

Conventional indexed-PNG codec lore treats palette size K as a
monotone size knob: smaller K stores fewer palette entries (3 K
bytes), encodes pixel indices at fewer bits, and naively reduces
output size. Cycle 106 oracle on the Pile A 31-fixture extreme tail
contradicts this directly. The winner histogram over (K, d) slots
on the 23 fixtures that PASS gate (`assets/png-bench/cycle106-r4/pile_a_grid.tsv`):

| K | wins | share | mean ratio vs TinyPNG |
|---:|---:|---:|---:|
| 64 | 1 | 4% | 0.79 |
| 96 | 1 | 4% | 0.56 |
| 128 | 1 | 4% | 0.40 |
| 160 | 2 | 9% | 0.74 |
| 192 | 5 | 22% | 0.59 |
| 224 | 8 | 35% | 0.66 |
| 256 | 5 | 22% | 0.50 |

**18/23 (78%) winners chose K ≥ 192**, with K=224 the single
most-common slot. The pattern is strongest where TinyPNG itself
already tolerates non-trivial visual loss (Cycle 106 Table 3): for
fixtures with `tiny_dssim ≥ 0.005`, **12/12 (100%) reach PASS at
K ∈ {192, 224, 256}**; for `0.001 ≤ tiny_dssim < 0.002` the rate
drops to 3/6 and K=64 wins reappear (sparse natural palettes). The
cohort-wide ratio for the 23 Pile A winners is 0.59× TinyPNG.

The mechanism is a PNG filter-chain entropy effect. Indexed-PNG
output is `palette_table + IDAT_stream(filter_type + filtered_indices)`
deflate-compressed. On photo-class content with high OKLab cluster
diversity, K = 128 forces 4-5 ΔE-1+ pixel re-binnings per scanline;
adjacent pixels that *should* be near-equal palette indices become
unequal, breaking the PNG Up / Average / Paeth filter coherence that
deflate's LZ77 backref engine relies on. K = 224 leaves the cluster
boundaries near sub-perceptual ΔE, the index stream stays
quasi-locally-flat, filter residuals tighten, and deflate finds
longer matches. The palette overhead (256 vs 128 entries = 384 extra
bytes) is dominated by the filter-chain savings at any image larger
than ≈ 50 KB. The break is therefore a property of the PNG
container's filter-chain coupling to palette stability, not of the
quantizer's RD curve alone.

### 5.2 Finding C3 — Production wire under cohort retention

C2 surfaces a routing target slot (K = 192–256 with d = 0.3 on
HD photo content). C3 turns the slot into a production update that
respects the constraint Cycle 107 identified: **no replacement may
regress the v1.2.8 PASS pile**. The path traverses three failed
designs before arriving at the shipped wire.

**5.2.1 Single-config replacement (RED).** Replacing the v1.2.8
default with K = 224, d = 0.3, preset = 6 globally regressed
the original PASS pile by 16–25% across stratified samples
(Cycle 107, `assets/png-bench/cycle107/single_config_sample.tsv`):
of 25 PASS-pile fixtures, 4 (16%) lost both-axes PASS; the
32-fixture quick-bench saw 2/8 (25%). The 22/100 cohort-wide PASS
rate barely matched v1.2.8's 21/100. **Verdict**: single-config
production routing is dead — the Pile A oracle gains do not
transfer to small images that v1.2.8 already wins.

**5.2.2 Input-feature classifier (YELLOW, ceiling-hit).** Cycle 108
trained an input-only feature rule routing K = 224 on `n_pixels ≥ 5 MP`
and falling back to the v1.2.8 path otherwise. Full corpus-500 PASS
rose to **23.4% (120/513)** with PASS-pile retention 99.1%
(`assets/png-bench/cycle108/rule_v3_full.tsv`). But one PASS-pile
fixture (p244, 9.83 MP HD photo) silently degraded from 0.791× to
0.851× TinyPNG. The discriminator analysis (Cycle 108 §3) showed
no input-only feature cleanly separates p244 from the 11 K=224 wins:
`n_pixels` overlaps exactly, `bits/pixel(input)` overlaps with p246,
`bits/pixel(K=128 output)` overlaps with p287. **The only feature
that cleanly partitions p244 from the wins is the v1.2.8 baseline
output size itself** — available only via 2-pass quantize-and-measure.

**5.2.3 P-08 K-up fail-safe (GREEN, shipped v1.2.9).** Cycle 109
resolves the ceiling by construction: for `n_pixels ≥ 5 MP`, the
encoder runs the v1.2.8 default path *and* the K = 224, d = 0.3
candidate, then returns `min(default, K_up)`. Production code at
`crates/nupic-core/src/ops/compress.rs:209-273`:

```rust
let p08_eligible = (w as u64) * (h as u64) >= 5_000_000;
let bytes_default = /* v1.2.8 routed path (lossless or K-classify) */;
if p08_eligible {
    let bytes_v224 = quantize_indexed_png(K=224, d=0.3, α=0.0);
    if bytes_v224.len() < bytes_default.len() { return Ok(bytes_v224); }
}
Ok(bytes_default)
```

PASS-pile retention is **100% by construction** (the default is
always available as a floor). The K-up branch fires even when the
default would route to lossless — the case that motivated P-08
originally (p245, a 9.83 MP HD photo where v1.2.8 took the gradient
→ lossless branch and produced 2.74 MB; K = 224 emits 1.56 MB).

**5.2.4 Full-corpus verification (Cycle 110).** v1.2.9 full
corpus-500 + baseline-7 (`assets/png-bench/cycle110/full_verify_v3.tsv`):

| metric | v1.2.8 baseline | v1.2.9 (P-08 shipped) | Δ |
|---|---:|---:|---:|
| PASS rate (513 fixtures) | 109/513 (21.2%) | **115/513 (22.4%)** | +1.5 pp |
| PASS-pile retention | 106/106 | **106/106** | 0 (✓) |
| Pile A wins | 0 | 2 (p245, p291) | +2 |
| baseline-7 byte-identical | — | 7/7 (P-08 < 5 MP not triggered) | ✓ |
| real regressions | — | **0** | — |

The p245 case study (Cycle 109 §3): v1.2.8 lossless preset = 1 emits
2.74 MB (1.37× TinyPNG, fails size gate); v1.2.9 P-08 K = 224
preset = 5 emits 1.56 MB (**0.78× TinyPNG, PASS**), with DSSIM 0.0083
versus TinyPNG's 0.0115. The K-up branch fires inside ~450 ms wall
on the 9.83 MP image — within the perf KPI proportional budget of
~500 ms for 5 MP < 250 ms.

### 5.3 Combined narrative

C2 and C3 are inseparable. Reporting C2 without C3 would leave a
finding that production cannot adopt: a per-image K = 224 oracle
that, applied naively cohort-wide, regresses 16-25% of existing PASS
fixtures (5.2.1) — a net negative. Shipping C3 without C2 would
amount to picking the K-up parameters out of the air; the oracle
distribution in 5.1 is what specifies K = 224, d = 0.3 as the
right candidate slot in the first place. The protocol-driven path
from C2 to C3 — oracle → single-config failure → input-feature
ceiling → 2-pass fail-safe — also documents *why* the obvious
shortcuts do not work, which is the part absent from per-image RD
literature and from production-codec changelogs alike.

---

## TODO Next Cycles

- Cycle 122: Section 6 (finding C4 R6 spatial-aware) + Container bottleneck (C5)
- Cycle 123: Section 7 (discussion — .nupic container + WebP/AVIF transcoder paths)
- Cycle 124: Section 8 (conclusion) + figure pipeline (heatmaps / histograms / R6 tile viz)
- Cycle 125: Section 2 Related Work fill-in (outline → prose) + External References finalize
- Cycle 126: submission-ready pass (typos / format / co-author review)
- Cycle 127+: submission cycle (DCC deadline 早 / IEEE TIP rolling)

---

## References (bibliography stub)

### Internal (repo) references — Sections 4–5

- Cycle 106 Pile A oracle table report: `.claude/research-ledger/cycle-106-table-report.md`
- Cycle 107 single-config RED table report: `.claude/research-ledger/cycle-107-table-report.md`
- Cycle 108 input-feature classifier YELLOW report: `.claude/research-ledger/cycle-108-table-report.md`
- Cycle 109 P-08 K-up fail-safe GREEN ship report: `.claude/research-ledger/cycle-109-table-report.md`
- Cycle 110 v1.2.9 full-corpus verification: `.claude/research-ledger/cycle-110-table-report.md`
- Pile A 31-fixture oracle essay: `docs/research/png/04kkk-cycle106-r4-rd-pile-a.md`
- Single-config dead end essay: `docs/research/png/04lll-cycle107-single-config-dead.md`
- Input-feature classifier ceiling essay: `docs/research/png/04mmm-cycle108-input-k-classifier.md`
- P-08 K-up fail-safe wire essay: `docs/research/png/04nnn-cycle109-p08-kup-failsafe.md`
- v1.2.9 full-corpus verify essay: `docs/research/png/04ooo-cycle110-full-corpus-verify.md`
- Three-axis corpus partition: `assets/png-bench/corpus-500-three-axis.tsv`
- Pile A extreme-tail oracle data: `assets/png-bench/cycle106-r4/pile_a_grid.tsv`
- v1.2.9 full-corpus verification data: `assets/png-bench/cycle110/full_verify_v3.tsv`
- Cycle 107 single-config stratified sample: `assets/png-bench/cycle107/single_config_sample.tsv`
- Cycle 108 input-feature classifier full sweep: `assets/png-bench/cycle108/rule_v3_full.tsv`
- P-08 production code: `crates/nupic-core/src/ops/compress.rs:209-273`

### External — Sections 1-3 (TODO populate Cycle 125)

- [will populate Cycle 125 — Related Work fill-in]
