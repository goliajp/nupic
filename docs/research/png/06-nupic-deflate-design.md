# 06 — `nupic-deflate` stage 1 设计 anchor

> Stage 1 of the [`docs/roadmap.md`](../../roadmap.md) self-built
> PNG / DEFLATE pipeline. Depends on Stage 0 (`nupic-bits`, just
> graduated in [05-nupic-bits-stage-0.md](05-nupic-bits-stage-0.md))
> for CRC-32 / Adler-32 / bit I/O.
>
> Sections ordered **perf > mem > disk > cov > doc** per
> [[feedback-ceiling-first-priorities]].
>
> Design essay only(per the Stone-design pattern from 03 / 03b / 03c).
> Implementation phases land in 06-bis through 06-quinquies。

---

## 0. nupic-deflate 是 PNG self-built codec 的关键 link

跟 stone A/B/C(perceptual / metric / quantize)不同,nupic-deflate 是
**format-level** stone — 它替换 nupic-quantize 当前依赖的 `oxipng`
最后一步(`oxipng::optimize_from_memory`)。oxipng 内部用 `zlib`(或
`libdeflate`)做 DEFLATE encode。

Stone C 当前(0.5.x)pipeline:

```
imagequant median-cut palette → OKLab argmin → indexed bytes
                                     ▼
                        png crate encode (palette + IDAT)
                                     ▼
                        oxipng (= filter try-all + zlib deflate)
                                     ▼
                                  PNG bytes
```

nupic-deflate stage 1 替换 `zlib deflate` 部分。再后续可继续替换
oxipng 的 filter 步(roadmap 阶段 7)。

---

## 1. perf — DEFLATE 性能 ceiling

### 1.1 Reference impl 数据(已知)

| impl | source | encode time / 1 MiB text | output ratio vs raw | output / zopfli |
|---|---|---:|---:|---:|
| zlib level 1 | C, scalar | ~2 ms | ~0.50 | 1.15× |
| zlib level 9 | C, scalar | ~30 ms | ~0.36 | 1.03× |
| libdeflate | C, scalar + SIMD | ~5 ms (level 9) | ~0.36 | 1.03× |
| zopfli | C, brute search | ~3 s | ~0.34 | 1.00 baseline |
| ECT(zopfli + tricks)| C++ | ~5 s | ~0.34 | 0.99× |

zopfli 是 DEFLATE 格式下的近似 entropy 下界(见 [`docs/png-pipeline.md` §3](../../png-pipeline.md))。
任何 DEFLATE encoder 的输出 size 都被 zopfli 的下界 cap。

### 1.2 Stage 1 phase plan

按 perf 优先排序,递增 implementation 复杂度:

| phase | what | output target | encode speed target |
|---|---|---|---|
| **1.0** | greedy LZ77 hash chain + static Huffman block | zlib level 1(~0.50 raw,1.15× zopfli)| ~2 ms / MiB |
| **1.1** | + dynamic Huffman per block | zlib level 6(~0.40 raw,1.10× zopfli)| ~10 ms / MiB |
| **1.2** | + lazy match evaluation | zlib level 9(~0.36 raw,1.03× zopfli)| ~30 ms / MiB |
| **1.3** | + block-splitting heuristic | libdeflate-class | ~5 ms / MiB(SIMD)|
| **1.4** | + zopfli-like brute search | zopfli(0.34 raw,1.00 baseline)| ~3 s / MiB |
| 1.5 | + arm NEON / x86 AVX2 SIMD on LZ77 hash + Huffman | zopfli output,libdeflate speed | ~0.5 s / MiB |
| 1.∞ | DEFLATE format ceiling = LZ77 entropy upper bound | absolute minimum | n/a |

**Stage 1 graduation criterion**:phase 1.2(lazy match)— output ≤ 5%
larger than zopfli on the standard `silesia` corpus subset(代表性
文本 / 图像 / 二进制 mix),encode speed ≥ zlib level 6。

