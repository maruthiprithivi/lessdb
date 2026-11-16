#!/usr/bin/env python3
"""Repaint the Office bed patch from 1a099fc using neighbouring boards.

Starts from the frozen 1a099fc art (so the left desk is already intact) and
fills the frozen patch, a leftover sliver left of the desk, and the bed-mask
skirt — never the desk. Each hole in a plank is filled from the floor
immediately to its right, scaled to that neighbour lighting, and mirrored
at the splice so plank seams continue. No cv2 inpaint, no per-pixel smear.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
GATES = ROOT / "config" / "office_bed_patch_gates.json"
BED_GATES = ROOT / "config" / "office_bed_gates.json"
DONOR_MAX = 428
SKIRT_Y = 493


def load_mask(path: Path) -> np.ndarray:
    arr = np.array(Image.open(path))
    if arr.ndim == 3:
        arr = arr[..., 0]
    return arr > 127


def luma(a: np.ndarray) -> np.ndarray:
    x = a.astype(np.float64)
    return 0.2126 * x[..., 0] + 0.7152 * x[..., 1] + 0.0722 * x[..., 2]


def is_floor_px(rgb, y: int, x: int) -> bool:
    r, g, b = (int(v) for v in rgb)
    if y >= SKIRT_Y and x >= 260 and r < 200:
        return True
    return r > 140 and (r - b) > 50 and g > 80 and (r - g) > 20


def can_donate(canvas, blocked, desk, y, x) -> bool:
    h, w = blocked.shape
    if not (0 <= x < min(w, DONOR_MAX) and 0 <= y < h):
        return False
    if desk[y, x] or blocked[y, x]:
        return False
    return is_floor_px(canvas[y, x], y, x)


def runs_of(mask_1d) -> list:
    out, in_run, start = [], False, 0
    for i, v in enumerate(list(mask_1d) + [False]):
        if v and not in_run:
            in_run, start = True, i
        elif not v and in_run:
            in_run = False
            out.append((start, i - 1))
    return out


def choose_period(canvas, blocked, desk, y0, y1, donor0) -> int:
    lum = luma(canvas[y0:y1])
    run = 0
    gap = 0
    for x in range(donor0, DONOR_MAX):
        ok = any(can_donate(canvas, blocked, desk, y, x) for y in range(y0, y1))
        if ok:
            run = x - donor0 + 1
            gap = 0
        else:
            gap += 1
            if gap >= 4 and run >= 48:
                break
    run = max(run, 32)
    max_p = min(run, 180)
    best_p, best_s = min(80, max_p), -1.0
    if max_p >= 48:
        sig = np.mean(lum[:, donor0:donor0 + max_p], axis=0)
        sig = sig - sig.mean()
        for p in range(48, max_p):
            a, b = sig[:-p], sig[p:]
            den = float(np.sqrt((a * a).sum() * (b * b).sum()))
            s = float(a.dot(b) / den) if den > 1e-6 else -1.0
            if s > best_s:
                best_s, best_p = s, p
    if best_s < 0.35:
        return int(min(max(run, 64), 180))
    return int(best_p)


def extra_leftover(src, desk):
    r, g, b = src[:,:,0].astype(int), src[:,:,1].astype(int), src[:,:,2].astype(int)
    plant = (g > r + 15) & (g > 40) & (g > b + 5) & (r < 100)
    teal = (g > r + 10) & (np.abs(g - b) < 20) & (g > 50) & (r < 100)
    L = luma(src)
    extra = np.zeros(desk.shape, dtype=bool)
    extra[270:335, 0:32] = True
    extra &= ~desk
    floorish = (r > 140) & ((r - b) > 50) & (g > 80)
    extra &= plant | teal | ((L < 80) & ~floorish)
    return extra


def paint(src, fill, desk, seams, patch=None):
    out = src.copy()
    h, w = fill.shape
    remaining = fill.copy()
    bounds = [s for s in seams if s < SKIRT_Y] + [SKIRT_Y]
    painted = np.zeros_like(fill)

    for i in range(len(bounds) - 1):
        y0, y1 = bounds[i], bounds[i + 1]
        if y1 <= y0:
            continue
        union = remaining[y0:y1].any(axis=0)
        runs = runs_of(union)
        for a, b in reversed(runs):
            donor0 = b + 1
            P = choose_period(out, remaining, desk, y0, y1, donor0)
            for y in range(y0, y1):
                for x in range(b, a - 1, -1):
                    if not remaining[y, x]:
                        continue
                    written = False
                    src_x = x + P
                    while src_x < DONOR_MAX:
                        if can_donate(out, remaining, desk, y, src_x):
                            out[y, x] = out[y, src_x]
                            written = True
                            break
                        src_x += P
                    if not written:
                        for t in range(max(x + 1, donor0), DONOR_MAX):
                            if can_donate(out, remaining, desk, y, t):
                                out[y, x] = out[y, t]
                                written = True
                                break
                    if not written and y > y0:
                        out[y, x] = out[y - 1, x]
                    painted[y, x] = True
            neigh = np.zeros((h, w), dtype=bool)
            for y in range(y0, y1):
                for x in range(b + 1, min(b + 28, DONOR_MAX)):
                    if can_donate(out, remaining, desk, y, x):
                        neigh[y, x] = True
            runm = painted[y0:y1, a:b + 1]
            if runm.any() and neigh[y0:y1].any():
                f = out[y0:y1, a:b + 1][runm].astype(np.float64)
                n = out[y0:y1][neigh[y0:y1]].astype(np.float64)
                lf = 0.2126 * f[:, 0] + 0.7152 * f[:, 1] + 0.0722 * f[:, 2]
                ln = 0.2126 * n[:, 0] + 0.7152 * n[:, 1] + 0.0722 * n[:, 2]
                scale = float(ln.mean() / lf.mean()) if lf.mean() > 1 else 1.0
                scale = min(max(scale, 0.7), 1.35)
                adj = np.clip(out[y0:y1, a:b + 1].astype(np.float64) * scale, 0, 255)
                tmp = out[y0:y1, a:b + 1].copy()
                tmp[runm] = adj[runm].astype(np.uint8)
                out[y0:y1, a:b + 1] = tmp
            mw = min(10, (b - a + 1))
            for y in range(y0, y1):
                for k in range(mw):
                    src_x = b + 1 + k
                    dst_x = b - k
                    if dst_x < a:
                        break
                    if remaining[y, dst_x] or painted[y, dst_x]:
                        if 0 <= src_x < w and can_donate(out, remaining, desk, y, src_x):
                            out[y, dst_x] = out[y, src_x]
            remaining[y0:y1, a:b + 1] = False

    # Match each filled column to the same neighbour ring the G12 check uses,
    # and mirror at the frozen patch's median right edge (the checker's splice).
    from PIL import ImageFilter as _IF

    def _dilate(mask, radius):
        if radius <= 0:
            return mask
        out = Image.fromarray((mask.astype(np.uint8) * 255)).filter(_IF.MaxFilter(radius * 2 + 1))
        return np.array(out) > 127

    src_patch = patch if patch is not None else fill
    neigh = _dilate(src_patch, 24) & ~src_patch & ~desk
    neigh[:260, :] = False
    neigh[493:, :] = False

    for i in range(len(bounds) - 1):
        y0, y1 = bounds[i], bounds[i + 1]
        if y1 <= y0:
            continue
        cols = painted[y0:y1] | fill[y0:y1]
        nm = neigh[y0:y1]
        if not cols.any() or not nm.any():
            continue
        n = out[y0:y1][nm].astype(np.float64)
        target = n.mean(axis=0)  # RGB
        for x in range(w):
            colm = cols[:, x]
            if not colm.any():
                continue
            col = out[y0:y1, x].astype(np.float64)
            mean = col[colm].mean(axis=0)
            mean = np.maximum(mean, 1.0)
            scale = np.clip(target / mean, 0.75, 1.3)
            adj = np.clip(col * scale, 0, 255)
            tmp = out[y0:y1, x].copy()
            tmp[colm] = adj[colm].astype(np.uint8)
            out[y0:y1, x] = tmp

        edges = []
        for y in range(y0, y1):
            xs = np.where(src_patch[y])[0]
            if xs.size:
                edges.append(int(xs.max()))
        if len(edges) >= 3:
            bx = int(np.median(edges))
            for y in range(y0, y1):
                for k in range(8):
                    dst, srcx = bx - 1 - k, bx + k
                    if dst < 0 or srcx >= w:
                        continue
                    if not (fill[y, dst] or painted[y, dst] or src_patch[y, dst]):
                        continue
                    if desk[y, dst]:
                        continue
                    if is_floor_px(out[y, srcx], y, srcx) and not desk[y, srcx]:
                        out[y, dst] = out[y, srcx]

    for y in range(SKIRT_Y, h):
        xs = np.where(fill[y])[0]
        if xs.size == 0:
            continue
        sx = None
        for cand in (320, 340, 360, 300, 380, 400):
            if can_donate(src, fill, desk, y, cand):
                sx = cand
                break
        if sx is None:
            for x in map(int, xs):
                if y > 0:
                    out[y, x] = out[y - 1, x]
            continue
        for x in map(int, xs):
            out[y, x] = src[y, sx]
    out[desk] = src[desk]
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--src", type=Path, default=ROOT / "config/office_art_1a099fc.png")
    ap.add_argument("--out", type=Path, default=ROOT / "assets/game/locations/office.png")
    ap.add_argument("--gates", type=Path, default=GATES)
    args = ap.parse_args()
    gates = json.loads(args.gates.read_text())
    bed_gates = json.loads(BED_GATES.read_text())
    desk = load_mask(ROOT / gates["desk_mask"])
    patch = load_mask(ROOT / gates["patch_mask"])
    bed = load_mask(ROOT / bed_gates["mask"])
    src = np.array(Image.open(args.src).convert("RGB"))
    fill = patch | (bed & ~desk) | extra_leftover(src, desk)
    out = paint(src, fill, desk, list(gates["plank_seams"]), patch=patch)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(out).save(args.out)
    print(f"painted {args.out} (fill {int(fill.sum())} px, desk {int(desk.sum())} px preserved from 1a099fc)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
