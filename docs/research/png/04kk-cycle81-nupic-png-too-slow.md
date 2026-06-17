# 04kk — Cycle 81: direct libdeflate ruled out as perf lever (negative)

## TL;DR

Cycle 79 backlog suggested bypassing oxipng with the project's own
`nupic-png` + `nupic-deflate` stack for sub-200 ms 5MP latency.
Empirical test shows nupic-png produces 5-10 % SMALLER output (a
genuine size win) but takes **100-500× longer** wall time at the
default `Level::Best`. Not viable for the perf budget.

## Per-fixture wall time

```
fixture              oxipng preset=0      nupic-png MinSad     nupic-png DflAware
                     size_KB  time_ms     size_KB  time_ms     size_KB  time_ms
17 aurora 5.9MP      1551     123         1373     69240       1373     72692
25 sofia 5.5MP       2736     128         2546     38788       2548     36711
27 whale 5.5MP       3266     130         3155     15038       3155     14068
19 iceberg 3.0MP     1357      71         1267     20740       1277     20043
28 orca 14MP        10351     274         9667     32867       9667     31476
18 snow 17MP         5271     394         4738    249894       4738    226380
```

nupic-png size advantage is real (-5 to -10 %) but the latency makes
it incompatible with the perf KPI (5MP < 250 ms target).

## Why nupic-deflate Level::Best is so slow

`nupic-deflate::Level::Best` does exhaustive LZ77 match-finding with
chain depth 512 and 5 iterations of dynamic-Huffman re-optimisation.
For 5MP+ inputs (raw indexed ~3-15 MB) this is a 30-250 second
computation. The optimisation budget is calibrated for the project's
"smallest possible PNG" research goal, not for latency.

There is a `Level::Fast` codepath used adaptively in
`encode_indexed_png_with` when content is "big and flat" (mrl >= 8)
— but photo content here is not flat. Lloyd-quantised photos give
mrl ~1.5-3, well below the Fast threshold.

## What's missing for nupic-png to be a perf option

1. Per-fixture override to force Level::Fast unconditionally
2. Faster filter selection (current MinSad + DeflateAware both
   compute exhaustive per-row filter costs)
3. Match-finder simplification at high MP (skip extended chain
   walks; rely on hash-table lookahead only)

None are quick — these are months of `nupic-deflate` engineering.
The Cycle 81 negative finding establishes that the perf gap to
< 250 ms 5MP must come from elsewhere:

- Cycle 79's preset=0 already extracted most of the easy oxipng
  wins.
- Apply SIMD (Cycle 79 backlog) could save 50-80 ms on 5MP.
- Lloyd cap=10 (shipped in Cycle 79) already at floor.
- imagequant train (~30-70 ms on 5MP) is library code — bypass
  would require replacing the median-cut palette init.

## Paper material

This is a useful framing for the P3/P4 narrative: the project HAS
a Rust-native PNG stack (`nupic-png` + `nupic-deflate`) optimised
for size, NOT speed. The perf path uses oxipng. The two represent
distinct operating points on the Pareto curve:

```
                 size (KB)    time (5MP)
oxipng preset=0  worse        very fast (123-394 ms)
oxipng preset=1  good         slow (500-1400 ms)
nupic-png Best   best         very slow (15-70 s)
nupic-png Fast   ?            ? (not tested at scale)
```

A Cycle 82+ candidate is bench `nupic-png` with all matching-finder
knobs at minimal settings to see if it can approach oxipng preset=0
latency at competitive size. Probably not for ship priority, but
research-worthy.

## Files touched

- `docs/research/png/04kk-cycle81-nupic-png-too-slow.md` (this essay)
- Research-only `examples/c81_nupic_png.rs` (not committed)
- No Cargo bump, no behaviour change
