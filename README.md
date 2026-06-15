# nupic

> **Nu**clear **pic**ture handler — cross-platform image processing CLI, written in Rust.

`nupic` is a single-binary CLI for everyday image operations: **resize / fit /
circle / mock / watermark / compress**, with more (denoise, upscale, similarity,
bbox, …) planned. It is also the public face of an underlying research project:
the implementations behind each subcommand are scheduled to be replaced one
pipeline at a time with self-built, zero-dep, math-first codecs that aim at the
information-theoretic / perceptual upper bound — see
[`docs/roadmap.md`](docs/roadmap.md).

## Install

### One-liner (macOS, Linux)

```bash
curl -sSL https://raw.githubusercontent.com/goliajp/nupic/develop/scripts/install.sh | bash
```

Detects your platform, downloads the latest release archive, verifies
SHA-256, and installs to `~/.local/bin/nupic`. Override the install
location with `INSTALL_DIR=/usr/local/bin` (and run with `sudo` if it's
system-owned). Pin a version with `NUPIC_TAG=v0.1.4`.

### Windows

Download the `.zip` for your architecture from the
[Releases page](https://github.com/goliajp/nupic/releases), extract
`nupic.exe`, and add its folder to `PATH`.

### From source

```bash
# latest from develop
cargo install --git https://github.com/goliajp/nupic --branch develop nupic-cli

# a specific tag
cargo install --git https://github.com/goliajp/nupic --tag v0.3.4 nupic-cli
```

Pre-built binaries for the six supported targets (mac arm/intel, linux
x64/arm, win x64/arm) are published on the
[Releases page](https://github.com/goliajp/nupic/releases) for every
`v*.*.*` tag.

## Quick start

```bash
# polished placeholder for a wireframe
nupic mock -W 800 -H 600 -o placeholder.png

# resize keeping aspect
nupic resize photo.jpg -W 1024 -o photo-1024.jpg

# fit into a square box (cover-crops to fill)
nupic fit photo.jpg -W 512 -H 512 -m cover -o thumb.jpg

# round avatar with anti-aliased edge
nupic circle photo.jpg --feather 2 -o avatar.png

# text watermark, bottom-right
nupic watermark photo.jpg --text "© 2026" -p bottom-right -o photo-wm.jpg

# format-aware compression — defaults to "visually lossless, smallest file"
nupic compress photo.jpg -o photo.opt.jpg           # JPEG at q=95
nupic compress photo.png -o photo.opt.avif          # AVIF at q=90
nupic compress photo.png  -o photo.opt.png          # PNG: Stone C (OKLab argmin palette, no dither) — 0.5.0 default
nupic compress photo.png  -o photo.opt.png -q 95    # PNG: imagequant + oxipng (0.4.x default, kept for the explicit quality knob)
nupic compress screenshot.png -q 100 -o ss.opt.png  # PNG: true lossless (oxipng only)
nupic compress photo.jpg -o photo.tiny.jpg -q 70    # explicit lossy
nupic compress *.jpg -o /tmp/out/                   # batch into a directory

# perceptual quality target — encoder binary-searches the smallest q
# that meets the metric. Working with DSSIM; SSIMULACRA2 / Butteraugli
# reserved.
nupic compress photo.jpg -o photo.opt.jpg --target-dssim 0.005

# compare two images
nupic compare photo.jpg photo.opt.jpg
# DSSIM: 0.004847  (lower is better (0 = identical))

# bench a dataset across formats
nupic bench ~/Pictures/test-set --formats png,jpeg,webp,avif

# filters, denoise, bbox, crop
nupic filter photo.jpg --kind blur --amount 2 -o blurred.jpg
nupic denoise scan.tiff --kind median --strength 2 -o cleaned.tiff
nupic bbox masked.png            # → '100 0 600 600' to stdout
nupic crop photo.jpg -x 10 -y 20 -W 200 -H 200 -o crop.jpg

# discover everything
nupic --help
nupic compress --help
```

## Op surface

### Image transforms

| Subcommand | What it does | Today's backend |
|---|---|---|
| `resize` | Lanczos3 / CatmullRom / Gaussian / Bilinear / Nearest | `fast_image_resize` |
| `fit` | `contain` / `cover` / `fill` / `inside` / `outside` (CSS `object-fit` semantics) | composes resize + crop/pad |
| `crop` | rectangular crop, clamps to image bounds | `image` crate |
| `circle` | alpha-mask into a circle with feathered edge | hand-rolled |
| `filter` | grayscale / invert / blur / sharpen / brightness / contrast / hue | `image::imageops` |
| `denoise` | gaussian smoothing or per-channel median filter | hand-rolled + `image::imageops` |
| `mock` | placeholder — faint diagonal-stripe bg + centered `W × H` label; `--font <path>` for CJK / custom typography | hand-rolled + `ab_glyph` |
| `watermark` | text or image overlay, 9 anchor positions, opacity / scale; `--font <path>` for text watermarks | composes resize + alpha-over composite |

### Compression & analysis

| Subcommand | What it does | Today's backend |
|---|---|---|
| `compress` | PNG / JPEG / WebP (lossless **+ lossy**) / AVIF / GIF / BMP / TIFF. Defaults to **smallest perceptually-best file per format** (`Quality::Auto`); **PNG `Auto` since 0.5.0** routes through `nupic-quantize` (Stone C: OKLab perceptual-uniform argmin assignment over an `imagequant` median-cut palette, no Floyd-Steinberg dither) + `oxipng` — cross 7-fixture SSIMULACRA2 improves +77 points average vs 0.4.x while total bytes drop to 0.89× TinyPNG; `Quality::Lossless` keeps the bit-exact PNG path (oxipng only); `Quality::{Format, Perceptual(Dssim)}` are the explicit knobs and still use the 0.4.x imagequant path for compatibility; multi-input batch + dir output | `nupic-color` / `nupic-quantize` / `imagequant` / `oxipng` / `image` / `webp` / `ravif` |
| `compare` | per-pixel metric between two images — DSSIM today, SSIMULACRA2 / Butteraugli reserved | `dssim` |
| `bbox` | tightest rectangle around non-transparent pixels (alpha threshold tunable) | hand-rolled |
| `bench` | sweep a dataset across formats; per-image + average size / encode-time / DSSIM table. `--baseline <json>` switches to a PNG-only mode that compares nupic against pinned external byte sizes (e.g. `assets/png-bench/baseline.json` for the TinyPNG reference) and exits non-zero if any input regresses past 1.15× the baseline | composes compress + compare |

### Shell

| Subcommand | What it does |
|---|---|
| `completions <shell>` | print bash / zsh / fish / elvish / powershell completion script |

Each of these is scheduled to be replaced by a self-built pipeline; the public
API surface is `#[non_exhaustive]` so future additions (perceptual targets,
new container formats, content-aware modes) slot in without SemVer breaks.

## Architecture

`nupic` follows the **steel-cement-stone** separation:

- **cement** — `crates/nupic-cli`: the CLI shell. Allows deps. Disposable.
- **steel** — `crates/nupic-core`: the stable public API surface (`Image`,
  `Filter`, `FitMode`, `Quality`, `EncodedImage`, op functions). Ceiling-first
  design — survives implementation swaps.
- **stone** — research-grade codec crates (`nupic-bits`, `nupic-color`,
  `nupic-deflate`, `nupic-quantize`, `nupic-ssimulacra`, `nupic-png`*) per
  `docs/roadmap.md`. **0 deps**, math/physics upper bound. (* `nupic-png`
  is the integration-stage stone, planned 0.6.x.)

The `Image` type is an opaque newtype — internal representation can change
without affecting callers. Every op function takes `Opts` and `Image`,
returns `Result<Image>` (or `Result<EncodedImage>` for `compress`), so they
compose: `img.resize(…)?.fit(…)?.compress(…)?`.

## Workflow

The repo follows **classic git-flow**:

- `develop` — integration branch (default)
- `master` — production / tagged releases only
- `feature/*` → branch off `develop`, merge back to `develop`
- `release/*` → branch off `develop`, merge to `master` (tagged) + `develop`
- `hotfix/*` → branch off `master`, merge to `master` + `develop`

There is **no PR**: branches are integrated with `git flow feature finish` /
`git flow release finish` locally. CI lives only on the release path —
tag push (`v*.*.*` on `master`) → `.github/workflows/release.yml` builds the 6
target binaries via `cargo-zigbuild` + native runners and uploads them to a
GitHub Release.

A repo-tracked `post-commit` hook (`.githooks/post-commit`) auto-reinstalls
the binary to `~/.cargo/bin/nupic` whenever a commit touches source. Opt out
with `SKIP_INSTALL=1 git commit …`.

Configure the hook path once after cloning:

```bash
git config core.hooksPath .githooks
```

## Versioning

`0.x.y` while the API surface is still moving. The next-milestone bumps:

- `0.x.0` → adding an op, changing an `Opts` shape, adding a `Quality`
  variant, etc.
- `0.x.y` → same-op visual / quality / perf / bugfix work.

Every change that affects the produced binary bumps
`workspace.package.version` in `Cargo.toml`, so `nupic --version` is a
reliable dogfood waypoint.

## Roadmap

The headline plan is in [`docs/roadmap.md`](docs/roadmap.md): an 8-stage,
self-built codec build-out, starting from PNG. The CLI shipped today is the
external surface against which each pipeline's progress is measured.

Recurring milestones:

- **0.1.x** — scaffold + 6 day-1 ops + wrapped backends + dogfood binary.
- **0.2.x** — GIF / BMP / TIFF encode, lossy WebP, `--font <path>`,
  visually-lossless `Quality::Auto` default.
- **0.3.x** — `metrics::dssim` (cement) + working
  `Quality::Perceptual(Dssim)` binary-search; `compare`, `crop`,
  `filter`, `denoise`, `bbox`, `bench` subcommands; batch compress;
  shell completions. SSIMULACRA2 / Butteraugli still reserved (need
  stone layer).
- **0.4.x** — Lossy PNG path via `imagequant` palette
  quantization + `oxipng`; PNG `Quality::Auto` reaches TinyPNG-class
  compression (≤ 1.15× on every fixture in `assets/png-bench/`; total
  ratio 0.92× vs TinyPNG). `nupic bench --baseline <json>` formalises
  the comparison and exits non-zero on regression.
- **0.5.x** — current. Five stone-layer crates land:
  `nupic-color` (OKLab perceptual color space), `nupic-ssimulacra`
  (self-built SSIMULACRA2 metric, bit-exact agreement with cement +
  ~21% faster on M2 via nested rayon), `nupic-quantize` (perceptual
  palette assignment + no-dither indexed PNG), `nupic-bits` (CRC-32 +
  Adler-32 + bit I/O, zero deps), `nupic-deflate` (self-built DEFLATE
  encoder, phase 1.2: lazy LZ77 + dynamic Huffman + multi-block split,
  strictly beats `zlib level 9` on heterogeneous text — cargo-lock
  0.99× zl_9, 02-pluto IDAT 0.999× zl_9). PNG `Quality::Auto` still
  routes through `nupic-quantize` → `oxipng` (`nupic-deflate`
  integration into the PNG pipeline is the 0.6.x candidate); SSIMULACRA2
  average jumps +77.3 points vs 0.4 (02-pluto -65 → +72 is the headline
  win) while total bytes drop to 0.89× TinyPNG. The 0.4.x imagequant
  path is kept reachable via `Quality::Format(q)` so callers wanting the
  explicit quality knob still have it.
- **0.6.x +** *(planned)* — `nupic-deflate` integration into the PNG
  pipeline (replace `oxipng`'s zlib backend), stage 1 graduation
  polish (libdeflate / zlib-ng / zopfli oracles, property fuzz,
  silesia corpus), and filter beam search (roadmap stage 7). Stone C
  polish: rayon/SIMD on per-pixel argmin, adaptive light-dither for
  the remaining ~1-2 SSIMULACRA2 gap on photographic fixtures.

## License

Dual-licensed under **MIT OR Apache-2.0** — your choice.

Bundled font: **Source Sans 3 Regular** (SIL Open Font License 1.1,
Adobe 2010–2024). License text in
`crates/nupic-core/assets/LICENSE-FONT.txt`.
