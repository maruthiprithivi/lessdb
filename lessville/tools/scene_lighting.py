#!/usr/bin/env python3
"""Art-derived glass masks, lamp sprites, and lighting measurements.

The check and the game both start from town.png (and rendered pixels). This
module does not read TOWN_WINDOWS, LAMPS, or any other hand-authored light
layout. Thresholds that decide a pass/fail live in atmosphere_gates.json.
"""
from __future__ import annotations

from collections import deque
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
TOWN_ART = ROOT / "assets" / "game" / "locations" / "town.png"

SKY_BAND = (0, 0, 1536, 56)
FLAG_ROI = (748, 488, 820, 560)
# Café front + terrace, from the building footprint down to the curb
# (the published café-terrace crop). Used to catch an orphan glow that
# has no lamp sprite in the terrace itself.
CAFE_FRONT = (1180, 180, 1536, 520)
OFFICE_FACADE = (600, 140, 1060, 360)
BUILDING_Y_MAX = 420  # storeys; same cutoff the glass rectangle picker uses
# Canopy regions that shear with the wind (same boxes the Town view sways).
# Displaced leaves must not be scored as a lamp glow or window spill.
FOLIAGE_ROIS = [
    (0, 0, 170, 170),
    (360, 0, 590, 130),
    (1080, 0, 1230, 150),
    (1450, 0, 1536, 180),
    (700, 430, 790, 600),
    (900, 430, 990, 600),
    (0, 890, 1536, 1024),
    (200, 600, 340, 690),
    (1280, 360, 1360, 450),
]


def _luma_arr(rgb: np.ndarray) -> np.ndarray:
    return 0.2126 * rgb[..., 0] + 0.7152 * rgb[..., 1] + 0.0722 * rgb[..., 2]


def _sat_mean(r: float, g: float, b: float) -> float:
    mx = max(r, g, b)
    mn = min(r, g, b)
    return 0.0 if mx <= 0 else (mx - mn) / mx


def load_town_rgb(path: Path | None = None) -> np.ndarray:
    return np.asarray(Image.open(path or TOWN_ART).convert("RGB"), dtype=np.int16)


def _foliage(rgb: np.ndarray) -> np.ndarray:
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    return (g > r + 12) & (g > b + 4) & (g > 50)


def _dark_map(rgb: np.ndarray, max_l: float = 92) -> np.ndarray:
    """Frame-ish dark pixels, including brown wood (home window frames)."""
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    L = _luma_arr(rgb)
    foliage = _foliage(rgb)
    dark = (L <= max_l) & ~foliage
    # Brown wood frames can sit just above the luma cut (R high, B low, mid L).
    wood = (r > g + 20) & (g > b) & (r - b >= 30) & (L <= 120) & (L >= 40) & (r < 160) & ~foliage
    return dark | wood


def _thin_lines(dark: np.ndarray, min_run: int = 10, max_run: int = 240, max_thick: int = 9) -> np.ndarray:
    h, w = dark.shape
    frame = np.zeros((h, w), dtype=bool)
    for x in range(w):
        y = 0
        while y < h:
            if not dark[y, x]:
                y += 1
                continue
            y1 = y
            while y1 < h and dark[y1, x]:
                y1 += 1
            if min_run <= (y1 - y) <= max_run:
                mid = (y + y1) // 2
                thick = 1
                xl = xr = x
                while xl > 0 and dark[mid, xl - 1] and x - (xl - 1) < max_thick:
                    xl -= 1
                    thick += 1
                while xr + 1 < w and dark[mid, xr + 1] and (xr + 1 - x) < max_thick:
                    xr += 1
                    thick += 1
                if 1 <= thick <= max_thick:
                    frame[y:y1, x] = True
            y = y1
    for y in range(h):
        x = 0
        while x < w:
            if not dark[y, x]:
                x += 1
                continue
            x1 = x
            while x1 < w and dark[y, x1]:
                x1 += 1
            if min_run <= (x1 - x) <= max_run:
                mid = (x + x1) // 2
                thick = 1
                yt = yb = y
                while yt > 0 and dark[yt - 1, mid] and y - (yt - 1) < max_thick:
                    yt -= 1
                    thick += 1
                while yb + 1 < h and dark[yb + 1, mid] and (yb + 1 - y) < max_thick:
                    yb += 1
                    thick += 1
                if 1 <= thick <= max_thick:
                    frame[y, x:x1] = True
            x = x1
    return frame


def _dilate(mask: np.ndarray, r: int = 1) -> np.ndarray:
    m = mask
    for _ in range(r):
        n = m.copy()
        n[1:, :] |= m[:-1, :]
        n[:-1, :] |= m[1:, :]
        n[:, 1:] |= m[:, :-1]
        n[:, :-1] |= m[:, 1:]
        m = n
    return m


def _close(mask: np.ndarray, r: int = 1) -> np.ndarray:
    d = _dilate(mask, r)
    m = d
    for _ in range(r):
        n = m.copy()
        n[1:, :] &= m[:-1, :]
        n[:-1, :] &= m[1:, :]
        n[:, 1:] &= m[:, :-1]
        n[:, :-1] &= m[:, 1:]
        m = n
    return m | mask


def _vertical_segments(frame: np.ndarray, min_len: int = 16) -> list[tuple[int, int, int]]:
    """(x, y0, y1) inclusive vertical frame runs, 2-px gaps tolerated."""
    h, w = frame.shape
    segs: list[tuple[int, int, int]] = []
    for x in range(w):
        col = frame[:, x]
        y = 0
        while y < h:
            if not col[y]:
                y += 1
                continue
            y0 = y
            y1 = y
            gap = 0
            while y1 + 1 < h and (col[y1 + 1] or gap < 2):
                y1 += 1
                if col[y1]:
                    gap = 0
                else:
                    gap += 1
            if gap:
                y1 -= gap
            if y1 - y0 + 1 >= min_len:
                segs.append((x, y0, y1))
            y = y1 + 1
    return segs


