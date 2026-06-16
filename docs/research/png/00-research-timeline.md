# PNG codec research timeline(0.4 → 0.5.22+)

> 单文件 chronological index, 每 ship pass / research finding 一行。
> 每个 entry:**版本**·**Stone/Phase**·**1 句 key insight + 数字**·
> **link**。新 pass 完时往这里 append。**Don't re-do research here**,
> 只是 cross-link 到具体 essay。

读 essay 顺序按 number 排序;此 file 给 "为什么这一步"。

---

## Stone graduation roadmap(meta)

| Stone | crate | role |
|---|---|---|
| A | `nupic-color` | OKLab perceptual color space |
| B | `nupic-ssimulacra` | self-built SSIMULACRA2 metric |
| **C** | **`nupic-quantize`** | **palette quantize + Stone D refine + Stone E dither** |
| bits | `nupic-bits` | CRC-32 + Adler-32 + bit I/O (0 deps) |
| deflate | `nupic-deflate` | self-built DEFLATE encoder |
| png | `nupic-png` | self-built PNG encoder (filter try-all) |

`Quality::Auto` Default Path A 走 cement(`oxipng`)+ Stone C/D/E。Opt-in
`--use-nupic-png` 走 Path B(完全 self-built nupic-png + nupic-deflate)。

---

## Phase-by-phase records

### 0.4.x baseline(cement imagequant + FS dither)
- SSIMULACRA2 mediocre on photo,size moderate
- TinyPNG = research reference,not target

### Stone A/B/C ship(0.5.0-0.5.4)
- 0.5.0 Stone C `nupic-quantize` PNG `Quality::Auto`:OKLab argmin **no dither**
  +77.3 SSIM avg vs 0.4(02-pluto -65→+72)→ [03c-bis](03c-bis-codebook-c0.md)
- 0.5.1 Stone B SSIMULACRA2 self-built metric ship → [03b-six](03b-six-graduation.md)
- 0.5.2 `nupic-bits` 0-dep stone(CRC-32 + Adler-32 + bit I/O)→ [05](05-nupic-bits-stage-0.md)
- 0.5.3 `nupic-deflate` phase 1.0.0 stored blocks → [06-bis](06-bis-deflate-stored-blocks.md)
- 0.5.4 `nupic-deflate` phase 1.0.1 LZ77 + static Huffman → [06-ter](06-ter-deflate-lz77.md)

### nupic-deflate stage-1 graduation(0.5.5-0.5.9)
- 0.5.5 phase 1.0.2 dynamic Huffman + chooser → close L6-L9 gap → [06-quater](06-quater-deflate-dynamic.md)
- 0.5.6 phase 1.1 lazy match + chain 128 → close text -3.4% → [06-quinquies](06-quinquies-deflate-lazy.md)
- 0.5.7 phase 1.2 multi-block split → **strictly beats zlib L9 on cargo-lock** → [06-six](06-six-deflate-multiblock.md)
- 0.5.8 phase 1.3 quickcheck fuzz + zopfli oracle + bug fix(stored bit-cost off by 24 bits — fuzz catches day-1)→ [06-seven](06-seven-deflate-graduation.md)
- 0.5.9 phase 1.4 iterative cost-DP(zopfli core trick)→ **7/7 inputs ≤ 1.05× zopfli**(full graduation)→ [06-nine](06-nine-deflate-iterative.md)

### nupic-png stone(0.5.10-0.5.11)
- 0.5.10 `nupic-png` foundation(filter try-all + chunk writer)→ [07](07-nupic-png-foundation.md)
  - **Critical finding**:filter selection accounts for 60% of pipeline gap,not deflate → [06-eight](06-eight-png-integration-readiness.md)
- 0.5.11 `--use-nupic-png` opt-in CLI flag wired → [07-bis](07-bis-nupic-png-integration.md)