Phase 1.3 / 1.4 是 post-graduation polish。

### 1.3 vs nupic-quantize 视角

当前 nupic-quantize 通过 `oxipng::optimize_from_memory(preset=5)`
触发 zlib level ~8 + filter try-all。Stage 1.0 nupic-deflate 输出会
**比 oxipng 大** ~10-15%(因为 1.0 是 zlib level 1 等价),但跨入
1.2 后 持平 oxipng。

User-facing reach:nupic-quantize 接 nupic-deflate 需要 nupic-quantize
内 swap `oxipng` 调用 → 自研 filter + deflate pipeline。这是 stage 7
filter beam search 之后的事(roadmap order)。

**stage 1 single-stone graduation 期间,不接 nupic-quantize**;两者
通过 stage 7 stone D + filter beam 才 integrate。

---

## 2. mem

DEFLATE encoder working set 分两块:

### 2.1 LZ77 sliding window

- 32 KiB window of uncompressed bytes
- Hash chain head + next pointers:32 KiB × 4 = **128 KiB** (zlib-class:
  hash bins of `u32` next-pointer per byte)
- Or compact variant:hash head 64 KiB + smaller next chain — trade off
  match quality

For phase 1.0:hash table size 65,536 entries(15-bit hash)× 2 byte
pointer = **128 KiB hash + 32 KiB window + 32 KiB next chain = 192 KiB
working set**。L2-friendly on M2(L2 = 12 MB)。

### 2.2 Huffman code generation

- literal/length tree:288 × u32 freq + 288 × u8 lens = ~ 2 KiB
- distance tree:32 × u32 freq + 32 × u8 lens = ~ 200 B
- 静态 Huffman code 表(generate-once)= ~ 1 KiB

Total Huffman state < 5 KiB。L1-friendly。

### 2.3 4K image input

PNG 4K(3840×2160 = ~ 33 MiB raw)的 DEFLATE 编码 working set 不随 input
size 增长 —— sliding window 始终 32 KiB。**Streaming-by-construction**。

---

## 3. disk

stage 1 不写盘,output 是 `Vec<u8>`(或 `Write` trait sink for
streaming)。public API §6 给出。

---

## 4. cov

### 4.1 Property tests(graduation 目标 30+)

| 类别 | 例子 |
|---|---|
| spec compliance | 输出可被 zlib decompress(roundtrip) |
| spec edge cases | empty input, single byte, 1 byte > window, exact window size, > window |
| huffman correctness | dynamic Huffman tree 反编码后跟输入 freq 一致 |
| LZ77 correctness | match length 3..258, distance 1..32K |
| 数值稳定 | 不 panic on adversarial input(repeated bytes, random, long runs) |
| streaming | finish() flush correctly,multiple write() calls 等价 single block |

### 4.2 Reference oracle 测

跨 corpus 跑 nupic-deflate output × 4 个 oracle:
- **flate2** crate(用 miniz_oxide)— pure Rust zlib-compat
- **libdeflate-sys**(C libdeflate)— SIMD reference
- **zopfli** (zopflipng) — output 下界
- **zlib**(system libz)— format reference

每 phase corpus 数字记下 + 跨 oracle delta < 1% byte size。

### 4.3 跨平台一致性

phase 1.0-1.2 都是纯 scalar,output bit-exact across M2 / x86 / Linux。

phase 1.5 加 SIMD 后,**output 仍 bit-exact**(SIMD 只 accelerate hash
chain lookups + Huffman code generation,不改 final byte stream 决策)。

---

## 5. doc

### 5.1 公共 API skeleton

