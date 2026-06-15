# 06-eight — PNG integration readiness:nupic-deflate vs oxipng's libdeflate on actual IDAT

> Earlier 06-seven essay reported nupic-deflate at 1.00× zopfli on
> `02-pluto-png-stream`(the PNG **file as raw bytes**)。But that's not
> the integration-target workload — the user-facing question is "what's
> the IDAT size delta if I swap oxipng's deflate backend for
> nupic-deflate, keeping oxipng's filter selection?" This essay measures
> that directly。Result:**+8% larger IDAT on average across the 7-fixture
> corpus**(13% on photos)— integration is **BLOCKED on phase 1.4
> zopfli-class iterative refinement**。

---

## 1. Bench setup

For each fixture in `assets/png-bench/current-nupic-0.5/`(current
production output, already optimized by `Quality::Auto` =
nupic-quantize → oxipng):

1. Walk PNG chunks, concatenate all IDAT data → `old_idat`(zlib stream
   produced by oxipng's libdeflate backend)
2. Decompress with flate2 → filtered rows
3. Re-compress filtered rows with `nupic_deflate::zlib_compress`
   (= `Level::Best`, phase 1.3)→ `new_idat`
4. Report `new_idat / old_idat`

bench:`crates/nupic-research/examples/png_idat_swap.rs`。Run:

```
cargo run --release -p nupic-research --example png_idat_swap
```

---

## 2. Result

| fixture | png_total | old_IDAT(oxipng)| nupic_B | nupic_F | **B / old** | F / old |
|---|---:|---:|---:|---:|---:|---:|
| 01-png-transparency-demo | 49 829 | 48 998 | 53 514 | 66 998 | **1.09×** | 1.37× |
| 02-pluto-transparent | 162 069 | 161 244 | 181 917 | 239 788 | **1.13×** | 1.49× |
| 03-wikipedia-logo | 13 198 | 12 829 | 13 336 | 15 905 | **1.04×** | 1.24× |
| 04-photo-portrait | 380 318 | 379 907 | 429 134 | 565 582 | **1.13×** | 1.49× |
| 05-photo-mountain | 402 741 | 401 904 | 440 963 | 539 378 | **1.10×** | 1.34× |
| 06-photo-landscape | 1 062 185 | 1 061 348 | 1 103 719 | 1 316 541 | **1.04×** | 1.24× |
| 07-photo-product | 325 525 | 324 964 | 361 529 | 460 300 | **1.11×** | 1.42× |
| **TOTAL** | | **2 391 194** | **2 584 112** | — | **1.08×** | — |

Headline:**nupic-deflate produces 8% larger IDAT on average,13% on
photographic content**(02, 04, 07)。Best case is logos / line-art
(03, 06) at 1.04×。

---

## 3. Diagnose — why?

oxipng's `Deflaters::Libdeflater { compression: 5..=12 }` does iterative
LZ77 with cost-model feedback(libdeflate "near-optimal" mode is 8-12),
similar in spirit to zopfli but faster wall-clock。Per the [oxipng
source comment](https://github.com/shssoichiro/oxipng/blob/main/src/deflate/mod.rs):

> Libdeflate has four algorithms: 0 = 'uncompressed', 1-4 = 'greedy',
> 5-7 = 'lazy', 8-9 = 'lazy2', 10-12 = 'near-optimal'

nupic-deflate phase 1.3:
- **Greedy / lazy LZ77**(single forward pass)
- **Multi-block split**(equal-sized N ∈ {1, 2, 4, 8})
- **Frequency-fitted Huffman per block**

Missing vs libdeflate "near-optimal":
- **Iterative LZ77 refinement**:run multiple LZ77 passes,each pass uses
  the previous pass's Huffman code-lengths as the per-token cost model
  → encoder makes match-vs-literal decisions in *output-size* space,
  not *match-length* space。This is the zopfli core trick;libdeflate
  level 10+ uses the same idea。
- **Variable-position block split**:libdeflate / zopfli find optimal
  block boundary positions(not equal-sized partitions)。
- **Optimal block splitting count**:up to 15 blocks default,searched
  vs cost。

Our [`deflate_compare` bench](../../../crates/nupic-research/examples/deflate_compare.rs)
on `cargo-lock` already exposed the same gap:nupic_B 15 251 vs zopfli
13 345 = **1.14× zopfli**。PNG IDAT confirms the gap holds(or widens)
on photographic content。

---

## 4. Implications for PNG-pipeline integration

**Current state**:
- `Quality::Auto` → `nupic-quantize` → `oxipng` → user
- `oxipng` uses **libdeflate near-optimal**(zopfli-class)
- 7-fixture total: 2.39 MB

**If we swap to nupic-deflate Level::Best today**:
- 7-fixture total: 2.58 MB (+8%)
- 02-pluto: +12.8% file size — visible regression
- 04-portrait: +13% — visible regression
- 03/06(logos/landscape):+4% — borderline

This would be a **user-visible regression** in PNG file size — exactly
the metric users care about when picking a PNG optimizer。**Blocked**。

**Path forward — phase 1.4 zopfli-class iterative refinement**:
1. Variable-position block split(close ~ 3% via better boundary fit)
2. Iterative LZ77 refinement with Huffman-cost feedback(close ~ 9%
   via match decisions in cost-space)
3. Re-bench:expect nupic_B / old_IDAT ≈ 0.98-1.02× across fixtures
4. Then ship integration as 0.6.0

If phase 1.4 closes the gap,integration also removes the `oxipng` and
its transitive `libdeflate`/`zopfli` deps from `nupic-core`(big stone
graduation step — nupic-deflate becomes the sole DEFLATE provider for
PNG path,fulfilling the 0.6.x roadmap target)。

---

## 5. ceiling update

Re-frame phase 1.4 ceiling distance with PNG-IDAT data:

| input class | current B/oxipng-IDAT | target |
|---|---:|---:|
| line-art / logos(03, 06)| 1.04× | ≤ 1.01× |
| transparent indexed(01, 02)| 1.09-1.13× | ≤ 1.02× |
| photo(04, 05, 07)| 1.10-1.13× | ≤ 1.02× |
| **corpus total** | **1.08×** | **≤ 1.02×** |

`zopfli` 在 7-fixture(if we benched it on filter rows)估 ≈ 0.98-1.00×
oxipng-IDAT(zopfli ~ libdeflate near-optimal,且 zopfli 略多 iteration)。
So:
- nupic phase 1.3 → 1.08× oxipng IDAT
- nupic phase 1.4 → ~ 1.00× oxipng IDAT(estimate from cargo-lock 1.14×
  → ~ 1.02× zopfli after iterative refinement)
- This unblocks integration。

---

## 6. cross-link

- 上游:[06-seven](06-seven-deflate-graduation.md)(phase 1.3 stage-1
  perf-graduate for PNG-class workloads;reported 1.00× zopfli on
  pluto-as-raw-bytes)
- 下游:phase 1.4(待开:variable-position split + iterative LZ77
  refinement)→ 之后 PNG integration ship 为 0.6.0
- bench:[`crates/nupic-research/examples/png_idat_swap.rs`](../../../crates/nupic-research/examples/png_idat_swap.rs)

---

## 7. 验收材料

- bench:`crates/nupic-research/examples/png_idat_swap.rs` 新增
- doc:本 essay
- 价值观:
  - [[feedback-ceiling-first-priorities]] — 重定义 ceiling distance,从
    "deflate corpus 1.14× zopfli on cargo-lock" 到 "PNG IDAT 1.08×
    oxipng across 7 fixtures"。User-facing metric 才是真正的 ceiling
  - [[feedback-metric-over-human-eye]] — 不靠"看起来差不多",直接 measure
    IDAT byte size。13% gap 是机器看得见的,用户也看得见
  - [[feedback-no-cost-thinking]] — 不评估"phase 1.4 implementation 多复杂"
    或"是否值得",只 quote ceiling distance(8% corpus,13% photo)和 phase 1.4
    estimated close-to-target
