#!/usr/bin/env python3
"""G12 local Office bed-patch check (BL-0148 step 4 a2).

Asserts the properties a reviewer inspects on the paint-out:
  (a) the patch's luminance and hue match neighbouring lit boards, per plank row
      (and in a sliding window, so a local blotch or wedge cannot average out)
  (b) no unexplained hard-edged dark region (a connected dark component with a
      sharp edge that is not explained by the 1a099fc unbedded-floor lighting)
  (c) plank seam continuity across the patch boundary (seam rows/columns line up)
  (d) the left-desk silhouette matches 1a099fc above the bed line
  (e) no nightstand-green pixels remain in the leftover ROI

Masks, 1a099fc reference art, plank seam rows and the green rule are frozen
from 1a099fc. They are never redrawn from the output under test.
"""
from __future__ import annotations

import argparse
import json
import sys
from collections import deque
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parents[1]
GATES_PATH = ROOT / "config" / "office_bed_patch_gates.json"


def load_gates(path: Path) -> dict:
    return json.loads(path.read_text())


def load_mask(path: Path) -> np.ndarray:
    im = Image.open(path)
    if im.size != (768, 512):
        raise SystemExit(f"mask must be 768x512, got {im.size}")
    arr = np.array(im)
    if arr.ndim == 3:
        arr = arr[..., 0]
    return arr > 127


def as_rgb(path: Path) -> np.ndarray:
    img = np.array(Image.open(path).convert("RGB"))
    if img.shape[0] != 512 or img.shape[1] != 768:
        raise SystemExit(f"expected 768x512, got {img.shape[1]}x{img.shape[0]} for {path}")
    return img


def luma(a: np.ndarray) -> np.ndarray:
    x = a.astype(np.float64)
    return 0.2126 * x[..., 0] + 0.7152 * x[..., 1] + 0.0722 * x[..., 2]


def hue_rb(img: np.ndarray) -> np.ndarray:
    return img[..., 0].astype(np.float64) - img[..., 2].astype(np.float64)


def morph(mask: np.ndarray, radius: int, op: str) -> np.ndarray:
    if radius <= 0:
        return mask
    fn = ImageFilter.MinFilter if op == "erode" else ImageFilter.MaxFilter
    out = Image.fromarray((mask.astype(np.uint8) * 255)).filter(fn(radius * 2 + 1))
    return np.array(out) > 127


def fill_rows_nan(field: np.ndarray) -> np.ndarray:
    out = field.copy()
    h = out.shape[0]
    for y in range(h):
        row = out[y]
        xs = np.where(np.isfinite(row))[0]
        if xs.size < 2:
            continue
        missing = np.where(~np.isfinite(row))[0]
        if missing.size:
            out[y, missing] = np.interp(missing.astype(np.float64), xs.astype(np.float64), row[xs])
    return out


def label_cc(mask: np.ndarray) -> tuple[np.ndarray, int]:
    h, w = mask.shape
    lab = np.zeros(mask.shape, dtype=np.int32)
    n = 0
    ys, xs = np.where(mask)
    for y, x in zip(ys, xs):
        if lab[y, x]:
            continue
        n += 1
        q: deque[tuple[int, int]] = deque([(int(x), int(y))])
        lab[y, x] = n
        while q:
            cx, cy = q.popleft()
            for dx, dy in ((-1, 0), (1, 0), (0, -1), (0, 1)):
                nx, ny = cx + dx, cy + dy
                if 0 <= nx < w and 0 <= ny < h and mask[ny, nx] and lab[ny, nx] == 0:
                    lab[ny, nx] = n
                    q.append((nx, ny))
    return lab, n


def is_nightstand_green(img: np.ndarray) -> np.ndarray:
    r = img[..., 0].astype(np.int32)
    g = img[..., 1].astype(np.int32)
    b = img[..., 2].astype(np.int32)
    plant = (g > r + 15) & (g > 40) & (g > b + 5) & (r < 100)
    teal = (g > r + 10) & (np.abs(g - b) < 20) & (g > 50) & (r < 100)
    return plant | teal


