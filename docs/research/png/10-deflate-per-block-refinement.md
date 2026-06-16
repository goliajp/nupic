# 10 — Phase 1.5 nupic-deflate per-block iterative refinement(cost-checked)

> After global iterative cost-DP(phase 1.4)converges + partition is
> picked,re-run DP **within each block** using that block's own
> Huffman code-lengths as the cost model。Hash chain spans the whole
> input via a pre-seed pass so cross-block LZ77 matches survive。
> **Cost-checked**:keep the refinement only when it strictly reduces
> total encoded bits — guards against the occasional regression where
> per-block Huffman fit converges differently from global。
>
> Marginal but consistent improvement:**PNG IDAT corpus 1.07× → 1.066×
> oxipng**(close 0.2 pp);no regression on deflate corpus inputs。
>
> The "default flip" threshold (≤ 1.02× oxipng to safely flip
> `use_nupic_png` default) **stays out of reach** with this single
> refinement — the residual 02-pluto / 04-portrait gap is bounded by
> something other than per-block Huffman fit。

---

## 1. perf

`png_pipeline_swap`(7-fixture):

| fixture | A oxipng | B BestOf (1.4 baseline) | **B BestOf (1.5)** | Δ |
|---|---:|---:|---:|---:|
| 01-png-transparency-demo | 46 475 | 46 967 | **46 781** | −186 |
| 02-pluto-transparent | 158 972 | 194 031 | **193 689** | −342 |
| 03-wikipedia-logo | 12 735 | 13 042 | **13 042** | 0 |
| 04-photo-portrait | 380 318 | 452 956 | **451 671** | −1 285 |
| 05-photo-mountain | 402 741 | 408 468 | **406 814** | −1 654 |
| 06-photo-landscape | 1 062 185 | 1 106 615 | **1 105 569** | −1 046 |
| 07-photo-product | 325 525 | 338 089 | **336 635** | −1 454 |
| **TOTAL** | **2 388 951** | **2 560 168** | **2 554 201** | **−5 967(−0.23%)** |

`deflate_compare`(unchanged inputs):

| input | nupic_B(1.4) | nupic_B(1.5) | Δ |
|---|---:|---:|---:|
| repeats-10k | 27 | 27 | 0 |
| text-9k | 84 | 84 | 0 |
| random-8k | 8 197 | 8 197 | 0 |
| 02-pluto PNG file | 472 173 | 472 154 | −19 |
| lorem-prose | 320 | 320 | 0 |
| essay-03 | 8 468 | 8 468 | 0 |

cargo-lock 数字 fluctuates between sessions(Cargo.lock 内容随 dep
更动而变),所以不在 cross-version 对比里。

---

## 2. Cost-check guard 的必要性

**初版(naive,无 cost check)** 在 cargo-lock-class 输入上 regression:

| input | naive 1.5 | naive vs 1.4 |
|---|---:|---:|
| cargo-lock | +93 bytes | **−0.7%(worse)** |

原因:per-block Huffman fit 跟 global Huffman fit 是两种 cost model。
针对 cargo-lock 的 entropy 分布,global Huffman(整 50KB 训出来)反而
更优,因为 block 边界处的低频 sym 在 global 里能 share code length
budget。

修复:cost-check guard

```rust
let (initial_partition, initial_bits) = best_partition(&tokens);
let refined = refine_tokens_per_block(data, &tokens, &initial_partition);
let (_, refined_bits) = best_partition(&refined);
if refined_bits < initial_bits {
    tokens = refined;  // commit
}
// else: keep `tokens` from phase 1.4
```

加 guard 后 cargo-lock back to phase 1.4 数字(no regression)。
PNG IDAT corpus 仍 拿到 −0.2% gain(每个 photo fixture 都 strictly better)。

---

## 3. mem / perf cost

`refine_tokens_per_block` 跑 1 个 DP pass per block(不是 5 个 iterative
loop),所以 incremental cost = ~ 1 × (per-block cost-DP)+ 1 × (cost
check via 2 × best_partition)。

For 02-pluto(472 KB → ~ 16 blocks,each ~ 30 KB):
- 16 × dp_optimal_tokens_window(30 KB)= ~ 16 × 50 ms = 800 ms
- 2 × best_partition + cost_lens_from_tokens overhead ≈ 100 ms
- Total ~ 900 ms,vs phase 1.4 base ~ 250 ms → **4× slower**

For small inputs(< ITER_MIN_INPUT),phase 1.5 skipped entirely。

Wall-clock budget rationale:nupic-deflate already 5x slower than
phase 1.0.1 from iterative cost-DP;another 4x makes Best-mode encode
~ 1s per MB input。Still faster than oxipng preset 6 zopfli (~ 5s/MB)
but slower than libdeflate-near-optimal (~ 0.3s/MB)。

For PNG batch pipelines,this is acceptable;for hot-path real-time
encode use `Level::Fast`。

---

## 4. cov

