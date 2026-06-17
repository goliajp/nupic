#!/usr/bin/env python3
"""Full two-axis (size × DSSIM) cross-tabulation for v1.2.8 vs TinyPNG."""
import csv
import re
import statistics as st

ROOT = "/Users/doracawl/workspace/labs/lab29-nupic"
SIZE_TSV  = f"{ROOT}/assets/png-bench/corpus-500-three-axis.tsv"
DSSIM_TSV = f"{ROOT}/assets/png-bench/corpus-500-dssim.tsv"
SSIM_TSV  = f"{ROOT}/assets/png-bench/corpus-500-ssim.tsv"


def classify(fname: str) -> str:
    if fname.startswith("mi"):
        return "mi (icons)"
    if re.match(r"^n\d+_", fname):
        return "n (NASA)"
    if re.match(r"^p\d+_", fname):
        return "p (Picsum HD)"
    if fname.startswith("s") and re.match(r"^s\d+_", fname):
        if "gradient" in fname: return "s (synth gradient)"
        if "stripes" in fname:  return "s (synth stripes)"
        if "solid" in fname:    return "s (synth solid)"
        if "noise" in fname:    return "s (synth noise)"
        return "s (synth other)"
    if fname.startswith("wm"):
        return "wm (Wikimedia)"
    return "?"


def load(tsv):
    rows = []
    with open(tsv) as f:
        r = csv.reader(f, delimiter="\t")
        next(r)
        for row in r:
            rows.append(row)
    return rows


size  = {row[0]: row for row in load(SIZE_TSV)}
dssim = {row[0]: row for row in load(DSSIM_TSV)}
ssim  = {row[0]: row for row in load(SSIM_TSV)}

merged = []
for fname, size_row in size.items():
    if fname not in dssim or fname not in ssim:
        continue
    in_s, nu_s, ti_s, ratio = size_row[1:5]
    dn, dt = dssim[fname][1:3]
    sn, st_ = ssim[fname][1:3]
    if not dn or not dt or not sn or not st_:
        continue
    merged.append({
        "f": fname,
        "cls": classify(fname),
        "in": int(in_s),
        "nu_b": int(nu_s),
        "ti_b": int(ti_s),
        "size_ratio": float(ratio),
        "nu_d": float(dn),
        "ti_d": float(dt),
        "nu_ss": float(sn),
        "ti_ss": float(st_),
    })

n = len(merged)
print(f"=== v1.2.8 vs TinyPNG · corpus-500 (n={n}) — DSSIM as primary quality metric ===\n")

# Two-axis verdict with DSSIM
def verdict_dssim(r):
    size_ok = r["size_ratio"] <= 0.80
    qual_ok = r["nu_d"] <= r["ti_d"]   # lower is better
    if size_ok and qual_ok: return "PASS"
    if (not size_ok) and qual_ok: return "FAIL-SIZE"
    if size_ok and (not qual_ok): return "FAIL-QUAL"
    return "FAIL-BOTH"

for r in merged:
    r["v_dssim"] = verdict_dssim(r)

buckets = {"PASS":0, "FAIL-SIZE":0, "FAIL-QUAL":0, "FAIL-BOTH":0}
for r in merged: buckets[r["v_dssim"]] += 1
print("Two-axis verdict (size ≤ 0.80× tiny AND DSSIM ≤ tiny DSSIM):")
for k, v in buckets.items():
    print(f"  {k:<10} {v:>4} ({100*v/n:.1f}%)")

# Compare to SSIMULACRA2-based verdict
def verdict_ssim(r):
    size_ok = r["size_ratio"] <= 0.80
    qual_ok = r["nu_ss"] >= r["ti_ss"]
    if size_ok and qual_ok: return "PASS"
    if (not size_ok) and qual_ok: return "FAIL-SIZE"
    if size_ok and (not qual_ok): return "FAIL-QUAL"
    return "FAIL-BOTH"

for r in merged:
    r["v_ssim"] = verdict_ssim(r)

buckets2 = {"PASS":0, "FAIL-SIZE":0, "FAIL-QUAL":0, "FAIL-BOTH":0}
for r in merged: buckets2[r["v_ssim"]] += 1
print("\nFor comparison, SSIMULACRA2-based verdict:")
for k, v in buckets2.items():
    print(f"  {k:<10} {v:>4} ({100*v/n:.1f}%)")

# Disagreement
print("\nMetric disagreement on quality axis:")
both_good = sum(1 for r in merged if r["nu_d"] <= r["ti_d"] and r["nu_ss"] >= r["ti_ss"])
both_bad  = sum(1 for r in merged if r["nu_d"] > r["ti_d"] and r["nu_ss"] < r["ti_ss"])
agree_either = both_good + both_bad
disagree = n - agree_either
print(f"  Agree (both metrics say same): {agree_either} / {n} ({100*agree_either/n:.1f}%)")
print(f"  Disagree:                      {disagree} / {n} ({100*disagree/n:.1f}%)")

# Per-class with DSSIM verdict
classes = {}
for r in merged:
    classes.setdefault(r["cls"], []).append(r)

