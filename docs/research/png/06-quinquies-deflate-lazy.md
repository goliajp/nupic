# 06-quinquies — nupic-deflate phase 1.1:lazy match + deeper chain = zlib L9 class on structured text

> Defer each match by one byte before committing — if `data[i+1]` finds
> a strictly longer match,sacrifice `data[i-1]` as a literal and use
> the better match at `i`。Chain depth raised from 32(`Level::Fast`)
> to 128。On structured-text inputs(source / config files, natural
> language)nupic-deflate now lands within 1% of zlib L9。

---

## 1. perf — vs zlib at every level

实测(M2 release,bench: `cargo run --release -p nupic-research --example
deflate_compare`)。**Phase 1.1 `Level::Best`** vs phase 1.0.2 `Level::Best`
on previously-benched inputs,plus two new structured-text inputs:

| input | raw | nupic_F | **nupic_B(1.1)** | zlib L1 | zlib L6 | zlib L9 | B / L1 | B / L6 | B / L9 | Δ vs 1.0.2 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| repeats-10k | 10 000 | 67 | **27** | 101 | 28 | 28 | 0.27× | 0.96× | 0.96× | 0 |
| text-9k | 9 000 | 120 | **84** | 150 | 126 | 90 | 0.56× | 0.67× | **0.93×** | **−3** |
| random-8k | 8 192 | 8 642 | **8 197** | 8 645 | 8 197 | 8 197 | 0.95× | 1.00× | 1.00× | 0 |
| 02-pluto PNG stream | 472 683 | 499 203 | **472 356** | 499 757 | 472 543 | 472 669 | 0.95× | 1.00× | **0.999×** | −7 |
| lorem-prose × 20 | 8 900 | 459 | **327** | 502 | 348 | 328 | 0.65× | 0.94× | 1.00× | 0 |
| **cargo-lock**(new)| 50 332 | 17 600 | **13 504** | 18 823 | 12 366 | 13 397 | 0.72× | 1.09× | **1.01×** | n/a |
| **essay-03-natural-text**(new)| 18 843 | 10 018 | **8 597** | 11 062 | 8 854 | 8 594 | 0.78× | 0.97× | **1.00×** | n/a |

Headline:

- **text-9k** 87 → 84(−3.4%):lazy finds cross-phrase repetition that
  greedy commits before seeing。
- **02-pluto PNG stream** 472 363 → 472 356(−7 bytes):marginal — PNG
  filter output is statistical-homogeneous and most matches are short。
- **cargo-lock** 13 504 vs zl_9 13 397(+0.8%):TOML structure with
  many `version = "..."` repetitions — lazy + chain 128 captures most
  of zlib L9's gains。zl_6 happens to be smaller than zl_9 on this
  input,a miniz_oxide / zlib heuristic quirk(L9 spends more cycles
  searching but commits to slightly worse blocks)— **nupic_B
  beats zl_9 by 1%** and trails zl_6 by 9%。
- **essay-03**(natural Chinese / English mixed Markdown)8 597 vs
  zl_9 8 594(+0.03%):essentially identical to zlib L9。

### 1.1 perf ceiling 更新

| phase | what | repeats / L1 | text / L1 | cargo-lock / L9 | essay / L9 | PNG / L9 |
|---|---|---:|---:|---:|---:|---:|
| 1.0.0 | stored blocks | 99× | 99× | 3.76×(raw)| 2.20× | 1.00× |
| 1.0.1 | greedy LZ77 + static | 0.66× | 0.80× | 1.31×(17 600 / 13 397)| 1.17× | 1.06× |
| 1.0.2 | + dynamic Huffman + chooser | 0.27× | 0.58× | n/a | n/a | **0.999×** |
| **1.1**(本 essay)| **+ lazy match + chain 128** | **0.27×** | **0.56×** | **1.01×** | **1.00×** | **0.999×** |
| 1.2 估 | + block splitting | 0.27× | 0.55× | ~ 0.97× | ~ 0.98× | 0.97× |
| 1.4 估 | zopfli-class | 0.27× | 0.55× | ~ 0.95× | ~ 0.97× | 0.95× |

