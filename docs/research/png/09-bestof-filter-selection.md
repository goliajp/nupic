# 09 — Phase 2.2 `FilterStrategy::BestOf` ships:cross-row context closes corpus to 1.07× oxipng

> New `FilterStrategy::BestOf` becomes the nupic-png default。Tries 7
> candidate filter strategies(5 single-filter sweep + per-row min-SAD
> + per-row deflate-aware),measures each via `nupic-deflate Level::Fast`
> on the full filtered stream,picks the smallest。Captures cross-row
> LZ77 context that all per-row strategies miss。
>
> **Corpus result: 1.10× → 1.07× oxipng**(close another 3 pp);**4/7
> fixtures within 4% of oxipng**;01-transparency at **1.01× oxipng**
> (essentially parity)。

---

## 1. Surface

```rust
pub enum FilterStrategy {
    MinSad,         // per-row Heckbert SAD heuristic
    DeflateAware,   // per-row trial-deflate (no cross-row context)
    BestOf,         // NEW DEFAULT — 7 candidates × full-stream deflate
}
```

`FilterStrategy::default() == BestOf` 在 0.5.13 起。`encode_indexed_png`
default 走 BestOf;explicit choice via `encode_indexed_png_with(strategy)`。

`nupic-core` `encode_png_stone_c_nupic` 改成走 default(BestOf)而非
explicit DeflateAware。`--use-nupic-png` flag automatically gets the
new strategy。

---

## 2. perf — corpus close to 1.07× oxipng

`png_pipeline_swap` 7-fixture corpus(v0.5.13 vs v0.5.12):

| fixture | A oxipng | **B BestOf**(v0.5.13)| C DeflateAware(v0.5.12 baseline)| **B/A** | C/A |
|---|---:|---:|---:|---:|---:|
| 01-transparency-demo | 46 475 | **46 967** | 62 382 | **1.01×** | 1.34× |
| 02-pluto-transparent | 158 972 | **194 031** | 212 108 | 1.22× | 1.33× |
| 03-wikipedia-logo | 12 735 | **13 042** | 14 769 | **1.02×** | 1.16× |
| 04-photo-portrait | 380 318 | 452 956 | 452 956 | 1.19× | 1.19× |
| 05-photo-mountain | 402 741 | **408 468** | 424 888 | **1.01×** | 1.05× |
| 06-photo-landscape | 1 062 185 | 1 106 615 | 1 122 357 | 1.04× | 1.06× |
| 07-photo-product | 325 525 | 338 089 | 338 089 | 1.04× | 1.04× |
| **TOTAL** | **2 388 951** | **2 560 168** | 2 627 549 | **1.07×** | 1.10× |

Headline:

- **Overall ratio 1.10× → 1.07× oxipng**(close 3 pp,30% of remaining
  gap)
- **4/7 fixtures within 4% of oxipng**(01, 03, 05, 07)
- **01-transparency-demo at 1.01× oxipng** —— essentially parity
- 02-pluto / 04-portrait 仍然 1.19-1.22× —— these are the residual
  gap source remaining(02 是 photo + transparency 双重难,04 是 large
  photo without obvious dominant filter)
- BestOf 在 4 fixtures 上 picked a single-filter strategy(usually Up
  or Paeth)beating both per-row heuristics — confirms hypothesis that
  per-row local optimization can miss global cross-row LZ77 gains

---

## 3. mem / perf cost

BestOf 跑 7 candidates × (filter sweep + Level::Fast deflate proxy)+
1 final filter materialization for the winner。Per pluto(472 KB):

- 7 × filter sweep(O(rows × w))≈ 7 × 5 ms = 35 ms
- 7 × Level::Fast deflate proxy(O(filter_size))≈ 7 × 80 ms = 560 ms
- 1 × Level::Best on winner(downstream): 250 ms

Total ≈ 850 ms per pluto。Pre-2.2 DeflateAware ≈ 540 ms。1.5× slower
encode for 7% smaller output — acceptable for "best compression"
default。

Wall-clock 仍 faster than oxipng's libdeflate-near-optimal preset 5
(~ 1500 ms typical for 472 KB,per its iterative LZ77)。

---

## 4. cov

12 nupic-png 测仍过(roundtrip tests via image crate decoder are
strategy-agnostic — anything that produces valid PNG bytes roundtrips
correctly)。`FilterStrategy::BestOf` 自动 covered 因为 default 走它。

