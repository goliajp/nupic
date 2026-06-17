# 04u — Cycle 58: Palette index re-ordering (NEGATIVE for P3, v1.1.5)

## Hypothesis (P3 sub-direction)

Sort palette by index frequency (descending) so most-used indices
become value 0, 1, 2, .... A more skewed index distribution should
reduce deflate entropy → smaller IDAT.

This is one approach to the larger "joint palette-filter
co-optimization" P3 paper thread.

## Implementation

```rust
fn reorder_by_frequency(indices, palette, alpha):
    counts = count occurrences of each palette index
    perm = order indices by count descending
    new_palette = palette permuted by perm
    inv_map = perm^-1
    new_indices = inv_map applied to every pixel
```

Output is byte-identical visually (same pixel→color mapping) but
the index byte sequence has lower entropy when frequencies are
skewed.

## Results

```
fixture            baseline_KB    reordered_KB    Δ%
04 portrait                451            451     0.00 %
05 mountain                317            317     0.00 %
06 landscape               974            974     0.00 %
07 product                 325            325     0.00 %
17 aurora 5MP             1266           1266     0.00 %
25 sofia 5MP              2152           2151    -0.05 %  (noise)
27 whale 5MP              2946           2946     0.00 %
─────────────────────────────────────────────
TOTAL                     8429           8428    -0.01 %
```

**Zero size benefit on every fixture.** Hypothesis rejected.

## Why

Three plausible explanations:

1. **Deflate is order-invariant for IDAT bytes** in our regime —
   the dictionary-based LZ77 + Huffman pipeline encodes per-byte
   context independently of specific byte values, and Huffman codes
   adapt to whatever the actual frequency distribution turns out
   to be. Re-mapping indices to (frequency-sorted) values gives
   the SAME entropy under Huffman coding.

2. **Our Lloyd output is already approximately frequency-ordered**
   via Stone D's split-on-empty heuristic — empty clusters get
   split from the highest-SSE (typically highest-population)
   donor, which roughly ladders into a frequency hierarchy.

3. **oxipng's filter sweep selects per-row filters that work well
   for any index ordering** — its filter heuristics are
   value-agnostic (Sub/Paeth depend on byte differences, not
   absolute values).

The lack of effect across all 7 fixtures suggests (1) and (2)
together dominate. Even pathologically-bad orderings would not
help; even (mostly-)optimal orderings cannot help.

## Implications for P3 paper

P3 "Joint palette-filter co-optimization" cannot improve via index
ordering alone. The remaining lever is:

- **Per-row filter selection driven by palette structure** rather
  than by raw byte values. e.g. if palette has clusters of similar
  colors, choosing the same filter for rows that move between
  clusters might compress better than per-row Adaptive heuristics.

This is structurally similar to oxipng's `Brute` / `MinSum`
strategies, which already approximate this. Cycle 53 ablation
showed those don't move the needle either.

P3 paper viability now hinges on **fundamentally restructuring the
palette assignment** (e.g. assigning palette such that adjacent
pixels frequently land in adjacent palette indices, exploiting
deflate's locality), not on post-hoc reordering. Substantially
harder and may not yield results — refining P3 plan accordingly.

## Negative-finding value

- Rules out a 1-line palette-reorder optimization.
- Refines P3 paper viability assessment downward.
- Confirms current pipeline output is near-Pareto-optimal for the
  oxipng path.

## Files touched

- `crates/nupic-research/examples/speed_sweep.rs` (Cycle 58 bench)
- `docs/research/png/04u-cycle58-palette-reorder-negative.md`
- `Cargo.toml` workspace 1.1.4 → 1.1.5