print(f"\n{'class':<22} {'n':>4}  {'PASS':>4} {'F-SZ':>4} {'F-QL':>4} {'F-BOTH':>6}   "
      f"{'med_sz':>6} {'med_n_d':>7} {'med_t_d':>7} {'nuD≤tD':>6}   "
      f"{'∑nu_MB':>7} {'∑ti_MB':>7} {'sz_x':>6}")
print("-" * 150)
for cls in sorted(classes):
    rows = classes[cls]
    nc = len(rows)
    p   = sum(1 for r in rows if r["v_dssim"] == "PASS")
    fz  = sum(1 for r in rows if r["v_dssim"] == "FAIL-SIZE")
    fq  = sum(1 for r in rows if r["v_dssim"] == "FAIL-QUAL")
    fb  = sum(1 for r in rows if r["v_dssim"] == "FAIL-BOTH")
    med_sz   = st.median(r["size_ratio"] for r in rows)
    med_n_d  = st.median(r["nu_d"] for r in rows)
    med_t_d  = st.median(r["ti_d"] for r in rows)
    win_q    = sum(1 for r in rows if r["nu_d"] <= r["ti_d"]) / nc
    sum_n    = sum(r["nu_b"] for r in rows)
    sum_t    = sum(r["ti_b"] for r in rows)
    print(f"{cls:<22} {nc:>4}  "
          f"{p:>4} {fz:>4} {fq:>4} {fb:>6}   "
          f"{med_sz:>6.3f} {med_n_d:>7.4f} {med_t_d:>7.4f} {100*win_q:>5.0f}%   "
          f"{sum_n/1e6:>7.1f} {sum_t/1e6:>7.1f} {sum_n/sum_t:>5.3f}×")
print("-" * 150)
print(f"{'ALL':<22} {n:>4}  "
      f"{buckets['PASS']:>4} {buckets['FAIL-SIZE']:>4} {buckets['FAIL-QUAL']:>4} {buckets['FAIL-BOTH']:>6}   "
      f"{st.median(r['size_ratio'] for r in merged):>6.3f} "
      f"{st.median(r['nu_d'] for r in merged):>7.4f} "
      f"{st.median(r['ti_d'] for r in merged):>7.4f} "
      f"{100*sum(1 for r in merged if r['nu_d'] <= r['ti_d'])/n:>5.0f}%   "
      f"{sum(r['nu_b'] for r in merged)/1e6:>7.1f} "
      f"{sum(r['ti_b'] for r in merged)/1e6:>7.1f} "
      f"{sum(r['nu_b'] for r in merged) / sum(r['ti_b'] for r in merged):>5.3f}×")

# Top-15 worst by DSSIM regression (nupic quality clearly worse than tiny in DSSIM)
worst_q = sorted(merged, key=lambda r: r["nu_d"] - r["ti_d"], reverse=True)[:15]
print("\nTop-15 fixtures where nupic DSSIM is most worse than tinypng (visual loss bucket):")
print(f"  {'fixture':<45} {'class':<22} {'nu_d':>7} {'ti_d':>7} {'Δdss':>7} {'size_x':>6} {'nu_KB':>7}")
for r in worst_q:
    print(f"  {r['f'][:43]:<45} {r['cls']:<22} {r['nu_d']:>7.4f} {r['ti_d']:>7.4f} {r['nu_d']-r['ti_d']:>+7.4f} "
          f"{r['size_ratio']:>6.3f} {r['nu_b']/1024:>7.0f}")

# Largest FAIL-SIZE (where we have quality headroom to trade for size)
print("\nTop-15 fixtures with biggest quality headroom (nupic DSSIM much lower than tiny → room to trade SSIM for size):")
fail_size = [r for r in merged if r["v_dssim"] == "FAIL-SIZE"]
fail_size.sort(key=lambda r: r["ti_d"] - r["nu_d"], reverse=True)
print(f"  {'fixture':<45} {'class':<22} {'nu_d':>7} {'ti_d':>7} {'Δdss':>7} {'size_x':>6} {'nu_KB':>7} {'ti_KB':>7}")
for r in fail_size[:15]:
    print(f"  {r['f'][:43]:<45} {r['cls']:<22} {r['nu_d']:>7.4f} {r['ti_d']:>7.4f} {r['nu_d']-r['ti_d']:>+7.4f} "
          f"{r['size_ratio']:>6.3f} {r['nu_b']/1024:>7.0f} {r['ti_b']/1024:>7.0f}")

# Bytes summary
sum_in = sum(r["in"] for r in merged)
sum_nu = sum(r["nu_b"] for r in merged)
sum_ti = sum(r["ti_b"] for r in merged)
print(f"\nBytes summary across n={n}:")
print(f"  Σ input   = {sum_in/1e6:>7.1f} MB  (1.00×)")
print(f"  Σ nupic   = {sum_nu/1e6:>7.1f} MB  ({sum_nu/sum_in:.3f}× input)")
print(f"  Σ tinypng = {sum_ti/1e6:>7.1f} MB  ({sum_ti/sum_in:.3f}× input)")
print(f"  cohort:    nupic / tiny = {sum_nu/sum_ti:.4f}×  ({100*(sum_nu-sum_ti)/sum_ti:+.2f}%)")
