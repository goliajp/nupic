# 04n — Cycle 51: Zero-copy imagequant input (-20 MB, -13% time, v1.0.8)

## Motivation

Cycle 50 RSS profile revealed:
- 25 sofia 5MP peak: 242 MB (target 100 MB; 2.4 × over)
- `train_palette_rgba.try_iq` accounted for +156 MB of allocation,
  the dominant memory event in the pipeline

Inspecting `try_iq`:

```rust
let pixels: Vec<rgb::RGBA8> = src_rgba.chunks_exact(4)
    .map(|c| rgb::RGBA8 { r: c[0], g: c[1], b: c[2], a: c[3] })
    .collect();
```

Allocates a fresh `Vec<rgb::RGBA8>` (20 MB on 5MP) just to convert
the existing `&[u8]` slice into a `&[RGBA8]` slice for
`imagequant::new_image`.

## Fix — zero-copy slice cast

`rgb::RGBA8` is `#[repr(C)] struct { r: u8, g: u8, b: u8, a: u8 }`
— byte-layout identical to the RGBA bytes already in `src_rgba`.
A raw-slice cast skips both the 20 MB allocation and the memcpy:

```rust
assert!(src_rgba.len() % 4 == 0);
let pixels: &[rgb::RGBA8] = unsafe {
    std::slice::from_raw_parts(
        src_rgba.as_ptr() as *const rgb::RGBA8,
        src_rgba.len() / 4,
    )
};
```

`imagequant::new_image` takes `&[impl PixelType]`, accepts the cast
borrow directly.

## Bench

RSS (25 sofia 5MP, after `train_palette`):
- Cycle 50: 195 MB
- Cycle 51: **174 MB** (-21 MB ✓)

End-to-end peak (after oxipng):
- Cycle 50: 242 MB
- Cycle 51: 230 MB

End-to-end latency (best of 3):
- 25 sofia 5MP: 1.53 s (Cycle 47) → **1.33 s** (-13 %)
- 17 aurora 5MP: 2.10 s → **1.83 s** (-13 %)

The 13 % perf bonus comes from avoiding the 20 MB memcpy + the
allocator pressure relief during peak.

Baseline-7 size ratio UNCHANGED at -17.93 % vs TinyPNG. SSIM
bit-equivalent (no algorithm change, just memory layout).

## Cumulative perf progression (25 sofia 5MP)

```
Cycle 36 baseline:                     15.6 s
Cycle 37 stride-8:                     ~6.3 s   (-60 %)
Cycle 45 SIMD Lloyd:                   2.51 s   (-60 %)
Cycle 46 adaptive stride:              2.47 s   (-2 %)
Cycle 47 adaptive oxipng preset:       1.53 s   (-38 %)
Cycle 51 zero-copy imagequant:         1.33 s   (-13 %)
─────────────────────────────────────
                                       12× total vs baseline
```

Still 5 × from the 250 ms NAS/CDN target. Remaining big chunks:
- Lloyd refine ~ 500 ms
- oxipng ~ 600 ms
- imagequant + apply + encode ~ 230 ms

## Safety note

The unsafe cast is sound because:
1. `rgb::RGBA8` is `#[repr(C)]` with exactly 4 × `u8` fields and
   no padding (verifiable by `mem::size_of` and `mem::align_of`).
2. The byte order in `src_rgba` matches: R, G, B, A in memory.
3. The slice length is checked to be a multiple of 4.
4. Lifetime is bounded by the borrow of `src_rgba`.

Aligned & little-endian platforms (all our targets) treat this as
identity. The function returns `Vec<rgb::RGBA8>` (palette only —
much smaller), so the unsafe cast doesn't leak.

## Files touched

- `crates/nupic-quantize/src/lib.rs::train_palette_rgba::try_iq`
  (zero-copy `&[u8] → &[RGBA8]` cast)
- `Cargo.toml` workspace 1.0.7 → 1.0.8