```rust
// crates/nupic-deflate/src/lib.rs

/// One-shot encode: raw bytes → DEFLATE-encoded bytes (RFC 1951 only,
/// no zlib header / Adler footer).
pub fn deflate(data: &[u8], level: DeflateLevel) -> Vec<u8>;

/// One-shot encode wrapped in zlib stream (RFC 1950: CMF + FLG header
/// + DEFLATE stream + Adler-32 footer). nupic-bits supplies the
/// Adler.
pub fn zlib_compress(data: &[u8], level: DeflateLevel) -> Vec<u8>;

/// Streaming encoder. Accepts arbitrary chunks; emits to a Write
/// sink with internal buffer flushed in 32-KiB-block boundaries.
pub struct DeflateEncoder<W: std::io::Write> {
    // private fields
}

impl<W: std::io::Write> DeflateEncoder<W> {
    pub fn new(sink: W, level: DeflateLevel) -> Self;
    pub fn write(&mut self, data: &[u8]) -> std::io::Result<()>;
    pub fn finish(self) -> std::io::Result<W>;
}

/// Compression level.
#[derive(Copy, Clone, Debug)]
pub enum DeflateLevel {
    Fast,       // phase 1.0 — greedy LZ77, static Huffman, no lazy
    Default,    // phase 1.2 — lazy match, dynamic Huffman per block
    Best,       // phase 1.4 — zopfli-class output
}
```

### 5.2 算法 sketch(phase 1.0,起手 target)

```
INPUT: raw bytes (any length)
OUTPUT: DEFLATE bit stream

Step 1: Build hash chain (Sliding window).
  - For each input byte i, compute hash3(data[i..i+3])
  - Store i in hash_head[hash3] (most-recent occurrence)
  - Chain previous occurrence in hash_next[i]
  - Hash table size = 64 K entries(15-bit hash);chain depth bounded

Step 2: Tokenise via greedy LZ77.
  - For each input position i:
    - Look up hash_head[hash3(data[i..i+3])]
    - Walk hash_next chain back up to 256 steps
    - Find longest match >= 3 within 32 KiB distance
    - If found, emit (length, distance) token
    - Else, emit literal byte token
  - End-of-block marker after all bytes consumed

Step 3: Pack into static Huffman block (phase 1.0).
  - BFINAL = 1 (single block in 1.0)
  - BTYPE = 01 (fixed Huffman, per RFC 1951 §3.2.6)
  - Emit literal codes(0..255):8 or 9 bits per RFC table
  - Emit length codes(256..285):7 or 8 bits + extra bits
  - Emit distance codes(0..29):5 bits + extra bits
  - Emit EOB (256) at end

Step 4: Bit-pack to byte stream via nupic-bits BitWriter.
  - LSB-first within bytes
  - DEFLATE convention: code high bits transmitted first within
    a code(per RFC 1951 §3.1.1, "packing into bytes")
```

Phase 1.1 adds:
- Frequency counting during tokenisation
- Dynamic Huffman tree from frequencies(canonical, length-limited 15)
- HLIT/HDIST/HCLEN header transmission
- Code-length code-length encoding

Phase 1.2 adds:
- Lazy match evaluation(check next position for longer match before
  committing this one)
- Multi-block splitting at frequency-density change-points

Phase 1.4(zopfli-class):
- Full search over (length, distance) Pareto frontier
- Iterate to convergence on tree weights

### 5.3 跟 zlib / libdeflate / zopfli 的关系

跟 stone-layer 哲学一致:cement crates(`flate2` via `miniz_oxide` /
`libdeflate-sys` / `zopfli`)是 reference + dev-only oracle,**nupic-
deflate 不 dep them at runtime**。stage 1.0 graduation 之后,自研
nupic-deflate 是 PNG codec self-built link 的关键。

---

## 6. graduation criteria

按 ceiling-first 硬序:

- [ ] **perf**:phase 1.2(lazy match)encode speed ≥ zlib level 6
  on `silesia` test corpus(~ 20 ms / MiB on M2)
- [ ] **mem**:working set ≤ 256 KiB regardless of input size(sliding
  window design)
- [ ] **disk**(output):phase 1.2 output ≤ 1.05× zopfli on
  `assets/png-bench/` PNG IDAT samples + standard `silesia` corpus
  subset(text + image + binary mix)
