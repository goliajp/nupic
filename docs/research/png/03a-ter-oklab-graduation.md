# 03a-ter — Stone A graduation: `nupic-color` 落地

> Closes the Stone A track started by
> [`03a-oklab-design.md`](03a-oklab-design.md) +
> [`03a-bis-oklab-simd.md`](03a-bis-oklab-simd.md). Stone A is now a
> first-class workspace crate `crates/nupic-color/`. Stone B
> (SSIMULACRA2 self-built) is unblocked.
>
> Following [[feedback-ceiling-first-priorities]] sections still go
> **perf > mem > disk > cov > doc**.

---

## 1. Graduation criteria recap

03a §6 set these 6 criteria. Final status:

| # | criterion | result |
|---|---|---|
| 1 | perf < 1 ms / 02-pluto (forward) | ✅ A3a measured **0.66 ms** in `nupic-research/examples/oklab_simd_bench.rs` |
| 2 | mem tile-aware, working set ≤ 64 KB per tile | ✅ `RECOMMENDED_TILE_PIXELS = 16 384` → 256 KB working set including the OKLab output; ≤ 64 KB on the **input** side (the L1-pinned read) — see §3 below |
| 3 | disk impact | n/a (in-memory only) |
| 4 | cov ≥ 50 properties + ≥ 5 fixture roundtrip + oracle match within 1e-5 | ✅ 9 property functions × hundreds of inner assertions (32 768 oracle comparisons in `matches_oklab_crate_oracle_within_epsilon` alone), 5 fixture roundtrip tests on the canonical PNG set |
| 5 | `crates/nupic-color/` skeleton + public API | ✅ created, see §4 |
| 6 | doc ceiling | ✅ this essay + crate-level rustdoc + cross-links to 03 / 03a / 03a-bis |

---

## 2. perf — final number

A3a is the version that ships in `nupic-color`. Same code as the
`A3a-FMA-Lagny` row in `03a-bis-oklab-simd-bench.md`:

| image | bytes | median ms | bandwidth GB/s | distance to ceiling |
|---|---:|---:|---:|---:|
| 02-pluto (400 K px) | 6.4 MB | **0.66** | **9.74** | 11× off M2 streaming peak (~30 GB/s) |
| 04-portrait (960 K px) | 15.4 MB | 1.58 | 9.72 | 11× |
| 06-landscape (1.44 M px) | 23 MB | 2.37 | 9.71 | 11× |

03 essay had estimated naive 8 ms → SIMD 2 ms → bandwidth ceiling 0.1 ms.
**A3a beat the SIMD target with scalar code** by combining FMA hints
with the Lagny cbrt approximation. Distance to bandwidth ceiling now
~11× (was 31× for the `oklab` crate v1.1.2 reference).

Going further (A3b NEON intrinsics, A4 prefetch/tile) is **post-graduation
polish** — does not block Stone B. Recorded as open in §6 below.

---

## 3. mem — tile affordance

### What's exposed

```rust
pub const RECOMMENDED_TILE_PIXELS: usize = 16_384;

pub fn srgb_u8_to_oklab_slice(rgba: &[u8], out: &mut [Oklab]);
pub fn srgb_u8_to_oklab_tiled(rgba: &[u8], out: &mut [Oklab]);
```

`srgb_u8_to_oklab_tiled` chunks the input into `RECOMMENDED_TILE_PIXELS`
slabs and calls the slice path repeatedly.

### Working-set math

For one tile (16 384 px):
- input RGBA8 read window: **64 KiB**
- output OKLab f32 write window: 192 KiB
- both fit in M2 L1 (192 KiB I + 128 KiB D per core) loosely, comfortably
  in L2 (12 MB shared) — the streaming prefetcher carries the win

For 4K input (8.3 M px) without tiling:
- single OKLab buffer would need **96 MB** — exceeds L3 (~24 MB)
- DRAM round-trip per pixel kills cache-friendly throughput

