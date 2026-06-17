#!/usr/bin/env python3
"""Extract Pile A — Cycle 106 R4 RD attack targets.

Pile A = fixtures where nupic has quality headroom but loses on size.
Specifically: nupic_DSSIM <= 0.001 AND size_ratio > 1.3× tinypng.

These are 'perfect-quality-but-too-big' cases — exactly where main
RD attack should land. Writes corpus-500-pile-a.tsv.
"""
import csv

ROOT = "/Users/doracawl/workspace/labs/lab29-nupic"
SIZE_TSV = f"{ROOT}/assets/png-bench/corpus-500-three-axis.tsv"
DSSIM_TSV = f"{ROOT}/assets/png-bench/corpus-500-dssim.tsv"
OUT_TSV = f"{ROOT}/assets/png-bench/corpus-500-pile-a.tsv"


def load(path):
    with open(path) as f:
        r = csv.reader(f, delimiter="\t")
        next(r)
        return [row for row in r]


size = {r[0]: r for r in load(SIZE_TSV)}
dssim = {r[0]: r for r in load(DSSIM_TSV)}

pile = []
for fname, srow in size.items():
    if fname not in dssim:
        continue
    in_b, nu_b, ti_b, ratio = srow[1:5]
    nu_d, ti_d = dssim[fname][1:3]
    if not nu_d or not ti_d:
        continue
    nu_d = float(nu_d)
    ti_d = float(ti_d)
    ratio = float(ratio)
    if nu_d <= 0.001 and ratio > 1.3:
        pile.append((fname, int(in_b), int(nu_b), int(ti_b), ratio, nu_d, ti_d))

# Rank by descending savings potential = ti_d - nu_d (proxy for "how much quality
# tinypng gave up that we could give up too")
pile.sort(key=lambda r: (r[6] - r[5]), reverse=True)

with open(OUT_TSV, "w") as f:
    f.write("fixture\tinput_size\tnupic_size_v128\ttinypng_size\tsize_ratio\tnupic_dssim\ttinypng_dssim\n")
    for r in pile:
        f.write(f"{r[0]}\t{r[1]}\t{r[2]}\t{r[3]}\t{r[4]:.4f}\t{r[5]:.6f}\t{r[6]:.6f}\n")

print(f"Pile A: {len(pile)} fixtures matching nu_d≤0.001 ∧ size > 1.3× tiny → {OUT_TSV}")
print()
print(f"{'fixture':<45} {'nu_d':>8} {'ti_d':>8} {'size_x':>6} {'nu_KB':>7} {'ti_KB':>7}")
for r in pile[:20]:
    print(f"{r[0][:43]:<45} {r[5]:>8.4f} {r[6]:>8.4f} {r[4]:>6.3f} {r[2]/1024:>7.0f} {r[3]/1024:>7.0f}")