def _coverage(frame: np.ndarray, x0: int, x1: int, y0: int, y1: int) -> float:
    """Fraction of frame pixels in the inclusive rectangle (thin band)."""
    h, w = frame.shape
    x0, x1 = max(0, x0), min(w - 1, x1)
    y0, y1 = max(0, y0), min(h - 1, y1)
    if x1 < x0 or y1 < y0:
        return 0.0
    sl = frame[y0 : y1 + 1, x0 : x1 + 1]
    return float(sl.mean()) if sl.size else 0.0


def _looks_like_glass(vals: np.ndarray, bw: int, bh: int) -> bool:
    if vals.size == 0:
        return False
    mr, mg, mb = (float(v) for v in vals.mean(axis=0))
    mL = 0.2126 * mr + 0.7152 * mg + 0.0722 * mb
    msat = _sat_mean(mr, mg, mb)
    warm = mr - mb
    # Painted cream (awning valance, door canopy, sign board).
    if mL >= 185 and msat < 0.32 and warm < 80:
        return False
    # Tall narrow lamp fixtures (sconces) are not panes.
    if bw <= 14 and bh >= 2.4 * max(bw, 1) and warm >= 40:
        return False
    # Very short wide bars (canopies, kerbs).
    if bh < 16 and bw > 3 * bh and warm < 80:
        return False
    if warm >= 18 and 70 <= mL <= 225:
        return True
    if mb > mr + 6 and 40 <= mL <= 200:
        return True
    if mb >= mr - 4 and msat <= 0.28 and 95 <= mL <= 195 and warm < 18:
        return True
    return False


def is_pane_pixel(r: np.ndarray, g: np.ndarray, b: np.ndarray) -> np.ndarray:
    """Glass colour: warm room-views or blue/pale panes, not cream, teal shutters, or doors."""
    L = 0.2126 * r + 0.7152 * g + 0.0722 * b
    warm = r - b
    sat = (np.maximum(np.maximum(r, g), b) - np.minimum(np.minimum(r, g), b)) / np.maximum(
        np.maximum(np.maximum(r, g), b), 1
    )
    teal = (g >= r + 8) & (np.abs(g - b) < 32) & (warm < 12) & (L >= 40) & (L <= 165)
    cream = (L >= 185) & (sat < 0.30) & (warm < 80)
    warm_room = (warm >= 16) & (L >= 75) & (L <= 225) & ~cream
    blue_pane = (b > r + 6) & (L >= 40) & (L <= 200) & (g < b + 30)
    pale = (b >= r - 4) & (sat <= 0.28) & (L >= 95) & (L <= 195) & (warm < 16)
    # Office / home facade stone: muted green-grey (G >= R), not a warm room view.
    # A leak of these pixels into the glass mask paints a yellow blotch on the wall.
    # Require G >= R - 2 so warm reddish interiors (café glass) stay pane seeds;
    # a looser |G-R|<=40 cut those seeds off the frame and dropped the café flood.
    wall = (
        (L >= 70) & (L <= 190) & (sat <= 0.36) & (warm <= 60)
        & ((g.astype(np.int16) - r) >= -2) & (np.abs(g.astype(np.int16) - r) <= 40)
    )
    return (warm_room | blue_pane | pale) & ~teal & ~wall


def _fill_rect(mask: np.ndarray, frame: np.ndarray, rgb: np.ndarray, x0: int, y0: int, x1: int, y1: int) -> None:
    h, w = mask.shape
    xa, xb = max(0, x0 + 1), min(w, x1)
    ya, yb = max(0, y0 + 1), min(h, y1)
    if xb <= xa or yb <= ya:
        return
    pane = is_pane_pixel(rgb[ya:yb, xa:xb, 0], rgb[ya:yb, xa:xb, 1], rgb[ya:yb, xa:xb, 2])
    mask[ya:yb, xa:xb] |= pane & ~frame[ya:yb, xa:xb]


def _facade_mask(rgb: np.ndarray) -> np.ndarray:
    """Cream stucco and green-grey office stone, dilated to cover the panes they surround."""
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    L = _luma_arr(rgb)
    warm = r - b
    sat = (np.maximum(np.maximum(r, g), b) - np.minimum(np.minimum(r, g), b)) / np.maximum(
        np.maximum(np.maximum(r, g), b), 1
    )
    foliage = _foliage(rgb)
    awning_red = (r > 160) & (r > g + 35) & (r > b + 35) & (g < 130)
    cream = (L >= 140) & (warm >= 12) & (warm <= 95) & (sat <= 0.42) & (r >= 150) & ~awning_red
    # Office stone is muted green-grey.
    stone = (L >= 80) & (L <= 175) & (sat <= 0.28) & (np.abs(g.astype(np.int16) - r) <= 25) & (g >= b - 5)
    facade = (cream | stone) & ~foliage
    return _dilate(facade, 18)



def _ring_is_facade(rgb: np.ndarray, x0: int, y0: int, x1: int, y1: int, pad: int = 8) -> bool:
    """True if the band around a rectangle looks like a building wall, not cobbles or grass."""
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    L = _luma_arr(rgb)
    h, w = L.shape
    xa, xb = max(0, x0 - pad), min(w, x1 + 1 + pad)
    ya, yb = max(0, y0 - pad), min(h, y1 + 1 + pad)
    ring = np.ones((yb - ya, xb - xa), dtype=bool)
    xi0, xi1 = x0 - xa, x1 + 1 - xa
    yi0, yi1 = y0 - ya, y1 + 1 - ya
    ring[max(0, yi0) : min(ring.shape[0], yi1), max(0, xi0) : min(ring.shape[1], xi1)] = False
    if not ring.any():
        return False
    rr, gg, bb = r[ya:yb, xa:xb][ring], g[ya:yb, xa:xb][ring], b[ya:yb, xa:xb][ring]
    LL = L[ya:yb, xa:xb][ring]
    warm = rr.astype(np.int16) - bb
    foliage = (gg > rr + 12) & (gg > bb + 4) & (gg > 50)
    cream = (LL >= 140) & (warm >= 12) & (warm <= 95) & (rr >= 150)
    stone = (LL >= 80) & (LL <= 175) & (np.abs(gg.astype(np.int16) - rr) <= 25)
    wood = (warm >= 20) & (LL >= 50) & (LL <= 160) & (rr > gg)  # window dado / frames
    return float((cream | stone | wood | foliage).mean()) >= 0.45  # foliage: shrubs under a sill are ok