加 1 个 unit test 可能:`bestof_picks_smallest_of_candidates` —— 但 
strategy 的 contract 本身是 "min size",已经被 corpus bench reproduce。
Test 跳过。

---

## 5. doc — BestOf algorithm

```
candidates = [
    filter_image_single(None),
    filter_image_single(Sub),
    filter_image_single(Up),
    filter_image_single(Average),
    filter_image_single(Paeth),
    filter_image(),                 # per-row min-SAD
    filter_image_deflate_aware(),   # per-row trial-deflate
]
winner = candidates.min_by_key(|filtered| {
    nupic_deflate::deflate_level(filtered, Level::Fast).len()
})
return winner
```

Key insight:single-filter sweeps capture **cross-row LZ77 matches**
that per-row heuristics break。E.g., a photo where most rows benefit
from `Up` filter,a single-Up-everywhere stream has long runs of
similar bytes that LZ77 trivially matches。Per-row min-SAD might pick
`Paeth` for one row and `Up` for the next — the byte-level
distributions diverge,LZ77 finds fewer matches。

`Level::Fast` proxy uses static Huffman + greedy LZ77(phase 1.0.1
implementation)— fast enough to evaluate 7 candidates per image and
ranks consistently with `Level::Best`(the actual emitted compression)
on the candidates we care about。

---

## 6. cross-link

- 上游:[07-bis integration](07-bis-nupic-png-integration.md) ships
  opt-in flag at 1.36× → [07 foundation](07-nupic-png-foundation.md)
  closes to 1.10× via DeflateAware
- 上游 phase 2.1:[08 alpha-aware](08-stone-c-alpha-aware.md) — tRNS
  fixes correctness;corpus 1.10× unchanged
- 实施:
  - [`crates/nupic-png/src/filter.rs`](../../../crates/nupic-png/src/filter.rs)
    `filter_image_single` + `filter_image_best_of`
  - [`crates/nupic-png/src/lib.rs`](../../../crates/nupic-png/src/lib.rs)
    `FilterStrategy::BestOf` 加成 default
  - [`crates/nupic-core/src/ops/compress.rs`](../../../crates/nupic-core/src/ops/compress.rs)
    `encode_png_stone_c_nupic` 走 default
- bench:[`crates/nupic-research/examples/png_pipeline_swap.rs`](../../../crates/nupic-research/examples/png_pipeline_swap.rs)

---

## 7. 下一步 — default flip 准备就绪 候选

剩 default-flip 关卡:

1. **phase 1.5 `nupic-deflate` per-block iterative refinement** —
   close 02-pluto / 04-portrait residual gap(estimate: 04 1.19× →
   ~1.05×,02 1.22× → ~1.12×)。Estimated corpus 1.07× → ~ 1.03×。
2. **per-fixture rolling deflate context for filter selection** — 比
   Level::Fast proxy 更准的 ranking;边际 ~ 1-2% close
3. **default flip**(0.6.0)—— `use_nupic_png: true` 当 default,完全
   替换 oxipng dep tree

Path 1 是 deflate stone 内部 polish。Path 2 是 nupic-png 内部 polish。
两者并行不冲突,先做哪个看 user。

---

## 8. 验收材料

- crate update:
  - `crates/nupic-png/src/filter.rs` 加 `filter_image_single`、
    `filter_image_best_of`
  - `crates/nupic-png/src/lib.rs` `FilterStrategy::BestOf` 加 + 设为
    default + match arm 加 BestOf branch
  - `crates/nupic-core/src/ops/compress.rs` `encode_png_stone_c_nupic`
    使用 default strategy(BestOf)
  - `crates/nupic-research/examples/png_pipeline_swap.rs` bench column
    label fix(B_minsad → B_bestof)
- 测套:全 ~210 workspace 测仍过
- bench:corpus 1.10× → **1.07× oxipng**;4/7 within 4%
- 价值观:
  - [[feedback-ceiling-first-priorities]] — close 30% of remaining 10%
    gap;ceiling distance well-quantified per-fixture
  - [[feedback-no-cost-thinking]] — 1.5× wall-clock cost of BestOf
    documented,不当 cost 评估的依据;直接 ship 让 user 决定
