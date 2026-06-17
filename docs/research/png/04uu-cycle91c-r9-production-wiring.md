# 04uu · Cycle 91c — R9 ICM SIMD production wiring (GREEN, bit-exact)

**Status:** GREEN, shipped to `nupic-quantize` Stone C. Bit-exact A/B match on all
baseline-7 fixtures (PNG SHA256 identical before/after). Wall-time wins on the two
production fixtures that trigger the joint-anneal gate: **04 portrait −257 ms (1.36×)**,
**07 product −204 ms (1.32×)**. 9/9 unit tests pass.

## What changed

`crates/nupic-quantize/src/lib.rs` — replaced the Cycle 71 scalar ICM inner loop inside
the joint-anneal block (`should_anneal == true` path) with the Cycle 89 SoA + `f32x4` SIMD
implementation. The Cycle 71 anneal schedule (`λ² ∈ {1e-4, 5e-5, 2e-5}` × 3 passes) and the
gate (`n_pixels < 2.5M ∧ opq ≥ 0.95 ∧ var < 200`) are unchanged. Algorithm is unchanged —
same data term (OKLab L²), same Potts smoothness (4-neighbor mismatch count × λ²), same
argmin tiebreak (first-min by index). 4-wide lanes consume the palette as Struct-of-Arrays
with +1 e9 padding so argmin never selects a pad slot.

```text
// before
for &lambda_sq in &LAMBDAS {
    for y in 0..h { for x in 0..w { /* 30 lines scalar inner loop */ } }
    /* palette_retrain */
}

// after
for &lambda_sq in &LAMBDAS {
    let soa = IcmSoAPalette::from_oklab(&pal_ok);
    icm_step_simd(&src_oklab, w, h, &soa, &mut idx, lambda_sq);
    /* palette_retrain (unchanged) */
}
```

`icm_step_simd` + `IcmSoAPalette` are file-private helpers placed directly above
`quantize_indexed_png`. No public API surface change.

## A/B validation (3-run min, M3 Max release)

`~/.cargo/bin/nupic compress <fixture>` — old binary built from HEAD commit `954bad5`
(Cycle 90 essay, scalar ICM), new binary built from working tree (Cycle 91c, SIMD ICM):

| fixture | old (ms) | new (ms) | Δt (ms) | speedup | old (B) | new (B) | SHA256 |
|---|---:|---:|---:|---:|---:|---:|---|
| 01-png-transparency-demo | 318 | 318 | 0 | 1.00× | 46 191 | 46 191 | identical |
| 02-pluto-transparent | 253 | 251 | −2 | 1.01× | 60 789 | 60 789 | identical |
| 03-wikipedia-logo | 50 | 50 | 0 | 1.00× | 14 781 | 14 781 | identical |
| **04-photo-portrait** | **963** | **706** | **−257** | **1.36×** | 434 158 | 434 158 | **identical** |
| 05-photo-mountain | 576 | 580 | +4 | 1.00× | 326 977 | 326 977 | identical |
| 06-photo-landscape | 392 | 393 | +1 | 1.00× | 997 089 | 997 089 | identical |
| **07-photo-product** | **840** | **636** | **−204** | **1.32×** | 296 363 | 296 363 | **identical** |

**Bit-exact on all 7.** Wins land on 04 portrait and 07 product — the two fixtures that
hit the `should_anneal` gate (opaque + low variance + small). 05 / 06 have `var > 200`
and skip joint anneal; 01 / 02 are transparent; 03 is too small for the inner loop to
register against the encoder fixed cost. This is the **expected** R9 deployment surface —
roadmap predicted "baseline-7 04/06/07 −50 to −200 ms" and we got 04 −257 ms, 07 −204 ms.
06 doesn't anneal in production so its SIMD spike win (Cycle 89 reference) doesn't transfer.

## Decision gate

Per roadmap R9 (configured Cycle 89 ship gate): "baseline-7 04/06/07 perf −50 ms total
ICM time → ship". 04 portrait −257 ms and 07 product −204 ms satisfy the gate.
**GREEN → wired and shipped to nupic-quantize.**

## Why this works (and why R1 didn't)

Same fixture set, same image data, but R9 fits production while R1 stacked badly:

- **R9 is bit-exact perf:** same algorithm, just race more lanes. No quality movement,
  so no gating logic needed. Blast radius is zero — only the perf number moves.
- **R1 is metric-redesign quality:** different distance, different palette centroids,
  different output. Per-content split (Cycle 87) means it cannot ship blanket;
  Cycle 90 confirmed stacking it hurts perf without a quality win on every fixture.

This is the **§7 vs §6 paper distinction** the Cycle 90 essay (`04tt`) flagged:
optimizer-level improvements (R9 SIMD) are safe to deploy unconditionally; metric-level
improvements (R1 M-weighted Lloyd) need content classifiers (Cycle 91a).

## Files

- `crates/nupic-quantize/src/lib.rs:88-209` — new `IcmSoAPalette` struct + `icm_step_simd`
  fn (file-private).
- `crates/nupic-quantize/src/lib.rs:295-300` — call site replaces 30-line scalar loop.
- 9/9 `cargo test --release -p nupic-quantize` pass.

## Next cycle

Cycle 91a (R1 routing classifier) — features = (mean chroma, edge density, gradient
smoothness) → R1 on/off gate. Spike on the four R1-hostile fixtures (03 wiki, 05 mountain,
06 landscape, 07 product) and the four R1-friendly fixtures (01 trans, 02 pluto, 04
portrait, 25 sofia, 27 whale). Goal: per-fixture-correct gate decision with one classifier
threshold. Paper §6 routing analysis material.
