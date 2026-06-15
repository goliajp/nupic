# 05 — `nupic-bits` stage 0 graduation:CRC-32 + Adler-32 + bit I/O

> Stage 0 of the [`docs/roadmap.md`](../../roadmap.md) self-built PNG /
> DEFLATE pipeline. Standalone stone — 0 runtime deps,跨平台 bit-exact
> 跟 zlib / RFC fixtures。
>
> Sections ordered **perf > mem > disk > cov > doc**.

---

## 1. perf

### 1.1 实测(M2 release,1 MiB random,11-run median)

```
== CRC-32 (1 MiB) ==
  nupic-bits   :   0.430 ms /  2.44 GB/s,  value = 0xCB3CB37F
  crc32fast    :   0.101 ms / 10.37 GB/s,  value = 0xCB3CB37F
  ratio        : 4.25× slower

== Adler-32 (1 MiB) ==
  nupic-bits   :   0.251 ms /  4.18 GB/s,  value = 0x3456B8E7
  adler32      :   0.243 ms /  4.32 GB/s,  value = 0x3456B8E7
  ratio        : 1.03× — ≈ parity
```

Values agree bit-exact across both pairs(跟 zlib / RFC 1952 / RFC 1950
oracle correlate)。

### 1.2 perf ceiling 分布

| 函数 | M2 ceiling | nupic | cement | nupic 距 ceiling | cement 距 ceiling |
|---|---:|---:|---:|---:|---:|
| CRC-32 | ~30 GB/s(DRAM stream)| 2.44 GB/s | 10.37 GB/s | 12× | 3× |
| Adler-32 | ~30 GB/s | 4.18 GB/s | 4.32 GB/s | 7× | 7× |

CRC-32 cement 用 **arm NEON `pclmulqdq` 等价 intrinsics**(`crc32fast` v1.x
on arm 走 `vmull_p64`)做 64-bit polynomial 乘法 → 一次处理 8 bytes,加
SIMD lane 并行,~3× faster than 我们的 slice-by-8 scalar。

Adler-32 没有 SIMD short-cut(reduction needs sequential a/b updates)→
cement 跟 nupic 都 scalar,perf 基本一致。

### 1.3 stage-0 attack plan

| phase | what | 02-pluto CRC-32 GB/s 目标 |
|---|---|---:|
| **S0**(本 essay)| scalar slice-by-8 baseline | 2.44 |
| S0.1 | NEON `vmull_p64` polynomial mul(arm specific) | ~ 8–10 估 |
| S0.2 | x86 `_mm_clmulepi64_si128`(x86 specific) | ~ 8–10 估 |
| S0.3 | tile + prefetch | ~ 12+ 估 |
| S∞ | DRAM streaming ceiling | ~ 30 |

S0.1 / S0.2 是 stone-A 的 lesson 重演 — portable SIMD wrappers 不行,
direct platform intrinsics 才能 beat cement。Polish backlog;**不阻塞
stage-0 graduation**。

---

## 2. mem

`CRC32_TABLE`(256 × u32 = 1 KB)+ `CRC32_TABLE_SLICE8`(8 × 256 × u32
= 8 KB)= **9 KB** 静态 RAM,所有 fit L1。无 heap alloc。

Adler-32 5 个 u32 stack variables,~20 byte total。

`BitWriter` heap 一个 `Vec<u8>`,按需 grow;`BitReader` borrow `&[u8]`,
无 alloc。

跨 fixture / 跨 input size memory 都 sub-microscopic。

---

## 3. disk

不写盘。

---

## 4. cov — 13 测,oracle + property based

`crates/nupic-bits/tests/properties.rs`:

| name | 类型 |
|---|---|
| `crc32_rfc1952_fixtures` | 4 fixtures(b"", "123456789", "a", "abc"),已知 zlib 值 |
| `crc32_matches_crc32fast_on_random_lengths` | sweep len 0..600, LCG-seeded data,跟 `crc32fast` 比 |
| `crc32_incremental_update_matches_one_shot` | 4096 byte buf split 5 处,incremental == one-shot |
| `adler32_rfc1950_fixtures` | 4 fixtures incl "Wikipedia" / RFC 1950 examples |
| `adler32_matches_adler32_crate_on_random_lengths` | sweep len 0..600, oracle |
| `adler32_incremental_update_matches_one_shot` | 8192 byte buf,split at NMAX=5552 boundary 等 5 处 |
| `bit_io_round_trip_every_width` | width 1..=32 × 257 values each,write→read |
| `bit_writer_aligns_to_byte` | `align_to_byte` 把 3-bit prefix 凑齐 8 bits |
| `bit_reader_returns_err_on_eof` | EOF 返回 `Err(())` |
| `bit_reader_lsb_first` | LSB-first bit layout 验证 |
| `bit_writer_length_tracking_basic` | `bit_len()` 跨 boundary 正确 |

Plus 2 doc tests:`crc32(b"123456789") == 0xCBF43926`,`adler32(b"abcdefgh") == 0x0E000325`。

总 13 test pass,跨 ~5000 内部 assertions(每 width × 257 values = 8 K
roundtrip checks alone)。

### 4.1 实测过程的 bug fix

`BitReader::read_bits` 初版用 `1u8 << take`,`take=8` 时 overflow → 全
0 return。fix:用 `((1u16 << take) - 1) as u8` 避开 overflow。**这是
typical bit-shift bug,property test(round-trip every width)直接 catch
住了**。

---

## 5. doc

