#!/usr/bin/env python3
"""G12 Office bed a3 checks (BL-0148 step 4 attempt 3).

Asserts the properties a reviewer inspects after the a2 fail:
  (a) small-artefact detection: thin lines, small high-contrast components,
      and a ragged dark sliver under the left pedestal, compared with the
      clean floor (and not with legitimate kept shadows)
  (b) stamp / duplicate-block matching in the patch, excluding the natural
      plank period of about 19-21 px
  (c) whole-desk silhouette completeness: both ends present and complete,
      judged against the frozen right-half pedestal, not exactness inside
      the a2 desk mask alone

Masks, 1a099fc reference art, plank seams and the right-pedestal template
are frozen from 1a099fc. They are never redrawn from the output under test.
"""
from __future__ import annotations

import argparse
import json
import sys
from collections import deque
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
GATES_PATH = ROOT / "config" / "office_bed_a3_gates.json"


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


def integral(a: np.ndarray) -> np.ndarray:
    out = np.zeros((a.shape[0] + 1, a.shape[1] + 1), dtype=np.float64)
    out[1:, 1:] = np.cumsum(np.cumsum(a, axis=0), axis=1)
    return out


def box_sum(ii: np.ndarray, y0: int, x0: int, y1: int, x1: int) -> float:
    return float(ii[y1, x1] - ii[y0, x1] - ii[y1, x0] + ii[y0, x0])


def ring_mean(field: np.ndarray, outer: int, hole: int) -> np.ndarray:
    """Mean of an outer x outer window minus a hole x hole centre."""
    h, w = field.shape
    pad = outer // 2
    ph = hole // 2
    p = np.pad(field, pad, mode="edge")
    ii = integral(p)
    out = np.empty((h, w), dtype=np.float64)
    area_o = outer * outer
    area_h = hole * hole
    area_r = area_o - area_h
    for y in range(h):
        for x in range(w):
            sy, sx = y, x
            so = box_sum(ii, sy, sx, sy + outer, sx + outer)
            sh = box_sum(ii, sy + (pad - ph), sx + (pad - ph), sy + (pad - ph) + hole, sx + (pad - ph) + hole)
            out[y, x] = (so - sh) / area_r
    return out


def is_floorish(img: np.ndarray) -> np.ndarray:
    r = img[..., 0].astype(np.int32)
    g = img[..., 1].astype(np.int32)
    b = img[..., 2].astype(np.int32)
    return (r > 140) & ((r - b) > 50) & (g > 80)


def is_desk_material(img: np.ndarray, L: np.ndarray) -> np.ndarray:
    """Wood in the desk's own darker palette, not bright floorboards."""
    r = img[..., 0].astype(np.int32)
    g = img[..., 1].astype(np.int32)
    b = img[..., 2].astype(np.int32)
    teal = (g > r + 10) & (np.abs(g - b) < 20) & (g > 50) & (r < 100)
    plant = (g > r + 15) & (g > b + 5) & (r < 100)
    wood = (r >= g) & (g >= b - 15) & (r > 40) & ~teal & ~plant
    return wood & (L < 140) & ~is_floorish(img)


