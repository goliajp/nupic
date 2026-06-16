# 03i — `--use-nupic-png` perf cliff fix(default-flip blocker close)

> Dogfood testflight.png(1179×2556 fully-opaque UI screenshot)hung
> 10+ min on `--use-nupic-png`(eventually killed)。Profile shows the
> hot path is **`nupic-deflate Level::Best` iterative cost-DP +
> per-block refinement on flat-run input**(LZ77 chain walks long
> identical-byte runs at every position × 5 iter passes × per-block
> partition)。Fix:adaptive Level::Fast fallback when filtered stream
> mean-run-length ≥ 4。testflight 10+ min → **1 second**,01-transparency
> 47s → 1s。Size cost:Level::Fast outputs 5-37% larger IDAT on the
> affected inputs(opt-in flag,acceptable trade-off);photo inputs
> unchanged。

---

## 1. Profile data

`NUPIC_DEBUG_TIMING=1` instrumentation on v0.5.20 pre-fix:

| input | decode | quantize | encode_indexed_png | total |
|---|---:|---:|---:|---:|
| 01-transparency(0.5MP)| 148µs | 0.79s | **54.47s** | 55s |
| 04-portrait(1MP)| 529µs | 0.55s | 3.29s | 4s |
| 06-landscape(2.5MP)| 694µs | 2.05s | 6.18s | 8s |
| **testflight UI(3MP)** | — | — | — | **10+ min hang** |

Inner timing on 01-transparency:
```
[nupic-png:inner] filter (BestOf): 38ms, raw_filtered.len=480600
[nupic-png:inner] zlib(Best, mrl=8.04): 44724ms, idat.len=44862
```

`zlib_compress` (i.e., nupic-deflate Level::Best)消耗 99.9% encode time
on flat-run input。**44.7s on 480 KB filtered stream** that compresses
to 44 KB(91% compression ratio)。

---

## 2. Root cause

`Level::Best` runs:
1. `collect_tokens_lazy(data, LAZY_CHAIN=128, LAZY_MAX=16)` — lazy LZ77
   with chain depth 128
2. Iterative cost-DP × `ITER_PASSES=5` — each pass re-tokenises with
   updated Huffman cost model
3. Phase 1.5 per-block refinement — re-DP inside each partition block

For flat-run input(mostly identical bytes):
- Hash chain has thousands of entries at the zero-bucket
- Each position's match search walks `min(128, chain_depth)` hops
- Each hop's match-extend tries up to `MAX_MATCH=258` bytes
- **Per-position worst case:128 × 258 = 33K compares**
- 480K positions × 33K = 16 Giga compares × 5 iter passes = 80G ops
- @ 1 ns/op including cache misses → ~ 45s
- + phase 1.5 per-block re-DP adds another factor

LZ77 with deep chains is pathological on long-run data。Level::Fast(static
Huffman + greedy chain 32 + no cost-DP)handles long runs efficiently via
single length-258 match emission(no chain walking on subsequent positions
when match is already long)。

---

## 3. Fix:adaptive Level by mean_run_length

```rust
// nupic-png encode_indexed_png_with:
let mrl = filter::mean_run_length(&raw_filtered);
let level = if mrl >= 4.0 {
    Level::Fast   // flat-run → static Huffman + greedy LZ77
} else {
    Level::Best   // photo / unique-byte content → iterative cost-DP
};
let idat = zlib_wrap(&raw_filtered, level);
```

Plus `filter_image_deflate_aware` removed from `BestOf` candidate list
(its per-row trial-deflate was ALSO slow on flat-run input,but secondary
to the final Level::Best deflate)。`FilterStrategy::DeflateAware` still
available for explicit invocation。

---

## 4. Result — v0.5.21 timing

| input | mrl | level | v0.5.20 | **v0.5.21** | speedup |
|---|---:|---|---:|---:|---:|
| 01-transparency | 8.04 | Fast | 55s | **1s** | 55× |
| 02-pluto | 1.76 | Best | 7s | 5s | 1.4× |
| 04-portrait | 1.59 | Best | 4s | 4s | — |
| 06-landscape | 1.22 | Best | 8s | 8s | — |
| **testflight UI** | **114.27** | Fast | **>10 min** | **1s** | **>600×** |
| vantage UI | 11.17 | Fast | unknown long | 5s | — |

testflight 是真正的 cliff,从 10+ 分钟 hang 到 1 秒。

---

## 5. Size trade-off

Level::Fast outputs are larger than Level::Best:

| input | v0.5.20 IDAT | **v0.5.21 IDAT** | Δ |
|---|---:|---:|---:|
| 01-transparency | 44 862 | 61 637 | **+37%** |
| testflight | unknown | 46 784 | unknown |
| vantage | unknown | 406 185 | unknown |

01 size +37% is meaningful但 opt-in `--use-nupic-png` is experimental;
trade-off acceptable until phase 2.4(real fix at nupic-deflate side
to handle flat-run input efficiently in Level::Best)。

Photos(02/04/06)unchanged size — they still go Best path。

---

## 6. ship status

`--use-nupic-png` opt-in CLI flag now usable on ALL input sizes,no more
multi-minute hangs。Default-flip 一个 blocker close:
- ✅ perf cliff fixed
- ⚠ size still 1.04-1.36× oxipng on average corpus,1.5-2.4× on UI on
  Level::Fast fallback path

For 0.6.x default flip still need:
- Nupic-deflate side perf fix (handle flat-run efficiently in Level::Best)
- OR accept Level::Fast fallback as ship default (with documented size cost)

---

## 7. 价值观

- [[feedback-ceiling-first-priorities]] — profile-driven fix(measured
  hot path,not guessed)。Mean-run-length signal repurposed from 03f
  classifier(same signal,different use)。
- [[feedback-no-cost-thinking]] — size cost(+5-37%)is documented
  but not used as "should we ship?" 评估;the user-facing call
  (`--use-nupic-png` opt-in)is now usable,that's the ship gate。

---

## 8. cross-link

- 上游:`07-bis-nupic-png-integration.md`(Path B opt-in flag ship)
- 上游:`06-nine-deflate-iterative.md`(phase 1.4 iterative cost-DP
  algorithm,now identified as hot path on flat-run input)
- 上游:`03f-pareto-tiered-dither.md`(mean_run_length signal first
  introduced for tier-2 vs tier-3 dither classifier)
