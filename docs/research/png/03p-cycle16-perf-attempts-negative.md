# 03p — Cycle 16: Lloyd's perf attempts that didn't pan out

## Context

After Cycle 14's -26% Lloyd's win and Cycle 15's split-cap fix, the
priority survey identified two further perf attack candidates:

- **A1**: parallelize the sum-accumulate loop (currently sequential)
- **A3**: SIMD-pack palette + alpha into one `Vec<(f32,f32,f32,f32)>`
  for fewer per-centroid reads in the parallel-assign loop

Both attempted and reverted as net-negative or null. Documented here
so we don't redo them.

## A1: par_chunks + per-thread Acc + reduce

**Attempt**: split `pixels_oklab_alpha` into per-thread chunks
(8 cores × 2 chunks each), each thread maintains its own
`Acc { sum_l: Vec<f64>; … nine [k=256] buffers }`, reduce via Rayon
tree-fold.

**Result**: 3.2× SLOWER (1669 ms → 5360 ms on 05).

**Root cause**:
- `Acc::new(k)` allocates 9 × 256 × 8 B ≈ 18 KB per chunk
- 16-32 chunks per iter × 100 iters × 18 KB = 30-60 MB alloc per encode
- `reduce` tree wrap-up costs more than per-chunk savings

The sequential accumulate is **memory-bandwidth-bound, not
CPU-bound** — loop body is < 50 ns/pixel with sum buffers fitting L1.
Parallelization can't improve memory bandwidth that's already
saturated; only adds sync overhead.

Tried chunking heuristic `chunks = N / (n_threads * 2)` — still
3× slower. Allocation cost dominates regardless of granularity.

## A3: pack palette + alpha_scaled into one Vec<(f32; 4)>

**Attempt**: before each Lloyd's iter, pack
`(palette[j].l, .a, .b, alpha[j] as f32 * ALPHA_SCALE)` into
`palette_packed: Vec<(f32,f32,f32,f32)>`. Inner assign loop reads
one `(f32; 4)` per centroid instead of two reads (palette + alpha
LUT).

**Result**: noise-bound (1819-2386 ms vs baseline 1587-1624 ms over
3-trial reruns; signal smaller than σ).

**Root cause**: both unpacked (256 × 12 B + 256 × 1 B = 3.25 KB) and
packed (256 × 16 B = 4 KB) easily fit L1. Cache benefit ≈ 0. Pack /
repack overhead (≈ 26 K f32 ops per iter) ≈ pack savings.

Plus measurement bench `cycle14_perf_breakdown` showed massive trial
variance (one run 1819 ms, next 3379 ms — likely system load /
thermal). Can't measure perf changes ≤ 5% reliably with current bench
setup. Need:
- Pinned bench machine
- Warm-up iterations
- 11-run median (like nupic-bits-bench)

## Implication for Cycle 17+

Lloyd's perf attacks below 10% improvement are not measurable with
current bench tooling. Either:

1. **Build a better bench harness** (warmup + 11-run median, like
   `nupic_bits_bench`)
2. **Pivot to attacks with measurable signal**: algorithmic
   (Elkan triangle inequality, 5-10× potential), or non-perf
   (quality / coverage)

Cycle 17 will pivot to quality / coverage attacks where signal is
clear (SSIM points, fixture coverage delta).

## Files

- (no production code changes — both attempts reverted)
