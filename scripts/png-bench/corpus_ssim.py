#!/usr/bin/env python3
"""Compute SSIMULACRA2(nupic_output, original) and SSIMULACRA2(tinypng_output, original)
for every corpus-500 fixture. Writes corpus-500-ssim.tsv.

Reads from existing artifacts:
  - assets/png-bench/corpus-500/<fixture>           (input ref)
  - assets/png-bench/tinypng-corpus-500/<fixture>   (tiny output, just generated)
  - /Users/doracawl/workspace/labs/lab29-nupic/assets/png-bench/nupic-corpus-500/<fixture>                 (nupic v1.2.8 output, just generated)
"""
import os
import subprocess
import sys
import time

ROOT = "/Users/doracawl/workspace/labs/lab29-nupic"
INPUT_DIR = f"{ROOT}/assets/png-bench/corpus-500"
TINY_DIR = f"{ROOT}/assets/png-bench/tinypng-corpus-500"
NUPIC_DIR = "/Users/doracawl/workspace/labs/lab29-nupic/assets/png-bench/nupic-corpus-500"
NUPIC_BIN = f"{ROOT}/target/release/nupic"
OUT_TSV = f"{ROOT}/assets/png-bench/corpus-500-ssim.tsv"


def ssim(orig, cand):
    try:
        r = subprocess.run(
            [NUPIC_BIN, "compare", "-m", "ssimulacra2", orig, cand],
            check=True,
            capture_output=True,
            timeout=120,
            text=True,
        )
        for line in r.stdout.splitlines():
            if line.startswith("SSIMULACRA2: "):
                return float(line.split()[1])
    except Exception as e:
        return None
    return None


def main():
    files = sorted(os.listdir(INPUT_DIR))
    rows = []
    start = time.time()
    for i, fname in enumerate(files):
        orig = f"{INPUT_DIR}/{fname}"
        tiny = f"{TINY_DIR}/{fname}"
        nup = f"{NUPIC_DIR}/{fname}"
        if not os.path.exists(tiny) or not os.path.exists(nup):
            continue
        s_t = ssim(orig, tiny)
        s_n = ssim(orig, nup)
        rows.append((fname, s_n, s_t))
        if (i + 1) % 50 == 0:
            print(f"  ... {i+1}/{len(files)} ({time.time()-start:.0f}s)")
    with open(OUT_TSV, "w") as f:
        f.write("fixture\tnupic_ssim\ttinypng_ssim\n")
        for r in rows:
            sn = f"{r[1]:.4f}" if r[1] is not None else ""
            st = f"{r[2]:.4f}" if r[2] is not None else ""
            f.write(f"{r[0]}\t{sn}\t{st}\n")
    print(f"\nDone {len(rows)} fixtures in {time.time()-start:.0f}s → {OUT_TSV}")
    # Quick stats
    pairs = [(r[1], r[2]) for r in rows if r[1] is not None and r[2] is not None]
    if pairs:
        nupic_geq = sum(1 for n, t in pairs if n >= t)
        nupic_gt_3 = sum(1 for n, t in pairs if n > t + 3)
        nupic_lt_m3 = sum(1 for n, t in pairs if n < t - 3)
        mean_delta = sum(n - t for n, t in pairs) / len(pairs)
        print(f"  nupic SSIM ≥ tinypng SSIM: {nupic_geq} / {len(pairs)} ({100*nupic_geq/len(pairs):.1f}%)")
        print(f"  nupic SSIM > tinypng + 3:  {nupic_gt_3} / {len(pairs)} ({100*nupic_gt_3/len(pairs):.1f}%)")
        print(f"  nupic SSIM < tinypng - 3:  {nupic_lt_m3} / {len(pairs)} ({100*nupic_lt_m3/len(pairs):.1f}%)")
        print(f"  Mean(nupic_SSIM - tinypng_SSIM): {mean_delta:+.2f}")


if __name__ == "__main__":
    main()
