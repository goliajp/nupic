#!/usr/bin/env python3
"""DSSIM(nupic_output, ref) and DSSIM(tinypng_output, ref) for corpus-500."""
import os
import subprocess
import time

ROOT = "/Users/doracawl/workspace/labs/lab29-nupic"
INPUT_DIR = f"{ROOT}/assets/png-bench/corpus-500"
TINY_DIR = f"{ROOT}/assets/png-bench/tinypng-corpus-500"
NUPIC_DIR = "/Users/doracawl/workspace/labs/lab29-nupic/assets/png-bench/nupic-corpus-500"
NUPIC_BIN = f"{ROOT}/target/release/nupic"
OUT_TSV = f"{ROOT}/assets/png-bench/corpus-500-dssim.tsv"


def dssim(orig, cand):
    try:
        r = subprocess.run(
            [NUPIC_BIN, "compare", "-m", "dssim", orig, cand],
            check=True, capture_output=True, timeout=120, text=True,
        )
        for line in r.stdout.splitlines():
            if line.startswith("DSSIM: "):
                return float(line.split()[1])
    except Exception:
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
        d_n = dssim(orig, nup)
        d_t = dssim(orig, tiny)
        rows.append((fname, d_n, d_t))
        if (i + 1) % 50 == 0:
            print(f"  ... {i+1}/{len(files)} ({time.time()-start:.0f}s)")
    with open(OUT_TSV, "w") as f:
        f.write("fixture\tnupic_dssim\ttinypng_dssim\n")
        for r in rows:
            dn = f"{r[1]:.6f}" if r[1] is not None else ""
            dt = f"{r[2]:.6f}" if r[2] is not None else ""
            f.write(f"{r[0]}\t{dn}\t{dt}\n")
    print(f"\nDone {len(rows)} fixtures in {time.time()-start:.0f}s → {OUT_TSV}")
    pairs = [(r[1], r[2]) for r in rows if r[1] is not None and r[2] is not None]
    if pairs:
        nupic_better = sum(1 for n, t in pairs if n < t)
        nupic_much_better = sum(1 for n, t in pairs if n < t * 0.5)
        nupic_much_worse = sum(1 for n, t in pairs if n > t * 2.0)
        mean_n = sum(n for n, _ in pairs) / len(pairs)
        mean_t = sum(t for _, t in pairs) / len(pairs)
        print(f"  nupic DSSIM < tinypng DSSIM (nupic better): {nupic_better}/{len(pairs)} ({100*nupic_better/len(pairs):.1f}%)")
        print(f"  nupic < 0.5× tinypng (much better):         {nupic_much_better}/{len(pairs)} ({100*nupic_much_better/len(pairs):.1f}%)")
        print(f"  nupic > 2.0× tinypng (much worse):          {nupic_much_worse}/{len(pairs)} ({100*nupic_much_worse/len(pairs):.1f}%)")
        print(f"  mean nupic = {mean_n:.4f}    mean tiny = {mean_t:.4f}")


if __name__ == "__main__":
    main()
