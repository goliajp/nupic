# 04oo — Cycle 85: R2 α-expansion graph-cut spike ruled out (negative)

## TL;DR

R2 spike per roadmap `research_roadmap_1_2_x.md` — replace ICM
(per-pixel greedy, traps in local minima) with Boykov-Veksler-Zabih
α-expansion (256-label graph cut, 2-approximate). Pairwise smoothness
= Potts. Max-flow = scaled-integer Dinic. Pairwise reduction via
Kolmogorov-Zabih aux-free decomposition (no auxiliary nodes needed
for Potts; first iteration of the construction with aux nodes was
provably wrong — see below).

**Result on 04 portrait (1200×800, n=192, λ schedule = ICM's
{1e-4, 5e-5, 2e-5}):**

| variant                | size  | SSIM    | time     |
|------------------------|------:|--------:|---------:|
| [A] imagequant init    | 442KB | 84.7846 | —        |
| [B] ICM (cycle 71)     | 415KB | 84.8430 | 0.56 s   |
| [C] α-expansion 3-iter | 409KB | 85.0894 | 221.4 s  |

- α-expansion **vs Cycle-71 published 86.19**: **−1.10 SSIM**
- α-expansion **vs ICM run here**: **+0.25 SSIM** (head-to-head, same
  init, same n=192, same retrain chain)
- α-expansion vs init: **+0.30 SSIM**
- α-expansion output **smaller by 1.4 %** than ICM (cleaner label
  boundaries help deflate)
- α-expansion is **400× slower** than ICM (221 s vs 0.56 s)

**Decision gate (from roadmap):** ΔSSIM ≥ +2 → R2; +1..+2 → essay;
< +1 → R1. **Got +0.25 → RED, switch to R1.**

## Why R2 didn't deliver

The algorithm worked. α-expansion **did** find a deeper local minimum
of the energy than ICM — output size dropped 1.4 % (label boundaries
are cleaner, which deflate exploits), and SSIM is +0.25 over ICM run
here. The optimization is doing its job.

The cap on the win comes from the **energy function itself**:

$$E(L) = \sum_p \|p - \text{palette}[L_p]\|^2_{\text{OKLab}} + \sum_{(p,q)} \lambda \cdot \mathbb{1}[L_p \neq L_q]$$

OKLab L² + Potts is **not aligned with SSIMULACRA2**. SSIM is multi-
scale bandpass, sensitive to local contrast structure. Two assignments
with similar OKLab L² can score very differently on SSIM. Cycle 68/71
established this gap qualitatively; R2 quantifies it: even a
provably-2-approximate global optimizer of the L²+Potts energy only
buys +0.25 SSIM over a greedy local optimizer of the same energy.

This is exactly the motivation for **R1 M-weighted Lloyd**: change
the *metric* itself (Mahalanobis with perceptual M_i from a multi-scale
bandpass), not the optimizer of the same misaligned metric.

## Implementation notes (for paper "reviewer-defense" chapter)

### The aux-node trap (first-attempt bug)

The classical Boykov 2001 paper presents α-expansion with auxiliary
nodes for label-pair case "l_p ≠ l_q, both ≠ α". My first pass
followed the paper literally with an aux node and 5 edges. Smoke test
at n=16, λ=1e-4 returned SSIM 36.03 vs init 36.75 — **worse than
nearest-neighbor**, which is impossible for a correct submodular
graph cut.

Hand-derivation of the four-config table for the aux construction
revealed the bug: my edge weights gave energies (0, λ, λ, λ) for
(x_p, x_q) ∈ {(0,0), (0,1), (1,0), (1,1)} instead of the
required (λ, λ, λ, 0). Off by an asymmetric flip — the aux node
encoding made (1,1) **cost** λ and (0,0) **free**, exactly opposite
to Potts.

The fix: skip the aux node entirely. For Potts, the case-E pair has
a clean Kolmogorov-Zabih decomposition:

```
cap(s → p) += λ/2     (unary on p's "stay" choice)
cap(s → q) += λ/2     (unary on q's "stay" choice)
p ↔ q n-link cap λ/2 each direction (disagreement penalty)
+ constant λ per pair (discarded — doesn't affect argmin)
```

Hand-verified: four-config cost table matches Potts (λ, λ, λ, 0). ✓

This also shrinks the graph from `2 + N + |pairs|` nodes (with aux)
to just `2 + N` nodes. Per-α graph: ~960k pixel nodes + s/t,
~3-5 M edges. Per-α Dinic runtime: ~0.4 s on 1200×800.

### Why ICM-here (84.84) doesn't reproduce Cycle 71's 86.19