### Stone C alpha-aware + Stone E ship(0.5.12-0.5.18)
- 0.5.12 phase 2.1 Stone C alpha-aware + tRNS chunk(both backends)→ correctness fix → [08](08-stone-c-alpha-aware.md)
- 0.5.13 phase 2.2 `BestOf` filter strategy(7-candidate全 stream deflate proxy)corpus 1.10× → **1.07× oxipng** → [09](09-bestof-filter-selection.md)
- 0.5.14 phase 1.5 per-block iterative refinement(cost-checked)→ [10](10-deflate-per-block-refinement.md)
- 0.5.15 Stone D Lloyd's k-means palette refinement default(5 iter)→ **avg +24.68 SSIM corpus,-0.6% size** → [03d](03d-stone-d-design.md)
  - Negative result Variant A (Bayer-swap):−6.7 SSIM avg → [03d §4]
- 0.5.16 Stone D iter 5 → 20 default(04-portrait +0.66 SSIM)
- 0.5.17 Stone E `--dither <float>` opt-in(FS-light)photo +1-5 SSIM,UI sensitive → [03e](03e-stone-e-fs-dither.md)
- 0.5.18 Stone E `--dither auto`(opaque-large → 0.25)— non-regression dogfood

### Cycle 14 — Lloyd's k-means perf -26%(v0.5.31,ship)
- Profile(`cycle14_perf_breakdown`):Lloyd's refine 100 iter dominates
  82.5% of 05-mountain encode time(2270 / 2751 ms)
- Root cause:`srgb_u8_to_oklab` called **3 times per pixel per iter**
  (assign / sum / SSE)= 288M conversions for 05 × 100 iter
- Fix 1:precompute OKLab+alpha into `pixels_oklab_alpha` **once**;
  iter loops read from precomputed vec(960K conversions total)
- Fix 2:collapse two sequential passes(sum→SSE)into one via
  algebraic identity `SSE = Σx² − (Σx)²/count`
- Lloyd's:**2270 → 1669 ms(-26%)**;total encode -21%。 7-fixture
  outputs **bit-exact identical**(verified;split-on-empty FP ordering
  preserved)
- Essay:`03o-cycle14-lloyd-perf.md`

### Cycle 12-13 — 05 ceiling profile + default policy(research-only,no ship)
- 05-mountain palette-saturated at 256;Lloyd's converged by iter=100
- imagequant s=1 corpus sweep:**-1.18 SSIM net** due to 02-pluto
  -8.45 collapse;don't wire effort → IQ speed
- `--dither auto` vs `off` corpus diff:+1.72 SSIM mean / +9.7% size
  total;**keep default `off`** to preserve TinyPNG size advantage on
  05/07 (mission "又小又好" needs both)
- 05 SSIM 76.82 is **practical ceiling** for current algorithm shape;
  further gains need orthogonal innovation(blue-noise dither /
  SSIMULACRA2-aware loss / regional dither)
- Essay:`03n-cycle12-13-ceiling-floor.md`

### Cycle 11 — tier-4 content-aware dither split(v0.5.30,ship)
- Cycle 9 sweep showed 04 portrait wants d=0.5(skin smooth),05/06/07
  want d=0.7(textured)。 Need signal to split within tier-4。
- `var(adjacent-pixel luminance diff)` cleanly separates:04 var=34
  vs {05=320, 06=665, 07=85}。 Threshold `var > 50`。
- Auto bench(`cycle11_tier4_split`):04 unchanged,05 +1.09 SSIM /
  +4% size,06 +0.41 / +2.3%,07 +0.38 / +3.8%。Total +1.88 SSIM
  across 3 textured photos;7-fixture average **+0.27 SSIM/image**。
- Essay:`03m-cycle11-tier4-content-split.md`。

### Cycle 10 — Path B filter selection deep dive(research-only,no ship)
- Per-chunk decomp of 04-portrait Path A vs Path B shows IDAT diff
  110 KB(484 KB vs 594 KB)
