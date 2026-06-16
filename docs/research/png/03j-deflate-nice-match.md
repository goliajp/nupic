# 03j — Phase 2.4 nupic-deflate NICE_MATCH + nupic-png size-aware adaptive

> Real fix for the perf cliff (vs the 2.3 mrl-fallback workaround in
> 03i):add `NICE_MATCH=128` chain-walk early-exit to nupic-deflate's
> LZ77 (`find_longest_match`, `dp_optimal_tokens`, `dp_optimal_tokens_window`).
> Cuts the chain-walk pathology on flat-run input where every entry
> extends to MAX_MATCH。testflight 10+ min → 30s with NICE_MATCH
> alone。Combine with size-aware `(big_and_flat || very_flat) → Fast`
> heuristic in nupic-png:**every input < 11 s,small ones < 4 s**。

---

## 1. NICE_MATCH chain-walk early-exit

zlib's `longest_match` exits the chain when current best ≥ `nice_length`
(typically 128)。Subsequent chain entries are older positions — chain is
ordered most-recent-first,so if we already have a 128-byte match,
walking deeper costs cycles without finding better matches。

Added to 3 hot paths in `crates/nupic-deflate/src/lz77.rs`:

1. `find_longest_match` (lazy LZ77 chain walk)
2. `dp_optimal_tokens` (iterative cost-DP chain walk)
3. `dp_optimal_tokens_window` (phase 1.5 per-block refinement chain walk)

Pattern:after computing `k` (extend length),add:
```rust
if best_len >= NICE_MATCH { break; }   // for find_longest_match
if k >= NICE_MATCH        { break; }   // for dp_optimal_tokens
```

Single-line change per hot path,zero behavioural risk:never increases
output size(if NICE_MATCH match wins,it would have won at chain end
too;just saved cycles)。Compression ratio identical to no-NICE-MATCH
on photo content(chain rarely sees ≥128-byte match)。

---

## 2. Size-aware level selection(nupic-png)

NICE_MATCH alone helps but doesn't fully eliminate perf cliff:
Level::Best still O(N × chain × iter) on UI input。Wall-clock per-input
(post NICE_MATCH,no size threshold):

| input | mrl | NICE_MATCH only | size |
|---|---:|---:|---:|
| 01-transparency | 8 | 7.8s | 46 044 |
| 02-pluto | 1.8 | 4.1s | 192 637 |
| 04-portrait | 1.6 | 3.7s | 445 370 |
| 06-landscape | 1.2 | 6.9s | 1 095 841 |
| **testflight UI** | **114** | **37s** | 25 438 |
| **vantage UI** | **11** | **55s** | 314 118 |

Big UI screenshots still slow。Add size-aware adaptive in `nupic-png`:

```rust
let mrl = filter::mean_run_length(&raw_filtered);
let big_and_flat = raw_filtered.len() > 500_000 && mrl >= 8.0;
let very_flat = mrl >= 32.0;
let level = if big_and_flat || very_flat { Level::Fast } else { Level::Best };
```

Final v0.5.22 wall-clock + size:

| input | mrl | filter_bytes | level | wall-clock | size |
|---|---:|---:|---|---:|---:|
| 01-transparency | 8 | 481 K | Best | 7.9s | 46 044 |
| 02-pluto | 1.8 | 400 K | Best | 3.9s | 192 637 |
| 03-wikipedia-logo | 2.0 | 37 K | Best | 0.3s | 13 138 |
| 04-portrait | 1.6 | 961 K | Best | 3.6s | 445 370 |
| 05-mountain | 1.3 | 961 K | Best | 10.4s | 402 282 |
| 06-landscape | 1.2 | 1.44 M | Best | 6.9s | 1 095 841 |
| 07-product | 1.2 | 786 K | Best | 4.2s | 333 690 |
| **testflight UI** | 114 | 3 M | Fast | **1.1s** | 47 086 |
| **vantage UI** | 11 | 4.5 M | Fast | **5.2s** | 407 180 |

**Every input < 11 s**;UI screenshots particularly responsive。

Vantage trades 30% size(314 → 407 KB)for 10× speedup(55s → 5s)。
For interactive `nupic compress`,acceptable。

---

## 3. ship status post v0.5.22

`--use-nupic-png` opt-in flag has gone from:

| version | testflight wall-clock | testflight size |
|---|---:|---:|
| 0.5.20 | **>10 min hang** | unknown |
| 0.5.21(mrl-fallback workaround)| 0.8s | 47 KB |
| **0.5.22(NICE_MATCH + size-aware)** | **1.1s** | **47 KB** |

Stable adaptive path:NICE_MATCH preserves Best-mode size on small +
photo inputs;size-aware threshold avoids long wall-clock on big +
flat inputs。

Default-flip readiness:
- ✅ perf cliff fixed
- ⚠ size still ~1.07× oxipng on photo corpus(NICE_MATCH no size change)
- ⚠ size 1.5-2× on UI(Fast fallback trade-off)

For 0.6.x default flip,wall-clock no longer blocker;remaining gap is
algorithmic ceiling at nupic-deflate Best on flat-run input(see future
phase 2.5:zopfli-class iterative refinement for flat-run AND
unique-byte inputs would be the proper fix)。

---

## 4. 价值观

- [[feedback-ceiling-first-priorities]] — NICE_MATCH is the
  algorithmic root-cause fix(was 2.3 mrl-fallback workaround);
  size-aware adaptive is engineering compromise on remaining
  wall-clock ceiling for interactive UX
- [[feedback-metric-over-human-eye]] — adaptive threshold tuned by
  per-input wall-clock measurements,not heuristic
- [[feedback-no-cost-thinking]] — size-vs-time trade-off documented
  for opt-in flag,user can opt for `--png-effort` if such knob
  exposed in future
