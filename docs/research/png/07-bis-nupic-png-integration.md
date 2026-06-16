# 07-bis — nupic-png integration shipped(opt-in `--use-nupic-png` flag)

> User-facing wiring lands as `CompressOpts::use_nupic_png` /
> `nupic compress --use-nupic-png`。Default behavior unchanged
> (`oxipng`)。Opt-in routes `Quality::Auto` PNG output through the
> self-built **nupic-quantize → nupic-png → nupic-deflate** path,
> producing ~ 1.10× larger files on average across the 7-fixture
> corpus(per [07 essay](07-nupic-png-foundation.md))。Lets users
> A/B compare integration vs production today,gathering feedback for
> the 0.6.x default-flip decision。

---

## 1. Surface

### CompressOpts

```rust
pub struct CompressOpts {
    pub format: Format,
    pub quality: Quality,
    pub strip_metadata: bool,
    pub effort: u8,
    /// **Experimental:** route Quality::Auto PNG through self-built backend.
    /// 0.5.11 default: false (uses oxipng).
    pub use_nupic_png: bool,
}
```

`use_nupic_png` toggle 只影响 `Quality::Auto` PNG path:
- `false`(default):走 `nupic-quantize::quantize_indexed_png` → png crate raw → oxipng(libdeflate near-optimal + filter try-all)
- `true`:走 `nupic-quantize::quantize` → `nupic-png::encode_indexed_png_with(FilterStrategy::DeflateAware)`

不影响 `Quality::Format(q)` / `Quality::Lossless` / `Quality::Perceptual(_)` /
non-PNG formats。

### CLI

```
nupic compress photo.png --use-nupic-png -o out.png
```

`nupic compress --help` 显示:

```
--use-nupic-png
    Experimental: route Quality::Auto PNG output through the
    self-built nupic-png + nupic-deflate backend instead of oxipng.
    As of 0.5.10 this produces ~ 1.10× larger files on average —
    opt in to A/B compare the integration path.
```

### End-to-end smoke

```
$ nupic compress wikipedia-logo.png             -o /tmp/oxipng.png   # 13 198 bytes
$ nupic compress wikipedia-logo.png --use-nupic-png -o /tmp/nupic.png   # 15 256 bytes
```

Reproduces the 1.16× number from [07 essay](07-nupic-png-foundation.md) bench
row on `03-wikipedia-logo`。

---

## 2. Implementation

`crates/nupic-core/src/ops/compress.rs`:

```rust
fn encode_png_stone_c(img: &Image, opts: &CompressOpts) -> Result<Vec<u8>> {
    if opts.use_nupic_png {
        return encode_png_stone_c_nupic(img, opts);
    }
    // ... existing oxipng path ...
}

fn encode_png_stone_c_nupic(img: &Image, _opts: &CompressOpts) -> Result<Vec<u8>> {
    let rgba = img.inner().to_rgba8();
    let qi = nupic_quantize::quantize(&raw, w, h, 256)?;
    let png_img = nupic_png::IndexedImage {
        width: w, height: h,
        palette: qi.palette_srgb.into_iter().collect(),
        indices: qi.indices,
        trns: None,  // alpha drop unchanged from oxipng path
    };
    Ok(nupic_png::encode_indexed_png_with(
        &png_img,
        nupic_png::FilterStrategy::DeflateAware,
    ))
}
```

`nupic-core/Cargo.toml`:`+ nupic-png = { path = "../nupic-png" }`。

`nupic-cli` 加 `--use-nupic-png` flag,wired through `runner::compress_one`
to `CompressOpts.use_nupic_png`。

26 个 existing call sites(tests + examples + CLI)mechanically updated
with `use_nupic_png: false,` field。No behavioral change at default。

---

## 3. ship 路径状态

