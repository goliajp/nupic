# 04mmm · Cycle 108 — input-feature K classifier(YELLOW,1 fixture retention break → Cycle 109 2-pass fail-safe)

**Status:** **YELLOW**(99.1% PASS retention,1 fixture regression,
net +14 PASS).

Cycle 107 closed the door on K=224 single-config default(PASS pile
−16-25%). Cycle 108 tested whether input-only features (`n_pixels`
threshold) could route to K=224 only on big content and avoid the
small-image regression. Full corpus-500 verdict on rule v3
(`n_pixels ≥ 5MP → K=224 d=0.3 p=6`):

- Total PASS **106 → 120 / 506(20.9% → 23.4%, +14 fixture)**
- **PASS pile: 105/106 retained(99.1%)** — 1 regression: `p244`
- baseline-7: 4/7(unchanged from v1.2.8 baseline)
- Pile A wins: 11/307(3.6% — all 9.83 MP HD photos)
- Pile B/C wins: 0/40 + 0/53

The single regression (p244) is structural: its v1.2.8 K=128 output
was already `0.791× tiny`(just inside the cap), and K=224 pushes it
to `0.851×`. **No input-only feature cleanly separates p244 from the
11 wins**(bpp / luma / chroma all overlap), so input-feature
classification has hit a ceiling at 99.1%.

User decision: **path B**(strict 100% retention,no PASS pile
regression). Cycle 109 implements **2-pass fail-safe routing** —
quantize at K=128 first(production default), measure output size; if
output size still > 0.80× cap-proxy, retry at K=224 and pick the
smaller. ~1.5× wall on big content,zero retention break.

## TL;DR

| spike | scope | PASS rate | PASS pile retention | baseline-7 | verdict |
|---|---|---:|---:|---:|:---:|
| Rule v1: n_pixels<2M→K128 raw(skips P-01/P-03)| 32 sample + b7 | 6/39(15.4%)| **2/8 (25%)** | 2/7 | **RED**(spike bypassed production routing)|
| Rule v2: + subprocess for b7 | 32 sample + b7 | 12/39(30.8%)| 7/8(87.5%)| 4/7 ✓ | **RED**(p220 2.07MP regression at 2M threshold)|
| Rule v3: threshold 5MP | 32 sample + b7 | 13/39(33.3%)| **8/8 (100%)** ✓ | 4/7 ✓ | **YELLOW**(sample passes)|
| Rule v3 full corpus-500 | 513 fixtures | 120/513(23.4%)| **105/106 (99.1%)** | 4/7 ✓ | **YELLOW**(1 regression: p244)|

## Spike progression

### Rule v1 — direct quantize(WRONG)

```text
K = 128 if n_pixels < 2_000_000 else 224
```

First implementation called `quantize_indexed_png(K=128, d=0.0, p=6)`
directly for the small branch. **This bypasses v1.2.8 production
routing**(P-01 trans / P-03 logo / gradient-lossless overrides from
Cycle 102-105). Result: PASS pile 2/8, baseline-7 2/7 — far worse
than v1.2.8 itself.

**Lesson:** spike's "keep small images as v1.2.8" must call the
actual production binary (`nupic compress` subprocess) or use the
cached baseline data, not re-quantize. Naive `K=128 d=0.0` is **not**
v1.2.8.

### Rule v2 — subprocess for baseline-7, baseline cache for pile sample

```rust
// pile sample: small branch uses Fixture.baseline_* (cached v1.2.8 data)
// baseline-7: small branch runs `nupic compress` subprocess
```

PASS pile 7/8(87.5%)— `p220` (3.84 MP) regressed. baseline-7 4/7
(v1.2.8 itself is 4/7,unchanged ✓). YELLOW but not strict.

### Rule v3 — threshold 5MP

```rust
fn pick_kd(n_pixels: u64) -> (usize, f32) {
    if n_pixels < 5_000_000 { (128, 0.0) }     // keep v1.2.8 + production overrides
    else { (224, 0.3) }                         // K-up only on HD photo (≥ 5MP)
}
```

