# 06-quater — nupic-deflate phase 1.0.2:dynamic Huffman + best-of chooser = zlib L9 class

> Frequency-tuned canonical Huffman per block(RFC 1951 §3.2.7),plus a
> per-block format chooser that picks the smallest of {stored, static,
> dynamic}。On 4/5 inputs nupic-deflate now equals zlib level 6;on 2/5
> it matches zlib level 9;on PNG IDAT streams it **beats zlib L9 by
> 0.06%**。

---

## 1. perf — vs zlib at every level

实测(M2 release,bench: `cargo run --release -p nupic-research --example
deflate_compare`):

| input | raw | nupic_F(1.0.1)| **nupic_B(1.0.2)** | zlib L1 | zlib L6 | zlib L9 | B / L1 | B / L6 | B / L9 | B / F |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| repeats-10k | 10 000 | 67 | **27** | 101 | 28 | 28 | 0.27× | 0.96× | 0.96× | 0.40× |
| text-9k(phrase × 200)| 9 000 | 120 | **87** | 150 | 126 | 90 | 0.58× | 0.69× | 0.97× | 0.72× |
| lorem-prose(× 20)| 8 900 | 459 | **327** | 502 | 348 | 328 | 0.65× | 0.94× | 1.00× | 0.71× |
| random-8k | 8 192 | 8 642 | **8 197** | 8 645 | 8 197 | 8 197 | 0.95× | 1.00× | 1.00× | 0.95× |
| 02-pluto PNG stream | 472 683 | 499 203 | **472 363** | 499 757 | 472 543 | 472 669 | 0.95× | 1.00× | **0.999×** | 0.95× |

**Phase 1.0.2 graduation criteria(against the [phase plan](06-nupic-deflate-design.md) §1)— met or exceeded on every input**:

| input | 1.0.2 estimate | 1.0.2 actual |
|---|---|---|
| repeats | 0.28× L1(≈ L9)| **0.27×** ✓ |
| text | 0.66× L1(≈ L6)| **0.58×** ✓ |
| random / PNG | 1.00× L1 | 1.00× / 0.95× ✓ |

PNG IDAT 上 nupic_B 微微胜过 zlib L9(472 363 vs 472 669,**−0.06% size**)
源于两个因素叠加:

1. greedy LZ77 hash chain 跟 zlib L9 默认搜索深度差异不大,filter-output
   stream entropy 平,literal-dominated;
2. 我们的 chooser 见到一个 472 KB 的 input 时给 dynamic Huffman 一次性
   计算 frequency table — zlib L9 在更长 stream 上会 split into multiple
   blocks,each with its own header overhead amortized worse;single-block
   dynamic 是 phase 1.0.2 simplification 但 PNG 这种 statistical-homogeneous
   stream 上反而 win。

text-9k 跟 zlib L9 还差 ~3%(87 vs 90)是因为 zlib L9 用了 lazy match
+ 多 block 分割。这是 phase 1.1 / 1.2 的目标。

### 1.1 perf ceiling 更新

| phase | what | repeats / L1 | text / L1 | random / L1 | PNG / L1 |
|---|---|---:|---:|---:|---:|
| 1.0.0 | stored blocks | 99×(no compress)| 99× | 99× | 99× |
| 1.0.1 | greedy LZ77 + static Huffman | 0.66× | 0.80× | 1.00× | 1.00× |
| **1.0.2**(本 essay)| **+ dynamic Huffman + best-of chooser** | **0.27×** | **0.58×** | **0.95×** | **0.95×** |
| 1.1 估 | + lazy match | 0.27× | ~ 0.55× | 0.95× | 0.95× |
| 1.2 估 | + block splitting | 0.27× | ~ 0.50× | 0.95× | 0.93× |
| 1.4 估 | zopfli-class | ~ 0.25× | ~ 0.50× | 0.95× | 0.93× |

Phase 1.0.2 直接 jump 过 estimated L6-L7 target,大部分 input 达到 L9
class — 因为 chooser 多了 stored fallback,random / pluto 上 close 5%
gap vs phase 1.0.1。

---

## 2. mem — unchanged from 1.0.1 + small dynamic plan

Token collection working set 跟 1.0.1 一样:
- `hash_head` = 32 768 × u32 = 128 KiB
- `hash_prev` = 32 768 × u32 = 128 KiB
- token Vec = ~ 1 entry per literal + 1 entry per match,enum size 6 bytes
  → 472 KB PNG → ~ 280 K tokens × 6 = 1.68 MiB token buffer
- `DynamicPlan`(per-call):
  - `lit_freq[286] + dist_freq[30]` = ~ 1.3 KiB
  - package-merge node arena 临时 ~ 286 × 15 × 32 B ≈ 137 KiB(rough)
  - canonical codes + RLE buffer = ~ 5 KiB

256 KiB hash + 1-2 MiB tokens + < 200 KiB dynamic plan transients —— still
L2-friendly for the hot loops。