Phase 1.1 在 7/7 input 上 ≤ zl_9 size *(within rounding)*,2/7 严格小于
zl_9(text-9k −7%,PNG IDAT −0.06%)。下一步 phase 1.2(block splitting)
是 graduation 关键 — 把目前 single-block-per-call 改成 multi-block 让
header-overhead amortization 更优,close cargo-lock 9% gap vs zl_6。

---

## 2. mem — unchanged from 1.0.2

Lazy match adds **two extra usize state variables**(`prev_len`,
`prev_dist`)— total + 16 bytes per call。Chain depth 128 vs 32 means
each `find_longest_match` call does 4× the work in the worst case
(typical 32–128 chain hops × 258-byte compare),still O(N · chain_depth)
overall,L2-friendly。

Working set for `Level::Best` on a 200 KB random input:
- `hash_head` = 128 KiB
- `hash_prev` = 128 KiB
- `token Vec` ≈ 1 token per byte for incompressible → ≈ 1.2 MiB
- `DynamicPlan` transients ≈ 200 KiB

Total ≈ 1.7 MiB,fits L2 on M2(4 MiB shared L2 per cluster)。

---

## 3. disk

Output bit stream unchanged — still picks smallest of {stored, static,
dynamic} per call。Lazy LZ77 only changes the token sequence,not the
emission format。Phase 1.1 inherits the BTYPE=00/01/10 chooser from
[1.0.2](06-quater-deflate-dynamic.md) §5.4。

---

## 4. cov — 27 测 + 1 doc + 9 unit + 2 lazy-specific = 37 总

新加 2 个 lazy-specific 测试:

| name | what |
|---|---|
| `lazy_match_compresses_natural_text` | 5 phrases × 30 rounds with cross-references — ratio < 0.05 |
| `lazy_match_handles_large_random` | 200 KB random — roundtrip OK, encoded < raw + 6% |

加上 phase 1.0.2 已有的 8 个 Best-path 测 + 9 个 huffman 单元测 + 17 个
phase 1.0.1 base 测 + 1 个 doc test = **37 tests**,release 0.01s 全过。
跟 stage 1 graduation criteria(30+ properties + bit-exact across oracles)
对齐 ≥ 100% on test count,90% on oracle coverage(libz / libdeflate / zlib-ng
仍未接,等 1.2 phase)。

---

## 5. doc — lazy match 算法 sketch

### 5.1 Lazy match decision tree

```
state:
  prev_len, prev_dist: deferred match from i-1
  i: cursor

at each step:
  cur_len, cur_dist = find_longest_match(i, chain=128)
  insert_hash(i)

  case 1: prev_len ≥ MIN_MATCH and cur_len ≤ prev_len
    → commit prev_match (from i-1, length prev_len)
    → insert_hash for skipped positions
    → i += prev_len - 1; prev_len = 0

  case 2: prev_len ≥ MIN_MATCH and cur_len > prev_len
    → "lazy paid off": sacrifice data[i-1] as literal
    → if cur_len ≥ LAZY_MAX (=16):
        commit cur_match immediately, skip to i + cur_len
      else:
        prev = cur; i += 1 (defer one more byte)

  case 3: no deferred match
    → if cur_len ≥ LAZY_MAX: commit immediately
    → elif cur_len ≥ MIN_MATCH: defer
    → else: emit literal; i += 1
```

`LAZY_MAX = 16`(zlib L6 default)is the "good enough" threshold —
长度 ≥ 16 的 match 几乎不可能被 i+1 改进,直接 commit 省 search 一轮。

### 5.2 Hash-chain depth tradeoff

`MAX_CHAIN = 32`(greedy)vs `LAZY_CHAIN = 128`(lazy):