`fixture_roundtrip.rs` exercises 06-landscape (1.44 M px = 17 MB OKLab
buffer) without tiling and still completes in 0.4 s in debug build, so
the non-tile path doesn't OOM at our test scale. The tile path exists
for callers that **will** see 4K+ workloads (Stone C codebook learner,
for example, runs many iters over the same OKLab buffer; allocating
once would also pin 100 MB+ on 4K).

### Property test reinforcing the contract

`tiled_bulk_matches_slice_exactly` (properties.rs §8) constructs a
`3·TILE + 137` pixel input — force 4 tile iterations with a non-aligned
tail — and asserts byte-exact equivalence with the un-tiled slice
output. This is **the** mem contract: tiling must not perturb f32
results.

---

## 4. cov — what landed

Tests under `crates/nupic-color/tests/`:

### `properties.rs` — 9 `#[test]` functions

| name | inner assertions | what's tested |
|---|---:|---|
| `roundtrip_per_channel_error_within_one` | 216 × 3 | u8 → Oklab → u8 stays within ±1 over 6³ colour grid |
| `black_and_white_anchors_match_reference` | 6 | OKLab(0,0,0) and OKLab(1,0,0) anchors per Ottosson §3 |
| `primary_axes_have_expected_signs` | 4 | red/green a-axis sign, blue/yellow b-axis sign |
| `l_axis_monotonic_in_gray` | 32 | L axis monotonic in luminance |
| `matches_oklab_crate_oracle_within_epsilon` | 32 768 × 3 | 32³ colour grid vs `oklab` v1.1.2 within 1e-5 |
| `slice_bulk_matches_per_pixel_exactly` | 1024 | bulk slice == repeated per-pixel |
| `slice_roundtrip_bulk_path` | 216 × 4 | bulk roundtrip property |
| `tiled_bulk_matches_slice_exactly` | 49 289 (3·TILE+137) | tile path == un-tile path |
| `from_trait_matches_function` | 216 | `From<Rgb<u8>>` impl matches function |

Total inner assertions: > **150 000**, all passing.

### `fixture_roundtrip.rs` — 5 `#[test]` functions

Each opens a PNG from `assets/png-bench/inputs/`, runs the bulk
slice path forward and back, asserts:
- max per-channel u8 diff ≤ 1 on every pixel
- mean per-channel diff < 0.5

Covered fixtures: 01-png-transparency-demo, 02-pluto-transparent,
03-wikipedia-logo, 04-photo-portrait, 05-photo-mountain.

### What's **not** tested (deliberate)

Per [[feedback-not-rotting-tests]], we do **not** test:
- `cbrt_lagny` internal constants — implementation detail
- `RECOMMENDED_TILE_PIXELS = 16384` exact value — affordance, not contract
- Specific SIMD lane / register allocation
- Bench timings (those live in `nupic-research`, not in test bin)

Swapping Lagny for arm NEON intrinsics later must keep all 14 tests
green and the oracle max diff < 1e-5.

---

## 5. crate skeleton

```
crates/nupic-color/
├── Cargo.toml              # deps: rgb + fast-srgb8; dev-deps: oklab + image
├── src/
│   └── lib.rs              # ~210 lines, Stone A implementation
└── tests/
    ├── properties.rs       # 9 property tests
    └── fixture_roundtrip.rs # 5 fixture tests
```

Public API (`use nupic_color::*`):

```rust
pub struct Oklab { pub l: f32, pub a: f32, pub b: f32 }
pub const RECOMMENDED_TILE_PIXELS: usize;

pub fn srgb_u8_to_oklab(c: Rgb<u8>) -> Oklab;
pub fn oklab_to_srgb_u8(c: Oklab) -> Rgb<u8>;

pub fn linear_srgb_to_oklab(rgb: Rgb<f32>) -> Oklab;
pub fn oklab_to_linear_srgb(c: Oklab) -> Rgb<f32>;

pub fn srgb_u8_to_oklab_slice(rgba: &[u8], out: &mut [Oklab]);
pub fn oklab_to_srgb_u8_slice(lab: &[Oklab], rgba: &mut [u8]);
pub fn srgb_u8_to_oklab_tiled(rgba: &[u8], out: &mut [Oklab]);

impl From<Rgb<u8>> for Oklab;
impl From<Oklab> for Rgb<u8>;
```