def glass_mask(rgb: np.ndarray) -> np.ndarray:
    """Boolean HxW mask of window-pane pixels, derived from town.png."""
    h, w = rgb.shape[:2]
    dark = _dark_map(rgb)
    frame = _thin_lines(dark)
    frame_c = _close(frame, r=2)
    mask = np.zeros((h, w), dtype=bool)

    # 1) Axis-aligned rectangles whose four sides sit on frame lines.
    # Restrict to building storeys (y < 420) and thin adjacent columns so the
    # pairing stays cheap — ground-level false "windows" are also skipped.
    vsegs = [
        (x, y0, y1)
        for (x, y0, y1) in _vertical_segments(frame_c, min_len=16)
        if y0 < 420 and (y1 - y0) >= 18
    ]
    by_x: dict[int, list[tuple[int, int]]] = {}
    for x, y0, y1 in vsegs:
        # Keep the longest span in each 2-px column bucket.
        key = x - (x % 2)
        prev = by_x.get(key)
        if prev is None or (y1 - y0) > (prev[0][1] - prev[0][0]):
            by_x[key] = [(y0, y1)]
        elif (y1 - y0) >= 18:
            by_x[key].append((y0, y1))
    xs = sorted(by_x)
    seen_rects: set[tuple[int, int, int, int]] = set()
    for i, x0 in enumerate(xs):
        for x1 in xs[i + 1 :]:
            gap = x1 - x0
            if gap < 16:
                continue
            if gap > 250:
                break
            for y0a, y1a in by_x[x0]:
                for y0b, y1b in by_x[x1]:
                    y0 = max(y0a, y0b)
                    y1 = min(y1a, y1b)
                    if y1 - y0 < 14:
                        continue
                    top = _coverage(frame_c, x0, x1, y0 - 2, y0 + 2)
                    bot = _coverage(frame_c, x0, x1, y1 - 2, y1 + 2)
                    if top < 0.45 or bot < 0.45:
                        continue
                    key = (x0, y0, x1, y1)
                    if key in seen_rects:
                        continue
                    seen_rects.add(key)
                    xa, xb = x0 + 2, x1 - 1
                    ya, yb = y0 + 2, y1 - 1
                    if xb <= xa or yb <= ya:
                        continue
                    interior = rgb[ya:yb, xa:xb]
                    # skip the frame itself when judging colour
                    frag = interior[~frame_c[ya:yb, xa:xb]]
                    if frag.size == 0:
                        continue
                    vals = frag.reshape(-1, 3)
                    if not _looks_like_glass(vals, xb - xa, yb - ya):
                        continue
                    if not _ring_is_facade(rgb, x0, y0, x1, y1):
                        continue
                    _fill_rect(mask, frame_c, rgb, x0, y0, x1, y1)

    # 2) Colour-seeded flood on building façades, stopped by frames / awning / foliage.
    # Recovers panes whose frame has a small gap (café header, home mullions).
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    foliage = _foliage(rgb)
    awning_red = (r > 160) & (r > g + 35) & (r > b + 35) & (g < 130)
    cream = (L := _luma_arr(rgb)) >= 175
    cream = cream & ((r - b) >= 20) & ((r - b) <= 110) & (r >= 190) & (g >= 130)
    blocked = frame_c | foliage | awning_red | cream
    seed = is_pane_pixel(r, g, b) & ~blocked
    facade = _facade_mask(rgb)
    # Seeds may sit in the middle of a large pane (away from the wall colour).
    seen = np.zeros((h, w), dtype=bool)
    dirs = ((1, 0), (-1, 0), (0, 1), (0, -1))
    for y in range(h):
        for x in range(w):
            if not seed[y, x] or seen[y, x]:
                continue
            q = deque([(x, y)])
            seen[y, x] = True
            cells: list[tuple[int, int]] = []
            while q:
                cx, cy = q.popleft()
                cells.append((cx, cy))
                for dx, dy in dirs:
                    nx, ny = cx + dx, cy + dy
                    if 0 <= nx < w and 0 <= ny < h and not seen[ny, nx] and seed[ny, nx]:
                        seen[ny, nx] = True
                        q.append((nx, ny))
            n = len(cells)
            if n < 40 or n > 20000:
                continue
            xs_ = [p[0] for p in cells]
            ys_ = [p[1] for p in cells]
            x0, x1, y0, y1 = min(xs_), max(xs_), min(ys_), max(ys_)
            bw, bh = x1 - x0 + 1, y1 - y0 + 1
            fill = n / max(bw * bh, 1)
            if bw < 8 or bh < 8 or bw > 260 or bh > 150 or fill < 0.40:
                continue
            vals = rgb[[p[1] for p in cells], [p[0] for p in cells]]
            if not _looks_like_glass(vals, bw, bh):
                continue
            if not _ring_is_facade(rgb, x0, y0, x1, y1):
                continue
            # High frame contact on the perimeter — a window, not a warm wall patch.
            border = hit = 0
            for cx, cy in cells:
                for dx, dy in dirs:
                    nx, ny = cx + dx, cy + dy
                    if not (0 <= nx < w and 0 <= ny < h) or not seed[ny, nx]:
                        border += 1
                        if 0 <= nx < w and 0 <= ny < h and frame_c[ny, nx]:
                            hit += 1
                        break
            if border == 0 or hit / border < 0.20:
                continue
            for cx, cy in cells:
                mask[cy, cx] = True

        # Never treat frame pixels as glass (mullions stay dark).
    mask &= ~frame
    return mask


@dataclass
class Lamp:
    x: float
    y: float
    kind: str  # "street" | "lantern" | "sconce"
    x0: int
    y0: int
    x1: int
    y1: int
    n: int = 0