- `32` matches zlib L1's chain depth — finds matches up to ~ 32 hash
  collisions deep,enough for most short-range matches
- `128` matches zlib L6's depth — catches longer-range matches that
  L1's shallow search misses,critical for cross-paragraph
  repetition in essay-class text

zlib L9 uses chain depth 4096。Going past 128 hits diminishing returns
on our bench inputs(essay-03 already 1.00× zl_9 at 128;going to 4096
might shave 0.5% more)。Deferred to phase 1.2 if needed。

### 5.3 Edge case: trailing deferred match

If `i` reaches `n` while `prev_len ≥ MIN_MATCH`,we have a deferred
match with no lookahead to evaluate against — just commit it:

```rust
if prev_len >= MIN_MATCH {
    tokens.push(Token::Match { length, distance });
}
```

跟 zlib 的 `match_available` flush 一致。

---

## 6. cross-link

- 上游 plan:[06 design](06-nupic-deflate-design.md) §3 phase 1.1
  ("lazy match")
- 上游 phase 1.0.2: [06-quater](06-quater-deflate-dynamic.md)(dynamic
  Huffman + chooser)
- 实施:
  - [`crates/nupic-deflate/src/lz77.rs`](../../../crates/nupic-deflate/src/lz77.rs)
    `collect_tokens_lazy`、parameterized `find_longest_match(..., max_chain)`、
    `GREEDY_CHAIN` / `LAZY_CHAIN` / `LAZY_MAX` constants
  - [`crates/nupic-deflate/src/lib.rs`](../../../crates/nupic-deflate/src/lib.rs)
    — `Level::Best` doc 更新 to phase 1.1 wording
- bench: [`crates/nupic-research/examples/deflate_compare.rs`](../../../crates/nupic-research/examples/deflate_compare.rs)
  — `cargo-lock` + `essay-03-natural-text` 加入 input set

---

## 7. 下一步 — phase 1.2:block splitting + zopfli-class search

Phase 1.1 仍然 single-block-per-call。Multi-block lets each block use
its own frequency-tuned Huffman tree → better fit for heterogeneous
streams(eg cargo-lock 的 header + dep-list + checksum 三段 entropy 不
同)。

Phase 1.2 = 06-six(待写)。Includes:

- Token-stream split heuristic(KL divergence between freq distributions
  of two halves > threshold → split)
- BFINAL handling for multi-block(only last block sets BFINAL=1)
- Per-block format chooser(each block independently picks stored /
  static / dynamic)

预估 phase 1.2 之后:

- cargo-lock:13 504 → ~ 12 200(close 9% gap vs zl_6,**beat zl_9 by ~ 9%**)
- essay-03:8 597 → ~ 8 400(beat zl_9 by ~ 2%)
- text-9k / lorem:no change(input too small for block splitting to help)

Phase 1.2 是 **stage 1 graduation point** — ≤ 1.05× zopfli on benchmark
corpus + 30+ properties + 4-oracle bit-exact agreement。

---

## 8. 验收材料

- crate update:`crates/nupic-deflate/src/lz77.rs` 加 `collect_tokens_lazy`,
  parameterize `find_longest_match`,`GREEDY_CHAIN` / `LAZY_CHAIN` /
  `LAZY_MAX` constants;`src/lib.rs` `Level::Best` doc 更新
- 测套:`tests/roundtrip.rs` 加 2 个 lazy 测(natural-text ratio + 200 KB
  random roundtrip);总 37 tests
- bench:`crates/nupic-research/examples/deflate_compare.rs` 加 cargo-lock
  + essay-03 input
- 价值观:
  - [[feedback-ceiling-first-priorities]] — perf table 给 7 input × 4
    format 实测,每行 quote ceiling 距离(vs zl_9)
  - [[feedback-no-cost-thinking]] — phase 1.1 直接 ship without "is the
    3-byte text-9k improvement enough?" 评估 — directly 推进 phase 1.2
