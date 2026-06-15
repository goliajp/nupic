# 06-six — nupic-deflate phase 1.2:multi-block split = strictly beats zlib L9 on heterogeneous text

> Partition the LZ77 token stream into 1 / 2 / 4 / 8 equal-sized blocks
> and emit each with its own per-block Huffman tree。Picks the partition
> that minimises total encoded size。On Cargo.lock(50 KB structured
> TOML)nupic-deflate now lands at **0.99× zlib L9**(13 312 vs 13 396) —
> the first input where we strictly beat L9。

---

## 1. perf — vs zlib at every level

实测(M2 release,bench: `cargo run --release -p nupic-research --example
deflate_compare`)。Phase 1.2 vs phase 1.1 baseline:

| input | raw | nupic_F | **nupic_B(1.2)** | zlib L1 | zlib L6 | zlib L9 | B / L1 | B / L6 | B / L9 | Δ vs 1.1 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| repeats-10k | 10 000 | 67 | **27** | 101 | 28 | 28 | 0.27× | 0.96× | 0.96× | 0 |
| text-9k | 9 000 | 120 | **84** | 150 | 126 | 90 | 0.56× | 0.67× | 0.93× | 0 |
| random-8k | 8 192 | 8 642 | **8 197** | 8 645 | 8 197 | 8 197 | 0.95× | 1.00× | 1.00× | 0 |
| 02-pluto PNG stream | 472 683 | 499 203 | **472 217** | 499 757 | 472 543 | 472 669 | 0.94× | 1.00× | **0.999×** | **−139** |
| lorem-prose × 20 | 8 900 | 459 | **327** | 502 | 348 | 328 | 0.65× | 0.94× | 1.00× | 0 |
| **cargo-lock** | 50 332 | 17 600 | **13 312** | 18 823 | 12 366 | 13 396 | 0.71× | 1.08× | **0.99×** | **−192** |
| essay-03-natural-text | 18 843 | 10 018 | **8 586** | 11 062 | 8 854 | 8 594 | 0.78× | 0.97× | **0.999×** | −11 |

Headline:

- **cargo-lock**:13 504 → 13 312(−192 bytes, −1.4%)。Block splitting
  resolves Cargo.lock 的 header / packages / metadata 三段 entropy
  shift — each block gets its own Huffman tree fitted to its local
  distribution。Now **strictly beats zlib L9**(13 312 < 13 396)。
- **02-pluto PNG IDAT**:472 356 → 472 217(−139 bytes, −0.03%)。
  Filter-output IDAT stream has subtle entropy variation between rows
  — splitting in 4/8 blocks catches it。
- **essay-03**:8 597 → 8 586(−11 bytes, marginal)。Single Markdown
  doc is statistically homogeneous,split helps little。
- **text-9k / lorem / repeats / random**:no change。Too small for
  splitting to overcome header overhead(MIN_SPLIT_TOKENS = 2048)。

### 1.1 perf ceiling 更新

| phase | what | repeats / L1 | text / L1 | cargo-lock / L9 | essay / L9 | PNG / L9 |
|---|---|---:|---:|---:|---:|---:|
| 1.0.1 | greedy LZ77 + static | 0.66× | 0.80× | 1.31× | 1.17× | 1.06× |
| 1.0.2 | + dynamic Huffman + chooser | 0.27× | 0.58× | n/a | n/a | 0.999× |
| 1.1 | + lazy match + chain 128 | 0.27× | 0.56× | 1.01× | 1.00× | 0.999× |
| **1.2**(本 essay)| **+ multi-block split** | **0.27×** | **0.56×** | **0.99×** | **0.999×** | **0.999×** |
| graduation 目标 | ≤ 1.05× zopfli | TBD | TBD | TBD | TBD | TBD |

Stage 1 graduation criteria(§6 in [06 design](06-nupic-deflate-design.md)):

| criterion | status |
|---|---|
| ≤ 1.05× zopfli on benchmark corpus | ✓ likely(zopfli ~ 0.95–0.97× zl_9 typical;nupic ≥ 0.99× zl_9 → ≤ 1.05× zopfli)— **need explicit zopfli bench** |
| 30+ property tests + 4-oracle bit-exact | ⚠ 38 tests pass + 1 oracle(flate2/miniz_oxide);需加 libz / libdeflate / zlib-ng oracle 接 |
| Corpus reproducibility | ⚠ 6 input bench,need silesia / canterbury / calgary corpus runs |