def _tight_bulb(r: int, g: int, b: int) -> bool:
    return r >= 200 and g >= 140 and b <= 100 and (r - b) >= 140 and (r - g) <= 90


def _components(pred: np.ndarray, min_n: int = 6, max_n: int = 800) -> list[dict]:
    h, w = pred.shape
    seen = np.zeros((h, w), dtype=bool)
    out: list[dict] = []
    dirs = ((1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1))
    for y in range(h):
        for x in range(w):
            if not pred[y, x] or seen[y, x]:
                continue
            q = deque([(x, y)])
            seen[y, x] = True
            cells: list[tuple[int, int]] = []
            while q:
                cx, cy = q.popleft()
                cells.append((cx, cy))
                for dx, dy in dirs:
                    nx, ny = cx + dx, cy + dy
                    if 0 <= nx < w and 0 <= ny < h and not seen[ny, nx] and pred[ny, nx]:
                        seen[ny, nx] = True
                        q.append((nx, ny))
            n = len(cells)
            if not (min_n <= n <= max_n):
                continue
            xs = [p[0] for p in cells]
            ys = [p[1] for p in cells]
            out.append(
                {
                    "n": n,
                    "cx": sum(xs) / n,
                    "cy": sum(ys) / n,
                    "x0": min(xs),
                    "y0": min(ys),
                    "x1": max(xs),
                    "y1": max(ys),
                    "w": max(xs) - min(xs) + 1,
                    "h": max(ys) - min(ys) + 1,
                    "cells": cells,
                }
            )
    return out


def _dark_frac(L: np.ndarray, x0: int, y0: int, x1: int, y1: int, cut: float = 70) -> float:
    h, w = L.shape
    x0, x1 = max(0, x0), min(w, x1)
    y0, y1 = max(0, y0), min(h, y1)
    if x1 <= x0 or y1 <= y0:
        return 0.0
    sl = L[y0:y1, x0:x1]
    return float((sl <= cut).mean()) if sl.size else 0.0


def _pole_len(L: np.ndarray, cx: int, y_start: int, max_len: int = 80) -> int:
    h, w = L.shape
    pole = 0
    for yy in range(y_start, min(h, y_start + max_len)):
        xa, xb = max(0, cx - 3), min(w, cx + 4)
        dark = int((L[yy, xa:xb] <= 60).sum())
        if dark >= 2:
            pole += 1
        elif pole >= 10:
            break
    return pole


def _pole_isolated(L: np.ndarray, cx: int, y_start: int, pole: int) -> bool:
    """A street-lamp pole is a narrow dark column with lighter ground on both sides."""
    if pole < 8:
        return False
    h, w = L.shape
    mid = y_start + pole // 2
    if mid >= h:
        return False
    xa, xb = max(0, cx - 3), min(w, cx + 4)
    dark = int((L[mid, xa:xb] <= 60).sum())
    if not (2 <= dark <= 5):
        return False
    left = L[mid, max(0, cx - 10) : max(0, cx - 4)]
    right = L[mid, min(w, cx + 5) : min(w, cx + 11)]
    # Door jambs and hedges are dark on one or both sides; a pole stands in the open.
    return (left.size == 0 or float(left.mean()) >= 70) and (right.size == 0 or float(right.mean()) >= 70)


def detect_lamps(rgb: np.ndarray, glass: np.ndarray | None = None) -> list[Lamp]:
    """Lamp sprites from the location art (street heads, hanging lanterns, sconces)."""
    h, w = rgb.shape[:2]
    r, g, b = rgb[..., 0].astype(np.int16), rgb[..., 1].astype(np.int16), rgb[..., 2].astype(np.int16)
    L = _luma_arr(rgb)
    warm = r - b
    lamps: list[Lamp] = []

    # --- Street-lamp heads: compact tight-yellow glass in a dark housing, on a pole.
    bulb = (r >= 200) & (g >= 140) & (b <= 100) & (warm >= 140) & ((r - g) <= 90)
    for c in _components(bulb, min_n=8, max_n=500):
        if not (8 <= c["w"] <= 26 and 8 <= c["h"] <= 28):
            continue
        if c["n"] < 80:
            continue
        aspect = c["w"] / max(c["h"], 1)
        if not (0.55 <= aspect <= 1.55):
            continue
        left = _dark_frac(L, c["x0"] - 4, c["y0"], c["x0"], c["y1"] + 1)
        right = _dark_frac(L, c["x1"] + 1, c["y0"], c["x1"] + 5, c["y1"] + 1)
        top = _dark_frac(L, c["x0"], c["y0"] - 4, c["x1"] + 1, c["y0"])
        housed = (left >= 0.20 and right >= 0.20) or (left + right + top >= 0.70)
        pole = _pole_len(L, int(round(c["cx"])), c["y1"] + 1)
        if housed and pole >= 28 and _pole_isolated(L, int(round(c['cx'])), c['y1'] + 1, pole):
            lamps.append(
                Lamp(c["cx"], c["cy"], "street", c["x0"], c["y0"], c["x1"], c["y1"], c["n"])
            )

    # --- Wall fixtures (hanging lanterns and sconces), from a yellow/pale-warm
    # core with a dark housing, on a light wall, no street-lamp pole.
    # Include almost-white warm cores (office sconces) and mid-yellow lanterns.
    fixture_core = (warm >= 40) & (L >= 100) & (L <= 250) & (r >= 170) & ((r - b) >= 40)
    for c in _components(fixture_core, min_n=16, max_n=280):
        bw, bh = c["w"], c["h"]
        # Hanging lantern glass is compact; sconces are tall and narrow.
        lantern_shape = 6 <= bw <= 18 and 6 <= bh <= 22 and 10 <= c["n"] <= 200
        sconce_shape = 4 <= bw <= 12 and 14 <= bh <= 40 and bh >= 1.5 * bw
        if not (lantern_shape or sconce_shape):
            continue
        left = _dark_frac(L, c["x0"] - 4, c["y0"], c["x0"], c["y1"] + 1, cut=80)
        right = _dark_frac(L, c["x1"] + 1, c["y0"], c["x1"] + 5, c["y1"] + 1, cut=80)
        top = _dark_frac(L, c["x0"], c["y0"] - 8, c["x1"] + 1, c["y0"], cut=70)
        # Lanterns have a dark cap above; sconces have dark frames on both sides.
        is_sconce = sconce_shape and left >= 0.28 and right >= 0.28
        # A hanging lantern has a real dark body above the glass, not a 2px shadow.
        cap = _dark_frac(L, c["x0"] - 1, c["y0"] - 22, c["x1"] + 2, c["y0"], cut=70)
        is_lantern = lantern_shape and cap >= 0.25 and top >= 0.18 and c["n"] >= 40
        if not (is_sconce or is_lantern):
            continue
        pole = _pole_len(L, int(round(c["cx"])), c["y1"] + 1, max_len=50)
        if pole >= 16:
            continue
        ring = _luma_mean_ring(L, c["x0"], c["y0"], c["x1"], c["y1"], pad=6)
        if ring < 100:
            continue
        # Drop candles and flowers: the ring should be a wall (cream / stone), not green or cobble.
        if not _ring_is_facade(rgb, c["x0"], c["y0"], c["x1"], c["y1"], pad=8):
            continue
        if glass is not None:
            core = glass[c["y0"] : c["y1"] + 1, c["x0"] : c["x1"] + 1]
            if core.size and float(core.mean()) > 0.50:
                continue  # interior pendant sitting in a pane
        if _too_close(lamps, c["cx"], c["cy"], 18):
            continue
        kind = "sconce" if is_sconce else "lantern"
        lamps.append(Lamp(c["cx"], c["cy"], kind, c["x0"], c["y0"], c["x1"], c["y1"], c["n"]))

    lamps = _filter_fixtures(rgb, lamps)
    lamps.sort(key=lambda lp: (lp.y, lp.x))
    return lamps


