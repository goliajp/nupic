# 08 — Stone C alpha-aware quantize:tRNS support across both PNG backends

> Phase 2.1 ships:Stone C's quantizer now carries source alpha through
> `train_palette_rgba` + `apply_palette_rgba`(4-D OKLab+alpha argmin)
> + emits a `tRNS` chunk on both the `oxipng` and `nupic-png`
> backends。Transparent fixtures stop bleeding alpha at the quantizer
> boundary。Side-effect:1 / 2 / 3(alpha-bearing fixtures)shrink ~ 2-7%
> across both backends because imagequant can pick fewer palette entries
> when alpha is a separate axis instead of being mashed into the RGB
> distance。

---

## 1. Surface

```rust
pub struct QuantizedImage {
    pub indices: Vec<u8>,
    pub palette_srgb: Vec<Rgb<u8>>,
    pub palette_alpha: Vec<u8>,   // NEW — always populated
}

pub fn train_palette_rgba(...) -> Result<(Vec<Oklab>, Vec<u8>), QuantizeError>;
pub fn apply_palette_rgba(
    src, w, h,
    palette_oklab: &[Oklab],
    palette_alpha: &[u8],
) -> (Vec<u8>, Vec<Rgb<u8>>);

pub fn encode_indexed_png_with_alpha(
    w, h, indices, palette_srgb,
    palette_alpha: Option<&[u8]>,
) -> Result<Vec<u8>, QuantizeError>;
```

Old `train_palette` / `apply_palette` / `encode_indexed_png` remain as
thin wrappers — fully opaque inputs go through them unchanged
(`ALPHA_WEIGHT * (255-255) = 0` collapses the 4-D distance back to
3-D OKLab L²)。 Stone C bit-exact 0.4-0.5 behaviour preserved for
non-alpha workloads。

`quantize_indexed_png`(Path A → oxipng)now also emits tRNS when any
palette entry is non-opaque,trimmed at the last non-opaque index per
PNG spec(trailing 255s implicit)。

`encode_png_stone_c_nupic`(Path B → nupic-png)threads `palette_alpha`
into `nupic_png::IndexedImage::trns`,which already emitted tRNS chunks
since the 0.5.10 foundation。

---

## 2. perf — bench post-2.1

`png_pipeline_swap` 7-fixture corpus:

| fixture | raw | A oxipng (v0.5.11 / v0.5.12) | B min-SAD (v0.5.11 / v0.5.12) | C deflate-aware (v0.5.11 / v0.5.12) | C / A |
|---|---:|---:|---:|---:|---:|
| 01-transparency-demo | 1 920 000 | 49 829 → **46 475**(−6.7%) | 66 842 → **61 956** | 67 365 → **62 382** | 1.34× |
| 02-pluto-transparent | 1 597 696 | 162 069 → **158 972**(−1.9%) | 274 237 → **271 417** | 216 341 → **212 108** | 1.33× |
| 03-wikipedia-logo | 146 400 | 13 198 → **12 735**(−3.5%) | 16 229 → **15 359** | 15 256 → **14 769** | 1.16× |
| 04-photo-portrait | 3 840 000 | 380 318 | 621 530 | 452 956 | 1.19× |
| 05-photo-mountain | 3 840 000 | 402 741 | 534 450 | 424 888 | 1.05× |
| 06-photo-landscape | 5 760 000 | 1 062 185 | 1 243 382 | 1 122 357 | 1.06× |
| 07-photo-product | 3 145 728 | 325 525 | 512 771 | 338 089 | 1.04× |
| **TOTAL** | | **2 388 951**(−0.3% vs v0.5.11) | **3 260 865** | **2 627 549** | **1.10×** |

Headline:

- **alpha-bearing fixtures**(01, 02, 03)shrink **2-7% on both
  backends** —— same imagequant call,but alpha now a separate quantize
  axis so fewer collisions in RGB-only space
- **opaque photographic fixtures**(04-07)unchanged ——
  `ALPHA_WEIGHT * 0 = 0` keeps them on the original OKLab-only path
  bit-exact
- **B/A and C/A ratios** mostly unchanged —— both backends benefited
  proportionally from the upstream Stone C polish。tRNS itself is
  cheap(256 bytes for full-palette alpha)。
- **correctness fix**:`02-pluto-transparent` is no longer rendered
  on whatever background the viewer composites against — the actual
  alpha mask is preserved。Verified via CLI smoke:

  ```
  $ nupic compress 02-pluto-transparent.png --use-nupic-png -o /tmp/out.png
  $ python3 -c "..."  # walk chunks
  chunks: [('IHDR', 13), ('PLTE', 768), ('tRNS', 256), ('IDAT', 211271), ('IEND', 0)]
  ```

  Pre-2.1 output had NO `tRNS` chunk despite the fixture having
  per-pixel alpha — a long-standing correctness regression that
  predated even the nupic-png foundation。

---

## 3. mem / perf

`apply_palette_rgba` adds one f32 subtract + one f32 mul-add per pixel
per palette entry vs `apply_palette`'s 3-axis loop。Per pixel
extra work:
- 1 byte alpha load
- 1 f32 cast + scale
- 1 mul-add into the distance accumulator