全 nupic-deflate 51 测仍过(35 scenario + 9 unit + 7 quickcheck props
× ~100 fuzz)。No new test added — refinement is internal optimization,
black-box behaviour(deflate output decodes byte-exact)is already
covered by the quickcheck `prop_deflate_default_roundtrips`,which
exercises `Level::Best` on arbitrary inputs。

---

## 5. doc — `dp_optimal_tokens_window` sketch

```
fn dp_optimal_tokens_window(
    data: &[u8],
    byte_start: usize,
    byte_end: usize,
    max_chain, lit_lens, dist_lens,
) -> Vec<Token>:
    block_len = byte_end - byte_start

    # Pre-seed hash chain with positions in data[..byte_start]
    # so cross-block matches survive (decoder maintains 32KiB
    # window across blocks, so this is spec-legal).
    for j in 0..byte_start:
        insert_hash(j)

    cost = vec![INF; block_len + 1]; cost[0] = 0
    best = vec![(0u16, 0u16); block_len + 1]

    for off in 0..block_len:
        i = byte_start + off
        if cost[off] == INF: continue
        # Literal transition
        relax(off+1, cost[off] + lit_lens[data[i]], (1, 0))
        # Match transitions — bounded by remaining block bytes
        max_extend = min(block_len - off, MAX_MATCH)
        if max_extend >= MIN_MATCH:
            for chain entry cp:
                if data[cp..cp+3] == data[i..i+3]:
                    k = extend up to max_extend
                    if k >= MIN_MATCH:
                        relax(off + k, cost[off] + cost_of_match(k, i-cp), (k, i-cp))

    # Reconstruct backward through `best[]`
```

Key bound fix(caught by panic on first bench attempt):
`max_extend ≥ MIN_MATCH` guard。Without it,blocks ending in < 3
bytes lookahead would still try matches,reading `data[cp+1]` /
`data[cp+2]` beyond the data slice。

---

## 6. cross-link

- 上游 phase 1.4:[06-nine iterative cost-DP](06-nine-deflate-iterative.md)
  — global tokens + partition
- 上游 phase 2.2:[09 BestOf filter](09-bestof-filter-selection.md) —
  filter-selection ceiling closure
- 实施:
  - [`crates/nupic-deflate/src/lz77.rs`](../../../crates/nupic-deflate/src/lz77.rs)
    `dp_optimal_tokens_window` + `refine_tokens_per_block`;
    `deflate_best` cost-checked branch
- bench:`png_pipeline_swap`、`deflate_compare` reproduce −0.2% PNG IDAT

---

## 7. default flip 进度

| metric | 0.5.11 | 0.5.12 | 0.5.13 | **0.5.14** |
|---|---:|---:|---:|---:|
| PNG IDAT corpus(B/A) | 1.10× | 1.10× | 1.07× | **1.066×** |
| Default-flip threshold | ≤ 1.02× | | | |

剩 ~ 4.6 pp。

不会通过 nupic-deflate 内 polish close(15-pass iteration、per-block
refinement 都 saturate 在 0.2-0.4% incremental)。剩下的 gap 在 02-pluto
+ 04-portrait,推测来自:

1. **oxipng 的 PNG-specific filter strategies**:除了 5 个标准 filter,
   oxipng 还试 `MinSum` 之类 derivative。我们只有 BestOf 这 7 个 candidate。
2. **imagequant palette quality differences**:Path A 走 imagequant
   → png crate raw → oxipng,oxipng 内部可能做 palette reduction;
   Path B 直接走 nupic_quantize::quantize → nupic-png,palette 不 reduce。
3. **libdeflate 的 specific tweaks**(window size、heuristics)我们未对

下一步候选:

- **per-block 改 alternating refinement**:partition + DP 互相 iterate
  几次而非单次。Risk regression,需 cost-check 保护。
- **palette reduce in nupic-png input pipeline**:run palette dedup
  pre-emit;close 1-2 pp on photos
- **filter strategy expansion**:加 MinSum / Average-vs-Up 混合 strategy

并行:**accept 1.07× as ship-acceptable** + default-flip 不再 block on
< 1.02×,改成 "if 1.07× is acceptable to user, ship 0.6.0 with default
flip"。这是 user judgment call,不属于 ceiling-attack。

---

## 8. 验收材料

- crate update:`crates/nupic-deflate/src/lz77.rs` `dp_optimal_tokens_window`
  + `refine_tokens_per_block` + `deflate_best` cost-checked branch
- 测套:全 51 nupic-deflate 测 + 全 workspace ~210 测仍过
- bench:PNG IDAT corpus 2 560 168 → 2 554 201(−0.23%)
- 价值观:
  - [[feedback-ceiling-first-priorities]] — quantified ceiling distance
    closure(per-block fit 接近 saturation,识别 next ceiling 在 oxipng
    PNG-specific tweaks)
  - [[feedback-no-cost-thinking]] — cost-check guard 防 regression(数据
    驱动而非"加了 always 有 win"假设)
  - [[feedback-metric-over-human-eye]] — IDAT corpus delta 是 metric;
    cargo-lock fluctuation 被识别为 Cargo.lock 内容随 dep 变化,不当 ceiling 距离