- **oxipng picks per-row Paeth mix(764 Paeth + 36 Sub)**;Path B BestOf
  picks **all-None**(even though all 6 candidates evaluated via Level::Fast)
- min-SAD per-row picks 60% Sub / 39% Paeth on 04 — different from oxipng's choice
- Tested:per-row Level::Best ranking in `filter_image_deflate_aware` →
  forced DeflateAware path → **uniformly WORSE** than BestOf(04 +22%,
  01 +33%,02 +19%)。Per-row deflate cost on 1200-byte rows doesn't
  correlate with cross-row final-stream cost。Reverted。
- Negative-result finding:**Path B closing oxipng filter-quality gap
  requires libdeflate-class cross-row deflate context,not just per-row
  decision**。Future:may need actual streaming-context-aware per-row
  selection。

### Cycle 9 — tier-4 fine-strength sweep(research-only,no ship)
- (research-only) Sweep d ∈ {0.5, 0.6, 0.7, 0.75} on photo fixtures:
  - 04-portrait:peak 0.5(0.6+ slight regress)
  - 05-mountain:monotonic to 0.75,+1.20 vs 0.5
  - 06-landscape:peak 0.7,+0.41 vs 0.5
  - 07-product:peak 0.7,+0.38 vs 0.5
- No simple signal differentiates 04 from 05/06/07 (all 1200×800 or
  similar,all 256 palette colors after Stone D pad+split,no aspect
  ratio distinction)。Future work:content-aware tier-4 strength
  (face vs landscape detection)— could capture +0.4-1.2 SSIM on 3 of
  4 photo fixtures without 04 regression。

### Cycle 8 — dogfood verify + 02-pluto tier-2 dither(0.5.27-0.5.28)
- 0.5.27 unpin `rust-version`(no longer pin 1.85,track latest stable
  toolchain per user directive)
- 0.5.28 `--dither auto` 4-tier classifier(was 3-tier):add **tier-2
  transparent-photo class**(0.50 ≤ opaque_ratio < 0.95 → 0.25 dither)。
  Discovered via 02-pluto fine-grain sweep:dither 0.05-0.25 monotonic
  +SSIM up to 80.44(+0.78 over default no-dither)。Tier-2 expansion
  catches 02-pluto class(transparent-but-mostly-opaque photos)
  without affecting fully-transparent UI panels(< 0.5 opaque)
  → essay TODO 03m
- Dogfood verify Cycle 7 Stone D pad+split on testflight:**SSIM 84.72 → 89.64**(+4.92);vantage unchanged
- 0.5.29 Pass 3 fix:`--use-nupic-png` Path B 没接 `dither_strength`
  (uniform-matrix bench 发现 B_auto = B_off)。Add `quantize_with_dither`
  API,wire `encode_png_stone_c_nupic` to pass `opts.dither_strength`。
  Now Path B mirrors Path A's SSIM gains on dither (02 +0.78,04 +0.87,
  05 +5.36) at corresponding size cost。

### Cycle 7 — palette ceiling close(0.5.25+)
- 0.5.25 Stone D **palette pad + split-on-empty**:imagequant returns
  < n_colors when quality threshold(95)met early(04-portrait 114 of
  256 slots used);pad palette in `train_palette_rgba`,Lloyd split-
  on-empty redistributes dupes to high-SSE clusters。**04-portrait
  SSIM 83.06 → 87.99**(beats TinyPNG by +2.13);03-wikipedia bit-
  exact 100;07-product +1.86;corpus 0.865× → 0.912× tinypng(+5.5%
  size)but **7/7 fixtures now beat TinyPNG SSIM** → [03l](03l-palette-pad-split.md)
  - Diagnostic via PNG chunk-walker python:`PLTE 342B = 114 colors`
    finding identifies palette-limited not algorithm-limited
  - Methodology: `portrait_deep.rs` n_colors×dither sweep first
    showed n_colors knob had no effect → pointed at imagequant fixed
    output