def lighting_field(ref: np.ndarray, patch: np.ndarray, desk: np.ndarray) -> np.ndarray:
    """Low-frequency luma on 1a099fc unbedded floor, interpolated across the patch."""
    lo = luma(ref)
    r = ref[..., 0].astype(np.int32)
    g = ref[..., 1].astype(np.int32)
    b = ref[..., 2].astype(np.int32)
    is_floor = (r > 140) & ((r - b) > 50) & (g > 80)
    h, w = patch.shape
    ys = np.arange(h)[:, None]
    good = (~patch) & (~desk) & is_floor & (ys >= 250) & (ys < 493)
    field = np.full((h, w), np.nan, dtype=np.float64)
    field[good] = lo[good]
    return fill_rows_nan(field)


def xcorr_lag0_and_best(a: np.ndarray, b: np.ndarray, maxlag: int) -> tuple[float, float, int]:
    a = a - a.mean()
    b = b - b.mean()
    n = min(len(a), len(b))
    a, b = a[:n], b[:n]
    if n < 8:
        return 0.0, 0.0, 0
    best_s, best_lag, lag0 = -2.0, 0, 0.0
    for lag in range(-maxlag, maxlag + 1):
        if lag < 0:
            aa, bb = a[-lag:], b[: n + lag]
        elif lag > 0:
            aa, bb = a[: n - lag], b[lag:]
        else:
            aa, bb = a, b
        den = float(np.sqrt((aa * aa).sum() * (bb * bb).sum()))
        s = float(aa.dot(bb) / den) if den > 1e-6 else 0.0
        if lag == 0:
            lag0 = s
        if s > best_s:
            best_s, best_lag = s, lag
    return lag0, best_s, best_lag