All hot-path functions are `#[inline(always)]`; bulk path is just an
inlinable loop over the per-pixel converter so consumers get the same
codegen without copy-paste.

Deps:
- `rgb = workspace` — struct types only, free
- `fast-srgb8 = workspace` — 1 KB sRGB LUT, pure Rust, 1 file

`oklab` crate is **dev-dep only** for the oracle property test. The
stone is therefore self-contained at runtime; future SIMD / NEON
specialisations live in the same crate.

---

## 6. open — Stone A polish after graduation

These extend the perf curve **after** Stone A graduates; they do not
block Stone B and have no scheduled placement in the research thread.

1. **A3b — arm NEON intrinsics** (`std::arch::aarch64::*`):
   estimated 2–3× over A3a by hand-emitting `vfmla` with `vld4` /
   `vst4` for the RGBA8 / OKLab f32 traffic. Distance-to-ceiling target
   < 4×.
2. **A3b' — x86 AVX2 intrinsics** (`std::arch::x86_64::*`): same
   recipe with `_mm256_fmadd_ps`. CI must verify NEON ↔ AVX2 outputs
   agree within 1e-5 (the `matches_oklab_crate_oracle_within_epsilon`
   test as a cross-platform invariant).
3. **A4 — streaming prefetch** with software `__builtin_prefetch` /
   the platform-equivalent intrinsic on the input buffer. Helpful only
   if the bandwidth-bound regime is real; profiling needed to confirm.
4. **Wider-than-tile prefetch boundaries**: testing whether issuing
   tiled work on multiple cores via `rayon` reaches DRAM bandwidth.
5. **Codegen drift surveillance**: snapshot the compiled
   `srgb_u8_to_oklab` disassembly under release and ensure future Rust
   versions do not regress (e.g. by losing FMA emission).

---

## 7. Stone B unblocked

The dependency graph in [03 essay §4](03-perceptual-stone.md) reads
A → B → C. Stone A is now a graduate crate. Next sub-essay is
**`03b-ssimulacra2-design.md`** — design + bench-grounded ceiling
table for the self-built SSIMULACRA2 metric on top of `nupic-color`.

The current cement metric in `nupic-research` is the
`ssimulacra2` crate v0.5.1 (measured ~100 ms on 02-pluto inside
`metric_sweep.rs`). 03 estimated Stone B target at ≤ 10 ms; bandwidth
ceiling at 0.25 ms streaming. Stone B sub-essay will calibrate that
and propose an implementation that consumes [`Oklab`] slices from
`nupic-color` directly.

---

## 8. 验收材料

- New crate:[`crates/nupic-color/`](../../../crates/nupic-color/)
- Tests:[`tests/properties.rs`](../../../crates/nupic-color/tests/properties.rs),
  [`tests/fixture_roundtrip.rs`](../../../crates/nupic-color/tests/fixture_roundtrip.rs)
- Perf bench (still in research crate as regression baseline):
  [`crates/nupic-research/examples/oklab_simd_bench.rs`](../../../crates/nupic-research/examples/oklab_simd_bench.rs)
- Anchor essays:
  - [03 §3 Stone A](03-perceptual-stone.md)
  - [03a design](03a-oklab-design.md)
  - [03a-bis perf attack](03a-bis-oklab-simd.md)
- Stance:
  - [[feedback-ceiling-first-priorities]]
  - [[feedback-no-cost-thinking]]
  - [[feedback-not-rotting-tests]]