def _filter_fixtures(rgb: np.ndarray, lamps: list[Lamp]) -> list[Lamp]:
    """Keep street lamps; keep sconces only as a flanking pair; keep lanterns under an awning."""
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    awning_red = (r > 160) & (r > g + 35) & (r > b + 35) & (g < 130)
    kept: list[Lamp] = [lp for lp in lamps if lp.kind == "street"]
    sconces = [lp for lp in lamps if lp.kind == "sconce"]
    paired: set[int] = set()
    for i, a in enumerate(sconces):
        for j, b_ in enumerate(sconces):
            if j <= i:
                continue
            if abs(a.y - b_.y) <= 14 and 80 <= abs(a.x - b_.x) <= 170:
                paired.add(i)
                paired.add(j)
    kept.extend(sconces[i] for i in sorted(paired))
    for lp in lamps:
        if lp.kind != "lantern":
            continue
        # Search a wide band above — awnings are striped, and the lantern may
        # hang under a cream stripe next to a red one.
        x0 = max(0, int(lp.x) - 40)
        x1 = min(rgb.shape[1], int(lp.x) + 40)
        self_band = awning_red[int(lp.y0) : int(lp.y1) + 1, max(0, int(lp.x0) - 4) : min(rgb.shape[1], int(lp.x1) + 4)]
        if self_band.size and float(self_band.mean()) >= 0.08:
            continue
        y0 = max(0, int(lp.y0) - 80)
        y1 = max(y0 + 1, int(lp.y0) - 20)
        band = awning_red[y0:y1, x0:x1]
        if band.size and float(band.mean()) >= 0.06:
            kept.append(lp)
    return kept


def _luma_mean_ring(L: np.ndarray, x0: int, y0: int, x1: int, y1: int, pad: int = 6) -> float:
    h, w = L.shape
    xa, xb = max(0, x0 - pad), min(w, x1 + 1 + pad)
    ya, yb = max(0, y0 - pad), min(h, y1 + 1 + pad)
    ring = L[ya:yb, xa:xb].copy()
    # punch out the interior
    xi0, xi1 = x0 - xa, x1 + 1 - xa
    yi0, yi1 = y0 - ya, y1 + 1 - ya
    ring[max(0, yi0) : min(ring.shape[0], yi1), max(0, xi0) : min(ring.shape[1], xi1)] = -1
    vals = ring[ring >= 0]
    return float(vals.mean()) if vals.size else 0.0


def _too_close(lamps: list[Lamp], x: float, y: float, n: float) -> bool:
    for lp in lamps:
        if (lp.x - x) ** 2 + (lp.y - y) ** 2 <= n * n:
            return True
    return False


_SCENE_CACHE: dict | None = None


def load_scene(path: Path | None = None) -> dict:
    global _SCENE_CACHE
    key = str(path or TOWN_ART)
    if _SCENE_CACHE and _SCENE_CACHE.get('key') == key:
        return _SCENE_CACHE['data']
    rgb = load_town_rgb(path)
    glass = glass_mask(rgb)
    lamps = detect_lamps(rgb, glass)
    data = {"rgb": rgb, "glass": glass, "lamps": lamps}
    _SCENE_CACHE = {"key": key, "data": data}
    return data


# ---------------------------------------------------------------------------
# Measurements on a rendered Town frame
# ---------------------------------------------------------------------------

def _as_rgb(im: Image.Image | np.ndarray | Path) -> np.ndarray:
    if isinstance(im, Path) or isinstance(im, str):
        im = Image.open(im)
    if isinstance(im, Image.Image):
        return np.asarray(im.convert("RGB"), dtype=np.int16)
    return np.asarray(im, dtype=np.int16)


def band_stats(rgb: np.ndarray, box: tuple[int, int, int, int]) -> dict:
    x0, y0, x1, y1 = box
    h, w = rgb.shape[:2]
    x0, y0 = max(0, x0), max(0, y0)
    x1, y1 = min(w, x1), min(h, y1)
    sl = rgb[y0:y1, x0:x1]
    if sl.size == 0:
        return {"n": 0, "r": 0.0, "g": 0.0, "b": 0.0, "lum": 0.0, "warm": 0.0, "sat": 0.0}
    r = float(sl[..., 0].mean())
    g = float(sl[..., 1].mean())
    b = float(sl[..., 2].mean())
    return {
        "n": int(sl[..., 0].size),
        "r": r,
        "g": g,
        "b": b,
        "lum": 0.2126 * r + 0.7152 * g + 0.0722 * b,
        "warm": r - b,
        "sat": _sat_mean(r, g, b),
    }


