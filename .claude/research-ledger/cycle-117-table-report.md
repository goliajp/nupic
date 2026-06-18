# Cycle 117 — P-09 WebP rescue wire — table 收尾报告

**Date**: 2026-06-19
**Verdict**: **GREEN, v1.2.10 SHIPPED**
**Essay**: `docs/research/png/04rrr-cycle117-webp-rescue-ship.md`
**Files**:
- `crates/nupic-cli/src/cli.rs` — `--photo-rescue-webp` flag
- `crates/nupic-cli/src/runner.rs` — trigger + extension swap
- `crates/nupic-core/src/image_handle.rs` — `Image::opaque_fraction()` public
- `Cargo.toml` — bump v1.2.10

## 1. Trigger semantics

`--photo-rescue-webp` opt-in。trigger 当所有 5 条同时满足:
1. flag set
2. resolved output format == PNG
3. output ≠ stdout
4. n_pixels ≥ 500_000(0.5 MP)
5. opaque_fraction ≥ 0.95

action:format → WebP, extension `.png` → `.webp`, quality default → 80(Cycle 116 sweet spot)。

## 2. Triggers on / does NOT trigger on

| fixture | n_pixels | opq | flag-on output | trigger? |
|---|---:|---:|---|:---:|
| baseline-7 01 trans demo | 0.48 MP | < 0.95 | PNG 35 KB | ✗ |
| baseline-7 02 pluto | 0.40 MP | ~ 0.90 | PNG 59 KB | ✗ |
| baseline-7 03 wiki logo | 0.04 MP | < 0.95 | PNG 10 KB | ✗ |
| baseline-7 04 photo portrait | 0.96 MP | 1.0 | **WebP 97 KB** | ✓ |
| baseline-7 06 photo landscape | 1.44 MP | 1.0 | **WebP 480 KB** | ✓ |
| R6 cohort p115 | 0.79 MP | 1.0 | **WebP 22 KB** | ✓ |
| R6 cohort p274 | 9.83 MP | 1.0 | **WebP 235 KB** | ✓ |

opt-in only — 默认行为 100% backwards-compatible。

## 3. v1.2.10 ship validation

| gate | result | OK |
|---|---|:---:|
| baseline-7 default(no flag,must match v1.2.9)| 0.799× cohort byte-identical | ✓ |
| 219 workspace tests | 219 pass 0 fail | ✓ |
| R6 cohort flag-on(should rescue all 6)| 6/6 WebP output | ✓ |
| baseline-7 flag-on(trigger predicate correct)| only opaque photo ≥ 0.5 MP swap | ✓ |
| binary version | nupic 1.2.10 | ✓ |

## 4. Cycle 116-117 完整 arc

| cycle | finding | role |
|---|---|---|
| 116 | WebP q=75 PASS 6/6 R6 cohort (mean 0.091× tiny) | algorithm verified |
| 117 | `--photo-rescue-webp` wire + v1.2.10 ship | production ship |

## 5. Cohort projection

P-09 仅对 opaque photo ≥ 0.5 MP 触发,且需要 user opt-in flag。corpus-500 中触发面:
- Pile A 307(大部分 Picsum HD photo,trigger 几乎全部)
- Pile B 40,Pile C 53 中 photo subset
- 加 baseline-7 04/05/06/07(4 张)
- 预估 `~300-350 fixture` trigger,几乎全部 11× smaller WebP

**但 opt-in only** — user 不 set flag → 默认 PNG path 不变,v1.2.9 byte-identical。

## 6. Algorithm-ideas board 更新

**WebP transcoder rank 1 → SHIPPED v1.2.10**

| 候选 | status |
|---|---|
| **J. 2-pass K-up fail-safe** | SHIPPED v1.2.9 |
| **WebP transcoder** | **SHIPPED v1.2.10 ✨** |
| C. slow-tier zopfli flag | open(Cycle 118 候选)|
| `.nupic` container | **retired**(WebP 取代)|
| paper writeup | optional research,not production blocker |
| preset=6 perf 优化 | deprecated(+1pp 不值)|

## 7. Cycle 118+ next-up

- **C. slow-tier `--effort 9` zopfli flag**(1 cycle,opt-in)
- 或 AVIF transcoder(类似 WebP wire 但 AVIF 在某些 fixture 更紧)
- 或 paper writeup(optional)

推荐 Cycle 118 = AVIF transcoder probe(类似 Cycle 116 WebP 流程,看 AVIF 是否更紧)。