def check_image(path: Path, gates: dict, ref: np.ndarray, desk: np.ndarray, patch: np.ndarray, label: str) -> list[str]:
    fails: list[str] = []
    img = as_rgb(path)
    lc = luma(img)
    h, w = patch.shape
    seams: list[int] = list(gates["plank_seams"])
    left_end = np.zeros(desk.shape, dtype=bool)
    if gates.get("left_end_support_roi"):
        left_end = load_mask(ROOT / gates["left_end_support_roi"])
    neigh = morph(patch, int(gates["neighbor_dilate_px"]), "dilate") & ~patch & ~desk
    neigh[:260, :] = False
    neigh[493:, :] = False
    win = int(gates["window_px"])
    max_dL = float(gates["max_row_luma_delta"])
    max_dH = float(gates["max_row_hue_delta"])

    print(f"  {label} (a) per-plank-row luma/hue vs neighbouring boards:")
    for y0, y1 in zip((s + 1 for s in seams[:-1]), seams[1:]):
        pm = patch[y0:y1] & ~left_end[y0:y1]
        nm = neigh[y0:y1]
        if int(pm.sum()) < 30 or int(nm.sum()) < 30:
            print(f"    y{y0}-{y1}: skip (nP={int(pm.sum())} nN={int(nm.sum())})")
            continue
        row_img = img[y0:y1]
        row_l = lc[y0:y1]
        row_h = hue_rb(row_img)
        nL = float(row_l[nm].mean())
        nH = float(row_h[nm].mean())
        # sliding window so a local blotch cannot average out
        worst_dL, worst_dH, worst_x = 0.0, 0.0, -1
        xs = np.where(pm.any(0))[0]
        if xs.size == 0:
            continue
        x_start, x_end = int(xs.min()), int(xs.max()) + 1
        for x0 in range(x_start, max(x_start + 1, x_end - win + 1), 8):
            x1 = min(x_end, x0 + win)
            wm = pm[:, x0:x1]
            if int(wm.sum()) < 20:
                continue
            dL = float(row_l[:, x0:x1][wm].mean() - nL)
            dH = float(row_h[:, x0:x1][wm].mean() - nH)
            if abs(dL) > abs(worst_dL):
                worst_dL, worst_x = dL, x0
            if abs(dH) > abs(worst_dH):
                worst_dH = dH
        print(
            f"    y{y0}-{y1}: worst-window dL={worst_dL:.2f} (max {max_dL}) "
            f"dH={worst_dH:.2f} (max {max_dH}) at x~{worst_x} nP={int(pm.sum())} nN={int(nm.sum())}"
        )
        if abs(worst_dL) > max_dL:
            fails.append(
                f"{label}: plank y{y0}-{y1} window luma differs from neighbouring boards "
                f"by {worst_dL:.1f} (max {max_dL})"
            )
        if abs(worst_dH) > max_dH:
            fails.append(
                f"{label}: plank y{y0}-{y1} window hue (R−B) differs from neighbouring boards "
                f"by {worst_dH:.1f} (max {max_dH})"
            )

    # Left-end rebuild ROI is frozen from 1a099fc geometry; a3 owns completeness there.
    left_end = np.zeros(desk.shape, dtype=bool)
    if gates.get("left_end_support_roi"):
        left_end = load_mask(ROOT / gates["left_end_support_roi"])

    # (b) unexplained hard-edged dark components vs 1a099fc unbedded lighting
    Lref = lighting_field(ref, patch, desk)
    margin = float(gates["dark_luma_margin"])
    unexp = patch & np.isfinite(Lref) & (lc < (Lref - margin)) & ~left_end
    for y in seams:
        if 0 <= y < h:
            unexp[y, :] = False
    lab, nlab = label_cc(unexp)
    gy, gx = np.gradient(lc)
    gmag = np.hypot(gx, gy)
    min_area = int(gates["min_dark_component_px"])
    min_h = int(gates["min_dark_height"])
    min_grad = float(gates["min_edge_grad"])
    max_dark = int(gates["max_sharp_dark_px"])
    print(f"  {label} (b) unexplained dark (luma < Lref-{margin:.0f}): {int(unexp.sum())} px, {nlab} components")
    for i in range(1, nlab + 1):
        comp = lab == i
        area = int(comp.sum())
        if area < min_area:
            continue
        ys, xs = np.where(comp)
        height = int(ys.max() - ys.min()) + 1
        edge = morph(comp, 1, "dilate") & ~comp
        sharp = float(gmag[edge].mean()) if edge.any() else 0.0
        print(
            f"    comp {i}: n={area} bbox x{int(xs.min())}-{int(xs.max())} "
            f"y{int(ys.min())}-{int(ys.max())} h={height} edge_grad={sharp:.2f}"
        )
        if height >= min_h and sharp >= min_grad and area > max_dark:
            fails.append(
                f"{label}: unexplained hard-edged dark region {area} px "
                f"at x{int(xs.min())}-{int(xs.max())} y{int(ys.min())}-{int(ys.max())} "
                f"(max {max_dark}, edge_grad {sharp:.1f})"
            )

    # (c) plank seam continuity across the right patch boundary.
    # Horizontal seams: the y-luma profile just inside the patch must match
    # the profile just outside (dark board-gap rows continue at the same y).
    # Vertical/cut: the luma jump at the boundary must not exceed the typical
    # adjacent difference on the neighbouring boards (no hard stair-step cut).
    min_ycorr = float(gates["min_seam_ycorr"])
    jump_mult = float(gates["seam_jump_mult"])
    jump_add = float(gates["seam_jump_add"])
    max_fail_rows = int(gates["max_failing_seam_rows"])
    seam_fails = 0
    print(f"  {label} (c) plank seam continuity at the patch boundary:")
    for y0, y1 in zip((s + 1 for s in seams[:-1]), seams[1:]):
        edges = []
        for y in range(y0, y1):
            xs = np.where(patch[y])[0]
            if xs.size:
                edges.append(int(xs.max()))
        if len(edges) < 3:
            continue
        bx = int(np.median(edges))
        if bx < 8 or bx + 12 >= w:
            continue
        col_l = np.mean(lc[y0:y1, bx - 8:bx], axis=1)
        col_r = np.mean(lc[y0:y1, bx:bx + 8], axis=1)
        a = col_l - col_l.mean()
        b = col_r - col_r.mean()
        den = float(np.sqrt((a * a).sum() * (b * b).sum()))
        ycorr = float(a.dot(b) / den) if den > 1e-6 else 0.0
        jump = float(np.mean(np.abs(lc[y0:y1, bx - 1].astype(np.float64) - lc[y0:y1, bx].astype(np.float64))))
        typ = float(np.mean(np.abs(np.diff(lc[y0:y1, bx + 2:bx + 12].astype(np.float64), axis=1))))
        jump_max = typ * jump_mult + jump_add
        print(
            f"    y{y0}-{y1} bx={bx} ycorr={ycorr:.3f} (min {min_ycorr}) "
            f"jump={jump:.2f} (max {jump_max:.2f}, typ {typ:.2f})"
        )
        if ycorr < min_ycorr or jump > jump_max:
            seam_fails += 1
    print(f"    failing seam rows: {seam_fails} (max {max_fail_rows})")
    if seam_fails > max_fail_rows:
        fails.append(
            f"{label}: plank seams do not line up across the patch boundary "
            f"in {seam_fails} rows (max {max_fail_rows})"
        )

    # (d) left-desk silhouette vs 1a099fc
    tol = int(gates["desk_channel_tol"])
    mismatch = desk & ~left_end & (np.max(np.abs(img.astype(np.int16) - ref.astype(np.int16)), axis=-1) > tol)
    n_mis = int(mismatch.sum())
    print(f"  {label} (d) left-desk mismatch vs 1a099fc: {n_mis} px (max {gates['max_desk_mismatch_px']})")
    if n_mis > int(gates["max_desk_mismatch_px"]):
        fails.append(
            f"{label}: left-desk silhouette differs from 1a099fc in {n_mis} px "
            f"(max {gates['max_desk_mismatch_px']})"
        )

    # (e) nightstand-green in the leftover ROI, excluding desk and chair
    x0, y0, x1, y1 = (int(v) for v in gates["green_roi"])
    cx0, cy0, cx1, cy1 = (int(v) for v in gates["chair_exclude"])
    roi = np.zeros((h, w), dtype=bool)
    roi[y0:y1, x0:x1] = True
    chair = np.zeros((h, w), dtype=bool)
    chair[cy0:cy1, cx0:cx1] = True
    green = is_nightstand_green(img) & roi & ~desk & ~chair
    n_green = int(green.sum())
    print(f"  {label} (e) nightstand-green pixels: {n_green} (max {gates['max_nightstand_green_px']})")
    if n_green > int(gates["max_nightstand_green_px"]):
        fails.append(
            f"{label}: {n_green} nightstand-green pixels remain in the Office leftover ROI "
            f"(max {gates['max_nightstand_green_px']})"
        )
    return fails


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--art", type=Path, default=ROOT / "assets/game/locations/office.png")
    ap.add_argument("--dir", type=Path, default=None)
    ap.add_argument("--frames", nargs="*", type=Path, default=None)
    ap.add_argument("--gates", type=Path, default=GATES_PATH)
    args = ap.parse_args()

    gates = load_gates(args.gates)
    ref = as_rgb(ROOT / gates["reference_art"])
    desk = load_mask(ROOT / gates["desk_mask"])
    patch = load_mask(ROOT / gates["patch_mask"])
    print(
        f"check_office_bed_patch: art={args.art} "
        f"patch={int(patch.sum())} px desk={int(desk.sum())} px "
        f"(frozen from {gates.get('source_commit', '?')})"
    )

    fails: list[str] = []
    if not args.art.exists():
        fails.append(f"missing Office art {args.art}")
    else:
        fails.extend(check_image(args.art, gates, ref, desk, patch, args.art.name))

    frames: list[Path] = []
    if args.frames:
        frames.extend(args.frames)
    if args.dir is not None:
        for name in ("office-1230.png", "office-2200.png"):
            p = args.dir / name
            if p.exists():
                frames.append(p)
            else:
                fails.append(f"missing rendered frame {p}")
    for fr in frames:
        if not fr.exists():
            fails.append(f"missing rendered frame {fr}")
            continue
        fails.extend(check_image(fr, gates, ref, desk, patch, fr.name))

    if fails:
        print("FAIL: Office patch lighting, seams, desk silhouette, or leftover green is wrong")
        for f in fails:
            print(f"  - {f}")
        return 1
    print("PASS: patch matches neighbouring boards, seams line up, desk matches 1a099fc, no nightstand-green")
    return 0


if __name__ == "__main__":
    sys.exit(main())
