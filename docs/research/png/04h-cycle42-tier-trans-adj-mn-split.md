# 04h — Cycle 42: tier-trans adj_mn-aware split (v0.5.53)

## Motivation

User-confirmed track A push: continue squeezing baseline-7 vs
TinyPNG ratio toward -20 % while strict SSIM ≥ TinyPNG gate.

Audit of transparency tier (opq < 0.95) on baseline-7 + extended
corpus revealed:

1. **01 (uniq=4348, adj_mn=1.92) at n=64 has SSIM buffer +428**
   vs TinyPNG — palette can drop to n=32 (saves -7 KB).
2. **02 (uniq=19444, adj_mn=3.17) at n=64 has SSIM buffer +133** —
   can drop to n=48 (saves -16 KB).
3. **03 (uniq=129, adj_mn=8.20) at n=256** stays — logo with crisp
   antialiased edges, lower palette breaks the SSIM gate (a naïve
   uniq-only split would route 03 to n=32 and SSIM crashes to 4.77).

Side-effect discovery: **Cycle 38 silently regressed Cycle 35 fixes
for transparency-smooth content**:

```
            pre-C38       post-C38 (v0.5.52)
14 soft-trans  70.44 SSIM    17.67 SSIM    (catastrophic)
22 tree-trans  66.99 SSIM    39.91 SSIM    (catastrophic)
```

These weren't in baseline-7 so the C38 marketing claim ("baseline
-8 % vs TinyPNG") wasn't false — but the corpus impact was real.

## Discriminator — `adj_mn` separates logo from smooth-gradient

Signals in the transparency-tier population:

| fixture       | opq    | adj_mn | uniq    | category          |
|---------------|--------|--------|---------|-------------------|
| 01 trans-demo | 0.036  | **1.92** | 4 348   | smooth gradient   |
| 02 pluto-trans| 0.781  | **3.17** | 19 444  | photo + alpha     |
| 03 wiki-logo  | 0.736  | **8.20** | 129     | crisp logo        |
| 14 soft-trans | 0.009  | **5.10** | 2 529   | gradient + edges  |
| 22 tree-trans | 0.302  | **5.61** | 26 345  | tree silhouette   |

`adj_mn > 5` cleanly catches 03/14/22 (all with crisp edges /
antialiasing) and routes them to n=256. The rest split by uniq.

## Rule

```rust
if opq < 0.95 {
    if adj_mn > 5.0 { return 256; }  // logo / crisp-edge content
    if uniq <  5_000 { return 32; }  // sparse smooth gradient (01)
    if uniq < 25_000 { return 48; }  // photo-with-alpha (02)
    return 64;                       // default
}
```

`adj_mn` is computed once per fixture via the existing
`compute_adj_lum_diff_stats` helper.

## Bench

### Baseline-7 vs TinyPNG

```
fixture                  TinyPNG          nupic Cycle 42       ratio
01 trans-demo            47 KB / -492.6   19 KB / -64.10       0.41×
02 pluto-trans           176 / -60.0      68 / 64.84           0.39×
03 wiki-logo             13 / -63.7       14 / 84.27           1.05×
04 portrait              556 / 85.9       450 / 86.07          0.81×
05 mountain              424 / 59.4       340 / 65.33          0.80×
06 landscape             1066 / 79.8      973 / 79.93          0.91×
07 product               358 / 80.3       324 / 84.07          0.91×
────────────────────────────────────────────
TOTAL                    2643 KB          2193 KB              0.8298
                                          (−17.02 % vs TinyPNG)
```

Crosses the **-17 % mark** with a +2.02 pp buffer above the -15 %
hard gate. All 7 fixtures retain SSIM ≥ TinyPNG (min buffer +0.16 on
06 unchanged).

### Side-effect: Cycle 38 regression fixed

```
                   v0.5.52        v0.5.53        ΔSSIM
14 soft-trans       82 / 17.67    150 / 66.36    **+48.69**
22 tree-trans      784 / 39.91   1451 / 65.19    **+25.28**
21 earth-hemi      682 / 41.81    682 / 41.81    0 (adj_mn=4.84 < 5)
```

14 and 22 (adj_mn 5.10 / 5.61) jump back to acceptable SSIM via the
new n=256 logo-branch. 21 (adj_mn=4.84, just below the threshold)
stays at n=64. Lowering the threshold to 4.0 would catch 21 too but
risks pulling baseline 02 (adj_mn=3.17) — deferred until we have
more N evidence.

## Files touched

- `crates/nupic-quantize/src/lib.rs`
  `classify_for_palette_size`: new adj_mn-aware transparency split
- `Cargo.toml` workspace version 0.5.52 → 0.5.53

## Open backlog

1. **21 earth-hemi** still at SSIM 41.81 (adj_mn=4.84 below 5
   threshold) — single-fixture, would need either a separate signal
   or threshold drop with corroborating evidence.
2. **−20 % gate remaining gap −2.98 pp**: nothing left in Track A
   palette-tuning on baseline-7 fixtures (04/06 gate-locked, 05/07
   already near best). Effort=10 zopfli adds only -0.3-0.5 pp.
   Move to Track B (algorithm).
