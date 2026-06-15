# 06-nine — nupic-deflate phase 1.4:iterative cost-DP LZ77 = full stage-1 graduation

> Variable-position block split(phase 1.4a)+ **5-pass forward-DP token
> search with Huffman code-length cost feedback**(phase 1.4b — the
> zopfli core trick)。`Level::Best` now lands at:
>
> - **All 7 corpus inputs ≤ 1.05× zopfli**(stage-1 graduation 满足)
> - cargo-lock 1.14× → **1.01×** zopfli(close 14% gap to 1%)
> - PNG IDAT 7-fixture corpus 1.08× → **1.04×** oxipng(libdeflate
>   near-optimal)— half the gap to oxipng closed
>
> PNG-pipeline integration unblocked at the corpus level(02-pluto / 04-
> portrait still have 6-9% IDAT gap which 15+ passes only shaves 0.2%
> further — diminishing returns,saved for phase 1.5)。

---

## 1. perf — full deflate_compare(stage-1 graduation criteria check)

实测(M2 release):

| input | raw | nupic_F | **nupic_B(1.4)** | zlib L1 | zlib L6 | zlib L9 | zopfli | B / L9 | **B / zopfli** | Δ vs 1.3 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| repeats-10k | 10 000 | 67 | **27** | 101 | 28 | 28 | 27 | 0.96× | **1.00×** ✓ | 0 |
| text-9k | 9 000 | 120 | **84** | 150 | 126 | 90 | 83 | 0.93× | **1.01×** ✓ | 0 |
| random-8k | 8 192 | 8 642 | **8 197** | 8 645 | 8 197 | 8 197 | 8 197 | 1.00× | **1.00×** ✓ | 0 |
| 02-pluto PNG stream | 472 683 | 499 203 | **472 173** | 499 757 | 472 543 | 472 669 | 471 924 | 1.00× | **1.00×** ✓ | −44 |
| lorem-prose × 20 | 8 900 | 459 | **320** | 502 | 348 | 328 | 310 | 0.98× | **1.03×** ✓ | **−7** |
| **cargo-lock** | 57 897 | 20 118 | **13 421** | 21 541 | 14 141 | 15 345 | 13 344 | 0.87× | **1.01×** ✓ | **−1 808** |
| essay-03-natural-text | 18 843 | 10 018 | **8 468** | 11 062 | 8 854 | 8 594 | 8 416 | 0.99× | **1.01×** ✓ | **−110** |

**Stage 1 graduation criterion = `Level::Best ≤ 1.05× zopfli` on benchmark
corpus** — 7/7 strict pass。Phase 1.3 was 5/7;phase 1.4 closes the
remaining 2(cargo-lock, lorem)。

cargo-lock 是 phase 1.3 essay 06-seven identified gap source(structured
text with non-trivial entropy shifts)— iterative DP with cost feedback
finds the LZ77 token sequence that minimises *output bits* directly,
rather than maximising match length。Reduction 11% (15 229 → 13 421)。

---

## 2. PNG IDAT corpus — close to oxipng

bench: `cargo run --release -p nupic-research --example png_idat_swap`

| fixture | old_IDAT(oxipng)| **nupic_B(1.4)** | **B / old** | Δ vs 1.3 |
|---|---:|---:|---:|---:|
| 01-png-transparency-demo | 48 998 | 49 683 | **1.01×** | **−3 831** |
| 02-pluto-transparent | 161 244 | 175 487 | 1.09× | **−6 430** |
| 03-wikipedia-logo | 12 829 | 12 836 | **1.00×** | **−483** |
| 04-photo-portrait | 379 907 | 403 254 | 1.06× | **−25 880** |
| 05-photo-mountain | 401 904 | 410 823 | 1.02× | **−30 140** |
| 06-photo-landscape | 1 061 348 | 1 093 193 | 1.03× | **−9 945** |
| 07-photo-product | 324 964 | 339 628 | 1.05× | **−21 901** |
| **TOTAL** | **2 391 194** | **2 484 904** | **1.04×** | **−99 208** |

Phase 1.3 was 1.08× oxipng corpus,phase 1.4 是 **1.04×** — close half
the gap。5/7 fixtures 现在 within 5% of oxipng;02-pluto / 04-portrait
仍有 6-9% gap(15-pass iteration 只 close 额外 0.2% — diminishing
returns,留 phase 1.5 改 per-block iterative + length-symbol-boundary
search)。

**Integration readiness**:phase 1.4 已让 PNG IDAT swap 从 8% regression
降到 4%。Acceptable for ship-as-experimental(`Quality::Auto` opt-in
flag)或 wait phase 1.5 to fully close。

---

## 3. mem