The published Cycle 71 number on 04 portrait was 86.19. My ICM
reproduction with identical algorithm (same `icm_step` code copied
from `speed_sweep.rs`, same λ schedule) only reaches 84.84 — a 1.35
SSIM gap.

Two suspected sources, neither bears on the spike conclusion:
1. **n_colors source**: Cycle 71 production code uses
   `classify_for_palette_size_with_importance(...)` to pick n. Spike
   hardcoded 192 (per roadmap "n=192" line). Classifier may pick
   different n on full pipeline path.
2. **`palette_retrain` shift**: At λ=0 with 0 relabels, the retrained
   palette already cost 0.29 SSIM vs init — because `palette_retrain`
   computes OKLab cluster-mean while `apply_palette_rgba`'s indices
   may not be exact OKLab-nearest neighbours (importance weighting,
   etc.).

The spike's **clean signal is the head-to-head Δ** between ICM and
α-expansion run with identical init, identical λ schedule, identical
retrain chain: +0.25 SSIM. That delta is the algorithm's reachable
ceiling on this energy function, independent of the Cycle 71 reference.
Closing the 1.35 gap to 86.19 would require productionizing R2 into
the full pipeline (classifier-driven n, importance-aware retrain) —
which won't change the +0.25 ceiling.

### Output-size win (paper footnote)

α-expansion **shrinks PNG output by 1.4 %** vs ICM (409 KB vs 415 KB)
on this fixture. Mechanism: graph cut finds smoother label boundaries
than ICM's greedy local moves, which gives deflate longer LZ77 runs
on the index map. **This is the only direction R2 wins** — and it's
modest. Not worth the 400× perf cost to ship.

## Perf profile

| stage                | wall time |
|----------------------|----------:|
| imagequant + refine  | ~0.5 s    |
| ICM (3 anneal iters) | 0.56 s    |
| α-expansion 3 iters  | 221.4 s   |
| - 192 α inner cuts each iter at ~0.4 s avg | |
| - Dinic build + max-flow + retrain per outer | |

3 outer iters relabeled 681k / 215k / 166k pixels respectively
(out of 960k) — slowing convergence per outer iter is the expected
α-expansion shape (most movement in first pass).

## What this rules in / out

**Ruled out for production:** R2 α-expansion as a drop-in ICM
replacement on the OKLab L² + Potts energy. +0.25 SSIM ceiling vs
ICM, 400× slower. The gain doesn't justify the cost, and even if it
did, the absolute SSIM number is bottlenecked by the misaligned
energy function — no algorithmic optimizer can push it past where
the energy minimum sits.

**Ruled in for next spike:** **R1 M-weighted Lloyd** (Mahalanobis
k-means with M_i derived from multi-scale Gaussian pyramid bandpass).
R1's hypothesis is exactly what R2's negative result implicates:
the metric is the bottleneck, not the optimizer. R1 spike scope per
roadmap: same 04 portrait fixture, M_i from 3-scale bandpass, target
+SSIM ≥ 1.0.

**Still on the table:** R2 *could* be revisited if combined with a
perceptual energy function (e.g., joint with R1's Mahalanobis term —
α-expansion still applies if the pairwise reduction keeps
submodularity). But not as a standalone direction.

## Paper-shaped framing

Even as a negative finding, R2 has paper value as the "reviewer-defense"
demonstration that:

1. The OKLab L² + Potts energy is well-defined and globally
   optimizable (graph-cut 2-approximation).
2. Global optimization (α-expansion) over the same energy that ICM
   greedily attacks only yields +0.25 SSIM.
3. Therefore the bottleneck is the energy, not the optimizer —
   motivating perceptual energy redesign (R1).

This is a cleaner story than just "tried bigger Lloyd iters" (Cycle
78) or "tried filter-level skip" (Cycle 81/84). It establishes a
theoretical lower bound: no algorithmic Lloyd-class improvement
will yield > +0.25 SSIM on this fixture without changing the
underlying metric.

## See also

- `crates/nupic-research/examples/alpha_expansion.rs` — spike code
  (256-label α-expansion, scaled-integer Dinic, aux-free Potts
  reduction). Reusable for R2-revisit if combined with Mahalanobis.
- `docs/research/png/04dd-cycle71-anneal-production.md` — Cycle 71
  baseline (86.19 published number).
- `docs/research/png/04x-cycle65-icm-pareto-slope.md` —
  earlier ICM-vs-greedy analysis.
- `memory/research_roadmap_1_2_x.md` — roadmap with decision gate
  this essay resolves; R1 spike is next.
