# 03e — Stone E:Floyd-Steinberg light dither(opt-in via `--dither`)

> Stone D plateau at 04-photo-portrait SSIMULACRA2 = 83.06 leaves a
> 2.91 pt gap to TinyPNG。Stone E ships **Floyd-Steinberg light dither
> in OKLab+alpha space** as opt-in `--dither <strength>` CLI flag /
> `QuantizeOpts::dither_strength`。Strength sweep shows photo-class
> fixtures gain +1-5 SSIMULACRA2 pts at +2-17% size cost,但 logos /
> transparent inputs see no benefit or slight regression。Default
> remains `dither_strength = 0.0` so the "又小又好" guarantee on
> average is unaffected — users explicitly opt into the trade-off
> when shipping photo-heavy collections。

---

## 1. 设计

Floyd-Steinberg dither(Heckbert 1975)classic algorithm:per pixel
in raster order,distribute quantization residual to 4 neighbors with
weights 7/16, 3/16, 5/16, 1/16。Stone E 把 residual scale 在 OKLab+alpha
4-D space 上(跟 Stone D 的 distance metric 一致),并加 `strength` 参数:

```
for each pixel (raster order):
    best_j = argmin(palette, pixel)
    residual = (pixel - palette[best_j]) * strength
    diffuse residual:
        pixel[x+1, y]   += residual * 7/16
        pixel[x-1, y+1] += residual * 3/16
        pixel[x  , y+1] += residual * 5/16
        pixel[x+1, y+1] += residual * 1/16
```

`strength = 0` → no dither(call `apply_palette_rgba` directly)。
`strength = 1` → canonical FS(数据显示 overshoots 多个 fixture)。
`strength = 0.5` → "light"(数据显示 photo sweet spot)。

---

## 2. 实测 — strength sweep

`crates/nupic-research/examples/stone_e_fs_dither.rs` on 7-fixture
corpus:

| fixture | s=0.0 | s=0.25 | s=0.5 | s=0.75 | s=1.0 |
|---|---:|---:|---:|---:|---:|
| 01-transparency | -46.4 | -43.6 | -35.7 | -32.3 | -58.5 |
| 02-pluto | 72.3 | 71.9 | 69.7 | 63.5 | 56.5 |
| 03-wikipedia | 89.5 | 89.6 | 89.3 | 89.4 | 89.3 |
| 04-portrait | 82.95 | **83.45** | **83.98** | **84.28** | 74.7 |
| 05-mountain | 70.3 | 73.1 | 75.4 | **76.5** | 59.3 |
| 06-landscape | 82.75 | 83.81 | 84.48 | **84.66** | 71.5 |
| 07-product | 82.83 | 83.70 | 84.24 | 84.06 | 68.0 |
| **AVG** | 62.02 | 63.13 | 64.49 | 64.30 | 51.55 |

| fixture | s=0.0 size | s=0.5 size | Δ% |
|---|---:|---:|---:|
| 01 | 65 282 | 84 105 | +29% |
| 02 | 274 048 | 285 483 | +4% |
| 03 | 16 352 | 16 820 | +3% |
| 04 | 627 012 | 653 901 | +4% |
| 05 | 540 319 | 632 809 | +17% |
| 06 | 1 238 544 | 1 268 685 | +2% |
| 07 | 517 535 | 568 449 | +10% |
| TOT | 3 279 092 | 3 510 252 | **+7%** |

Pattern:

- **Photo content**(04 / 05 / 06 / 07):**dither help**(+1-5 pts)
- **02-pluto**(transparent photo):**dither hurt**(-3 to -8 pts at higher strength)
- **01-transparency-demo**:dither help **for SSIM**(-46.4 → -32.3 at 0.75)but heavy size cost(+29%)
- **03-wikipedia-logo**:basically unchanged(flat colors,no banding)
- **strength = 1.0**(full FS):over-diffuses,every fixture collapses

Sweet spot:**strength = 0.5** for "moderate photo dither" or **0.75**
for "max photo SSIM at modest size hit"。

---

## 3. Ship 决策

**Default `dither_strength = 0.0`** —— 大多数 user workload 是 mixed
content,opt-in for photo-heavy。

**CLI**:`nupic compress photo.png --dither 0.5 -o out.png`

**dogfood test results**(v0.5.17 binary):

