# PNG compression benchmark fixtures

Sample PNGs and external baselines used to evaluate nupic's PNG path against
TinyPNG and other lossy-PNG tooling.

## Layout

- `inputs/` — original PNGs (committed)
- `baseline.json` — byte sizes + sha256 of each input, plus the TinyPNG byte
  size collected from the tinypng.com web UI on 2026-06-15. This is the bench
  fixture; the bench harness reads it. (committed)
- `tinypng-web/` — raw TinyPNG outputs, kept locally; **not committed** (the
  TinyPNG terms of service are unclear about redistributing optimised
  outputs; the numbers in `baseline.json` are enough for our purposes)
- `current-nupic/` — scratch dir for running the local nupic binary; **not
  committed**

## inputs/

| File | Source | Bytes | Notes |
|------|--------|------:|-------|
| `01-png-transparency-demo.png` | Wikimedia Commons `PNG_transparency_demonstration_1.png` | 224 566 | RGBA, 4 translucent dice, classic alpha test |
| `02-pluto-transparent.png` | Wikimedia Commons `Pluto-transparent.png` (NASA, PD) | 472 683 | Photographic RGBA with soft alpha edge |
| `03-wikipedia-logo.png` | Wikimedia Commons `Wikipedia-logo-v2.png` | 15 829 | Vector-rendered logo, sparse palette |
| `04-photo-portrait.png` | picsum.photos id=237 1200x800, JPG re-encoded to PNG via `sips` | 1 159 107 | Photo-as-PNG, typical "user saved photo as PNG" case |
| `05-photo-mountain.png` | picsum.photos id=1015 1200x800, same path | 1 552 606 | Photo-as-PNG |
| `06-photo-landscape.png` | picsum.photos id=29 1600x900, same path | 2 695 291 | Larger photo-as-PNG |
| `07-photo-product.png` | picsum.photos id=338 1024x768, same path | 887 025 | Small photo-as-PNG |

picsum.photos serves random JPEGs by ID; the IDs above are stable but the
underlying photo selection is theirs. License is "free for any purpose"
(see https://picsum.photos).

Wikimedia files are public domain or CC0 / CC-BY-SA; see file pages on
commons for the exact licence of each.

## Current state (2026-06-15)

| | total bytes | ratio vs input |
|---|---:|---:|
| input (7 PNGs) | 7 007 107 | 1.00 |
| nupic 0.3.4 (oxipng only) | 5 873 637 | 0.838 |
| TinyPNG | 2 706 076 | 0.386 |

nupic 0.3.4 is **2.17×** the size of TinyPNG. The 0.4.0 goal is to bring this
ratio to **≤ 1.15× on every file** by routing the default `Quality::Auto`
PNG path through `imagequant` (palette quantisation) before `oxipng`.

## How to refresh the TinyPNG baseline

1. Open https://tinypng.com (web UI, no API key needed for up to 20 files / batch)
2. Drag everything in `inputs/` onto the page
3. Wait until all rows show "Finished"
4. Click "Download all" — you'll get a zip
5. Unzip into `tinypng-web/`, keeping the original filenames
6. Run the bench harness — it will update `baseline.json`
