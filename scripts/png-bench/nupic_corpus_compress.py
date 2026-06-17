#!/usr/bin/env python3
"""Compare nupic v1.2.8 Auto output vs TinyPNG output across corpus-500.

Run AFTER tinypng_corpus.py completes. Reads:
  - assets/png-bench/corpus-500/*.png (input)
  - assets/png-bench/tinypng-corpus-500/*.png (TinyPNG output)
  - assets/png-bench/corpus-500-tinypng-results.tsv (sizes + status)

For each fixture where both inputs exist:
  - Run `nupic compress` to /Users/doracawl/workspace/labs/lab29-nupic/assets/png-bench/nupic-corpus-500/<fixture>
  - Record input_size, nupic_size, tinypng_size, ratio
  - Aggregate ratio vs TinyPNG across all successfully-processed fixtures

Output:
  - assets/png-bench/corpus-500-three-axis.tsv: per-fixture row
  - stdout: summary table + 'pass cohort gate?' verdict
"""
import os
import subprocess
import sys
import time

ROOT = "/Users/doracawl/workspace/labs/lab29-nupic"
INPUT_DIR = f"{ROOT}/assets/png-bench/corpus-500"
TINY_DIR = f"{ROOT}/assets/png-bench/tinypng-corpus-500"
NUPIC_BIN = f"{ROOT}/target/release/nupic"
WORK_DIR = "/Users/doracawl/workspace/labs/lab29-nupic/assets/png-bench/nupic-corpus-500"
OUT_TSV = f"{ROOT}/assets/png-bench/corpus-500-three-axis.tsv"

os.makedirs(WORK_DIR, exist_ok=True)


def nupic_compress(input_path: str, out_path: str) -> int:
    subprocess.run(
        [NUPIC_BIN, "compress", "-o", out_path, input_path],
        check=True,
        capture_output=True,
        timeout=120,
    )
    return os.path.getsize(out_path)


def main():
    files = sorted(os.listdir(INPUT_DIR))
    rows = []
    start = time.time()
    for i, fname in enumerate(files):
        in_path = f"{INPUT_DIR}/{fname}"
        tiny_path = f"{TINY_DIR}/{fname}"
        out_path = f"{WORK_DIR}/{fname}"
        if not os.path.exists(tiny_path):
            continue  # no tinypng reference, skip
        in_size = os.path.getsize(in_path)
        tiny_size = os.path.getsize(tiny_path)
        try:
            nupic_size = nupic_compress(in_path, out_path)
        except subprocess.CalledProcessError as e:
            print(f"FAIL nupic {fname}: {e}")
            continue
        except subprocess.TimeoutExpired:
            print(f"TIMEOUT nupic {fname}")
            continue
        ratio_n_t = nupic_size / tiny_size
        rows.append((fname, in_size, nupic_size, tiny_size, ratio_n_t))
        if (i + 1) % 50 == 0:
            print(f"  ... {i+1}/{len(files)} processed ({time.time()-start:.0f}s)")

    # Write TSV
    with open(OUT_TSV, "w") as f:
        f.write("fixture\tinput_size\tnupic_size\ttinypng_size\tratio_n_over_t\n")
        for r in rows:
            f.write(f"{r[0]}\t{r[1]}\t{r[2]}\t{r[3]}\t{r[4]:.4f}\n")
    print(f"\nTSV → {OUT_TSV}")

    # Aggregate
    n_total = sum(r[2] for r in rows)
    t_total = sum(r[3] for r in rows)
    n_under_t = sum(1 for r in rows if r[2] < r[3])
    n_under_80 = sum(1 for r in rows if r[4] <= 0.80)
    print(f"\n=== nupic v1.2.8 vs TinyPNG on corpus-500 (n={len(rows)}) ===")
    print(f"  Aggregate ratio:    {n_total/t_total:.4f}×")
    print(f"  Per-fix nupic ≤ tiny: {n_under_t} / {len(rows)} ({100*n_under_t/len(rows):.1f}%)")
    print(f"  Per-fix nupic ≤ 0.80× tiny: {n_under_80} / {len(rows)} ({100*n_under_80/len(rows):.1f}%)")
    # Worst-case
    rows.sort(key=lambda r: -r[4])
    print("\n  Top-10 worst ratio (nupic / tiny):")
    for r in rows[:10]:
        print(f"    {r[4]:.3f}×  {r[0]}  nupic={r[2]:,}  tiny={r[3]:,}")
    print("\n  Top-10 best ratio:")
    for r in rows[-10:]:
        print(f"    {r[4]:.3f}×  {r[0]}  nupic={r[2]:,}  tiny={r[3]:,}")


if __name__ == "__main__":
    main()