def _night_grade(town: np.ndarray, darkness: float) -> np.ndarray:
    """Simple luminance grade of the source art (no assumed overlay hue)."""
    return np.clip(town.astype(np.float32) * (1.0 - float(darkness)), 0, 255)


def warm_overlay_mask(render: np.ndarray, town: np.ndarray, darkness: float,
                      dR: float = 10, dWarm: float = 8) -> np.ndarray:
    """Pixels in the render that are substantially warmer/brighter-red than a darkened source."""
    graded = _night_grade(town, darkness)
    d_r = render[..., 0].astype(np.float32) - graded[..., 0]
    d_g = render[..., 1].astype(np.float32) - graded[..., 1]
    d_b = render[..., 2].astype(np.float32) - graded[..., 2]
    d_warm = d_r - d_b
    return (d_r >= dR) & (d_warm >= dWarm) & (d_r >= d_g - 8)


def other_object_mask(render: np.ndarray, town: np.ndarray, darkness: float,
                      warm: np.ndarray) -> np.ndarray:
    """Characters, labels, the bus, the flag — differences that are not a yellow overlay.

    Only the non-warm part of a difference is marked. A sitter's skin can still
    look like a small warm blob; the extent gate (200 px / 8%) absorbs that
    residue so a real orphan halo (thousands of px) still fails.
    """
    graded = _night_grade(town, darkness)
    dl = np.abs(_luma_arr(render) - _luma_arr(graded))
    other = (dl >= 28) & ~warm
    return _dilate(other, 2)


def overlay_components(mask: np.ndarray, min_n: int = 20) -> list[dict]:
    return _components(mask, min_n=min_n, max_n=200000)



def _exclude_foliage(region: np.ndarray) -> np.ndarray:
    out = region
    h, w = out.shape
    for x0, y0, x1, y1 in FOLIAGE_ROIS:
        out[max(0, y0):min(h, y1), max(0, x0):min(w, x1)] = False
    return out


def glass_containment(render: np.ndarray, town: np.ndarray, glass: np.ndarray,
                      lamps: list[Lamp], darkness: float, glow_radius: float) -> dict:
    """Lit-window pixels must fall inside the art-derived glass mask.

    Glow around lamp sprites is excluded (that's check b/c, not window spill).
    """
    h, w = render.shape[:2]
    warm = warm_overlay_mask(render, town, darkness)
    other = other_object_mask(render, town, darkness, warm)
    # Sky, flag, labels-as-objects already in `other` or clipped here.
    sx0, sy0, sx1, sy1 = SKY_BAND
    region = np.ones((h, w), dtype=bool)
    region[sy0:sy1, sx0:sx1] = False
    fx0, fy0, fx1, fy1 = FLAG_ROI
    region[fy0:fy1, fx0:fx1] = False
    region = _exclude_foliage(region)

    lamp_disk = np.zeros((h, w), dtype=bool)
    yy, xx = np.ogrid[:h, :w]
    for lp in lamps:
        lamp_disk |= (xx - lp.x) ** 2 + (yy - lp.y) ** 2 <= glow_radius ** 2

    window_lit = warm & region & ~other & ~lamp_disk
    inside = window_lit & glass
    spill = window_lit & ~glass
    n_lit = int(window_lit.sum())
    n_spill = int(spill.sum())
    frac = (n_spill / n_lit) if n_lit else 0.0
    return {
        "n_lit": n_lit,
        "n_inside": int(inside.sum()),
        "n_spill": n_spill,
        "spill_frac": frac,
        "spill_mask": spill,
        "window_lit": window_lit,
        "warm": warm,
        "other": other,
        "lamp_disk": lamp_disk,
    }