def sobel(field: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    kx = np.array([[-1, 0, 1], [-2, 0, 2], [-1, 0, 1]], dtype=np.float64)
    ky = kx.T
    pad = np.pad(field, 1, mode="edge")
    sx = np.zeros_like(field)
    sy = np.zeros_like(field)
    for dy in range(3):
        for dx in range(3):
            sl = pad[dy : dy + field.shape[0], dx : dx + field.shape[1]]
            sx += kx[dy, dx] * sl
            sy += ky[dy, dx] * sl
    return sx, sy


def check_image(img: np.ndarray, label: str, gates: dict, masks: dict, ref: np.ndarray) -> list[str]:
    fails: list[str] = []
    desk = masks["desk"]
    patch = masks["patch"]
    keep = masks["keep"]
    right = masks["right"]
    left_end = masks.get("left_end", np.zeros(desk.shape, dtype=bool))
    seams = list(gates["plank_seams"])
    L = luma(img)
    h, w = L.shape

    # --- (a) small artefacts ---
    # The new left panel is furniture, not a floor artefact.
    search = patch & ~desk & ~keep & ~left_end & ~is_desk_material(img, L)
    search[:270, :] = False
    search[493:, :] = False
    seam_row = np.zeros(h, dtype=bool)
    for s in seams:
        seam_row[max(0, s - 1) : min(h, s + 2)] = True
    search[seam_row, :] = False

    ring = ring_mean(L, int(gates["ring_size"]), int(gates["ring_hole"]))
    contrast = ring - L
    darker = (contrast >= float(gates["min_artefact_contrast"])) & search
    lab, nlab = label_cc(darker)
    artefacts = []
    min_px = int(gates["min_artefact_px"])
    min_w = int(gates["min_artefact_width"])
    for i in range(1, nlab + 1):
        yy, xx = np.where(lab == i)
        area = int(yy.size)
        width = int(xx.max() - xx.min() + 1) if area else 0
        height = int(yy.max() - yy.min() + 1) if area else 0
        # skip 1-2 column vertical plank joints
        if width <= 2 and height >= 8:
            continue
        if area >= min_px and width >= min_w:
            artefacts.append((area, int(xx.min()), int(xx.max()), int(yy.min()), int(yy.max()), float(contrast[yy, xx].mean())))
    print(
        f"  {label} (a) small high-contrast artefacts off seams and kept shadows: "
        f"{len(artefacts)} (max {int(gates['max_artefact_components'])})"
    )
    for area, x0, x1, y0, y1, dL in artefacts:
        print(f"    blob {area} px x{x0}-{x1} y{y0}-{y1} mean dL={dL:.1f}")
        if len(artefacts) > int(gates["max_artefact_components"]):
            fails.append(
                f"{label}: small high-contrast artefact {area} px at x{x0}-{x1} y{y0}-{y1} "
                f"(max {int(gates['max_artefact_components'])} components of this kind)"
            )

    sx, sy = sobel(L)
    diag = (np.abs(sx) >= 6) & (np.abs(sy) >= 6) & search
    lab, nlab = label_cc(diag)
    chains = []
    min_chain = int(gates["min_diag_chain_px"])
    for i in range(1, nlab + 1):
        yy, xx = np.where(lab == i)
        if int(yy.size) >= min_chain:
            chains.append((int(yy.size), int(xx.min()), int(xx.max()), int(yy.min()), int(yy.max())))
    print(
        f"  {label} (a) diagonal thin-line chains (>= {min_chain} px): "
        f"{len(chains)} (max {int(gates['max_diag_chain_components'])})"
    )
    for area, x0, x1, y0, y1 in chains:
        print(f"    chain {area} px x{x0}-{x1} y{y0}-{y1}")
    if len(chains) > int(gates["max_diag_chain_components"]):
        fails.append(
            f"{label}: {len(chains)} diagonal thin-line chains in the patch "
            f"(max {int(gates['max_diag_chain_components'])})"
        )

    # ragged sliver / stray block under the left pedestal.
    # A finished left support has to sit LEFT of the a2 cut (x < 28). The
    # existing drawer body at x >= 28 is not that support.
    x0, y0, x1, y1 = [int(v) for v in gates["left_foot_roi"]]
    foot = np.zeros((h, w), dtype=bool)
    foot[y0:y1, x0:x1] = True
    foot_dark = foot & (L < float(gates["left_foot_dark_luma"]))
    left_of_cut = np.zeros((h, w), dtype=bool)
    left_of_cut[y0:y1, x0:min(x1, 28)] = True
    support = left_of_cut & is_desk_material(img, L)
    col_w = int((support.any(axis=0)).sum())
    solid = col_w >= int(gates["min_solid_support_width"])
    if solid:
        thin_dark = 0
    else:
        thin_dark = int(foot_dark.sum())
    print(
        f"  {label} (a) left-foot dark sliver/block: {thin_dark} px "
        f"(support width {col_w} px, solid={solid}, max thin {int(gates['max_left_foot_thin_px'])})"
    )
    if thin_dark > int(gates["max_left_foot_thin_px"]):
        ys, xs = np.where(foot_dark)
        fails.append(
            f"{label}: ragged dark sliver or stray block under the left pedestal, "
            f"{thin_dark} px at x{int(xs.min())}-{int(xs.max())} y{int(ys.min())}-{int(ys.max())} "
            f"(max {int(gates['max_left_foot_thin_px'])} px when no solid left support is present)"
        )

    # --- (b) stamps ---
    lag_lo = int(gates["natural_period_lo"])
    lag_hi = int(gates["natural_period_hi"])
    lag_min = int(gates["stamp_lag_min"])
    lag_max = int(gates["stamp_lag_max"])
    min_w = int(gates["stamp_min_block_w"])
    min_copies = int(gates["stamp_min_copies"])
    exact_min = int(gates["stamp_exact_col_min"])
    stamp_hits = []
    # Search in the patch (and the leftover floor at the left), excluding desk and keep,
    # one plank at a time, x up to the patch's right edge + a little.
    xs_patch = np.where(patch.any(axis=0))[0]
    x_right = int(min(w, xs_patch.max() + 8)) if xs_patch.size else 320
    for i in range(len(seams) - 1):
        ya, yb = int(seams[i]), int(seams[i + 1])
        if yb <= ya:
            continue
        # columns that have any searchable floor in this plank
        row_mask = patch[ya:yb] & ~desk[ya:yb] & ~keep[ya:yb] & ~left_end[ya:yb]
        if not row_mask.any():
            continue
        strip = img[ya:yb, :x_right]
        for lag in range(lag_min, min(lag_max, x_right - min_w)):
            if lag_lo <= lag <= lag_hi:
                continue
            m = x_right - lag
            if m < min_w * 2:
                continue
            a = strip[:, :m]
            b = strip[:, lag : lag + m]
            # Exact over a sliding 8-row band, then OR, so a stamp shorter
            # than the full plank still counts.
            band_h = min(8, strip.shape[0])
            exact_col = np.zeros(m, dtype=bool)
            for yoff in range(0, max(1, strip.shape[0] - band_h + 1)):
                aa = strip[yoff:yoff + band_h, :m]
                bb = strip[yoff:yoff + band_h, lag:lag + m]
                exact_col |= (aa == bb).all(axis=-1).all(axis=0)
            in_region = row_mask[:, :m].any(axis=0) | row_mask[:, lag:lag + m].any(axis=0)
            eq = exact_col & in_region
            if eq.any() and int(eq.sum()) >= exact_min * min_copies:
                xs = np.where(eq)[0]
                span = int(xs.max() - xs.min())
                n_copies = 1 + (span // lag)
                if n_copies >= min_copies and span >= lag:
                    stamp_hits.append((ya, yb, lag, n_copies, int(eq.sum())))
    print(
        f"  {label} (b) stamp lags with {min_copies}+ exact {exact_min}px runs, "
        f"excluding period {lag_lo}-{lag_hi}: {len(stamp_hits)} (max {int(gates['max_stamp_lags'])})"
    )
    for ya, yb, lag, nruns, ncols in stamp_hits[:12]:
        print(f"    y{ya}-{yb} lag={lag} runs={nruns} exact_cols={ncols}")
    if len(stamp_hits) > int(gates["max_stamp_lags"]):
        example = stamp_hits[0]
        fails.append(
            f"{label}: repeated tiles in the patch at lag {example[2]} on planks y{example[0]}-{example[1]} "
            f"({len(stamp_hits)} lag/plank hits, max {int(gates['max_stamp_lags'])}, natural period {lag_lo}-{lag_hi} excluded)"
        )

    # --- (c) whole-desk silhouette ---
    lx0, ly0, lx1, ly1 = [int(v) for v in gates["left_end_roi"]]
    # The leg/side panel has to occupy the lower left, under the desktop
    # and left of the a2 cut, where attempt 2 left a floor rectangle.
    left_roi = np.zeros((h, w), dtype=bool)
    left_roi[max(ly0, 270):ly1, lx0:min(lx1, 28)] = True
    left_support = left_roi & is_desk_material(img, L)
    n_sup = int(left_support.sum())
    width = int(left_support.any(axis=0).sum())
    if left_support.any():
        foot_y = int(np.where(left_support)[0].max())
        top_y = int(np.where(left_support)[0].min())
    else:
        foot_y, top_y = -1, -1
    right_foot_y = int(np.where(right)[0].max()) if right.any() else int(gates.get("right_foot_y", 323))
    floor_frac = float((left_roi & is_floorish(img) & ~left_support).sum()) / float(left_roi.sum())
    foot_delta = (abs(foot_y - right_foot_y) if foot_y >= 0 else 999)
    print(
        f"  {label} (c) left-end support: {n_sup} px width={width} foot_y={foot_y} "
        f"(need >= {int(gates['min_left_support_px'])} px, width >= {int(gates['min_left_support_width'])}, "
        f"foot within {int(gates['max_foot_y_delta'])} of right foot y={right_foot_y}), "
        f"floor_frac={floor_frac:.2f} (max {float(gates['max_floor_fraction_in_left_end'])})"
    )
    complete = (
        n_sup >= int(gates["min_left_support_px"])
        and width >= int(gates["min_left_support_width"])
        and foot_y >= 0
        and foot_delta <= int(gates["max_foot_y_delta"])
        and floor_frac <= float(gates["max_floor_fraction_in_left_end"])
    )
    if not complete:
        fails.append(
            f"{label}: left desk end is incomplete (support {n_sup} px, width {width}, "
            f"foot_y {foot_y} vs right {right_foot_y}, floor_frac {floor_frac:.2f}). "
            f"Both ends must be present and complete."
        )
    else:
        print(f"  {label} (c) left and right ends present and complete")

    # de-dup fail strings (artefact blobs can add many lines)
    uniq = []
    seen = set()
    for f in fails:
        # keep all artefact lines; they are distinct
        if f not in seen:
            uniq.append(f)
            seen.add(f)
    return uniq


def collect_targets(args, gates) -> list[tuple[str, Path]]:
    out: list[tuple[str, Path]] = []
    if args.art:
        out.append((args.art.name, args.art))
    frames: list[Path] = []
    if args.dir:
        frames.extend(sorted(Path(args.dir).glob("office-*.png")))
    for f in args.frames or []:
        frames.append(Path(f))
    for p in frames:
        out.append((p.name, p))
    if not out:
        raise SystemExit("pass --art and/or --dir / --frames")
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--art", type=Path, default=None)
    ap.add_argument("--dir", type=Path, default=None)
    ap.add_argument("--frames", nargs="*", default=None)
    ap.add_argument("--gates", type=Path, default=GATES_PATH)
    args = ap.parse_args()
    gates = load_gates(args.gates)
    ref = as_rgb(ROOT / gates["reference_art"])
    masks = {
        "desk": load_mask(ROOT / gates["desk_mask"]),
        "patch": load_mask(ROOT / gates["patch_mask"]),
        "keep": load_mask(ROOT / gates["keep_shadow_mask"]),
        "clean": load_mask(ROOT / gates["clean_floor_mask"]),
        "right": load_mask(ROOT / gates["right_pedestal_mask"]),
        "left_end": load_mask(ROOT / gates["left_end_support_roi"]),
    }
    print(
        "check_office_bed_a3: "
        f"patch={int(masks['patch'].sum())} px desk={int(masks['desk'].sum())} px "
        f"keep_shadow={int(masks['keep'].sum())} px right_pedestal={int(masks['right'].sum())} px "
        f"(frozen from {gates['source_commit']})"
    )
    fails: list[str] = []
    for label, path in collect_targets(args, gates):
        img = as_rgb(path)
        print(f"  {label}")
        fails.extend(check_image(img, label, gates, masks, ref))
    if fails:
        print("FAIL: small artefacts, stamped tiles, or an incomplete desk silhouette")
        for f in fails:
            print(f"  - {f}")
        print("EXIT:1")
        return 1
    print("PASS: no small floor artefacts, no stamped tiles, desk ends complete")
    print("EXIT:0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
