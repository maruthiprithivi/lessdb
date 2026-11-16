#!/usr/bin/env python3
"""Paint the baked Office bed out of office.png using existing floorboards.

Uses the frozen 1a099fc bed mask. Each hole pixel is copied from the same row
of clean floor to the right (so board seams stay aligned). The bottom skirt is
copied from the existing skirt. No new room style is invented.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
GATES = ROOT / "config" / "office_bed_gates.json"
DONOR_MAX = 428
SKIRT_Y = 493


def load_mask(path: Path) -> np.ndarray:
    arr = np.array(Image.open(path))
    if arr.ndim == 3:
        arr = arr[..., 0]
    return arr > 127


def is_floor(src: np.ndarray, mask: np.ndarray, y: int, x: int) -> bool:
    h, w = mask.shape
    if not (0 <= x < w and 0 <= y < h) or mask[y, x] or x >= DONOR_MAX:
        return False
    r, g, b = (int(v) for v in src[y, x])
    return r > 165 and (r - b) > 75 and 95 < g < 200 and (r - g) > 30


def is_skirt(src: np.ndarray, mask: np.ndarray, y: int, x: int) -> bool:
    h, w = mask.shape
    if not (0 <= x < w and 0 <= y < h) or mask[y, x]:
        return False
    r, g, b = (int(v) for v in src[y, x])
    return y >= SKIRT_Y and x >= 260 and r < 200


def paint(src: np.ndarray, mask: np.ndarray) -> np.ndarray:
    out = src.copy()
    h, w = mask.shape
    offsets = (200, 190, 180, 170, 160, 150, 140, 130, 120, 110, 100)
    for y in range(0, SKIRT_Y):
        xs = np.where(mask[y])[0]
        if xs.size == 0:
            continue
        for x in map(int, xs):
            sx = None
            for n in offsets:
                t = x + n
                if is_floor(src, mask, y, t):
                    sx = t
                    break
            if sx is None:
                for t in range(min(w - 1, max(x + 1, 260)), DONOR_MAX):
                    if is_floor(src, mask, y, t):
                        sx = t
                        break
            if sx is None and y > 0:
                out[y, x] = out[y - 1, x]
            elif sx is not None:
                out[y, x] = src[y, sx]
    for y in range(SKIRT_Y, h):
        xs = np.where(mask[y])[0]
        if xs.size == 0:
            continue
        sx = None
        for cand in (320, 340, 360, 300, 380, 400):
            if is_skirt(src, mask, y, cand) or is_floor(src, mask, y, cand):
                sx = cand
                break
        if sx is None:
            for x in map(int, xs):
                if y > 0:
                    out[y, x] = out[y - 1, x]
            continue
        for x in map(int, xs):
            out[y, x] = src[y, sx]
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--art", type=Path, default=ROOT / "assets/game/locations/office.png")
    ap.add_argument("--out", type=Path, default=None)
    ap.add_argument("--gates", type=Path, default=GATES)
    args = ap.parse_args()
    gates = json.loads(args.gates.read_text())
    mask = load_mask(ROOT / gates["mask"])
    src = np.array(Image.open(args.art).convert("RGB"))
    out = paint(src, mask)
    dest = args.out or args.art
    Image.fromarray(out).save(dest)
    print(f"painted {dest} ({int(mask.sum())} px from frozen mask)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
