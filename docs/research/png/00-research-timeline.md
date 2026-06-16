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