### Cycle 6 — default-flip final push(0.5.23-0.5.24)
- 0.5.23 Pass 1-4:gap decomposition research + Level::Best always(temporarily)
- 0.5.24 Pass 5-6 reversion:chain×iter sensitivity sweep showed
  diminishing returns(2-4× wall-clock for < 0.5% size);revert to
  0.5.22-baseline trade-off(chain=512,iter=5,size-aware Fast fallback)。
  Cycle 6 conclusion:**no algorithmic improvement available on deflate
  side beyond NICE_MATCH**;default-flip blocked on libdeflate-class
  C implementation or zopfli-class iterative work → [03k](03k-default-flip-gap.md)
  - Pass 1 misread methodology / Pass 2 filter pick = correct /
    Pass 3 cleaner ratio = 1.5-6.5% gap / Pass 4 ship Best-only /
    Pass 5-6 chain×iter sweep negative

### Cycle 5 — research density(0.5.19-0.5.22)
- 0.5.19 Pareto sweep + **tiered `--dither auto`**(photo 0.5 / UI 0.25 by mean-run-length signal)→ [03f](03f-pareto-tiered-dither.md)
  - 04-portrait +1.03,05-mountain +5.08,UI +0.11~0.18,logos 0 — perfect
- 0.5.20 Stone D adaptive iter:cap 20 → 100 + EPS auto-stop → **02-pluto +7.38 SSIM** → [03g](03g-adaptive-iter.md)
- (research-only)dither variants(serpentine / Sierra-3 / Sierra-Lite):vanilla FS Pareto-optimal,no ship → [03h](03h-dither-variants.md)
- 0.5.21 `--use-nupic-png` perf cliff workaround(mrl-fallback to Level::Fast):testflight **10+ min → 0.8s** → [03i](03i-perf-cliff.md)
- 0.5.22 Pass 6 root-cause:nupic-deflate `NICE_MATCH=128` chain-walk early-exit + nupic-png size-aware adaptive → **all inputs < 11 s,small ones < 4 s** → [03j](03j-deflate-nice-match.md)

---

## Cumulative scoreboard(v0.5.22 vs 0.4 baseline)

### Default Path A(`nupic compress`, no flag)
- Size:0.871 → **0.865× TinyPNG**(7/7 fixture all smaller than tiny)
- SSIM:6/7 strictly > TinyPNG(04-portrait gap -3.5 closeable via `--dither auto`)
- 04-portrait wall-clock unchanged from 0.5.0(quantize 0.5s,oxipng 1-2s)

### Opt-in Path B(`--use-nupic-png`)
- size vs Path A:1.04-1.5× depending on content(photo close,UI 1.5×)
- wall-clock:**<11 s every input**(was 10+ min hang on testflight pre-0.5.21)
- Default-flip blocker:still needs size gap close 5-30pp before flip

### research artifact list
- 14 essays in `docs/research/png/`(00-research-timeline + 03/03b/.../03j + 04 perf + 05 bits + 06 deflate + 07/07-bis png + 08/09/10 phase 2.x)
- 8 research example binaries in `crates/nupic-research/examples/`
- 700+ quickcheck verifications per `cargo test` run on nupic-deflate
- ~ 210 workspace tests
- v0.5.5 → v0.5.22 = 18 tags shipped through Cycles 4-5

---

## How to read this file

- New ship pass:append **1 row** to the relevant Cycle section above
- New research-only(no version bump):still 1 row,labeled `(research-only)`
- Don't put data tables here — they live in per-pass essays
- Cross-link via `[essay-id](essay-id.md)` — keeps this file a pure index
- Update "Cumulative scoreboard" section after each cycle (not each pass)

This file is the **change log,for stone-research consumers**。
`MEMORY.md` is the **shorthand for AI/session continuation**。Don't
duplicate;cross-link。