公共 API:

```rust
// CRC-32 (PNG / gzip / zlib / RFC 1952)
pub fn crc32(data: &[u8]) -> u32;
pub fn crc32_update(data: &[u8], init: u32) -> u32;

// Adler-32 (RFC 1950)
pub fn adler32(data: &[u8]) -> u32;
pub fn adler32_update(data: &[u8], init: u32) -> u32;

// Bit I/O (LSB-first, DEFLATE)
pub struct BitReader<'a> { /* private */ }
pub struct BitWriter { /* private */ }

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self;
    pub fn read_bits(&mut self, n: u8) -> Result<u32, ()>;
    pub fn skip_bits(&mut self, n: usize) -> Result<(), ()>;
    pub fn bit_position(&self) -> usize;
    pub fn bit_len(&self) -> usize;
}

impl BitWriter {
    pub fn new() -> Self;
    pub fn with_capacity(cap: usize) -> Self;
    pub fn write_bits(&mut self, value: u32, n: u8);
    pub fn align_to_byte(&mut self);
    pub fn into_bytes(self) -> Vec<u8>;
    pub fn as_bytes(&self) -> &[u8];
    pub fn bit_len(&self) -> usize;
}

// Tables exposed for downstream SIMD-aware libs (e.g. nupic-deflate).
pub const CRC32_TABLE: [u32; 256];
pub const CRC32_TABLE_SLICE8: [[u32; 256]; 8];
```

`#![no_std]`(test-only `std`)— stage 0 stone 进 embedded / wasm 也
直接可用。`extern crate alloc` for `Vec<u8>` in `BitWriter`。

跟 [`docs/png-pipeline.md` §3](../../png-pipeline.md) 的 DEFLATE 数学
墙互引;`docs/roadmap.md` 阶段 0 dependency。

---

## 6. graduation criteria

跟原 memory [next-step-research-0_5_x](../../../../../.claude-profile-1/projects/-Users-doracawl-workspace-labs-lab29-nupic/memory/next_step_research_0_5_x.md)
§项目宪法约束 对照:

- [x] **0 deps**:`[dependencies]` 为空,`[dev-dependencies]` 跑 `crc32fast` / `adler32` oracle
- [x] **数学物理边界**:CRC ~30 GB/s DRAM,nupic 2.44 GB/s,distance 12×
- [x] **跨平台 bit-exact**:测套验证 == `crc32fast` v1 (which == zlib)
- [x] PNG-first:CRC-32 是 PNG `IDAT` chunk checksum / Adler-32 是 zlib stream checksum;bit I/O 给 DEFLATE 用
- [x] 不腐性:测 RFC fixtures + property(对 spec)不测 LUT 内 byte 值

stage 0 graduate ✓。

---

## 7. cross-link

- [`docs/roadmap.md` 阶段 0](../../roadmap.md) — stage 0 出现位置
- [`docs/png-pipeline.md` §3](../../png-pipeline.md) — DEFLATE 算法
- 实施:[`crates/nupic-bits/`](../../../crates/nupic-bits/)
- 测套:[`crates/nupic-bits/tests/properties.rs`](../../../crates/nupic-bits/tests/properties.rs)
- bench:[`crates/nupic-research/examples/nupic_bits_bench.rs`](../../../crates/nupic-research/examples/nupic_bits_bench.rs)
- oracle:`crc32fast` v1 / `adler32` v1(dev-deps only,not runtime)

---

## 8. 下一步 — 阶段 1:`nupic-deflate`

按 [`docs/roadmap.md`](../../roadmap.md) 顺序,stage 1 = self-built
DEFLATE encoder。Dependency:nupic-bits(本 stone,刚 graduate)。

阶段 1 是真正的 zlib / zopfli replacement → 自研 PNG IDAT 链条第一步。
预计 substantial — LZ77 dictionary + Huffman code 都要 reimpl,跟 zopfli
match 是 graduation 目标(zopfli output ≤ 5% larger than nupic-deflate
on standard corpus)。

不在 0.5.x 范围;**0.6.x 主线候选**。

---

## 9. polish backlog(stage-0 post-graduation)

按 perf 优先,**不阻塞 stage 1 起手**:

- **S0.1 NEON `vmull_p64`** — arm SIMD polynomial mul,~ 8–10 GB/s CRC 目标
- **S0.2 x86 AVX2 `_mm_clmulepi64_si128`** — x86 SIMD CRC
- **S0.3 tile + prefetch** — for 4K+ buffers
- **bit I/O bench** — current essay doesn't measure;`nupic-deflate`
  阶段会让 it matter,届时加 bench

---

## 10. 验收材料

- crate:[`crates/nupic-bits/`](../../../crates/nupic-bits/)
- 测套:`crates/nupic-bits/tests/properties.rs` — 11 props + 2 doc tests
- bench raw:`target/research-out/05-nupic-bits-bench.csv`
- 价值观:
  - [[feedback-ceiling-first-priorities]] — 跑 ceiling 数字 + 距离
  - [[feedback-no-cost-thinking]] — 4.25× slower than crc32fast 标 polish
    backlog,不卡 graduation
- 触发本 essay 的 long-standing plan:
  [next-step-research-0_5_x](../../../../../.claude-profile-1/projects/-Users-doracawl-workspace-labs-lab29-nupic/memory/next_step_research_0_5_x.md)
  — 原 0.5.x 计划被 PNG research 抢走,现在 stone A/B/C 都 graduate 后
  回到 nupic-bits