For 02-pluto(400 K pixels × 256 palette = 102 M comparisons),that
adds ~ 100 M extra mul-adds = ~ 10 ms on M2 release。Negligible against
the LZ77 + Huffman cost downstream。

Stone C original algorithmic point — single-pass argmin,no dither —
preserved。 No new heap allocations beyond `palette_alpha: Vec<u8>` of
length k ≤ 256。

---

## 4. cov

- 全 workspace ~210 测仍过(no regression — opaque path is bit-exact
  to phase 1.x via the `apply_palette → apply_palette_rgba(all-255)`
  shim)
- Manual smoke:`nupic compress 02-pluto-transparent.png --use-nupic-png`
  reads tRNS chunk count 256 bytes via Python chunk-walker
- `nupic-png` tests already cover tRNS round-trip from 0.5.10 stone
  foundation — every `IndexedImage { trns: Some(...) }` round-trip via
  `image` crate decoder was verified at stone landing time

---

## 5. doc — alpha-aware argmin sketch

```
d² = (L_pix - L_pal)²
   + (a_pix - a_pal)²
   + (b_pix - b_pal)²
   + (ALPHA_WEIGHT × (alpha_pix - alpha_pal) / 255)²
```

with `ALPHA_WEIGHT = 2.0`。Rationale:

- OKLab `L` typically ranges 0..1,full `L` mismatch → `(ΔL)² ≈ 1`
- Single OKLab axis(a / b)typically ranges -0.4..0.4,full mismatch
  → `(Δa)² ≈ 0.6`
- Full alpha mismatch(0 ↔ 255):`(2.0 × 1.0)² = 4.0` ≫ a typical
  triple-OKLab full mismatch ≈ 2.2

→ Opaque pixels strongly prefer opaque palette entries;transparent
pixels strongly prefer transparent palette entries。Within an alpha
bucket,the OKLab L² ordering dominates and Stone C's "no-dither
argmin" insight remains intact。

Weight 选 2.0 而不是 10.0 是因为:imagequant 训出来的 palette 通常已经
按 RGBA 聚类好,palette 内不会有"RGB 相同但 alpha 差很多"的两个 entry
让 distance metric 必须 break tie。2.0 足够避免边界 case,大一点会让
半透明像素的颜色质量退化(被推到 alpha 完全匹配但颜色稍差的 entry)。

---

## 6. cross-link

- 上游:[07-bis integration](07-bis-nupic-png-integration.md) — wired
  Path B; identified tRNS as default-flip blocker
- 上游 Stone C:[03c-bis Stone C C0](03c-bis-codebook-c0.md) — original
  OKLab argmin algorithm preserved as the opaque-bucket inner step
- 实施:
  - [`crates/nupic-quantize/src/lib.rs`](../../../crates/nupic-quantize/src/lib.rs)
    `train_palette_rgba` / `apply_palette_rgba` / `encode_indexed_png_with_alpha`
    + `QuantizedImage::palette_alpha` field;`apply_palette` and
    `encode_indexed_png` kept as thin wrappers
  - [`crates/nupic-core/src/ops/compress.rs`](../../../crates/nupic-core/src/ops/compress.rs)
    `encode_png_stone_c_nupic` threads `palette_alpha` into
    `nupic_png::IndexedImage::trns`
- bench:`png_pipeline_swap` 重跑 — 7-fixture corpus 总 2 627 549,
  default-flip 距离 1.10× oxipng(unchanged from 0.5.11,since both
  backends benefited equally)

---

## 7. 下一步

剩 default-flip 两道关卡:

1. **phase 2.2 `nupic-png` cross-row filter selection** — 2-pass:per-row
   min-SAD initial → full-stream deflate → revise based on actual cost。
   Close 04 portrait 1.19× → ~ 1.05×
2. **phase 1.5 `nupic-deflate` per-block iterative refinement** — close
   4% IDAT residual gap to oxipng's libdeflate-near-optimal

任何顺序都可。两项做完后 corpus 估 ≤ 1.02× oxipng → default flip → 0.6.0
ship 完全替换 oxipng dep tree。

---

## 8. 验收材料

- crate update:
  - `crates/nupic-quantize/src/lib.rs` — 加 `train_palette_rgba` /
    `apply_palette_rgba` / `encode_indexed_png_with_alpha` /
    `palette_alpha` field;old API 收缩为 thin wrapper
  - `crates/nupic-core/src/ops/compress.rs` — `encode_png_stone_c_nupic`
    thread `palette_alpha` into `nupic-png` `trns`
- 测套:全 ~210 workspace 测仍过(opaque bit-exact preservation)
- smoke:CLI 验证 tRNS chunk 256 bytes 写出
- bench:1/2/3 fixture 2-7% smaller on both backends;photos unchanged
- 价值观:
  - [[feedback-ceiling-first-priorities]] — 修了 correctness bug 同时
    把 1/2/3 拉低,non-zero free-lunch
  - [[feedback-no-cost-thinking]] — 没评估"该不该改 Stone C 核心";
    transparency 正确性 = absolute requirement,直接做