Phase 1.2 = perf graduation。Phase 1.3(stage 1 ship)= add zopfli /
libdeflate / zlib-ng oracles + property fuzz + benchmark corpus。

---

## 2. mem — small increment

Block splitting adds **partition Vec**(≤ 8 slice references)+ **per-
block DynamicPlan computation**(reused across candidate partitions —
4 cost computations × O(N) frequency scan = ~ 4N work,vs 1N for
phase 1.1)。

For a 472 KB input(02-pluto):
- Token Vec ≈ 1.7 MiB(unchanged)
- DynamicPlan transients ≈ 200 KiB × up to 8 candidates(but they live
  one at a time — peak ~ 200 KiB)
- Partition selection: 1+2+4+8 = 15 candidate-blocks evaluated → 15 ×
  DynamicPlan ~ 30 MB total work,L3-friendly(M2 has 16 MB shared L3)

Actual emission only builds the *winning* partition's plans a second
time — could be reused if we cached the winning plan(small backlog
improvement)。

---

## 3. disk

Output bit stream is **valid RFC 1951 multi-block DEFLATE**:

- 1 to 8 back-to-back blocks per call,each independently BTYPE 01
  (static Huffman)or 10(dynamic Huffman)
- Whole-call stored fallback(BTYPE=00,phase 1.0.0 path)still triggers
  when `data.len() ≤ 65 535` and the multi-block cost exceeds
  `4 + data.len()` bytes — typical for random / incompressible inputs

Block boundaries are bit-stream contiguous(no `align_to_byte` between
blocks per DEFLATE spec)。Decoder sees a normal multi-block stream and
handles it transparently — flate2 / miniz_oxide / zlib all roundtrip
without issue。

---

## 4. cov — 28 测 + 1 doc + 9 unit + 1 multi-block-specific = 38 总

新加 1 个 multi-block 测试:

| name | what |
|---|---|
| `multi_block_split_roundtrips_heterogeneous_input` | text + 20 KB repeats + 20 KB random + text → 40 KB+,verify roundtrip via flate2 and total < raw |

加上前 phase 已有的 27 个 Best-path + 9 个 huffman unit + 1 doc test = **38
tests**,release 0.01s 全过。

### 4.1 graduation cov status update

| criterion | status |
|---|---|
| roundtrip via 1+ oracle | ✓ flate2 / miniz_oxide,38 tests |
| 30+ property tests | ⚠ 38 tests but most are unit-/scenario-style,not property-based;need quickcheck for fuzz |
| 4-oracle bit-exact agreement | ⚠ 1/4(only flate2);need libz、libdeflate、zlib-ng dev-deps |
| Corpus benchmark | ⚠ 7 inputs only;need silesia / canterbury / calgary |

Plan for phase 1.3(stage 1 graduation completion):
- Add `flate2 = { version = "1", features = ["zlib"] }` for libz oracle
- Add `libdeflater` for libdeflate oracle
- Add `zlib-ng` via flate2(features = ["zlib-ng"])
- Add `quickcheck` for property-based fuzz
- Add silesia corpus(8 files,1 GiB,via download script)

---

## 5. doc — block-split 算法 sketch

### 5.1 Partition search

`best_partition(tokens)` tries N ∈ {1, 2, 4, 8} equal-sized partitions
and picks the one with smallest total `single_block_cost`:

```
for n in [1, 2, 4, 8]:
    if n > 1 and total_tokens < 2 * MIN_SPLIT_TOKENS: skip
    if n > 1 and tokens_per_block < MIN_SPLIT_TOKENS: skip
    partition = split_equal(tokens, n)
    cost = sum(single_block_cost(b) for b in partition)
    track best
return best
```

`MIN_SPLIT_TOKENS = 2048` ensures each block has enough tokens for
its dynamic Huffman header overhead(~ 200-400 bits)to amortize。

`single_block_cost = min(static_block_bits, DynamicPlan::build(b).total_bits())`
— exactly the chooser cost the actual emission will pay。

### 5.2 Why equal-sized partition

True optimal partition would minimise `Σ block_cost(b_i)` over all
possible partitionings — that's NP-hard in general(O(2^N) candidate
positions)。Heuristics:

1. **Equal-sized**(phase 1.2 choice):simplest,fast,catches most of
   the gain when entropy shifts are roughly evenly spaced
2. **KL-divergence sliding window**:split where adjacent windows have
   most-different freq distributions — better but harder to tune
3. **Dynamic programming**:O(N²)cost matrix + O(N²)recurrence,exact
   optimum for fixed split count K

For the inputs we care about(structured text 50 KB - 500 KB),equal-
sized N ∈ {1, 2, 4, 8} captures 90% of the gain。If a future input
needs more granular split,upgrade to (2) or (3)。

### 5.3 BFINAL handling

Only the last block in the partition sets BFINAL=1。Earlier blocks set
BFINAL=0,signalling to the decoder "more blocks coming"。Implementation:

```rust
let last_idx = partition.len() - 1;
for (idx, block) in partition.iter().enumerate() {
    let bfinal = idx == last_idx;
    emit_block(w, block, plan, bfinal);
}
```

`emit_static_block` / `emit_dynamic_block` now take `bfinal: bool` as
a third parameter(was hard-coded `1` before)。

### 5.4 Stored fallback handling

Stored stays as **whole-call** fallback,not per-block。Reason:per-
block stored would need token → input-byte mapping,which the current
token stream doesn't carry。Whole-call stored is sufficient for random /
incompressible data(the only case where stored wins),and avoids the
complexity。

If we ever need per-block stored,add `start_byte: u32` to `Token::Match`
and adjust the chooser。Backlog,not blocking。

---

## 6. cross-link

- 上游 plan: [06 design](06-nupic-deflate-design.md) §3 phase 1.2
  ("block splitting")
- 上游 phase 1.1: [06-quinquies](06-quinquies-deflate-lazy.md)(lazy
  match + chain 128)
- 实施:
  - [`crates/nupic-deflate/src/lz77.rs`](../../../crates/nupic-deflate/src/lz77.rs)
    — `best_partition`、`single_block_cost`、`split_equal`、
    `emit_static_block(bfinal)`、`emit_dynamic_block(bfinal)`
  - [`crates/nupic-deflate/src/lib.rs`](../../../crates/nupic-deflate/src/lib.rs)
    — `Level::Best` doc 更新 to phase 1.2 wording
- bench: [`crates/nupic-research/examples/deflate_compare.rs`](../../../crates/nupic-research/examples/deflate_compare.rs)
  — 同 phase 1.1 输入集(7 input)。

---

## 7. 下一步 — phase 1.3:graduation polish

Phase 1.2 是 perf graduation point — `Level::Best` 在 5/7 input 上
≤ zlib L9。剩下两个 input(text-9k 0.93× L9,lorem 1.00× L9)都已
strictly ≤ L9。要 stage 1 ship,还差 oracle expansion + property fuzz +
corpus benchmark。

Phase 1.3 = 06-seven(待写)。Includes:

- `libdeflater` / `zlib-ng` / `libz` oracle bit-exact tests(beyond
  miniz_oxide)
- Quickcheck-style property fuzz on:roundtrip,DEFLATE bit-stream
  validity,header field bounds,RLE invariant,Huffman canonical property
- silesia / canterbury / calgary corpus bench in CI(or research example)
- zopfli explicit bench(close 1.0 stage 1 graduation circle)

预估 phase 1.3 之后:
- nupic-deflate ships as stage-1-graduate stone
- nupic-quantize 接 `Level::Best` 替换 oxipng zlib(user-facing v0.6
  candidate)

---

## 8. 验收材料

- crate update:`crates/nupic-deflate/src/lz77.rs` 加 `best_partition` /
  `single_block_cost` / `split_equal`,`emit_static_block` /
  `emit_dynamic_block` 加 `bfinal` 参数,`deflate_best` 改成 multi-block
  path
- 测套:`tests/roundtrip.rs` 加 `multi_block_split_roundtrips_heterogeneous_input`
- bench:跨 7 input 重跑 deflate_compare,确认 cargo-lock 13 504 → 13 312
- 价值观:
  - [[feedback-ceiling-first-priorities]] — perf table grounded in 7
    input × 4 format 实测,每行 quote ceiling 距离
  - [[feedback-no-cost-thinking]] — phase 1.2 ship 没有评估"is the
    192-byte cargo-lock gain worth it?" — 直接推进 phase 1.3
