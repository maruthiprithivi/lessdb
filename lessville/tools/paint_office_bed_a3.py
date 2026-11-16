#!/usr/bin/env python3
"""Rebuild the Office floor and left desk end (BL-0148 step 4 a3).

Starts from the frozen 1a099fc art. Keeps the frozen desk body, the
window/plant shadow and the stool shadow. Fills the bed patch with whole
clean plank rows copied from an untouched lighting-consistent span, each
donor column used at most once on a plank (no period tiling). Redraws
the skirting from a clean continuous span. The left desk end is a simple
side panel and leg in the desk's own left-face palette and outline; a
mirrored right pedestal was previewed and rejected (see the module doc
and the BL-0148 log) because the 3/4 view flipped the drawers and the
shading direction.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
GATES = ROOT / "config" / "office_bed_a3_gates.json"
BED_GATES = ROOT / "config" / "office_bed_gates.json"
SKIRT_Y = 493


def load_mask(path: Path) -> np.ndarray:
    arr = np.array(Image.open(path))
    if arr.ndim == 3:
        arr = arr[..., 0]
    return arr > 127


def luma(a: np.ndarray) -> np.ndarray:
    x = a.astype(np.float64)
    return 0.2126 * x[..., 0] + 0.7152 * x[..., 1] + 0.0722 * x[..., 2]


def is_floorish(img, y, x) -> bool:
    r, g, b = (int(v) for v in img[y, x])
    return r > 140 and (r - b) > 50 and g > 80


def runs_of(mask_1d) -> list[tuple[int, int]]:
    out, in_run, start = [], False, 0
    for i, v in enumerate(list(mask_1d) + [False]):
        if v and not in_run:
            in_run, start = True, i
        elif not v and in_run:
            in_run = False
            out.append((start, i - 1))
    return out


def near_palette(img, palette, tol):
    out = np.zeros(img.shape[:2], dtype=bool)
    pix = img.astype(np.int16)
    for c in palette:
        c = np.array(c, dtype=np.int16)
        out |= (np.max(np.abs(pix - c), axis=-1) <= tol)
    return out


def donor_columns_on_plank(floorish, teal, plant, desk, keep, L, row_med, y_mid, tol=18):
    """Clean unique x on one plank mid-row, x 300-427 only."""
    xs = []
    for x in range(300, 428):
        if not floorish[y_mid, x] or teal[y_mid, x] or plant[y_mid, x]:
            continue
        if desk[y_mid, x] or keep[y_mid, x]:
            continue
        if abs(float(L[y_mid, x]) - float(row_med[y_mid])) > tol:
            continue
        xs.append(x)
    return xs


def gather_donor_map(dest_y0, dest_y1, dest_width, bounds, floorish, teal, plant, desk, keep, L, row_med, used):
    """Map each dest column to a unique (src_y0, src_y1, src_x).

    Whole clean spans from x 300-427. The dest plank's own span is
    placed at the right of the hole. Shortfall is whole spans from the
    nearest other planks, copied with a row offset. A (src plank, src x)
    pair is used at most once in this paint (shared `used` set). Never
    furniture, rug, or radiator.
    """
    planks = []
    dest_mid = (dest_y0 + dest_y1) // 2
    for i in range(len(bounds) - 1):
        y0, y1 = int(bounds[i]), int(bounds[i + 1])
        if y1 <= y0:
            continue
        planks.append((y0, y1, (y0 + y1) // 2))

    def cols_for(y0, y1, mid, tol=18):
        out = []
        for x in donor_columns_on_plank(floorish, teal, plant, desk, keep, L, row_med, mid, tol):
            if (y0, x) in used:
                continue
            out.append((y0, y1, x))
        return out

    # Prefer tighter color match, but take a whole span from a plank at one tol.
    def best_span(y0, y1, mid):
        for tol in (14, 18, 24):
            c = cols_for(y0, y1, mid, tol)
            if c:
                return c
        return []

    dest_span = []
    other_spans = []
    for y0, y1, mid in sorted(planks, key=lambda p: abs(p[2] - dest_mid)):
        span = best_span(y0, y1, mid)
        if not span:
            continue
        if y0 == dest_y0:
            dest_span = span
        else:
            other_spans.append(span)

    mapping = [None] * dest_width
    # right-align dest span
    take = dest_span[:dest_width]
    n = len(take)
    for i, d in enumerate(take):
        mapping[dest_width - n + i] = d
        used.add((d[0], d[2]))
    # fill remaining left slots from other whole spans, left to right
    cursor = 0
    for span in other_spans:
        if cursor >= dest_width - n:
            break
        for d in span:
            if cursor >= dest_width - n:
                break
            if mapping[cursor] is not None:
                cursor += 1
                if cursor >= dest_width - n:
                    break
                continue
            mapping[cursor] = d
            used.add((d[0], d[2]))
            cursor += 1
    if any(m is None for m in mapping):
        return [m for m in mapping if m is not None]
    return mapping


def paint_left_end(out, ref, desk):
    """Simple side panel + leg, and a cleaned left cabinet face.

    Palette and outline come from the existing left face of this desk in
    1a099fc (the dark side, y 241-294, x 29-35). The panel sits under the
    already-finished desktop underside (y 251) and meets the floor at y 323.
    """
    L = luma(ref)
    # fill / outline sampled from the intact left face above the old bed cut
    face = ref[250:294, 29:35]
    fill = np.median(face.reshape(-1, 3), axis=0).astype(np.uint8)
    # slightly darker toward the left (shadow side)
    dark = np.median(ref[250:294, 28:31].reshape(-1, 3), axis=0).astype(np.uint8)
    outline = np.median(ref[248:294, 28][L[248:294, 28] < 40], axis=0) if (L[248:294, 28] < 40).any() else np.array([28, 26, 27], np.uint8)
    outline = outline.astype(np.uint8)
    foot = np.median(ref[291:295, 29:36].reshape(-1, 3), axis=0).astype(np.uint8)

    # End panel / leg under the left desktop.
    xL, xR = 8, 22
    yT, yB = 251, 323
    for y in range(yT, yB + 1):
        for x in range(xL, xR + 1):
            t = (x - xL) / max(1, (xR - xL))
            col = (dark.astype(np.float64) * (1 - t) + fill.astype(np.float64) * t).astype(np.uint8)
            out[y, x] = col
        out[y, xL] = outline
        out[y, xR] = outline
    # foot
    for y in range(yB - 2, yB + 1):
        for x in range(xL, xR + 1):
            out[y, x] = foot
        out[y, xL] = outline
        out[y, xR] = outline
    # top join just under the desktop outline
    for x in range(xL, xR + 1):
        out[yT, x] = outline

    # Finish the cabinet's own left face down to the floor and remove the
    # ragged sliver at the a2 cut (x 28-32, y 295-323).
    for y in range(295, 324):
        for x in range(28, 36):
            if x == 28 or (desk[y, x] and x <= 30 and y >= 300):
                # extend the dark left face
                src_y = 250 + ((y - 250) % 44)
                if src_y > 293:
                    src_y = 293
                if x <= 29:
                    out[y, x] = outline if x == 28 else dark
                elif desk[y, x]:
                    out[y, x] = fill
        if y >= 321:
            for x in range(28, 36):
                if x == 28 or desk[y, x]:
                    out[y, x] = outline if x == 28 else foot
    return out, (xL, yT, xR, yB)


def right_connected_floor(ref, fill, desk, keep, teal, plant, pal, floorish):
    """Keep a narrow collar of original 1a099fc boards at the patch's
    right edge, so plank seams stay continuous with the untouched floor.

    This is not a flood into the bed: only pixels within 12 columns of
    a non-fill wood pixel to their right, themselves tight floorish and
    not furniture.
    """
    h, w = fill.shape
    keep_floor = np.zeros((h, w), dtype=bool)
    collar = 12
    blocked = desk | teal | plant | pal
    for y in range(260, SKIRT_Y):
        for x in range(w - 1):
            if not fill[y, x] or blocked[y, x] or not floorish[y, x]:
                continue
            # must have a non-fill wood pixel within `collar` to the right
            ok = False
            for x2 in range(x + 1, min(w, x + 1 + collar)):
                if blocked[y, x2]:
                    break
                if not fill[y, x2] and floorish[y, x2]:
                    ok = True
                    break
                if not floorish[y, x2]:
                    break
            if ok:
                keep_floor[y, x] = True
    return keep_floor


def lighting_field(ref, patch, desk, floorish):
    lo = luma(ref)
    h, w = patch.shape
    ys = np.arange(h)[:, None]
    good = (~patch) & (~desk) & floorish & (ys >= 250) & (ys < SKIRT_Y)
    field = np.full((h, w), np.nan, dtype=np.float64)
    field[good] = lo[good]
    # row-wise interpolate
    for y in range(h):
        row = field[y]
        xs = np.where(np.isfinite(row))[0]
        if xs.size == 0:
            continue
        miss = np.where(~np.isfinite(row))[0]
        if miss.size:
            row[miss] = np.interp(miss, xs, row[xs])
            field[y] = row
    return field


def paint(ref, fill, desk, keep, seams, clean, palette=None, pal_tol=28):
    out = ref.copy()
    h, w = fill.shape
    r = ref[..., 0].astype(np.int32)
    g = ref[..., 1].astype(np.int32)
    b = ref[..., 2].astype(np.int32)
    floorish = (r > 140) & ((r - b) > 50) & (g > 80)
    teal = (g > r + 10) & (np.abs(g - b) < 20) & (g > 50) & (r < 100)
    plant = (g > r + 15) & (g > b + 5) & (r < 100)
    L = luma(ref)
    row_med = np.array([
        float(np.median(L[y, 300:428][floorish[y, 300:428]])) if floorish[y, 300:428].any() else 158.0
        for y in range(h)
    ])

    pal = near_palette(ref, palette, pal_tol) if palette else np.zeros(fill.shape, dtype=bool)
    keep_floor = np.zeros(fill.shape, dtype=bool)
    remaining = fill & ~keep & ~desk

    bounds = [s for s in seams if s <= SKIRT_Y]
    if bounds[-1] != SKIRT_Y:
        bounds.append(SKIRT_Y)

    for i in range(len(bounds) - 1):
        y0, y1 = int(bounds[i]), int(bounds[i + 1])
        if y1 <= y0:
            continue
        union = remaining[y0:y1].any(axis=0)
        for a, bcol in runs_of(union):
            W = bcol - a + 1
            donors = gather_donor_map(y0, y1, W, bounds, floorish, teal, plant, desk, keep, L, row_med, set())
            if len(donors) < W:
                raise SystemExit(f"not enough unique donors for plank y{y0}-{y1} need {W} got {len(donors)}")
            # Same-plank donors were collected first; put them on the
            # right so they meet any already-clean floor at the hole edge.
            for y in range(y0, y1):
                off = y - y0
                for i_x, x in enumerate(range(a, bcol + 1)):
                    if not remaining[y, x]:
                        continue
                    sy0, sy1, sx = donors[i_x]
                    src_h = sy1 - sy0
                    sy = sy0 + min(off, src_h - 1)
                    out[y, x] = ref[sy, sx]
            remaining[y0:y1, a:bcol + 1] = False

    # Skirting: one clean continuous span, unique columns, same mapping each row.
    # Force a 1-D column mask (a 2-D & here previously made np.where return
    # row indices, so only x 0-18 was painted).
    skirt_fill = fill[SKIRT_Y:].any(axis=0) & ~desk[SKIRT_Y:].any(axis=0) & ~keep[SKIRT_Y:].any(axis=0)
    xs = np.where(skirt_fill)[0]
    if xs.size:
        a, bcol = int(xs.min()), int(xs.max())
        # Clean mid-brown skirting only, before the plant at x 660.
        donors = []
        for x in range(320, 650):
            rr, gg, bb = (int(v) for v in ref[500, x])
            if plant[500, x]:
                continue
            if rr >= gg >= bb - 10 and 80 <= rr <= 180 and 50 <= luma(ref[500:501, x:x+1]).mean() <= 130:
                donors.append(x)
        if len(donors) < (bcol - a + 1):
            raise SystemExit(f"not enough unique skirt donors need {bcol-a+1} got {len(donors)}")
        donors = donors[: (bcol - a + 1)]
        for y in range(SKIRT_Y, h):
            for i_x, x in enumerate(range(a, bcol + 1)):
                if fill[y, x] and not desk[y, x] and not keep[y, x]:
                    out[y, x] = ref[y, donors[i_x]]

    # Last plank + skirt as one vertical strip, same donor x, same y, so
    # the board-to-skirting transition matches the real room (a1 autocorr
    # at lag 19-21 looks at this join). Right-align the unique clean
    # span x 300-427.
    last_y0 = max(s for s in bounds if s < SKIRT_Y)
    clean_xs = [x for x in range(300, 428) if floorish[last_y0 + 1, x] and not teal[last_y0 + 1, x] and not plant[last_y0 + 1, x] and not desk[last_y0 + 1, x]]
    if clean_xs and xs.size:
        a0, b0 = int(xs.min()), int(xs.max())
        n = min(len(clean_xs), b0 - a0 + 1)
        start = b0 - n + 1
        for i, dest_x in enumerate(range(start, b0 + 1)):
            src_x = clean_xs[i]
            for y in range(last_y0, h):
                if fill[y, dest_x] and not desk[y, dest_x] and not keep[y, dest_x]:
                    out[y, dest_x] = ref[y, src_x]

    # Kept shadows and original desk body stay from 1a099fc (canvas start).
    out[keep] = ref[keep]
    out[desk] = ref[desk]
    out, panel_box = paint_left_end(out, ref, desk)
    return out, panel_box


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
    keep = load_mask(ROOT / gates["keep_shadow_mask"])
    clean = load_mask(ROOT / gates["clean_floor_mask"])
    bed = load_mask(ROOT / bed_gates["mask"])
    src = np.array(Image.open(args.src).convert("RGB"))
    r, g, b = src[..., 0].astype(int), src[..., 1].astype(int), src[..., 2].astype(int)
    teal = (g > r + 10) & (np.abs(g - b) < 20) & (g > 50) & (r < 100)
    plant = (g > r + 15) & (g > b + 5) & (r < 100)
    leftover = np.zeros(desk.shape, dtype=bool)
    leftover[270:335, 0:32] = (teal | plant)[270:335, 0:32]
    fill = (patch | (bed & ~desk) | leftover) & ~keep & ~desk
    out, panel_box = paint(src, fill, desk, keep, list(gates["plank_seams"]), clean, palette=bed_gates["palette"], pal_tol=float(bed_gates["palette_tol"]))
    args.out.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(out).save(args.out)
    print(
        f"painted {args.out} (fill {int(fill.sum())} px, keep {int(keep.sum())} px, "
        f"desk {int(desk.sum())} px from 1a099fc, left panel {panel_box}; "
        f"method=side-panel-and-leg, not a mirrored right pedestal)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