def rectangular_fill_mask(warm: np.ndarray, min_side: int = 40, min_fill: float = 0.85) -> np.ndarray:
    """Axis-aligned, nearly-solid warm blobs — painted window rectangles, not lamp halos.

    A filled circle in its bbox has area fraction pi/4 ≈ 0.785; a draw_rect is ~1.0.
    """
    out = np.zeros(warm.shape, dtype=bool)
    for c in overlay_components(warm, min_n=max(80, min_side * min_side // 2)):
        fill = c["n"] / max(c["w"] * c["h"], 1)
        if c["w"] >= min_side and c["h"] >= min_side and fill >= min_fill:
            for x, y in c["cells"]:
                out[y, x] = True
    return out


def glow_mask(render: np.ndarray, town: np.ndarray, glass: np.ndarray,
              darkness: float, contain: dict | None = None) -> np.ndarray:
    if contain is None:
        warm = warm_overlay_mask(render, town, darkness)
        other = other_object_mask(render, town, darkness, warm)
    else:
        warm, other = contain["warm"], contain["other"]
    h, w = render.shape[:2]
    sx0, sy0, sx1, sy1 = SKY_BAND
    region = np.ones((h, w), dtype=bool)
    region[sy0:sy1, sx0:sx1] = False
    fx0, fy0, fx1, fy1 = FLAG_ROI
    region[fy0:fy1, fx0:fx1] = False
    # Glow is warm overlay that is not on glass and not a painted window rectangle.
    # (Hand-drawn window rects that spill off the pane would otherwise look like
    # a glow sitting on a door lantern or a wall sconce.)
    rects = rectangular_fill_mask(warm & region & ~other)
    return warm & region & ~other & ~glass & ~rects


def glow_on_lamp(glow: np.ndarray, lamps: list[Lamp], max_centroid_dist: float,
                 max_extent: float, roi: tuple[int, int, int, int] | None = None,
                 max_far_pixels: int = 200, max_far_frac: float = 0.08) -> dict:
    """Each glow centroid must sit within max_centroid_dist of a lamp sprite.

    Also: almost all glow pixels must lie within max_extent of a lamp (catches
    a huge halo whose centroid is on a lamp but whose edge is an orphan in a
    crop), and a local ROI (café terrace) glow centroid must also sit on a lamp.
    """
    # Wind-swayed canopy pixels are not lamp halos. Punch foliage here only —
    # lamps_lit still uses the un-punched glow, because south posts sit on the
    # tree line and their honest halos overlap it.
    glow = _exclude_foliage(glow.copy())
    h, w = glow.shape
    fails: list[str] = []
    comps = overlay_components(glow, min_n=25)
    lamp_xy = [(lp.x, lp.y) for lp in lamps]
    details = []

    def nearest(x, y):
        best = 1e9
        for lx, ly in lamp_xy:
            d = ((lx - x) ** 2 + (ly - y) ** 2) ** 0.5
            if d < best:
                best = d
        return best

    far_centroids = 0
    for c in comps:
        d = nearest(c["cx"], c["cy"])
        details.append({"cx": c["cx"], "cy": c["cy"], "n": c["n"], "dist": d})
        if d > max_centroid_dist and c["n"] >= 80:
            far_centroids += 1
            fails.append(f"glow centroid ({c['cx']:.1f},{c['cy']:.1f}) n={c['n']} is {d:.1f}px from nearest lamp")

    # Extent: glow pixels farther than max_extent from every lamp.
    if lamps and glow.any():
        yy, xx = np.where(glow)
        dist2 = np.full(yy.shape, 1e18)
        for lp in lamps:
            dist2 = np.minimum(dist2, (xx - lp.x) ** 2 + (yy - lp.y) ** 2)
        n_far = int((dist2 > max_extent * max_extent).sum())
        n_glow = int(glow.sum())
        far_frac = n_far / n_glow if n_glow else 0.0
    else:
        n_far, n_glow, far_frac = 0, int(glow.sum()), 0.0
    if n_far > max_far_pixels and far_frac > max_far_frac:
        fails.append(f"{n_far} glow pixels ({far_frac:.1%}) sit more than {max_extent}px from any lamp")

    roi_info = None
    if roi is not None:
        x0, y0, x1, y1 = roi
        sl = glow[y0:y1, x0:x1]
        n_roi = int(sl.sum())
        if n_roi >= 40:
            ys, xs = np.where(sl)
            cx = float(xs.mean() + x0)
            cy = float(ys.mean() + y0)
            d = nearest(cx, cy)
            roi_info = {"n": n_roi, "cx": cx, "cy": cy, "dist": d}
            if d > max_centroid_dist:
                fails.append(
                    f"ROI {roi} glow centroid ({cx:.1f},{cy:.1f}) n={n_roi} is {d:.1f}px from nearest lamp"
                )
        else:
            roi_info = {"n": n_roi, "cx": None, "cy": None, "dist": None}

    return {
        "n_glow": int(glow.sum()),
        "n_components": len(comps),
        "far_centroids": far_centroids,
        "n_far_extent": n_far,
        "far_frac": far_frac,
        "details": details,
        "roi": roi_info,
        "fails": fails,
    }


def lamps_lit(glow: np.ndarray, lamps: list[Lamp], radius: float, min_pixels: int,
              min_sectors: int = 10, n_sectors: int = 12, r_inner: float = 4.0) -> dict:
    """Every art-derived lamp sprite must have a circular halo after dark.

    A nearby painted window rectangle does not count: we require glow pixels in
    an annulus around the sprite, covering most angular sectors (a radial halo).
    """
    import math
    h, w = glow.shape
    yy, xx = np.ogrid[:h, :w] if lamps else (None, None)
    missing = []
    lit = []
    for lp in lamps:
        d2 = (xx - lp.x) ** 2 + (yy - lp.y) ** 2
        ann = (d2 >= r_inner * r_inner) & (d2 <= radius * radius)
        pix = glow & ann
        n = int(pix.sum())
        counts = np.zeros(n_sectors, dtype=np.int32)
        if n:
            ys, xs = np.where(pix)
            ang = np.arctan2(ys - lp.y, xs - lp.x)
            bins = ((ang + math.pi) / (2 * math.pi) * n_sectors).astype(np.int32) % n_sectors
            for b in bins:
                counts[b] += 1
        sectors = int((counts >= 3).sum())
        rec = {"x": lp.x, "y": lp.y, "kind": lp.kind, "n": n, "sectors": sectors}
        if n < min_pixels or sectors < min_sectors:
            missing.append(rec)
        else:
            lit.append(rec)
    return {
        "n_lamps": len(lamps),
        "n_lit": len(lit),
        "n_unlit": len(missing),
        "missing": missing,
        "lit": lit,
    }



def facade_wall_mask(town: np.ndarray) -> np.ndarray:
    """Stone / muted stucco of a building wall in the source art — not interior glass.

    The glass mask can leak a few wall pixels next to a pane; those pixels are
    stone-coloured in town.png (R≈G, modest warmth) while a lit room view is
    strongly yellow. Used so the glow-on-lamp check does not trust a leaky mask.
    """
    r = town[..., 0].astype(np.int16)
    g = town[..., 1].astype(np.int16)
    b = town[..., 2].astype(np.int16)
    L = _luma_arr(town)
    sat = (np.maximum(np.maximum(r, g), b) - np.minimum(np.minimum(r, g), b)) / np.maximum(
        np.maximum(np.maximum(r, g), b), 1
    )
    warmth = r - b
    # Green-grey stone only (G >= R). Cream stucco is R-heavy and is not the
    # office-facade leak the whole-frame check is meant to catch.
    return (L >= 70) & (L <= 190) & (sat <= 0.36) & (warmth <= 60) & ((g - r) >= -2) & (np.abs(g - r) <= 40)


def large_window_mask(glass: np.ndarray, min_n: int = 400, min_side: int = 16,
                      dilate: int = 4) -> np.ndarray:
    """Compact, large glass components — the actual window interiors, not wall slivers."""
    allowed = np.zeros(glass.shape, dtype=bool)
    for c in overlay_components(glass, min_n=min_n):
        if min(c["w"], c["h"]) >= min_side:
            for x, y in c["cells"]:
                allowed[y, x] = True
    return _dilate(allowed, dilate) if dilate else allowed


def whole_frame_glow(render: np.ndarray, town: np.ndarray, glass: np.ndarray,
                     lamps: list[Lamp], darkness: float,
                     contain: dict | None = None) -> np.ndarray:
    """Warm overlay on the town that is not inside a real window pane.

    Extends the terrace glow-on-lamp idea to the whole building band: a stray
    yellow patch on a facade wall (even one the glass mask wrongly claimed) is
    treated as a glow and must sit on a lamp sprite.
    """
    if contain is None:
        warm = warm_overlay_mask(render, town, darkness)
        other = other_object_mask(render, town, darkness, warm)
    else:
        warm, other = contain["warm"], contain["other"]
    h, w = render.shape[:2]
    region = np.ones((h, w), dtype=bool)
    sx0, sy0, sx1, sy1 = SKY_BAND
    region[sy0:sy1, sx0:sx1] = False
    fx0, fy0, fx1, fy1 = FLAG_ROI
    region[fy0:fy1, fx0:fx1] = False
    region = _exclude_foliage(region)
    region[BUILDING_Y_MAX:, :] = False
    allowed = large_window_mask(glass)
    wall = facade_wall_mask(town)
    return warm & region & ~other & ~allowed & wall


def lamps_off_daylight(render: np.ndarray, lamps: list[Lamp],
                       head_r: float = 6.0, warm_max: float = 28.0,
                       sat_max: float = 0.16) -> dict:
    """Every art-derived lamp must look unlit in a daylight Town frame.

    Checks the lamp-head sprite itself (the baked glass is yellow in town.png,
    so a daytime frame that leaves it untouched still reads as 'on'). A halo
    is not required for a fail — the yellow head is enough.
    """
    lit = []
    off = []
    for lp in lamps:
        h, w = render.shape[:2]
        yy, xx = np.ogrid[:h, :w]
        m = (xx - lp.x) ** 2 + (yy - lp.y) ** 2 <= head_r * head_r
        sl = render[m]
        if sl.size == 0:
            continue
        r = float(sl[:, 0].mean())
        g = float(sl[:, 1].mean())
        b = float(sl[:, 2].mean())
        warmth = r - b
        mx, mn = max(r, g, b), min(r, g, b)
        sat = 0.0 if mx <= 0 else (mx - mn) / mx
        lum = 0.2126 * r + 0.7152 * g + 0.0722 * b
        rec = {"x": lp.x, "y": lp.y, "kind": lp.kind, "warm": warmth, "sat": sat, "lum": lum,
               "r": r, "g": g, "b": b}
        if warmth >= warm_max and sat >= sat_max:
            lit.append(rec)
        else:
            off.append(rec)
    return {
        "n_lamps": len(lamps),
        "n_lit": len(lit),
        "n_off": len(off),
        "lit": lit,
        "off": off,
        "fails": [
            f"{m['kind']} at ({m['x']:.1f},{m['y']:.1f}) looks lit in daylight "
            f"(R-B={m['warm']:.1f}, sat={m['sat']:.2f}, lum={m['lum']:.1f})"
            for m in lit
        ],
    }


def dusk_warmth(render: np.ndarray) -> dict:
    """Absolute warmth (R-B) and saturation on the 18:30 *scene*, not only the sky cap."""
    h, w = render.shape[:2]
    body = band_stats(render, (0, SKY_BAND[3], w, h))
    office = band_stats(render, OFFICE_FACADE)
    terrace = band_stats(render, CAFE_FRONT)
    sky = band_stats(render, SKY_BAND)
    return {"body": body, "office": office, "terrace": terrace, "sky": sky}


def write_debug(out: Path, town: np.ndarray, glass: np.ndarray, lamps: list[Lamp]) -> None:
    out.mkdir(parents=True, exist_ok=True)
    vis = town.astype(np.uint8).copy()
    vis[glass] = (vis[glass] * 0.30 + np.array([0, 255, 80]) * 0.70).astype(np.uint8)
    im = Image.fromarray(vis)
    from PIL import ImageDraw as _ImageDraw
    d = _ImageDraw.Draw(im)
    cols = {"street": (255, 0, 0), "lantern": (0, 220, 255), "sconce": (255, 80, 255)}
    for lp in lamps:
        col = cols.get(lp.kind, (255, 255, 0))
        d.rectangle([lp.x0 - 2, lp.y0 - 2, lp.x1 + 2, lp.y1 + 2], outline=col, width=2)
        d.ellipse([lp.x - 2, lp.y - 2, lp.x + 2, lp.y + 2], fill=col)
        d.text((lp.x + 6, lp.y - 8), lp.kind, fill=col)
    im.save(out / "scene-overlay.png")
    im.crop((140, 140, 360, 360)).resize((440, 440), Image.Resampling.LANCZOS).save(out / "scene-home.png")
    im.crop((600, 140, 1060, 360)).resize((920, 440), Image.Resampling.LANCZOS).save(out / "scene-office.png")
    im.crop((1180, 180, 1536, 520)).resize((712, 680), Image.Resampling.LANCZOS).save(out / "scene-cafe.png")


if __name__ == "__main__":
    import argparse

    ap = argparse.ArgumentParser()
    ap.add_argument("--debug", type=Path, default=Path("/tmp/lessville-layout/step2a2/scene"))
    args = ap.parse_args()
    sc = load_scene()
    print(f"glass pixels: {int(sc['glass'].sum())}")
    print(f"lamps: {len(sc['lamps'])}")
    for lp in sc["lamps"]:
        print(f"  {lp.kind:8} ({lp.x:.1f},{lp.y:.1f}) box=({lp.x0},{lp.y0})-({lp.x1},{lp.y1}) n={lp.n}")
    write_debug(args.debug, sc["rgb"], sc["glass"], sc["lamps"])
    print("wrote", args.debug)
