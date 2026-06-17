#!/usr/bin/env python3
"""Batch TinyPNG compress assets/png-bench/corpus-500/ → tinypng-corpus-500/.

Strategy:
- Sort files by size ascending (smaller files first, higher success prob).
- Skip files where output already exists (resumable).
- Stop if Compression-Count >= 500 (monthly quota guard).
- Parallel pool of N workers (default 4).
- Skip files > 5 MB if the API rejects them (415).
- Write TSV: assets/png-bench/corpus-500-tinypng-results.tsv with
  columns: fixture, input_size, tinypng_size, ratio.
"""
import base64
import concurrent.futures as cf
import json
import os
import sys
import threading
import time
import urllib.error
import urllib.request

KEY = os.environ["TINIFY_KEY"]
ROOT = "/Users/doracawl/workspace/labs/lab29-nupic"
INPUT_DIR = f"{ROOT}/assets/png-bench/corpus-500"
OUTPUT_DIR = f"{ROOT}/assets/png-bench/tinypng-corpus-500"
TSV_PATH = f"{ROOT}/assets/png-bench/corpus-500-tinypng-results.tsv"
QUOTA_CAP = 500
WORKERS = 4

AUTH = base64.b64encode(f"api:{KEY}".encode()).decode()

quota_lock = threading.Lock()
quota_count = 0  # latest seen Compression-Count
quota_stopped = threading.Event()

results = []  # (fixture, input_size, tinypng_size_or_None, ratio_or_None, status)
results_lock = threading.Lock()


def shrink_one(fname: str, input_size: int) -> dict:
    """POST file body to /shrink, return parsed JSON.  Raises on error."""
    with open(f"{INPUT_DIR}/{fname}", "rb") as f:
        body = f.read()
    req = urllib.request.Request(
        "https://api.tinify.com/shrink",
        data=body,
        headers={
            "Authorization": f"Basic {AUTH}",
            "Content-Type": "image/png",
        },
        method="POST",
    )
    resp = urllib.request.urlopen(req, timeout=120)
    cc = resp.headers.get("Compression-Count")
    payload = json.loads(resp.read().decode())
    payload["_compression_count"] = int(cc) if cc else None
    payload["_output_url"] = resp.headers.get("Location")
    return payload


def download_output(url: str, dest: str) -> int:
    req = urllib.request.Request(
        url,
        headers={"Authorization": f"Basic {AUTH}"},
    )
    resp = urllib.request.urlopen(req, timeout=120)
    data = resp.read()
    with open(dest, "wb") as f:
        f.write(data)
    return len(data)


def process(fname: str):
    global quota_count
    if quota_stopped.is_set():
        return
    in_path = f"{INPUT_DIR}/{fname}"
    out_path = f"{OUTPUT_DIR}/{fname}"
    input_size = os.path.getsize(in_path)
    if os.path.exists(out_path):
        out_size = os.path.getsize(out_path)
        with results_lock:
            results.append((fname, input_size, out_size, out_size / input_size, "cached"))
        return
    try:
        info = shrink_one(fname, input_size)
        cc = info.get("_compression_count")
        if cc:
            with quota_lock:
                if cc > quota_count:
                    quota_count = cc
                if cc >= QUOTA_CAP:
                    quota_stopped.set()
        url = info["_output_url"]
        out_size = download_output(url, out_path)
        ratio = info["output"]["ratio"]
        with results_lock:
            results.append((fname, input_size, out_size, ratio, f"ok cc={cc}"))
        print(f"OK  {fname}  in={input_size}  out={out_size}  ratio={ratio:.3f}  cc={cc}")
    except urllib.error.HTTPError as e:
        body = e.read().decode()[:200]
        with results_lock:
            results.append((fname, input_size, None, None, f"http{e.code} {body}"))
        print(f"ERR {fname}  http{e.code}  {body}")
        # Auth or quota fatal — stop everything
        if e.code in (401, 429):
            quota_stopped.set()
    except Exception as e:
        with results_lock:
            results.append((fname, input_size, None, None, f"err {type(e).__name__}: {e}"))
        print(f"ERR {fname}  {type(e).__name__}: {e}")


def main():
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    files = sorted(os.listdir(INPUT_DIR))
    # Sort by file size ascending
    files = sorted(files, key=lambda f: os.path.getsize(f"{INPUT_DIR}/{f}"))
    print(f"Total files: {len(files)}, workers={WORKERS}, quota cap={QUOTA_CAP}")
    start = time.time()
    with cf.ThreadPoolExecutor(max_workers=WORKERS) as ex:
        futures = [ex.submit(process, f) for f in files]
        for f in cf.as_completed(futures):
            f.result()
    dur = time.time() - start
    print(f"\nDone in {dur:.1f}s.  Latest Compression-Count={quota_count}.")
    # Write TSV
    with open(TSV_PATH, "w") as f:
        f.write("fixture\tinput_size\ttinypng_size\tratio\tstatus\n")
        for r in sorted(results):
            fixture, input_size, ts, ratio, status = r
            ts_s = str(ts) if ts is not None else ""
            ratio_s = f"{ratio:.4f}" if ratio is not None else ""
            f.write(f"{fixture}\t{input_size}\t{ts_s}\t{ratio_s}\t{status}\n")
    print(f"TSV → {TSV_PATH}")
    # Summary
    ok = [r for r in results if r[2] is not None]
    err = [r for r in results if r[2] is None]
    print(f"Success: {len(ok)} / {len(results)} (errors: {len(err)})")
    if ok:
        sum_in = sum(r[1] for r in ok)
        sum_out = sum(r[2] for r in ok)
        print(f"Aggregate input={sum_in:,} B  tinypng={sum_out:,} B  ratio={sum_out/sum_in:.4f}")


if __name__ == "__main__":
    main()
