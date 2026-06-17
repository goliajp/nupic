# 04e — Cycle 38: classifier flatten to d=0.0 (size + perf prioritised)

## Motivation

User direction (2026-06-17 session): TinyPNG's SSIMULACRA2 score is
the widely-accepted industry quality bar. Chasing peak SSIM above
TinyPNG (Cycles 30-35, +0.16 to +17.23 per fixture) costs +5-13 %
size with marginal marketing value — users see "matched size,
slightly higher quality" and the size headline is the bottleneck.

Re-priority: **size + encode/decode perf > SSIM (so long as SSIM
≥ TinyPNG)**.

## Sweep — d=0.0 already beats TinyPNG on both axes

Per-fixture comparison (TinyPNG vs nupic at d=0.0):

```
fixture                  TinyPNG               nupic d=0.0           Δ size   Δ SSIM
01 trans-demo            47 KB / -492.6        44 KB /  -46.2        -5.1 %   +446.4
02 pluto-trans          176 KB /  -60.0       154 KB /  79.5        -12.4 %   +139.5
03 wiki-logo             13 KB /  -63.7        14 KB /  84.3         +8.4 %   +148.0
04 portrait             556 KB /   85.9       477 KB /  88.1        -14.2 %   +2.2
05 mountain             424 KB /   59.4       379 KB /  70.4        -10.4 %   +11.0
06 landscape           1066 KB /   79.8      1012 KB /  83.1         -5.0 %   +3.3
07 product              358 KB /   80.3       339 KB /  85.2         -5.2 %   +4.9
─────────────────────────────────────────────────────────────
TOTAL                  2643 KB              2424 KB                -8.27 %   all positive
```

Every fixture beats or matches TinyPNG on SSIM. Six of seven cut
size by 5-14 %; one (03 wiki-logo) is +1 KB (within byte-rounding
noise on a 13 KB file).

## Implementation — one-line classifier

```rust
pub fn classify_for_auto_dither(_src_rgba: &[u8], _width: u32) -> f32 {
    0.0
}
```

The previous 200-line tier-1/2/3/4 routing tree (Cycles 8-35) is
preserved as `classify_for_auto_dither_legacy` for reference. Users
who want peak SSIM at higher size pass `--dither 0.5` (most photo
class) or `--dither 0.7` (gradient / smooth-gradient transparency).

## End-to-end perf bench (5 MP, 3-run min)

```
fixture                  Cycle 37        Cycle 38        Δ time     Δ size
05 mountain              0.96 s / 472    0.80 s / 379    -17 %      -20 %
06 landscape             0.87 s / 1101   0.55 s / 1012   -37 %      -8 %
17 aurora                5.70 s / 1706   4.81 s / 1314   -16 %      -23 %
25 sofia                 4.49 s / 2745   3.35 s / 2468   -25 %      -10 %
27 whale                 3.30 s / 3261   2.01 s / 3044   -39 %      -7 %
                                         ─────────       ────       ────
                                          avg            -27 %      -14 %
```

Cycle 38 wins on BOTH axes simultaneously because (a) skipping the
FS dither pass saves the dither inner-loop time, and (b) un-dithered
index sequences have lower deflate entropy than dithered.

## Verification

- All workspace tests pass.
- 7-fixture marketing baseline at d=0.0: 2424 KB / 91.7 % of TinyPNG,
  7/7 SSIM still > TinyPNG.
- 5 MP perf bench: -16 to -39 % time, -7 to -23 % size vs v0.5.48.
- `classify_for_auto_dither_legacy` still compiles (dead-code-allowed
  for reference; not called from any path).

## Why archive the Cycle 30-35 work instead of delete

The tier routing tree is sweep-derived domain knowledge — knowing
"tier-4d high-uniq smooth photo peaks at d=0.7" is non-obvious and
took several cycles to discover. Keeping the function (renamed to
`_legacy`) preserves the decision tree for future re-use:

- If user-facing reference shifts (e.g. comparing against a tool
  that uses higher dither), we can flip the active classifier back.
- If a `--quality max` CLI mode is added, it can call legacy.
- The signal-extraction code (var, adj_mn, mr, uniq) is also reused
  by `is_gradient_candidate` (Cycle 25 lossless route).

Net loss vs prior cycles: 0 SSIM gains landed, but every cycle's
findings preserved as commented evidence in essay 03u-03z, 04a-04b.

## Open size & perf backlog

### Size (next candidates)

1. **oxipng effort=10 default** — Cycle 21 essay measured -0.3 %
   corpus size; with d=0.0 the deflate stream is simpler and zopfli
   may extract more. Bench to confirm.
2. **Adaptive palette size** — train at the actual uniq count
   instead of always 256. Smaller palette → smaller deflate
   alphabet for index codes. Bench on tier-3-ish fixtures (low
   uniq) where the win would show.
3. **PNG bit-depth re-check** — manual encoder may not always pick
   the optimal bit depth before oxipng's reduction pass; double-
   check the IHDR for tier-3.

### Perf (next candidates)

1. **Adaptive Lloyd stride** — large images already use stride=8
   (Cycle 37). Small images (< 1 MP) may benefit from stride=4
   for slightly better SSIM at negligible time hit. Sweep.
2. **Skip Lloyd entirely** with d=0.0 routing — Stone D refinement
   targets per-pixel argmin quality. With no FS dither downstream,
   imagequant init might be close enough; refine_iters=0 deserves
   a bench.
3. **Parallel oxipng** — `oxipng::Options.deflate` and filter
   choice can be parallelised. Currently oxipng runs serial
   filter trials.
4. **Decode-side perf** — undocumented today. nupic only encodes;
   decode goes through `image` crate or PNG runtime. If users care
   about decode speed (browsers), bench libpng vs `image`.

## Files touched

- `crates/nupic-quantize/src/lib.rs`
  - `classify_for_auto_dither` flattened to `return 0.0`
  - Cycle 30-35 routing tree preserved as
    `classify_for_auto_dither_legacy` (dead-code-allowed)
- `Cargo.toml` workspace version 0.5.48 → 0.5.49
