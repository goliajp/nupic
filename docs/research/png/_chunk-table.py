#!/usr/bin/env python3
"""
Dump PNG chunk structure across the assets/png-bench fixtures.

Used by docs/research/png/00-attack-surface.md to ground its analysis
in chunk-level facts (color_type, palette size, tRNS length, IDAT total).
Standalone — no dependencies beyond the stdlib.

`inputs/` is committed. `tinypng-web/` and `current-nupic-0.4/` are
**not** committed (TinyPNG TOS + scratch); regenerate them with:
    1. drop assets/png-bench/inputs/*.png on https://tinypng.com,
       unzip to assets/png-bench/tinypng-web/
    2. for f in assets/png-bench/inputs/*.png; do
           nupic compress "$f" -o "assets/png-bench/current-nupic-0.4/$(basename "$f")"
       done

Run:
    python3 docs/research/png/_chunk-table.py
"""
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
BENCH = ROOT / "assets" / "png-bench"
SRCS = ["inputs", "current-nupic-0.4", "tinypng-web"]
CT_NAME = {0: "L", 2: "RGB", 3: "idx", 4: "LA", 6: "RGBA"}


def parse(path: Path):
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"{path}: not a PNG")
    color_type = data[25]
    out = {}
    i = 8
    while i < len(data):
        ln = struct.unpack(">I", data[i : i + 4])[0]
        ty = data[i + 4 : i + 8].decode("ascii", "replace")
        out.setdefault(ty, []).append(ln)
        i += 12 + ln
    palette_n = (out.get("PLTE", [0])[0] // 3) if "PLTE" in out else 0
    trns_n = out["tRNS"][0] if "tRNS" in out else 0
    idat = sum(out.get("IDAT", []))
    return color_type, palette_n, trns_n, idat


def main() -> int:
    missing = [s for s in SRCS if not (BENCH / s).is_dir()]
    if missing:
        print(
            f"warning: missing source dirs: {missing} — see header comment "
            "for regeneration steps. proceeding with the rest.",
            file=sys.stderr,
        )

    print(f"{'file':<32} {'src':<20} {'ct':>4} {'palette':>8} {'tRNS':>5} {'IDAT':>10}")
    print("-" * 80)
    for n in range(1, 8):
        cand = sorted((BENCH / "inputs").glob(f"0{n}-*.png"))
        if not cand:
            continue
        base = cand[0].name
        for src in SRCS:
            p = BENCH / src / base
            if not p.exists():
                continue
            ct, pal, trns, idat = parse(p)
            print(
                f"{base:<32} {src:<20} {CT_NAME.get(ct, '?'):>4} "
                f"{pal:>8} {trns:>5} {idat:>10}"
            )
        print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
