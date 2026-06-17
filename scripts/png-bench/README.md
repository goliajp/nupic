# scripts/png-bench

Research-track tools for benchmarking nupic against TinyPNG across the
corpus-500 cohort. **Not** shipped with the binary — these are reproducibility
aids for the Cycle 102+ three-axis gate methodology.

## Files

| script | what it does | requires |
|---|---|---|
| `tinypng_ingest.py` | Batch-compress `corpus-500/` via TinyPNG HTTPS API → `tinypng-corpus-500/` + `corpus-500-tinypng-results.tsv`. Resumable, monthly quota guard at 500 calls. | `TINIFY_KEY` env var |
| `nupic_corpus_compress.py` | Run `nupic compress` (default Quality::Auto) on every fixture → `nupic-corpus-500/` + `corpus-500-three-axis.tsv` (size ratios). | `nupic 1.2.8+` binary |
| `corpus_dssim.py` | Compute DSSIM(ref, nupic) + DSSIM(ref, tinypng) for every fixture → `corpus-500-dssim.tsv`. **Primary** quality metric. | `nupic` binary |
| `corpus_ssim.py` | Same but SSIMULACRA2 (cross-check / cross-validate; alpha-floor unreliable). | `nupic` binary |
| `analyze_dssim_verdict.py` | Cross-tab `three-axis.tsv` × `dssim.tsv` × `ssim.tsv` → per-class verdict table, Pile A (RD opportunity) + worst-quality lists. | python only |

## Standard pipeline order

```bash
# 1. Get fresh TinyPNG baseline (uses quota, only run if corpus changed)
TINIFY_KEY=<key> python3 scripts/png-bench/tinypng_ingest.py

# 2. Run current nupic over corpus
python3 scripts/png-bench/nupic_corpus_compress.py

# 3. Quality head-to-head (primary)
python3 scripts/png-bench/corpus_dssim.py

# 4. Quality cross-check (optional, slower)
python3 scripts/png-bench/corpus_ssim.py

# 5. Verdict + per-class breakdown
python3 scripts/png-bench/analyze_dssim_verdict.py
```

All scripts write to `assets/png-bench/`. The three large directories
(`corpus-500/` inputs, `tinypng-corpus-500/` outputs, `nupic-corpus-500/`
outputs) are gitignored — `.tsv` baseline tables are committed.

## Cycle 106-pre baseline state (2026-06-18, v1.2.8)

| metric | value |
|---|---|
| corpus-500 two-axis PASS (size ≤ 0.80× ∧ DSSIM ≤ tiny) | **106 / 506 = 20.9%** |
| nupic DSSIM ≤ tinypng DSSIM | 415 / 506 = 82% |
| cohort total bytes (nupic / tinypng) | 1.012× |
| Pile A (RD opportunity, n=15 top) | see `corpus-500-pile-a.tsv` |