32-sample bench: PASS pile **8/8 ✓**(all sub-5MP), baseline-7 4/7 ✓,
total 33%.

Full corpus-500: **1 regression** (`p244`), 11 Pile A wins, net +14.

## Why p244 regresses

| fixture | input_KB | v1.2.8 KB | tiny_KB | v1.2.8 ratio | K=224 ratio | verdict |
|---|---:|---:|---:|---:|---:|:---:|
| p244_3840x2560 | 6235 | **1778** | 2247 | **0.791× ✓** | **0.851× ✗** | regression |
| p245_3840x2560(WIN ref) | 3932 | 2671 | 1944 | 1.374× ✗ | 0.665× ✓ | win |

p244's v1.2.8 K=128 was already 79% of tiny — narrow PASS. K=224's
larger palette pushes size up by ~5-6%, breaking the 80% cap.

Tried input features to distinguish p244 from wins:

| feature | p244 | win range(11 fixtures) | discriminative? |
|---|---:|---|:---:|
| bits_per_pixel(input PNG)| 5.20 | 1.62 - 5.14 | partial(p246=5.14 too close)|
| bits_per_pixel(v1.2.8 output)| 1.48 | 0.97 - 3.63 | no(overlaps p287=2.14)|
| n_pixels | 9.83 MP | 9.83 MP | no(same)|
| luma / chroma entropy | (not extracted)| — | likely overlap |

**Conclusion:** no clean input-only feature. The true discriminator is
**v1.2.8 baseline output size**, which production sees only via a
2-pass quantize.

## Cycle 109 path B — 2-pass fail-safe

```text
1. Quantize at production K=128 (incl. P-01/P-03/gradient routing)
   → bytes_v128, size_v128
2. If n_pixels >= 5_000_000 AND size_v128 > 0.78 × input_size:
   2a. Quantize at K=224 d=0.3 → bytes_v224, size_v224
   2b. Pick min(bytes_v128, bytes_v224)
3. Else: ship bytes_v128
```

`size_v128 > 0.78 × input_size` is the production-side proxy for
"this image's K=128 output isn't dropping bytes enough" — input PNG
size is a free signal, no tiny-baseline needed. Threshold 0.78 keeps
p244-class fixtures (already-narrow PASS) on the v1.2.8 path.

**Production cost:** ~1.5× wall on ≥ 5MP content(~14.8% of corpus).
Per the perf NAS/CDN target(5MP < 250ms, RSS < 100MB),this fits
inside the budget on 9.83MP photos(~750ms total).

**Expected verdict:** 100% PASS pile retention,Pile A wins ≥ 11
(probably more — 2-pass catches mid-size photos current rule misses).

## Files

- `crates/nupic-research/examples/cycle108_input_k_classifier.rs` —
  rule v1/v2/v3 spike with env-mode toggle (`CYCLE108_MODE=sample` or
  `full`)
- `crates/nupic-research/src/bench.rs` — bench helpers used by spike
  (`pile_sample_24`, `bench_pool`, `Fixture` pre-loaded baselines)
- `assets/png-bench/cycle108/rule_v{1,2,3,3_full}.{tsv,log}` —
  per-rule data
- `.claude/research-ledger/cycle-108-table-report.md` — table verdict

## Decision

- **No v1.2.9 ship this cycle** — rule v3 is YELLOW (99.1% retention,
  not 100%). Path A (ship YELLOW with 1 regression) was offered but
  user chose path B (strict).
- **Cycle 109 = 2-pass fail-safe production wiring** at
  `nupic-core/src/ops/compress.rs:225`. Add K-up trigger:
  `if n_pixels >= 5M && size_v128 > 0.78 * input_size → retry K=224
  and pick smaller`.
- **219 workspace tests + baseline-7 + 32 quick bench + full
  corpus-500** all must pass before v1.2.9 bumps.
