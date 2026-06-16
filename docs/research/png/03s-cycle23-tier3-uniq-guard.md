# 03s — Cycle 23: tier-3 uniq-color guard (v0.5.37)

## Background

Cycles 17+ exhausted the easy ceilings on the original 7-fixture
corpus. To find new attack surfaces, generated an 8-fixture
extended set (`assets/png-bench/inputs-ext/`) covering:

- 08 gradient-large (2400×1600 synthetic RGB gradient, opaque)
- 09 ui-checker-text (1920×1080 UI mockup with text bars)
- 10 comic-flat (1200×900 hard-edged flat regions)
- 11 photo-noisy (05-mountain with per-pixel noise added)
- 12 tiny-icon (64×64 colorful icon, transparent BG)
- 13 very-large-photo (3600×2400 = 8.64 MP upscaled 05)
- 14 soft-transparent (800×600 photo with alpha gradient)
- 15 mono-text (1024×768 grayscale text-like)

## Misclassification found

`classify_for_auto_dither` returned d=0.25 for 08-gradient-large,
producing SSIM 37.72 / 190 KB. Per-d sweep on 08:

| d | size | SSIM |
|---|---|---|
| 0.00 | 23694 | **0.59** (severe banding) |
| 0.25 | 190404 | **37.72** ← auto picked |
| 0.50 | 364399 | 58.98 |
| 0.70 | 496883 | **68.08** (peak) |

Root cause: 08's RGB gradient `r = x * 255 / 2400` produces ~10
adjacent pixels with the same r value (integer division), triggering
`mean_run > 2.0` → tier-3 (UI screenshot) → d=0.25.

But 08 is photo-class content needing tier-4 dither, not flat-region
UI. The mean_run signal is fooled by synthetic-gradient quantization.

## Signal sweep across all 15 fixtures

| fixture | mean_run | uniq | adj_var | content class |
|---|---|---|---|---|
| 09 ui-checker | 15.96 | **5** | 234 | true UI |
| 10 comic-flat | 444.63 | **5** | 24 | comic |
| 15 mono-text | 24.92 | **3** | 1501 | text |
| 03 wiki-logo | 1.99 | **129** | 484 | logo |
| 08 gradient | 6.13 | **117045** | 0.1 | gradient (photo-class) |
| 04 portrait | 1.18 | **25514** | 34 | photo |
| 05 mountain | 1.29 | **224691** | 320 | photo |
| 11 photo-noisy | 1.00 | **614865** | 297 | photo |

**uniq RGB color count** cleanly separates: UI / logo / text ≤ 130,
photo / gradient ≥ 4348. Big gap between 129 and 4348 — threshold
1000 safe.

## Fix

`classify_for_auto_dither` tier-3 branch now requires BOTH:
- `mean_run > 2.0` (low-frequency content)
- AND `uniq < 1000` (limited palette characteristic of UI/logo/text)

Implementation: one O(N) pass with HashSet early-exit at 1000:

```rust
if mean_run > 2.0 {
    let mut uniq = HashSet::with_capacity(1024);
    for p in src_rgba.chunks_exact(4).step_by(step_u) {
        if p[3] != 255 { continue; }
        uniq.insert((p[0] as u32) | ((p[1] as u32) << 8) | ((p[2] as u32) << 16));
        if uniq.len() >= 1000 { break; }
    }
    if uniq.len() < 1000 {
        return 0.25; // genuine tier-3
    }
    // else fall through to tier-4
}
```

## Result

Original 7-fixture corpus: **bit-exact identical** (all are tier-1/2
or true tier-4 paths; none triggered the new branch).

Extended 8-fixture corpus:

| fixture | pre-fix size/SSIM | post-fix size/SSIM | delta |
|---|---|---|---|
| **08 gradient-large** | 190404 / **37.72** | 364399 / **58.98** | **+21.26 SSIM** / +174 KB |
| 09 ui-checker | 2805 / 100 | 2805 / 100 | unchanged ✓ |
| 10 comic-flat | 3039 / 100 | 3039 / 100 | unchanged ✓ |
| 11 photo-noisy | 674431 / 81.47 | 674431 / 81.47 | unchanged ✓ |
| 12 tiny-icon | 302 / 100 | 302 / 100 | unchanged ✓ |
| 13 very-large | 2697707 / 66.52 | 2697707 / 66.52 | unchanged ✓ |
| 14 soft-trans | 148361 / 66.90 | 148361 / 66.90 | unchanged ✓ |
| 15 mono-text | 2882 / 100 | 2882 / 100 | unchanged ✓ |

08 alone: SSIM **+21.26** (37.72 → 58.98). Not yet at peak (d=0.7
sweep = 68.08), but a big step up. Tier-4a's d=0.5 picked because
08's var_diff = 0.1 (extremely smooth gradient, no local variance).
A future Cycle could add adj_mn < 1.0 → d=0.7 to catch
gradient-class content specifically.

219 workspace tests pass.

## Remaining ceilings (found by extended corpus)

| fixture | currently | sweep best | gap |
|---|---|---|---|
| 08 gradient | d=0.5, SSIM 58.98 | d=0.7, SSIM 68.08 | +9 SSIM possible |
| 13 very-large | d=0.5, SSIM 66.52 | d=0.7, SSIM 68.84 | +2.3 SSIM |
| 14 soft-trans | d=0, SSIM 66.90 | d=0.5, SSIM 70.0 | +3 SSIM |
| 11 photo-noisy | d=0.7, SSIM 81.47 | (TBD probe) | — |

Cycle 24+ can target these.

## Files

- `crates/nupic-quantize/src/lib.rs` — uniq guard in tier-3 branch
- `crates/nupic-research/examples/gen_ext_corpus.rs` — 8 new fixtures
- `crates/nupic-research/examples/probe_classify_signals.rs` — signal table
- `crates/nupic-research/examples/probe08_classify.rs` — initial diagnosis
- `crates/nupic-research/examples/cycle22_01_classify.rs` — (prior cycle)
- `assets/png-bench/inputs-ext/*.png` — 8 generated fixtures
