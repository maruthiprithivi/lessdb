#!/usr/bin/env python3
"""Validate Manus run 2 profile sit/type frames against existing Lessville sprites.

Reports (does not hide) off-palette pixel counts. Exit 0 only when dimensions,
anchor row, and file set are acceptable and off-palette pixels are within the
documented tolerance (or zero).
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from PIL import Image

CHAR_DIR = Path("/workspace/lessville-godot-src/work/assets/game/characters")
IDS = ["maya", "sam", "lin", "ama", "bo"]
KINDS = ["seat_e", "seat_w", "type_e", "type_w"]
REQUIRED = [f"{i}_{k}.png" for i in IDS for k in KINDS]
WIDTH, HEIGHT = 96, 112
HIP_ROW = 78
# Euclidean RGB distance above this counts as off-palette.
PALETTE_TOLERANCE = 16
# Fail the frame if more than this fraction of opaque pixels are off-palette.
# A handful of silhouette antialias pixels is expected; a new hue is not.
MAX_OFF_FRACTION = 0.02
MIN_HIP_BAND_PIXELS = 20  # opaque pixels in rows 70-86
MIN_FEET_BAND_PIXELS = 8  # opaque pixels in rows 96-111


def existing_colors(agent: str) -> set[tuple[int, int, int]]:
    colors: set[tuple[int, int, int]] = set()
    for p in CHAR_DIR.glob(f"{agent}_*.png"):
        # Skip the new profile frames if they already landed in CHAR_DIR
        stem = p.stem
        if stem.endswith("_seat_e") or stem.endswith("_seat_w") or stem.endswith("_type_e") or stem.endswith("_type_w"):
            continue
        im = Image.open(p).convert("RGBA")
        for r, g, b, a in im.getdata():
            if a >= 32:
                colors.add((r, g, b))
    return colors


def nearest_ok(rgb: tuple[int, int, int], colors: set[tuple[int, int, int]], tol: int) -> bool:
    if rgb in colors:
        return True
    r, g, b = rgb
    # Search a cube of Chebyshev radius tol; cheaper than a full scan.
    for dr in range(-tol, tol + 1):
        rr = r + dr
        if rr < 0 or rr > 255:
            continue
        for dg in range(-tol, tol + 1):
            gg = g + dg
            if gg < 0 or gg > 255:
                continue
            # remaining budget for db from Euclidean: skip exact euclidean for speed
            # Use Chebyshev as a conservative "close enough", then confirm Euclidean.
            for db in range(-tol, tol + 1):
                bb = b + db
                if bb < 0 or bb > 255:
                    continue
                cand = (rr, gg, bb)
                if cand in colors:
                    if (dr * dr + dg * dg + db * db) <= tol * tol:
                        return True
    return False


def inspect(path: Path, colors: set[tuple[int, int, int]]) -> dict:
    im = Image.open(path).convert("RGBA")
    w, h = im.size
    report = {
        "file": path.name,
        "size": [w, h],
        "mode": Image.open(path).mode,
        "ok_size": w == WIDTH and h == HEIGHT,
        "opaque": 0,
        "off_palette": 0,
        "hip_band": 0,
        "feet_band": 0,
        "issues": [],
    }
    if not report["ok_size"]:
        report["issues"].append(f"size {w}x{h} != {WIDTH}x{HEIGHT}")
        return report
    px = im.load()
    off = 0
    opaque = 0
    hip = 0
    feet = 0
    for y in range(h):
        for x in range(w):
            r, g, b, a = px[x, y]
            if a < 32:
                continue
            opaque += 1
            if 70 <= y <= 86:
                hip += 1
            if 96 <= y <= 111:
                feet += 1
            if not nearest_ok((r, g, b), colors, PALETTE_TOLERANCE):
                off += 1
    report["opaque"] = opaque
    report["off_palette"] = off
    report["hip_band"] = hip
    report["feet_band"] = feet
    if opaque < 200:
        report["issues"].append(f"too few opaque pixels ({opaque})")
    if hip < MIN_HIP_BAND_PIXELS:
        report["issues"].append(f"hip-row band (rows 70-86) has only {hip} opaque pixels; expected the hips on row {HIP_ROW}")
    if feet < MIN_FEET_BAND_PIXELS:
        report["issues"].append(f"feet band (rows 96-111) has only {feet} opaque pixels")
    frac = (off / opaque) if opaque else 1.0
    report["off_palette_fraction"] = round(frac, 4)
    if off and frac > MAX_OFF_FRACTION:
        report["issues"].append(
            f"{off} off-palette pixels ({frac:.2%} of opaque) exceed {MAX_OFF_FRACTION:.0%} at tolerance {PALETTE_TOLERANCE}"
        )
    return report


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("src", type=Path, help="directory containing the 20 profile frames")
    ap.add_argument("--json-out", type=Path, default=None)
    args = ap.parse_args()
    src: Path = args.src
    problems = []
    reports = []
    missing = [n for n in REQUIRED if not (src / n).exists()]
    extra = sorted({p.name for p in src.glob("*.png")} - set(REQUIRED))
    if missing:
        problems.append(f"missing files: {missing}")
    print(f"source={src} missing={len(missing)} extra_pngs={extra}")

    color_cache = {i: existing_colors(i) for i in IDS}
    for i in IDS:
        print(f"  existing palette {i}: {len(color_cache[i])} unique opaque RGB")

    for name in REQUIRED:
        path = src / name
        if not path.exists():
            reports.append({"file": name, "issues": ["missing"]})
            continue
        agent = name.split("_", 1)[0]
        rep = inspect(path, color_cache[agent])
        reports.append(rep)
        status = "OK" if not rep["issues"] else "FAIL"
        print(
            f"  {status} {name} size={rep.get('size')} opaque={rep.get('opaque')} "
            f"off_palette={rep.get('off_palette')} hip_band={rep.get('hip_band')} feet_band={rep.get('feet_band')}"
            + (f" issues={rep['issues']}" if rep["issues"] else "")
        )
        problems.extend(f"{name}: {x}" for x in rep["issues"])

    summary = {
        "source": str(src),
        "tolerance": PALETTE_TOLERANCE,
        "max_off_fraction": MAX_OFF_FRACTION,
        "missing": missing,
        "extra_pngs": extra,
        "frames": reports,
        "problems": problems,
    }
    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(json.dumps(summary, indent=2) + "\n")
        print(f"wrote {args.json_out}")
    if problems:
        print(f"FAIL: {len(problems)} problems")
        return 1
    print(f"PASS: {len(REQUIRED)} frames, size {WIDTH}x{HEIGHT}, hip row {HIP_ROW} band present, palette within tolerance {PALETTE_TOLERANCE}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