| metric | oxipng baseline | nupic-png 0.5.11 | gap |
|---|---:|---:|---:|
| 7-fixture corpus total | 2 395 865 | 2 637 252 | 1.10× |
| 03 wikipedia-logo | 13 198 | 15 256 | 1.16× |
| 02 pluto-transparent | 162 069 | 216 341 | 1.33× |
| 04 photo-portrait | 380 318 | 452 956 | 1.19× |
| 05 photo-mountain | 402 741 | 424 888 | 1.05× |
| 06 photo-landscape | 1 062 185 | 1 122 357 | 1.06× |
| 07 photo-product | 325 525 | 338 089 | 1.04× |
| 01 transparency-demo | 49 829 | 67 365 | 1.35× |

ship 进度:
- ✅ 0.5.11 ships opt-in flag — users can A/B today
- ⚠ default flip(`use_nupic_png: true` 当成 default)等以下 close:
  - `nupic-quantize` tRNS support → 01 / 02 alpha 正确性
  - `nupic-png` cross-row filter selection → close 04 19%
  - `nupic-deflate` phase 1.5 per-block iterative → close residual ~ 4% IDAT
- 0.6.x default-flip 目标:corpus 总 ≤ 1.02× oxipng on average

---

## 4. cov

加 `use_nupic_png` field 后 26 个 `CompressOpts { ... }` literal 全 sweep
patched(0 behavioral change at default)。新增的 `encode_png_stone_c_nupic`
path 通过 CLI smoke 验证 byte-exact 重现 bench 中的 15256 bytes 数字。

所有 ~210 workspace test 仍全过(default 行为 unchanged)。

---

## 5. cross-link

- 上游:[07 nupic-png foundation](07-nupic-png-foundation.md)(bench finding:
  1.10× oxipng corpus with `FilterStrategy::DeflateAware`)
- 实施:
  - [`crates/nupic-core/src/ops/compress.rs`](../../../crates/nupic-core/src/ops/compress.rs)
    — `use_nupic_png` field + `encode_png_stone_c_nupic` branch
  - [`crates/nupic-core/Cargo.toml`](../../../crates/nupic-core/Cargo.toml)
    — `+ nupic-png` dep
  - [`crates/nupic-cli/src/cli.rs`](../../../crates/nupic-cli/src/cli.rs)
    `crates/nupic-cli/src/runner.rs` — `--use-nupic-png` flag

---

## 6. 下一步

整合层 ship 完。剩下让 default flip 实现的子任务(任何一项都不 block flag
existence):

1. **`nupic-quantize` tRNS support** — extract palette alpha from imagequant
   output,expose as `QuantizedImage::palette_alpha: Option<Vec<u8>>`,
   plumb through nupic-png。Close 01 / 02 correctness + size。
2. **`nupic-png` cross-row filter selection** — 2-pass:per-row min-SAD
   initial → full-stream deflate → per-row revise based on actual cost
   reduction。Close 04 portrait 1.19× → ~ 1.05×。
3. **`nupic-deflate` phase 1.5 per-block iterative refinement** — close
   residual ~ 4% IDAT gap to oxipng's libdeflate-near-optimal。

并行 backlog 同 07 essay。

---

## 7. 验收材料

- crate update:
  - `crates/nupic-core/src/ops/compress.rs` — `use_nupic_png` field,
    `encode_png_stone_c_nupic` branch
  - `crates/nupic-core/Cargo.toml` — `+ nupic-png` dep
  - `crates/nupic-cli/src/cli.rs` — `--use-nupic-png` flag
  - `crates/nupic-cli/src/runner.rs` — flag plumbed through
  - 26 个 `CompressOpts { ... }` literal sites(tests + examples)— add
    `use_nupic_png: false,` mechanically
- test:全 workspace ~210 测仍过(no behavior change at default)
- smoke:CLI on `03-wikipedia-logo.png` 重现 bench 数字(13198 / 15256)
- 价值观:
  - [[feedback-ceiling-first-priorities]] — integration ship 是 user-facing
    PNG file-size ceiling 的 hand-off point;default flip 由 nupic-png /
    nupic-quantize / nupic-deflate polish 决定
  - [[feedback-no-cost-thinking]] — 没评估"该不该 ship at 1.10× regression";
    选择 opt-in flag 让 user 决定 A/B 测,数据驱动 default-flip 时机