新加 cost-DP 数据结构,per-pass:
- `cost: Vec<u32>` of size N+1 = 4N bytes
- `best: Vec<(u16, u16)>` of size N+1 = 4N bytes
- hash_head + hash_prev = 256 KiB(reset per pass)
- previous-pass `lit_lens: [u8; 286]`、`dist_lens: [u8; 30]` — trivial

For 472 KB pluto input:
- cost+best ≈ 4 MiB
- hash tables ≈ 256 KiB
- per-pass total ≈ 4.3 MiB,reused across 5 passes
- Token Vec ≈ 1.7 MiB(post-DP)

L2-friendly per pass(M2 has 16 MiB shared L3,cluster L2 4 MiB)。

---

## 4. perf cost(wall-clock)

Single-pass lazy → 5-pass iterative changes:

| input | size | lazy-only ms | iterative-5 ms | ratio |
|---|---:|---:|---:|---:|
| 02-pluto IDAT | 472 KB | ~ 50 ms | ~ 250 ms | 5× |
| cargo-lock | 58 KB | ~ 8 ms | ~ 40 ms | 5× |

5× slowdown is the natural cost of 5 iterations。15 iterations would
be 15×;diminishing-returns analysis(15-pass IDAT corpus is 2 479 956
vs 5-pass 2 484 904,−0.2%)justifies stopping at 5。

User-facing impact:**PNG encode path adds ~ 200ms per MB of input**。
For typical 1 MB PNG that's 200ms vs current oxipng's ~ 500ms。Net
*faster* and tighter compression。

---

## 5. cov

35 scenario + 9 unit + 7 quickcheck property × ~100 random = **~ 700
distinct verifications per run**(unchanged from 1.3,all still pass)。
`prop_best_never_loses_to_fast` covers the iterative path automatically
since it uses `Level::Best`。

---

## 6. doc — iterative cost-DP algorithm sketch

### 6.1 Forward DP token search

```
state:
  cost[i] = min bits to encode data[0..i]
  best[i] = (length-covering-to-i, distance);
            distance=0 ⇒ literal step (length=1)
  hash_head / hash_prev — re-initialised per pass

for i in 0..n:
    if cost[i] == INF: continue
    # Literal transition
    relax(cost, best, i + 1, cost[i] + lit_lens[data[i]], (1, 0))
    # Match transitions
    if i + MIN_MATCH ≤ n:
        for chain entry cp at hash_head[hash3(data[i..])]:
            if data[cp..cp+3] == data[i..i+3]:
                k = extend as far as possible up to MAX_MATCH
                target = i + k
                cost' = cost[i] + cost_of_match(k, i-cp,
                                                lit_lens, dist_lens)
                relax(cost, best, target, cost', (k, i-cp))
        insert hash for position i

# Reconstruct
walk backward from n via best[]
```

`relax(cost, best, j, new_cost, parent)` = `if new_cost < cost[j] {
cost[j] = new_cost; best[j] = parent; }`。Single u32 per position,
no DAG explicit。

### 6.2 Multi-pass refinement

```
tokens = dp_optimal(data, max_chain, STATIC_LIT_LENS, STATIC_DIST_LENS)
for pass in 1..ITER_PASSES:
    lit_lens, dist_lens = build_huffman_lens(tokens)
    next = dp_optimal(data, max_chain, lit_lens, dist_lens)
    if tokens == next: break  # converged
    tokens = next
return tokens
```

Cost model evolves:pass 0 = static Huffman(naive);pass 1+ uses actual
freq-fitted Huffman from previous pass。Typically converges in 3-5
passes;hardcoded max 5 to bound wall-clock。

### 6.3 Unused-symbol cost model

`build_huffman_lens` returns 0 for unused symbols,but the DP cost model
needs a *defined* cost for every symbol(else cost-DP may "discover"
new ways to use unused symbols and produce zero-cost paths)。Fix:
unused symbols get **MAX_LIT_LEN_BITS = 15**(natural maximum),so the
cost-DP treats them as expensive but well-defined。

### 6.4 Length-symbol boundary variants(deferred)

Each chain entry's max-extend length `k` could be split into multiple
candidates at length-symbol boundaries(11, 13, 15, …, 258)to consider
"shorter match with cheaper length symbol"。Adds ~ 5× DP work per pass
for ~ 0.5% extra compression。Defer to phase 1.5。

### 6.5 Variable-position block split(phase 1.4a)

`best_partition` augmented with greedy bisection:try 7 evenly-spaced
candidate split positions inside each block;commit if total cost
drops below baseline;recurse on each half。Complementary to the
existing equal-sized {1,2,4,8} search — picks whichever is smaller。