```
=== testflight (UI screenshot) ===
  dither=0.0:  19828 bytes, SSIM 84.72  ← best
  dither=0.25: 20405 bytes, SSIM 84.83 (+0.1 SSIM, +3% size)
  dither=0.5:  21327 bytes, SSIM 84.59 (-0.13)
  dither=0.75: 22265 bytes, SSIM 84.03 (-0.69)

=== 04-photo-portrait ===
  dither=0.0:  380748 bytes, SSIM 82.95
  dither=0.25: 386115 bytes, SSIM 83.45 (+0.5)
  dither=0.5:  395134 bytes, SSIM 83.98 (+1.03)
  dither=0.75: 401992 bytes, SSIM 84.28 (+1.33, gap to TinyPNG 1.58)
```

testflight 实际数据 confirm:UI screenshot 不需要 dither(Stone D
已 close,dither 只 hurt)。Photo fixture 04 confirm:dither=0.75
close 大半 gap to TinyPNG。

**ship 不改 default behavior** —— 现状("dither=0")已经是 "又小又好"
sweet spot on mixed corpus。`--dither` 是 advanced knob for photo
batch processing 时 opt-in。

---

## 4. cross-link

- 上游 Stone D plateau:[03d Stone D design](03d-stone-d-design.md) §5
  (n_iters sweep 找 plateau 83.06 on 04-portrait)
- 实施:
  - [`crates/nupic-quantize/src/lib.rs`](../../../crates/nupic-quantize/src/lib.rs)
    `apply_palette_rgba_fs_dither` + `QuantizeOpts::dither_strength` field
  - [`crates/nupic-core/src/ops/compress.rs`](../../../crates/nupic-core/src/ops/compress.rs)
    `CompressOpts::dither_strength` field + wiring
  - [`crates/nupic-cli/src/cli.rs`](../../../crates/nupic-cli/src/cli.rs)
    `--dither <strength>` flag
- bench:[`crates/nupic-research/examples/stone_e_fs_dither.rs`](../../../crates/nupic-research/examples/stone_e_fs_dither.rs)

---

## 5. v0.5.18 — content-adaptive `--dither auto`(已 ship)

`classify_for_auto_dither(src_rgba) -> f32`:

```
opaque_ratio = pixels with alpha=255 / total
if opaque_ratio >= 0.95 && n_pixels >= 200_000:
    return 0.25
else:
    return 0.0
```

**Strength 0.25(not 0.5)**:dogfood 实测 testflight(3 MP fully-opaque
UI)at 0.5 small regression(-0.13 SSIM);0.25 stays in the positive
on UI screenshots while still helping photos。 Conservative default。

CLI:`--dither off`(=0.0,旧 default)| `--dither auto`(classifier)|
`--dither <float>`(explicit 0.0..=1.0)。Default 仍 `off` 保持"严格 size
sweet spot",`auto` 是 opt-in "free quality bump for opaque large images"。

dogfood 实测 v0.5.18:

| input | off size/SSIM | **auto size/SSIM** | Δsize | ΔSSIM |
|---|---|---|---|---|
| testflight UI | 19 828 / 84.72 | 20 405 / **84.83** | +577 | **+0.11** |
| vantage UI | 279 259 / 81.32 | 304 035 / **81.50** | +24 776 | **+0.18** |
| 04 photo | 380 748 / 82.95 | 386 115 / **83.45** | +5 367 | **+0.51** |
| 03 logo | 12 658 / 89.49 | 12 658 / 89.49 | 0 | 0 |

每个 case **non-regression**。Logo classifier 正确 skip,opaque-large
input get small-to-meaningful quality bump。

未来 cleaner classifier 候选:

- **detect UI screenshot vs photo**:UI 有 long flat runs;photo has
  smooth gradients。Per-image local-variance metric 可分辨。
- 让 `auto` 在 photo 上 pick 0.5,UI 上 pick 0.25(分级 strength)。

## 6. 下一步

剩余 ceiling 攻击空间:
- **selective dither**:per-pixel adaptive,only diffuse residual if
  `|residual| > THRESHOLD`。Preserves flat regions exactly。Research
  candidate。
- **04-portrait residual 1.58 pt to TinyPNG at --dither=0.75**:可能需要
  更精细的 alpha-aware quantize 或 sub-palette specialisation。

---

## 6. 价值观

- [[feedback-ceiling-first-priorities]] — Stone E close 04-portrait 1.33
  pt of remaining 2.91 pt gap to TinyPNG(46%)at acceptable opt-in size
  cost
- [[feedback-metric-over-human-eye]] — strength sweep on each fixture
  drives ship decision;default 0.0 reflects "average corpus needs no
  dither" not "feels safer"
- [[feedback-no-cost-thinking]] — opt-in design 让 user 看实测 +X% size
  / +Y SSIM 数字决定,不替 user 评估 ROI