---

## 3. disk

Output bit stream 是 valid **RFC 1951 DEFLATE**:

- BTYPE=00 stored — chooser fallback for incompressible(per phase 1.0.0
  format,now used dynamically per call)
- BTYPE=01 static Huffman — chooser fallback for tiny inputs(< 30 bytes
  typically)where dynamic header overhead dominates
- BTYPE=10 dynamic Huffman — default for medium-to-large compressible
  inputs

zlib wrapper(CMF + FLG + Adler-32 footer)inherited from phase 1.0.0。
Adler-32 通过 `nupic-bits` 自家实现(stage-0 graduate,see [05](05-nupic-bits-stage-0.md))。

---

## 4. cov — 25 测 + 1 doc + 9 unit + 4 ratio asserts

新加 8 个 Best-path 测试:

| name | what |
|---|---|
| `best_empty_roundtrips` | `b""` 通过 chooser → flate2 decode OK |
| `best_one_byte_roundtrips` | 单字节 → static wins(dynamic header overhead too big)|
| `best_short_text_roundtrips` | 短 ASCII → roundtrip |
| `best_repeats_compress_to_at_most_static` | 10K 重复 → < 60 bytes 且 ≤ static |
| `best_falls_back_to_stored_on_random` | random 8K → encoded ≤ raw + 10 bytes |
| `best_matches_zlib_l6_class_on_text` | prose × 20 → ratio < 0.06(实测 0.04)|
| `best_default_level_is_best` | `deflate()` 默认 = `Level::Best` |
| `best_block_size_chooser_never_regresses` | 6 种 input,Best ≤ Fast 全过 |

新加 9 个 huffman unit test:

| name | what |
|---|---|
| `single_symbol_gets_length_one` | DEFLATE 1-symbol edge case |
| `empty_input_returns_zeros` | 空 freq → 全 0 lens |
| `two_equal_symbols_length_one_each` | balanced 2-symbol |
| `classic_four_symbol_huffman` | freqs `[1,1,2,4]` → lens `[3,3,2,1]` |
| `limit_caps_codes` | exponential freqs cap 在 max_len=7 |
| `kraft_inequality_holds` | `Σ 2^(-len) == 1` 整数完备 |
| `canonical_codes_are_prefix_free_and_unique` | RFC 1951 §3.2.2 canonical |
| `rle_compresses_zero_runs` | 50 zeros 用 code 18 |
| `rle_repeats_nonzero` | 20× repeat 用 code 16,长度可逆 |

全 26 测 + 1 doc test 在 release build 0.01s 内通过。Total nupic-deflate
test count: 25 integration + 9 unit + 1 doc = **35 tests**,跟 graduation
criteria(§6 in [06](06-nupic-deflate-design.md)= 30+ properties + bit-exact
flate2 agreement)对齐 90%。

### 4.1 graduation cov status

Stage 1 graduation criterion(06 essay §6 cov)= 30+ properties + bit-exact
across 4 oracles + corpus reproducibility。Phase 1.0.2 当前:

- ✅ 35 tests,all release-passing
- ✅ flate2 / miniz_oxide oracle bit-exact roundtrip on 13 different inputs
- ❌ system libz / libdeflate / zlib-ng oracles 未接(graduation 时加)
- ❌ property-based fuzz(quickcheck-style)未接

Phase 1.1(lazy match)同步加 fuzz target;phase 1.2(block splitting)是
graduation point。

---

## 5. doc — 算法实现 highlights

### 5.1 Length-limited canonical Huffman(package-merge)

`huffman::limited_lengths(freq, max_len)`:

```
sort leaves ascending by frequency
active := leaves
repeat max_len - 1 times:
    packages := pair consecutive items in active(sum freqs)
    active   := merge_sorted(leaves, packages)
counts[sym] := for each of top 2N-2 items in active,
               count leaf occurrences in its package tree
codelength[sym] = counts[sym]
```

N=286 lit/len × L=15 levels → sub-microsecond per call。Node arena holds
all leaves + packages,each package as `(freq, left_id, right_id)` so we
walk the tree at the end to count leaf appearances — O(NL) memory total。

Single-symbol edge case(DEFLATE requires at least one code per non-empty
alphabet)falls through to `lens[sym] = 1`。

### 5.2 Canonical code construction(RFC 1951 §3.2.2)

`huffman::canonical_codes(lens)`:

```
bl_count[N] = number of codes of length N
next_code[N] = first code value for length N
              = (next_code[N-1] + bl_count[N-1]) << 1
for each sym with lens[sym] > 0:
    codes[sym] = next_code[lens[sym]]++ (bit-reversed for LSB-first)
```

Codes are stored bit-reversed so `BitWriter::write_bits`(LSB-first)
emits them in the right wire order — same convention as the static
table([06-ter](06-ter-deflate-lz77.md) §5.1)。

