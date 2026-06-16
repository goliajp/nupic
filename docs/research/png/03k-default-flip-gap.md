# 03k — Cycle 6 default-flip gap decomposition

> Cycle 6 mission:close `--use-nupic-png` Path B vs Default Path A
> size gap to ≤ 1.02× (currently 1.04-1.5×)。Pass 1 gap decomposition
> bench(`default_flip_gap.rs`)was misread as showing 18% deflate
> gap on 04-portrait;Pass 3 cleaner bench(`iter_passes_sweep.rs`)
> shows real gap **only 1.5-6.5% per-fixture deflate-quality vs
> libdeflate near-optimal**。Path B big gap was actually from the
> size-aware Level::Fast fallback added in 2.4 to escape the perf
> cliff。Now NICE_MATCH=128 has bounded the cliff,fallback removed
> in 0.5.23 — testflight IDAT 47 KB → **25 KB**(-46%),vantage 407 KB
> → **314 KB**(-23%)。Wall-clock 1s → 34s on testflight is the new
> trade-off。

---

## 1. Pass 1:gap decomposition(misread)

`default_flip_gap.rs` cross-product:

| fixture | A_total | B_total | gap | "filter_contrib" | "deflate_contrib" |
|---|---:|---:|---:|---:|---:|
| 04-portrait | 378428 | 445370 | +66942 | -24579 | +23718 |

Initial(wrong)interpretation:"filter contributes 24579 if Path B used
miniz_oxide,deflate contributes 23718 if Path A used nupic-deflate"。
But `bf_lib` used `flate2 Compression::new(9)` which is **miniz_oxide
zlib L9**,NOT libdeflate near-optimal — much weaker than what oxipng
uses。Decomp invalid。

---

## 2. Pass 3:per-fixture deflate-quality vs libdeflate

`iter_passes_sweep.rs` direct comparison — extract Path A's IDAT
(libdeflate near-optimal compressed)decode it back to filtered rows,
recompress with `nupic_deflate::zlib_compress`(Level::Best):

| fixture | libdeflate | **nupic_Best** | ratio |
|---|---:|---:|---:|
| 01-transparency | 44 259 | 44 857 | 1.014× |
| 02-pluto | 157 241 | **165 307** | 1.051× |
| 03-wikipedia | 12 223 | 12 334 | 1.009× |
| 04-portrait | 378 017 | **402 146** | 1.064× |
| 05-mountain | 388 427 | 396 037 | 1.020× |
| 06-landscape | 1 035 128 | 1 038 359 | 1.003× |
| 07-product | 320 303 | **330 730** | 1.033× |

**Real deflate-quality gap: 0.3% (06) to 6.4% (04)**,avg ~ 2.8% across
photo+UI corpus。Gap exists but small。

---

## 3. Pass 2 BestOf picks optimal filter

`filter_pick_diag.rs` enumerated 6 candidate filter strategies × Level::Best:

| fixture | min-Best winner | BestOf pick(via Fast proxy)| mispredict cost |
|---|---|---|---|
| 01-7(all)| **None** | **None** | **+0 bytes** |

BestOf's Level::Fast proxy correctly identifies None as winner on every
fixture。No proxy mispredict on photo / UI / logo / transparent。**Filter
selection is not the gap source**。

---

## 4. Pass 4 ship — remove size-aware Fast fallback

Conclusion:Path B's residual size gap is(a)1.5-6.5% nupic-deflate Best
deflate-quality(small)+(b)the size-aware Fast fallback I added in 2.4
to mitigate wall-clock(big)。Now NICE_MATCH bounds wall-clock,fallback
can come off。

```rust
// v0.5.23: always Level::Best, NICE_MATCH protects perf cliff
let idat = zlib_wrap(&raw_filtered, Level::Best);
```

Result v0.5.23 vs v0.5.22:

| input | v0.5.22 size | **v0.5.23 size** | v0.5.22 ms | **v0.5.23 ms** | size Δ | wall-clock Δ |
|---|---:|---:|---:|---:|---:|---:|
| 01-transparency | 46 044 | 46 044 | 7900 | 8600 | 0 | +9% |
| 02-pluto | 192 637 | 192 637 | 3900 | 5200 | 0 | +33% |
| 04-portrait | 445 370 | 445 370 | 3700 | 4100 | 0 | +11% |
| 05-mountain | 402 282 | 402 282 | 10400 | 10900 | 0 | +5% |
| 06-landscape | 1 095 841 | 1 095 841 | 6900 | 7200 | 0 | +4% |
| 07-product | 333 690 | 333 690 | 4200 | 4400 | 0 | +5% |
| **testflight UI** | 47 086 | **25 438** | 1100 | 33800 | **-46%** | **+30×** |
| **vantage UI** | 407 180 | **314 118** | 5200 | 55300 | **-23%** | **+10×** |

UI screenshots get the size win,but the wall-clock penalty on those
specific inputs is 30-55s。For "又小又好" research-density default,
size is the goal — ship Level::Best everywhere,size-vs-time knob
left for future `--png-effort` flag。

---

## 5. Path B vs Path A — current gap

| input | Path A | Path B v0.5.23 | B/A |
|---|---:|---:|---:|
| 01 | 45 364 | 46 044 | 1.015× |
| 02 | 158 109 | 192 637 | 1.218× |
| 03 | 12 658 | 13 138 | 1.038× |
| 04 | 378 428 | 445 370 | 1.177× |
| 05 | 389 264 | 402 282 | 1.033× |
| 06 | 1 035 965 | 1 095 841 | 1.058× |
| 07 | 320 864 | 333 690 | 1.040× |
| testflight | 19 828 | 25 438 | 1.283× |
| vantage | 275 332 | 314 118 | 1.141× |

**04-portrait + 02-pluto + UI 仍 are ≥ 1.15× gap**。Path A's oxipng
benefits from libdeflate's "near-optimal" deflate level which uses
much more aggressive iterative search than nupic-deflate's Level::Best。

Default-flip threshold not yet met(want ≤ 1.02×)。Phase 2.6 candidates:
- True per-row deflate-aware filter ranking (use Level::Best not Fast)
  but cap by image size — would close maybe 1-2 pp
- Nupic-deflate algorithmic improvements (zopfli-class search,longer
  iter,or specific tweaks on 02 and 04)
- Block-split iteration count(currently fixed)

---

## 6. cross-link

- 03i: original perf cliff workaround at nupic-png level
- 03j: NICE_MATCH root-cause fix at nupic-deflate level
- 03d: Stone D Lloyd k-means(palette refinement,not deflate)

---

## 7. 价值观

- [[feedback-metric-over-human-eye]] — Pass 1's confused decomposition
  caught because Pass 3's direct measurement gave different numbers。
  Always re-measure with cleaner methodology when "interpretation"
  feels uncertain。
- [[feedback-no-cost-thinking]] — wall-clock 1s → 34s on testflight is
  documented;not used to gate the ship decision。User opts in or out。
