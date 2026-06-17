# 04m — Cycle 49: nupic-png self-built path bench (NEGATIVE)

## Hypothesis

After Cycle 48 ruled out smarter `png` crate settings, the next
obvious oxipng replacement is the in-tree `nupic-png` (Stone-layer
self-built PNG encoder using nupic-deflate Level::Best from
phase 1.4). Hypothesis:

> `--use-nupic-png` path can replace oxipng-p1 at competitive size,
> avoiding the ~640 ms oxipng pipeline on 5MP.

The Cycle 25 essay noted nupic-png was 1.04-1.35× larger than oxipng
back in 0.5.10. Cycle 49 re-benches at v1.0.5.

## Experiment

`nupic compress --use-nupic-png --dither auto` vs the default
`nupic compress --dither auto` (oxipng preset=1) on 3 × 5MP fixtures.

## Result

```
fixture                    oxipng-p1            nupic-png            ratio
17 aurora                  1242 KB / 2.20 s     1341 KB / 70.98 s    1.080× size, 32× slower
25 sofia                   2153 KB / 1.33 s     2529 KB / 40.14 s    1.174× size, 30× slower
27 whale                   2942 KB / 1.36 s     3122 KB / 16.97 s    1.061× size, 12× slower
```

nupic-png is **both bigger (8-17 %) AND much slower (12-32 ×)** than
oxipng on every 5MP fixture.

## Root cause notes

The nupic-deflate Phase 1.4 (zopfli-class iterative cost-DP) was
benchmarked vs zopfli on text-class corpora and graduated. The
50-60 s latency on 5MP IDAT here suggests the iterative refinement
is running its full cost-DP loop on a ~5MB compressed stream — that
matches the zopfli-class wall-clock characteristic (zopfli on 5MP
PNG is typically 30-60 s too).

Meanwhile the size deficit (1.08-1.17 × oxipng) likely comes from
the filter selection in nupic-png — it tries 5 filters by
minimum-sum-of-absolute-differences whereas oxipng's preset=1
includes the Brute/Adaptive strategies that beat that heuristic on
palette indices.

## Implications

- **Self-built path not viable as oxipng replacement** under current
  nupic-deflate impl. The zopfli-class deflate is too slow; the
  filter heuristic is too narrow.
- To revisit, would need:
  1. Nupic-deflate Phase 1.5+ — add a fast Level (libdeflate-class)
     for the encode-then-iterate pattern.
  2. nupic-png filter selection that matches or beats oxipng's
     algorithms (Brute, Adaptive heuristics).
- For Cycle 50+, defer custom encoder path; focus on RSS profile
  (memory KPI gap 480 MB → 100 MB target) and Lloyd algorithm
  refinements (further perf).

## Value of negative result

Establishes empirically that "we already have a self-built encoder"
is NOT the answer to oxipng latency. Refines the next-step thinking
toward:

1. RSS / memory profile (untouched axis)
2. Lloyd iter count + smarter init
3. nupic-deflate fast-level work (separate research thread)

## Files touched

- `docs/research/png/04m-cycle49-nupic-png-negative.md` (essay)
- `Cargo.toml` workspace version 1.0.6 → 1.0.7
- (no runtime behaviour change)
