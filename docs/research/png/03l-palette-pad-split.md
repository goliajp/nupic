# 03l — Stone D palette pad + split-on-empty(04-portrait beats TinyPNG)

> Cycle 7 Pass 1 discovery:`nupic` v0.5.24 output for 04-portrait had
> PLTE chunk **342 bytes(114 colors)** while TinyPNG used full
> **768 bytes(256 colors)**。imagequant returns < n_colors entries
> when quality threshold(95)is met early on photo-class inputs;
> Stone D Lloyd refinement starts at that count,can only shrink。
> 
> Pass 2 fix:**pad palette to n_colors** in `train_palette_rgba`
> (duplicates of first entry)+ **split-on-empty heuristic in Lloyd
> refinement**(empty slots receive split of highest-SSE cluster)。
>
> Result on 04-portrait: SSIM **83.06 → 87.99**(+4.93,**beats
> TinyPNG by +2.13**),size +28%。03-wikipedia hits SSIM 100(bit-
> exact)。Corpus 0.865× → 0.912× TinyPNG。Net "又小又好" win on
> previously-failing fixture。

---

## 1. Diagnostic

PLTE chunk size per fixture(v0.5.24 vs TinyPNG):

| fixture | nupic PLTE | nupic colors | tiny PLTE | tiny colors |
|---|---:|---:|---:|---:|
| 01-transparency | 768 | 256 | n/a(non-indexed)| |
| 02-pluto | 768 | 256 | 768 | 256 |
| 03-wikipedia | 330 | **110** | 78 | 26(actually used)|
| **04-portrait** | 342 | **114** | 768 | **256** |
| 05-mountain | 768 | 256 | 768 | 256 |
| 06-landscape | 768 | 256 | 768 | 256 |
| 07-product | 492 | **164** | 768 | 256 |

Photo / logo inputs lose palette slots after Stone D Lloyd refinement;
photos with smooth gradients(04 portrait skin tones)benefit most from
having full 256 distinct palette entries。

---

## 2. Root cause

Two stages:

1. **`train_palette_rgba`** calls `imagequant::quantize` with
   `set_quality(70, 95)`(or fallback `(0, 95)`)。imagequant quality
   = output similarity to source。If image is "easy"(few perceptually
   distinct colors,e.g. logos)or has smooth gradient that's
   captured well by < 256 centroids,imagequant stops at the smallest
   palette meeting quality 95。04 portrait returns ~ 200 entries。

2. **`refine_palette_kmeans`** runs Lloyd's k-means on N input
   centroids。Each iteration:assign pixels → recompute means。
   Clusters with zero assigned pixels stay at their centroid but are
   never picked by argmin → effectively dead。Lloyd reduces effective
   palette,never grows it。

End result:04 portrait final palette 114 colors despite n_colors=256
budget。Lost 142 slots of perceptual budget。

---

## 3. Fix

### 3.1 Pad in `train_palette_rgba`

```rust
// Pad to n_colors via duplication so Lloyd has full slot budget
if let (Some(&first_ok), Some(&first_a)) = (oklab.first(), alpha.first()) {
    while oklab.len() < n {
        oklab.push(first_ok);
        alpha.push(first_a);
    }
}
```

All dupes are identical to first entry。Lloyd's argmin picks the
first one(tie-break by lower index)so dupes get 0 pixels → empty。
**Split-on-empty heuristic** then redistributes them。

### 3.2 Split-on-empty in `refine_palette_kmeans`

After each Lloyd iteration,compute SSE(sum squared error)per cluster。
For each empty slot,take the highest-SSE non-empty cluster as donor,
**split its centroid via ±σ perturbation along L axis**(σ derived
from cluster's RMS error)。Next iteration argmin partitions the
high-SSE cluster's pixels into the two halves。

```rust
let sigma = (donor_sse / count[donor] as f64).sqrt().max(0.001) as f32;
palette[empty_j] = Oklab { l: donor_c.l - sigma * 0.5, ..donor_c };
palette[donor]   = Oklab { l: donor_c.l + sigma * 0.5, ..donor_c };
```

Force `max_move ≥ EPS_SQ * 4` to prevent early exit after a split。

---

## 4. Result(v0.5.25)

| fixture | v0.5.24 SSIM | **v0.5.25 SSIM** | Δ | v0.5.24 size | v0.5.25 size | Δ |
|---|---:|---:|---:|---:|---:|---:|
| 01-transparency | -46.42 | -46.42 | 0 | 45 364 | 45 364 | 0 |
| 02-pluto | 79.66 | 79.66 | 0 | 158 109 | 158 109 | 0 |
| **03-wikipedia** | 89.49 | **100.00** | **+10.51** | 12 658 | 14 718 | +2 060 |
| **04-portrait** | 83.06 | **87.99** | **+4.93** | 378 428 | 484 513 | +106 085 |
| 05-mountain | 70.38 | 70.38 | 0 | 389 264 | 389 264 | 0 |
| 06-landscape | 82.77 | 82.77 | 0 | 1 035 965 | 1 035 965 | 0 |
| **07-product** | 82.84 | **84.70** | **+1.86** | 320 864 | 340 640 | +19 776 |
| TOTAL | | | | 2 340 652 | 2 468 573 | +127 921(+5.5%)|
| **vs TinyPNG** | 0.865× | **0.912×** | +5.4 pp | | | |

Photo fixtures(04 / 07)and logo(03)gain;flat-fill / mountain /
landscape unchanged。

**04-portrait now beats TinyPNG**(87.99 vs 85.86,**+2.13 pt**)at
**0.85× TinyPNG size**(484 KB vs 570 KB)。"又小又好" achieved on the
only previously-failing fixture。

**03-wikipedia bit-exact reproduction**(SSIM 100)— Lloyd-with-split
finds the source palette exactly。Size +16% vs v0.5.24(13K → 15K)
to add the extra entries,but logo is tiny enough that absolute cost
is negligible。

---

## 5. Cycle 7 ship summary

| metric | v0.5.24 | **v0.5.25** |
|---|---|---|
| corpus size | 2 340 652 | **2 468 573** |
| size vs TinyPNG | 0.865× | **0.912×** |
| SSIM > TinyPNG fixtures | 6/7(04 fail)| **7/7** |
| 04-portrait SSIM vs tiny | -2.80 | **+2.13** |
| 03-wikipedia SSIM | 89.49 | **100.00** |

Trade-off:**+5.5% corpus size for +SSIM on 3 fixtures + perfect
correctness on logo**。"又小又好" research-density default ships this。

For size-strict users,future `--effort N` flag could restore the
no-pad path(or `--palette-cap N` for explicit smaller palette)。

---

## 6. cross-link

- 03d Stone D Lloyd k-means(introduced refine_palette_kmeans)
- 03c-ter Stone C graduation(established imagequant + Lloyd pipeline)
- portrait_deep.rs(Cycle 7 Pass 1 bench that found 84.43 SSIM ceiling
  was actually palette-size-limited,not algorithm-limited)

---

## 7. 价值观

- [[feedback-ceiling-first-priorities]] — ceiling distance to TinyPNG
  on 04-portrait was +5 pt;cap removal closes it。Right level of attack
  identified by PLTE-byte diagnostic on actual output。
- [[feedback-metric-over-human-eye]] — investigating "why nupic loses
  on 04" instead of "trying things",found the structural cause in
  one diagnostic comparison。
- [[feedback-no-cost-thinking]] — +5.5% corpus size traded for +5 SSIM
  on photo fixtures + bit-exact logo;no ROI evaluation,just data。
