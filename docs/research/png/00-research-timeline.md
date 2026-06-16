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

### Cycle 24-25 — gradient → lossless auto-routing(v0.5.38,ship)
- Cycle 24:`classify_for_auto_dither` 加 `adj_mn < 1.0 → 0.7` tier-4c
  bucket(extreme-smooth gradient need strong dither against banding)。
  08 SSIM 58.98 → 68.08(+9.1),其他 fixture bit-exact
- Cycle 25:probe 15-fixture lossless vs auto:**只有 08 lossless 双胜**
  (53 KB vs 497 KB,SSIM 100 vs 68)
- 实装 `nupic-quantize::is_gradient_candidate`(opaque + adj_mn<1.0 +
  uniq≥1000),`encode_png_stone_c` 检测到 gradient → 走 lossless
- **08 终态:53 KB / SSIM 100**(pre-Cycle-23 是 190 KB / 37.72 → 
  **-72% size AND +62 SSIM** 一个数量级 quality jump)
- 原 7-fixture + 其他 7 ext fixture 全 bit-exact unchanged
- 219 tests 全绿
- Essay:`03t-cycle24-25-gradient-lossless.md`

### Cycle 23 — extended corpus + tier-3 uniq-color guard(v0.5.37,ship)
- **新增 8 fixture**(`assets/png-bench/inputs-ext/`):08 gradient-large、
  09 ui-checker-text、10 comic-flat、11 photo-noisy、12 tiny-icon、
  13 very-large-photo(8.6 MP)、14 soft-transparent、15 mono-text
- **Misclass 发现**:08 gradient(mean_run=6.13,uniq=117K)被误为
  tier-3 UI(d=0.25)→ SSIM 37.72。 sweep 显示 d=0.7 peak SSIM 68
- **Signal sweep**:UI/logo/text uniq ≤ 129,photo/gradient uniq ≥ 4348。
  big gap → threshold 1000 cleanly split
- Fix:tier-3 加 uniq-color guard `mean_run > 2 AND uniq < 1000`,
  实现 O(N) HashSet + early-exit at 1000
- 原 7-fixture corpus **bit-exact identical**;ext corpus 仅 08 改变:
  **+21.26 SSIM(37.72 → 58.98)**,size +174 KB
- 08 离 peak(d=0.7)仍有 +9 SSIM 余地;Cycle 24+ 候选
- 219 tests 全绿
- Essay:`03s-cycle23-tier3-uniq-guard.md`

### Cycle 22 — 01-transparency-demo dither sweep(research-only)
- 01 当前 tier-1(opaque_ratio 0.036 → transparency-dominant → d=0)
- finer dither sweep:每 KB +0.5 SSIM linear,无 sweet spot
- d=0.10 → +2 KB / +1.6 SSIM,d=0.15 → +3.1 KB / +2.1 SSIM,d=0.30 → +6.7 KB / +5.4 SSIM
- 01 SSIM 仍 -46 ~ -41(fundamental palette quantize loss on synthetic
  transparency grid);nupic vs Tiny 01:nupic -46 vs Tiny -492 = 已 +446 优势
- 决策:**保留 tier-1 = 0.0**(N=1 fixture 无法 generalize tier-1b
  sub-rule,且 size penalty 显著 vs SSIM gain)
- Example:`cycle22_01_classify.rs` 记录分类诊断

### Cycle 21 — `--effort 7-10` unlocks oxipng Zopfli deflater(v0.5.35,ship)
- Probe(`cycle21_zopfli_probe`):full corpus Libdeflater vs Zopfli iter=15
- Zopfli **每 fixture -23~-2496 B**,total **-8714 B(-0.32%)**;
  **零 SSIM regression**(deflate 是 lossless 变量)
- Wall time 2.7× 慢
- Wire:`--effort 7-10` → Zopfli iter=(effort-6)×5,backward compatible
  (default `--effort 5` 不变);Cargo workspace 启用 oxipng "zopfli" feature
- vs TinyPNG:e5 -8.8%,e10 **-9.1%** corpus 小
- 7 fixture e10 vs e5 SSIM **完全不变**,size -509 ~ -1906 B 每 fixture
- 219 tests 全绿
- Essay:`03r-cycle21-zopfli-effort.md`

