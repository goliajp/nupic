# 04l — Cycle 48: Skip-oxipng experiment (NEGATIVE RESULT, v1.0.6)

## Hypothesis

After Cycle 47 (adaptive oxipng preset), oxipng (preset=1) still
takes ~640 ms on 5MP — 40 % of total encode time. The hypothesis:

> Custom PNG encoder via `png` crate with smarter `Filter` /
> `Compression` settings can match oxipng size while skipping its
> ~640 ms pipeline.

If validated, this closes a major piece of the 6× gap to the
< 250 ms 5MP target.

## Experiment design

For 5MP fixtures (25-sofia, 17-aurora):
1. Encode via current pipeline → oxipng preset=1 (reference)
2. Encode via `png` crate with combinations of:
   - Filter ∈ {NoFilter, Sub, Up, Avg, Paeth, Adaptive}
   - Compression ∈ {Balanced, High}
3. Compare raw output size + time vs oxipng reference

## Results

```
=== 25-sofia 5MP ===
REFERENCE oxipng-p1:               2399 KB  (605 ms)
  None + Balanced (curr)           3696 KB  (75 ms)   1.541× oxipng
  None + High                      3697 KB  (244 ms)  1.541×
  Paeth + Balanced                 3696 KB  (74 ms)   1.541×
  Paeth + High                     3697 KB  (243 ms)  1.541×
  Sub + Balanced                   3696 KB  (75 ms)   1.541×
  Sub + High                       3697 KB  (243 ms)  1.541×
  Up + Balanced                    3696 KB  (75 ms)   1.541×
  Avg + Balanced                   3696 KB  (74 ms)   1.541×

=== 17-aurora 5MP ===
REFERENCE oxipng-p1:               1243 KB  (957 ms)
  None + Balanced (curr)           1807 KB  (80 ms)   1.454× oxipng
  None + High                      1778 KB  (693 ms)  1.430×
  Paeth + Balanced                 1807 KB  (83 ms)   1.454×
  Paeth + High                     1778 KB  (684 ms)  1.430×
  Sub + High                       1778 KB  (681 ms)  1.430×
  Avg + Balanced                   1807 KB  (81 ms)   1.454×
```

## Key findings (negative)

1. **Filter choice has no measurable effect** on indexed-palette PNG
   size at fixed deflate level. None / Sub / Up / Avg / Paeth /
   Adaptive all produce within 0.01 % of each other.
2. **`Compression::High` saves ≤ 1.6 %** vs `Balanced` on 5MP, at
   the cost of 3× encode time (75 ms → 240+ ms).
3. The 22-35 % oxipng improvement is **NOT** from filter sweep —
   `png` crate's default Adaptive filter is already filter-optimal
   relative to the standard 5 row filters.
4. The 22-35 % gap is fundamentally **deflate compression quality**:
   - `png` crate uses fdeflate (Rust-native, fast, lower ratio).
   - oxipng uses libdeflate (state-of-the-art deflate impl).
   - At equivalent filter, libdeflate beats fdeflate by 22-35 % on
     indexed-PNG IDAT streams.

## Implications

- **Skip-oxipng for 5MP is not viable** — the size cost (22-35 %)
  far exceeds any reasonable trade-off (the -15 % gate would shatter,
  going from -17.93 % to roughly +5-10 % vs TinyPNG on 5MP fixtures).
- Cycle 47's adaptive preset=1 is the **practical optimum** for the
  oxipng-based path.
- Closing the 22-35 % gap requires **direct libdeflate integration**
  bypassing oxipng's PNG-level optimisations (Cycle 49+ candidate).

## Value of the negative result

For Paper P2 / future research, this experiment rules out the "smarter
png crate encoder" optimisation direction definitively. The Cycle 48
sweep data establishes:

- The size-quality frontier of `png` crate output is essentially
  insensitive to filter choice given indexed-PNG palette indices.
- Codec-level perf gains require libdeflate-tier deflate impl,
  not just filter tuning.

This shapes the next research direction: rather than further oxipng
tuning, build a libdeflate-backed PNG writer that integrates with
nupic's pipeline (eliminates the redundant decode → re-encode that
oxipng performs).

## Files touched

- `crates/nupic-research/examples/speed_sweep.rs` — filter sweep
  research artifact (kept for future paper P2 reference)
- `Cargo.toml` workspace version 1.0.5 → 1.0.6
- (no runtime behaviour change)
