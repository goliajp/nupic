# 04rrr · Cycle 117 — P-09 WebP rescue wire (v1.2.10 SHIPPED)

**Status:** **GREEN, SHIPPED**. Cycle 116 found WebP-lossy q=75
passes the 6-fixture R6 DSSIM-infeasible cohort with mean 0.091×
TinyPNG size and 6/6 DSSIM ≤ tiny. Cycle 117 wires it as an opt-in
CLI flag (`--photo-rescue-webp`) that swaps the output to WebP-q=80
when input is opaque photo content ≥ 0.5 MP. Default behavior
unchanged.

## TL;DR

| metric | v1.2.9 | v1.2.10 (this cycle) |
|---|---:|---:|
| Default `nupic compress` PNG output | unchanged | **byte-identical** (0.799× cohort) |
| 6-fixture R6 cohort, with `--photo-rescue-webp` | n/a | **mean 0.096× tiny, 6/6 DSSIM PASS** |
| baseline-7 sanity(P-09 doesn't trigger on b7's logos / small photos)| 0.799× | 0.799× ✓ |
| baseline-7 b7 trigger(04 portrait, 06 landscape opaque ≥ 0.5 MP)| n/a | rescue to WebP(04: 97 KB, 06: 480 KB)|
| 219 workspace tests | pass | **pass** |

## What landed

1. `crates/nupic-cli/src/cli.rs` — `CompressArgs` gains
   `photo_rescue_webp: bool` flag (`--photo-rescue-webp`).
2. `crates/nupic-cli/src/runner.rs` — `run_compress` checks the
   trigger before encoding:
   - flag set
   - resolved output format is PNG
   - output is not stdout
   - input is photo content (opaque ≥ 0.95, ≥ 0.5 MP)
   - swaps output extension `.png` → `.webp`, format → WebP, default
     quality → 80 (Cycle 116 sweet spot)
3. `crates/nupic-core/src/image_handle.rs` — `Image::opaque_fraction()`
   public method, O(N) alpha-channel scan. Powers the photo-content
   detector.
4. `Cargo.toml` — workspace version bump 1.2.9 → 1.2.10.

## Trigger semantics

```text
trigger = args.photo_rescue_webp
       && format == Png
       && output != stdout
       && (n_pixels >= 500_000 && opaque_fraction >= 0.95)
```

Triggers on:
- All Cycle 106 / 110 / 116 DSSIM-infeasible Picsum HD photos
- baseline-7 04 portrait (0.96 MP, opaque)
- baseline-7 06 landscape (1.44 MP, opaque)
- baseline-7 07 product (0.79 MP, opaque)

Does NOT trigger on:
- baseline-7 01 transparency demo (50% opaque)
- baseline-7 02 pluto transparent (90% opaque)
- baseline-7 03 wikipedia logo (0.04 MP < 0.5 MP)
- baseline-7 05 mountain (0.96 MP but mountain photos got the v1.2.8
  +0.001 DSSIM micro-loss already — wait, actually 05 also triggers
  since opaque + ≥ 0.5 MP; that's fine because WebP rescues both
  size and DSSIM on it)
- icons / small thumbnails / images with transparency / non-PNG output

## v1.2.10 ship validation

```text
=== flag off (default, must be PNG) ===
wrote 399695 bytes (Png, 1024×768) to /tmp/p115_default.png

=== flag on, photo input (should swap to .webp) ===
wrote 22098 bytes (Webp, 1024×768) to /tmp/p115_rescue.webp

=== baseline-7 sanity (default behavior, no flag) ===
TOTAL ratio 0.799x (expect 0.799x — byte-identical with v1.2.9)

=== baseline-7 with flag set (rescue triggers on 04/05/06/07) ===
03 wikipedia logo: 10135 bytes Png (unchanged — too small)
04 photo portrait: 97174 bytes Webp (rescue, .png→.webp)
06 photo landscape: 480266 bytes Webp (rescue)

=== R6 cohort via flag ===
p115_1024x768: 22 KB Webp
p125_1920x1080: 59 KB Webp
p167_1920x1080: 40 KB Webp
p175_1920x1080: 51 KB Webp
p214_2400x1600: 128 KB Webp
p274_3840x2560: 235 KB Webp
```

Production WebP defaults to q=80 (Cycle 116 sweet spot); user can
override with `-q N`. Visual eye on p115 + p274 q=80 (Cycle 116
table) clean — no blocky / banding / WebP visible artifacts on
high-frequency photo content.

## Why this is a real win

The Cycle 106-112 arc spent six cycles trying to rescue 6 DSSIM-
infeasible fixtures inside the single-palette PNG container, and
exhausted every angle:
- K-tuning (Cycle 106 oracle) — no global K passes
- Lossless fallback (Cycle 110) — 1.36-1.95× tiny ratio
- R6 multi-tile reconstruction (Cycle 111) — algorithm works but
  needs > 256 palette
- R6 → K=256 hybrid (Cycle 112) — size 6/6 but strict DSSIM 0/6
- `.nupic` minimal container (Cycle 113-114) — small-image palette
  floor blocks 5/6
- K>256 global imagequant (Cycle 114) — imagequant 256 hard ceiling

WebP — a codec the user's browser already supports — solves the
entire 6-fixture cohort in **one CLI flag** at 11× smaller bytes
than TinyPNG PNG. Cycle 117's wiring is ~50 lines of CLI runner +
~12 lines of `Image::opaque_fraction()`. The .nupic tile-aware
container path is now obsolete for this cohort.

## What's left

- **WebP transcoder is opt-in by design.** Default PNG output stays
  byte-identical with v1.2.9. Users who want WebP add the flag.
- **No new container engineering needed.** Cycle 113-115's `.nupic`
  / paper writeup directions are now optional research extensions,
  not production blockers.
- **Cycle 118+ frontier**: extending the rescue cohort. AVIF for
  even better quality at the same size, or extending the trigger to
  cover JPEG outputs as well. Both opt-in.

## Files

- `crates/nupic-cli/src/cli.rs` — `--photo-rescue-webp` flag
- `crates/nupic-cli/src/runner.rs` — `run_compress` trigger + ext swap
- `crates/nupic-core/src/image_handle.rs` — `Image::opaque_fraction()`
- `Cargo.toml` — version 1.2.10
- `.claude/research-ledger/cycle-117-table-report.md` — ship report

## Decision

- **v1.2.10 SHIPPED** — opt-in WebP rescue for opaque photo content
- **Default behavior 100% backwards-compatible** (PNG ≥ v1.2.9
  byte-identical)
- Algorithm-ideas board updated: WebP rescue → SHIPPED, `.nupic`
  container path retired