On its own gives 0.1-0.3% gain。Combined with iterative refinement,
helps when iterative finds different optimal block boundaries。

---

## 7. cross-link

- 上游 plan: [06 design](06-nupic-deflate-design.md) §3 phase 1.4
  ("block splitting + iterative refinement")
- 上游 phase 1.3: [06-seven](06-seven-deflate-graduation.md)
  (fuzz + zopfli oracle,identified cargo-lock 1.14× zopfli gap)
- 上游 PNG integration readiness:[06-eight](06-eight-png-integration-readiness.md)
  (identified 1.08× oxipng gap as the user-facing blocker)
- 实施:
  - [`crates/nupic-deflate/src/lz77.rs`](../../../crates/nupic-deflate/src/lz77.rs)
    — `dp_optimal_tokens`、`cost_of_match`、`cost_lens_from_tokens`、
    `collect_tokens_iterative`、`variable_split_recursive`、constants
    `ITER_CHAIN=512`、`ITER_PASSES=5`、`STATIC_LIT_LENS`
  - [`crates/nupic-deflate/src/lib.rs`](../../../crates/nupic-deflate/src/lib.rs)
    — module doc to phase 1.4
- bench(unchanged inputs,both updated to read post-1.4 numbers):
  - [`crates/nupic-research/examples/deflate_compare.rs`](../../../crates/nupic-research/examples/deflate_compare.rs)
  - [`crates/nupic-research/examples/png_idat_swap.rs`](../../../crates/nupic-research/examples/png_idat_swap.rs)

---

## 8. 下一步 — phase 1.5 / integration

Phase 1.4 ships stage-1 graduation。剩下两条路径:

### Path A: nupic-deflate phase 1.5(IDAT polish)
Close 02-pluto / 04-portrait residual 6-9% gap to oxipng:
- **Per-block iterative refinement**:current code does global cost
  Huffman across whole input;zopfli iterates per-block(after block
  split)→ tighter fit
- **Length-symbol boundary search in DP**:as noted §6.4
- **Increase ITER_PASSES adaptively**:large inputs benefit from more
  passes

预估 phase 1.5 之后:PNG IDAT corpus 1.04× → ~ 1.01× oxipng(zopfli-class
ceiling)。

### Path B: nupic-deflate → PNG pipeline integration(0.6.x ship)
Now feasible since 1.04× oxipng is "release-acceptable":
- Replace `oxipng::optimize_from_memory(raw, ...)` call with a new
  `nupic_png::optimize(raw, ...)` path
- Need PNG filter try-all rewrite(per-row filter selection)+ chunk
  walker + nupic-deflate IDAT compression
- Removes oxipng dep tree(libdeflate + zopfli + others)from nupic-core

Path B 可以 ship as-is(1.04× regression on average)或 wait-for-1.5 then
ship at parity。

**优先级**:看用户决定 — Path A 是 stone polish(deflate ceiling),
Path B 是 user-facing ship(integration)。

并行 backlog:
- libdeflate / zlib-ng decoder oracle(close 4-oracle gap)
- silesia / canterbury corpus reproducibility
- nupic-bits NEON pclmul CRC32(close 4× perf gap)
- 03d Stone D adaptive light dither(05/06 -1.67/-0.62 SSIM gap)

---

## 9. 验收材料

- crate update:
  - `crates/nupic-deflate/src/lz77.rs` 加 5 个新函数 + 6 个 constants
    (`STATIC_LIT_LENS`、`STATIC_DIST_LENS`、`ITER_CHAIN=512`、
    `ITER_PASSES=5`、`ITER_MIN_INPUT=1024`、`MIN_SPLIT_TOKENS` 抽出)
  - `crates/nupic-deflate/src/lib.rs` 模块 doc + `Level::Best` doc 更新
    to phase 1.4
- 测套:35 scenario + 9 unit + 7 quickcheck property × ~100 random
  fuzz(全 unchanged from 1.3,iterative path 由 prop_best_never_loses_to_fast
  自动覆盖)
- bench:
  - deflate_compare 7/7 ≤ 1.05× zopfli(stage-1 graduation criterion)
  - png_idat_swap corpus 1.04× oxipng(close half the 1.08× gap)
- 价值观:
  - [[feedback-ceiling-first-priorities]] — stage-1 graduation ceiling
    achieved on every corpus input
  - [[feedback-no-cost-thinking]] — phase 1.4 没评估"iterative DP 5× slowdown
    是否值得",只 quote ceiling closure(8% → 4%)
  - [[feedback-metric-over-human-eye]] — 全靠 bench number 驱动 stop point
    (5 vs 15 iterations:15 给 −0.2%,5 ship)
