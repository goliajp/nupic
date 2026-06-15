# 06-bis — nupic-deflate phase 1.0.0:stored-blocks infrastructure

> First implementation step on top of the [06 design](06-nupic-deflate-design.md).
> Land the encoder framework + stored-block path + zlib wrapper. **No
> compression yet** — that's phase 1.0.1(LZ77 + static Huffman),the
> next sub-essay.

---

## 1. perf

Stored blocks emit raw bytes wrapped in a 5-byte block header. Encode
speed is **streaming-memcpy-bound**(~30 GB/s on M2)— no LZ77 chain
walking, no Huffman lookup。Output `~ 1.0005 × len(data) + 5` bytes
per 65 535-byte block.

Distance to ceiling = 1×(已经 streaming peak)。

This phase 是 **infrastructure landing**,不参与 perf 攻击。Phase 1.0.1
起才进入真正 LZ77 + Huffman ceiling 表(zlib level 1 ~ 2 ms/MiB)。

---

## 2. mem

`BitWriter` grows linearly with output。Stored blocks 不持有 LZ77 sliding
window 状态。Per-block:
- 5-byte header(BFINAL + BTYPE + LEN + NLEN)
- 65 535 bytes raw payload max

零 working-set overhead beyond the output buffer。

---

## 3. disk

输出为 `Vec<u8>`(API §5)+ zlib wrapper option。byte-exact roundtrip
through `flate2 / miniz_oxide` validated across 15 tests。

---

## 4. cov — 15 测全过

`crates/nupic-deflate/tests/roundtrip.rs`:

### DEFLATE(`deflate()` returning RFC 1951 bytes)

| name | what |
|---|---|
| `deflate_empty` | b"" → flate2 decodes to b"" |
| `deflate_one_byte` | b"a" |
| `deflate_short_text` | b"Hello, world!" |
| `deflate_alphabet` | 62-char alphanumeric |
| `deflate_kilobyte_random` | 1 024-byte LCG-seeded random |
| `deflate_repeats_one_byte` | 4 096 × 0x5A |
| `deflate_block_boundary` | 65 536 bytes(exact stored-block boundary)|
| `deflate_multiple_blocks` | 200 000 bytes(forces 4 stored blocks)|
| `stored_block_overhead_is_small` | overhead ∈ [5, 10] bytes on 10 K input |

### zlib stream(`zlib_compress()` returning RFC 1950 bytes)

| name | what |
|---|---|
| `zlib_empty` | b"" → flate2 ZlibDecoder == b"" |
| `zlib_short` | b"Hello, zlib!" |
| `zlib_kilobyte_random` | 1 024-byte random |
| `zlib_starts_with_cmf_byte` | byte 0 == 0x78,(CMF × 256 + FLG) % 31 == 0 |
| `zlib_ends_with_adler32` | b"abcdefgh" tail == 0x0E000325(matches nupic-bits Adler) |

### + 1 doc test

`assert!(z[0] == 0x78)` from rustdoc example。

Total **15 integration + 1 doc = 16 tests pass** at release build。

---

## 5. doc

API surface:

```rust
pub fn deflate(data: &[u8]) -> Vec<u8>;
pub fn zlib_compress(data: &[u8]) -> Vec<u8>;
```

Internal:
- `STORED_MAX = 65_535` — block payload cap per RFC 1951 §3.2.4
- CMF = 0x78(DEFLATE + 32 KiB window)
- FLG runtime-adjusted to satisfy `(CMF*256 + FLG) % 31 == 0`(RFC 1950 §2.2)
- Adler-32 via `nupic-bits::adler32_update`
- Bit packing via `nupic-bits::BitWriter`(LSB-first per DEFLATE convention)

---

## 6. cross-link

- 上游 stages:
  - [05 nupic-bits stage 0](05-nupic-bits-stage-0.md) — supplies BitWriter + Adler-32
  - [06 design anchor](06-nupic-deflate-design.md) — full 6-phase plan
- 实施:
  - [`crates/nupic-deflate/src/lib.rs`](../../../crates/nupic-deflate/src/lib.rs)
  - [`crates/nupic-deflate/tests/roundtrip.rs`](../../../crates/nupic-deflate/tests/roundtrip.rs)
- Cement oracle:`flate2` v1(via miniz_oxide,pure Rust)— dev-dep only
- 价值观:
  - [[feedback-ceiling-first-priorities]] — 即使 phase 1.0.0 是
    infrastructure,仍 量化 distance to streaming ceiling(= 1×)
  - [[feedback-no-cost-thinking]] — LZ77 + Huffman 推 1.0.1,不评估
    "stage 1.0 complete by phase 1.0.0" cost benefit

---

## 7. 下一步 — phase 1.0.1:greedy LZ77 + static Huffman

设计已在 [06 design §5.2 algorithm sketch](06-nupic-deflate-design.md):
- 15-bit hash chain over 32 KiB window
- length 3..258, distance 1..32 768
- RFC 1951 §3.2.6 fixed Huffman tree(literal/length + distance)
- BFINAL = 1 single block per call(initially)

Phase 1.0.1 essay = 06-ter,待写。

---

## 8. 验收材料

- crate:[`crates/nupic-deflate/`](../../../crates/nupic-deflate/)
- 测套:16 tests(15 integration + 1 doc)
- 上游 essay:[06](06-nupic-deflate-design.md)
- release:0.5.3 candidate(nupic-deflate v0.5.3 graduated as workspace
  member,no nupic-cli surface change yet)