### Cycle 20 — tier-2 dither 0.25 → 0.35 Pareto bump(v0.5.34,ship)
- 02-pluto-transparent dither sweep:d=0.25 给 SSIM 80.44,d=0.35 给
  80.73(+0.29 SSIM / +1.7 KB),d=0.50 给 80.87(peak,但 +4 KB)
- Pareto best = d=0.35:最优 SSIM/byte trade
- 02 是 corpus 唯一 tier-2 fixture(partial transparent);其他 6 fixture
  output bit-exact 不变
- 219 workspace tests 全绿

### Cycle 19 — serpentine FS dither A/B(negative,research-only)
- 假设:serpentine scan(alternate L→R / R→L per row)减少 directional
  smear,boost photo SSIM
- 实测:5 dithered fixtures mean Δ SSIM = **-0.045**(02 lost 0.20,
  其他 ≈ 0)
- 结论:OKLab+alpha 高维 diffusion + Lloyd's-refined palette 已经
  symmetric enough;serpentine 在我们 setup 下 no signal
- 评论留在 `apply_palette_rgba_fs_dither`,no version bump

### Cycle 18 — Path B filter ranking Level::Best test(negative,research-only)
- 假设:`filter_image_best_of` 用 Level::Fast ranking 与 final Level::Best
  不一致,可能导致 sub-optimal filter pick
- 试改:ranking 用 final Level(mrl-determined)
- 结果:**sizes 完全不变**(04 594681,05 402282,06 1095841,07 379504),
  wall-clock 慢 5-15×(05 from 2s → 62s)
- 结论:Fast 和 Best ranking 选同一个 filter — Path B size gap **不在
  filter selection**,而在 deflate quality(Cycle 10 已确认)。 唯一通路
  closing Path B gap = nupic-deflate stage 2(libdeflate-class cost-DP)
- 评论保留在 `filter_image_best_of`,no version bump

### Cycle 17 — var-diff sampling bias fix(v0.5.33,ship)
- Coverage attack:`cycle17_var_diff_sampling` 3-part bench 测原 corpus
  外的大图行为
- **Bug 发现**:n_total > 1M 时 step=4 + count > 500K break 截断到
  top ~50% 行;1200×6400 adversarial:smooth_top+textured_bot 给
  d=0.5,textured_top+smooth_bot 给 d=0.7 — 同 pixel pool 仅因垂直翻转
  得到不同 d 选择
- 影响:真实 4K photo(8 MP,sky-top + ground-bot)会被 top 半 var 主导
- Fix:proportional step `h / target_rows`(TARGET=500K samples),
  step 按 image height 缩放,no break,reach full image
- 7-fixture 原 corpus **bit-exact identical**(均 ≤ 2.7 MP,走 step=1
  或 step=4-reachable 路径,fix 不影响)
- Essay:`03q-cycle17-sampling-bias-fix.md`

### Cycle 16 — Lloyd's perf attempts(both negative,research-only)
- A1:par_chunks + per-thread Acc + reduce → **3× SLOWER**
  (Acc{9 Vec × 256 × 8B} alloc + reduce 远超 sequential's L1-cache
  fastpath。sequential accumulate 是 memory-bandwidth-bound,not
  CPU-bound)
- A3:pack palette + alpha 进 Vec<(f32; 4)> → noise-bound
  (both packed/unpacked fit L1,no cache delta)
- bench tooling 噪声大(1698-3379 ms σ),需 warmup + median
- 结论:Lloyd's < 10% perf 改进 在当前 bench 测不出来。Cycle 17+
  pivot 到 measurable signal(quality / coverage)
- Essay:`03p-cycle16-perf-attempts-negative.md`

### Cycle 15 — Lloyd's split-on-empty force-iter cap(v0.5.32,ship)
- Instrumented Lloyd's iter count per fixture(NUPIC_DEBUG_LLOYD env):
  03-wiki-logo runs all **100** iters,others 22-67
- Root cause:logo has < 50 unique colors;split-on-empty perpetually
  finds empty slots and PERTURBS centroids — that perturbation gets
  counted as "movement" in next iter's max_move,blocking EPS_SQ exit
- Fix:skip split-on-empty block entirely after `SPLIT_FORCE_ITERS=30`;
  let `compact_palette` post-process drop unused slots
- 03 iter count:**100 → 32**(-68%);7-fixture outputs **bit-exact
  identical**

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