- [ ] **cov**:30+ property tests + 跨 4 oracle bit-exact agreement
  + corpus 跨 platform reproducibility
- [ ] **API**:`crates/nupic-deflate/` public:
  - `deflate(data, level) -> Vec<u8>`
  - `zlib_compress(data, level) -> Vec<u8>`
  - `DeflateEncoder<W: Write>` streaming
  - `DeflateLevel { Fast, Default, Best }` enum
- [ ] **doc**:本 essay + sub-essays 06-bis(1.0)→ 06-six(1.4)+
  crate-level rustdoc

---

## 7. sub-essay roadmap

按 perf 优先 + 算法增量:

| seq | sub-essay | focus | encode target |
|---|---|---|---|
| 06 | 本篇 | design + DEFLATE format ground + 6 phase plan | — |
| 06-bis | phase 1.0 | greedy LZ77 + static Huffman + zlib roundtrip | zlib level 1 |
| 06-ter | phase 1.1 | + dynamic Huffman per block | zlib level 6 |
| 06-quater | phase 1.2 | + lazy match + corpus benchmark + graduation | zlib level 9 ≈ |
| 06-quinquies | phase 1.3 | + block-splitting + libdeflate parity | libdeflate-class |
| 06-six(optional)| phase 1.4 | + zopfli-grade brute search | zopfli output |
| 06-septem(optional)| phase 1.5 | + SIMD NEON / AVX2 | libdeflate-class speed |

Stage 1 graduation = 06-quater(phase 1.2)。Phase 1.3-1.5 是 polish。

---

## 8. open questions

1. **Block splitting heuristic**:single big block vs many small blocks
   有 trade-off。zlib 用 64 KiB block;libdeflate uses 16 KiB +
   density-based split。我们简单起步 = 整 input 一个 block(phase 1.0);
   phase 1.3 加 adaptive split。
2. **Hash function choice**:zlib uses simple `data[0] << shift ^
   data[1] << shift ^ data[2]`;现代 alternatives 更 collision-friendly
   但 cycle-heavier。Phase 1.0 mirror zlib choice for simplicity。
3. **Pre-PNG-specific tuning**:PNG IDAT 字节流 是 filter output(N/S/U/A/Paeth
   per row)— DEFLATE 编码这种 row-structured data 跟 generic text 不同。
   有 specific tricks 可探索(phase 1.5+)。
4. **Streaming flush semantics**:`flush()` 跟 `finish()` 区分。zlib
   `Z_SYNC_FLUSH` vs `Z_FINISH` 都需要 supporting。Stage 1.0 仅 finish;
   流式 sync 后置 1.3+。

---

## 9. cross-link

- 上游 stage:[05 nupic-bits stage 0](05-nupic-bits-stage-0.md)
- [`docs/roadmap.md` 阶段 1](../../roadmap.md) — DEFLATE encoder
- [`docs/png-pipeline.md` §3](../../png-pipeline.md) — DEFLATE 物理墙分析
- 算法 reference:RFC 1950(zlib stream),RFC 1951(DEFLATE format)
- Cement crates(dev-deps only,oracle):flate2 + libdeflate-sys + zopfli
- 价值观:
  - [[feedback-ceiling-first-priorities]] — 6 phase plan 每 phase 量化 distance to ceiling
  - [[feedback-no-cost-thinking]] — phase 1.4 zopfli 实施 ~ 几 K 行,但
    不评估 cost,只看 ceiling 距离

---

## 10. 验收材料

- crate skeleton(本 essay graduation 之后落地):`crates/nupic-deflate/`
- reference fixture:RFC 1951 examples + `silesia` corpus subset
- 上游:[next-step-research-0_5_x](../../../../../.claude-profile-1/projects/-Users-doracawl-workspace-labs-lab29-nupic/memory/next_step_research_0_5_x.md)
  原 0.5.x 计划 — 现在 0.5.x 已 ship PNG stones,nupic-deflate 是
  0.6.x 主线候选(本 essay 是 design anchor,不是 implementation)