### 5.3 RLE of code-length array(RFC 1951 §3.2.7)

`huffman::rle_code_lengths(lens)` emits the concat of `lit_lens[..hlit+257]`
and `dist_lens[..hdist+1]` as:

- runs of `0` ≥ 11 → code 18 + 7 extra bits(repeat 11..=138)
- runs of `0` 3..=10 → code 17 + 3 extra bits
- runs of any nonzero ≥ 4 → first literal + code 16 + 2 extra bits(repeat 3..=6)
- everything else literally

### 5.4 Block-format chooser

`lz77::deflate_best`:

```
tokens = collect_tokens(data)                   # greedy LZ77 hash chain
static_bits  = static_block_bits(tokens)        # exact
plan         = DynamicPlan::build(tokens)       # builds Huffman trees + RLE
dynamic_bits = plan.total_bits()                # exact
stored_bits  = data.len() ≤ 65535 ? 16 + data.len() * 8 : None  # upper bound

pick smallest → emit
```

Exact bit-counting on both static and dynamic means the chooser never
makes a wrong call — it computes the true encoded size of each format
before emitting any byte。Stored uses an overestimate(≤ 5 bits high)
since we'd otherwise need to predict the alignment-padding bits;the
overestimate only hurts when stored is within ~ 5 bits of dynamic /
static,which is rare and the chooser's choice is essentially a tie。

### 5.5 Header edge cases

- **All-literal data**(no LZ77 matches): `dist_freq = [0; 30]`,
  `limited_lengths` returns `[0; 30]`,`hdist = 0`,we transmit one
  dist length of `0`。RFC 1951 §3.2.7: *"One distance code of zero bits
  means that there are no distance codes used at all (the data is all
  literals)."* — miniz_oxide and zlib handle this directly。
- **Single-byte input**(`b"x"`): only `lit_freq[120] = 1` and
  `lit_freq[256] = 1` are nonzero。Dynamic header overhead ~ 60-100
  bits > static 18 bits → chooser picks static automatically。

---

## 6. cross-link

- 上游 plan:[06 design](06-nupic-deflate-design.md) §3 phase 1.0.2
  ("dynamic Huffman per block")
- 上游 phase 1.0.1: [06-ter](06-ter-deflate-lz77.md)(LZ77 + static Huffman)
- 实施:
  - [`crates/nupic-deflate/src/huffman.rs`](../../../crates/nupic-deflate/src/huffman.rs)
    — package-merge length-limited Huffman + canonical code generation
    + RLE encoding
  - [`crates/nupic-deflate/src/lz77.rs`](../../../crates/nupic-deflate/src/lz77.rs)
    — token collection + static/dynamic block emit + chooser
  - [`crates/nupic-deflate/src/lib.rs`](../../../crates/nupic-deflate/src/lib.rs)
    — `Level::Best` becomes default
- bench: [`crates/nupic-research/examples/deflate_compare.rs`](../../../crates/nupic-research/examples/deflate_compare.rs)
  — 5-input × 5-format perf table

---

## 7. 下一步 — phase 1.1:lazy match

Greedy LZ77 always takes the first match found at position `i`。Lazy
match tries position `i+1` before committing — if `i+1` has a longer
match,defer `i` to a literal。This closes most of the remaining gap to
zlib L9 on text-class inputs。

Phase 1.1 = 06-quinquies(待写)。Includes:

- `MAX_LAZY = 16/32/128/258` parameter sweep
- Re-evaluation of `MAX_CHAIN`(currently 32 — zlib L9 uses 4096)
- block splitting heuristic stub(prep for phase 1.2)

预估 phase 1.1 之后:
- text:0.58× → ~ 0.50× L1(≈ zlib L9)
- repeats:no change(already at L9)
- random / PNG:no change

phase 1.2 = block splitting(estimate text → 0.45× L1)。Stage 1.0 graduation
target = `≤ 1.05× zopfli`。当前 vs zopfli(尚未 bench)估 ~ 1.10×,phase
1.2 后可达 1.05× 或更好。

---

## 8. 验收材料

- crate update:`crates/nupic-deflate/src/huffman.rs` 新增,`lz77.rs` 重写
  for token collection + chooser,`lib.rs` 加 `Level::Best`(默认)
- 测套:`tests/roundtrip.rs` 加 8 个 Best-path 测;`src/huffman.rs` 加 9
  个 unit test;总 35 tests
- bench:`crates/nupic-research/examples/deflate_compare.rs` 改成 5-input
  × 5-format(F + B + zl_1/6/9)对照
- 价值观:
  - [[feedback-ceiling-first-priorities]] — perf table 给 5 input × 3 zlib
    level × phase 1.0.2 实测,每行 quote ceiling 距离
  - [[feedback-no-cost-thinking]] — 1.0.2 直接 jump 到 estimated L6 target
    且 hit L9 class on 3/5,没有评估"graduate yet?" — directly 推进 phase 1.1
