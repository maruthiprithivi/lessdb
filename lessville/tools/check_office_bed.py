#!/usr/bin/env python3
"""G12 Office bed paint-out check (BL-0148 step 4).

Asserts the property a reviewer inspects: there is no baked bed in the Office,
and the paint-out region matches the surrounding floorboards (no seam, no ghost).

The bed mask and bed palette are frozen from office.png at 1a099fc. They are
never redrawn from the output under test.

Checks, on the location art and on rendered Office frames when given:
  (a) bed-palette pixels inside the frozen mask are at most max_bed_palette_px
  (b) mean RGB and luma std of the (eroded) region match the surrounding floor
  (c) the region's y-autocorrelation at the floorboard lag matches the floor
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from PIL import Image, ImageFilter
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
GATES_PATH = ROOT / "config" / "office_bed_gates.json"


def load_gates(path: Path) -> dict:
    return json.loads(path.read_text())


def load_mask(path: Path) -> np.ndarray:
    im = Image.open(path)
    if im.size != (768, 512):
        raise SystemExit(f"bed mask must be 768x512, got {im.size}")
    arr = np.array(im)
    if arr.ndim == 3:
        arr = arr[..., 0] if arr.shape[2] > 1 else arr[..., 0]
    return arr > 127


def as_rgb(path: Path) -> np.ndarray:
    return np.array(Image.open(path).convert("RGB"))


def luma(rgb: np.ndarray) -> np.ndarray:
    x = rgb.astype(np.float64)
    return 0.2126 * x[..., 0] + 0.7152 * x[..., 1] + 0.0722 * x[..., 2]


def morph(mask: np.ndarray, radius: int, op: str) -> np.ndarray:
    if radius <= 0:
        return mask
    size = radius * 2 + 1
    fn = ImageFilter.MinFilter if op == "erode" else ImageFilter.MaxFilter
    out = Image.fromarray((mask.astype(np.uint8) * 255)).filter(fn(size))
    return np.array(out) > 127


def is_floor_or_skirt(img: np.ndarray, y: int, x: int) -> bool:
    r, g, b = (int(v) for v in img[y, x])
    if y >= 493 and x >= 260 and r < 200:
        return True
    return r > 165 and (r - b) > 75 and 95 < g < 200 and (r - g) > 30


def surround_mask(bed: np.ndarray, img: np.ndarray, dilate_px: int) -> np.ndarray:
    ring = morph(bed, dilate_px, "dilate") & ~bed
    h, w = ring.shape
    out = np.zeros_like(ring)
    ys, xs = np.where(ring)
    for y, x in zip(ys, xs):
        if is_floor_or_skirt(img, y, x):
            out[y, x] = True
    return out


def near_palette(img: np.ndarray, palette: list, tol: float) -> np.ndarray:
    pix = img.astype(np.int32)
    dmin = np.full(pix.shape[:2], 1.0e9, dtype=np.float64)
    for c in palette:
        cc = np.array(c, dtype=np.int32)
        delta = pix - cc
        dmin = np.minimum(dmin, np.sqrt((delta * delta).sum(-1).astype(np.float64)))
    return dmin <= tol


def y_autocorr_at_lags(img: np.ndarray, region: np.ndarray, lo: int, hi: int) -> float:
    """Mean Pearson correlation of vertical luma at lags lo..hi, over columns in region."""
    lum = luma(img)
    ys, xs = np.where(region)
    if ys.size == 0:
        return 0.0
    scores: list[float] = []
    for lag in range(lo, hi + 1):
        lag_vals: list[float] = []
        for x in range(int(xs.min()), int(xs.max()) + 1):
            col_y = ys[xs == x]
            if col_y.size < lag + 8:
                continue
            y0, y1 = int(col_y.min()), int(col_y.max()) + 1
            col = lum[y0:y1, x]
            a = col[:-lag]
            b = col[lag:]
            m = region[y0 : y1 - lag, x] & region[y0 + lag : y1, x]
            if int(m.sum()) < 8:
                continue
            a, b = a[m], b[m]
            a = a - a.mean()
            b = b - b.mean()
            denom = float(np.sqrt((a * a).sum() * (b * b).sum()))
            if denom < 1e-6:
                continue
            lag_vals.append(float((a * b).sum() / denom))
        if lag_vals:
            scores.append(float(np.mean(lag_vals)))
    return max(scores) if scores else 0.0


def region_stats(img: np.ndarray, region: np.ndarray, ref: np.ndarray) -> dict:
    a = img[region].astype(np.float64)
    b = img[ref].astype(np.float64)
    if a.size == 0 or b.size == 0:
        return {"dmean": 999.0, "dstdev": 999.0, "mean_a": [0, 0, 0], "mean_b": [0, 0, 0],
                "std_a": 0.0, "std_b": 0.0, "n_a": 0, "n_b": 0}
    ma, mb = a.mean(0), b.mean(0)
    la, lb = float(luma(a).std()), float(luma(b).std())
    return {
        "dmean": float(np.linalg.norm(ma - mb)),
        "dstdev": abs(la - lb),
        "mean_a": [round(float(v), 1) for v in ma],
        "mean_b": [round(float(v), 1) for v in mb],
        "std_a": round(la, 2),
        "std_b": round(lb, 2),
        "n_a": int(region.sum()),
        "n_b": int(ref.sum()),
    }


def check_image(path: Path, mask: np.ndarray, gates: dict, label: str, desk: np.ndarray | None = None) -> list[str]:
    fails: list[str] = []
    img = as_rgb(path)
    if img.shape[1] != 768 or img.shape[0] != 512:
        fails.append(f"{label}: expected 768x512 Office frame, got {img.shape[1]}x{img.shape[0]}")
        return fails
    region = mask
    if desk is not None:
        region = mask & ~desk
    # The rebuilt left desk end sits inside the frozen bed mask; it is
    # furniture, not leftover bed, and is scored by the a3 silhouette check.
    if gates.get("left_end_support_roi"):
        region = region & ~load_mask(ROOT / gates["left_end_support_roi"])
    pal = near_palette(img, gates["palette"], float(gates["palette_tol"])) & region
    pal_n = int(pal.sum())
    print(f"  {label} bed-palette: {pal_n} px inside frozen mask (max {gates['max_bed_palette_px']})")
    if pal_n > int(gates["max_bed_palette_px"]):
        fails.append(f"{label}: {pal_n} bed-palette pixels in the frozen mask (max {gates['max_bed_palette_px']})")

    eroded = morph(region, int(gates.get("erode_px", 4)), "erode")
    surround = surround_mask(mask, img, int(gates.get("surround_dilate_px", 24)))
    st = region_stats(img, eroded, surround)
    print(
        f"  {label} floor match: dmean={st['dmean']:.2f} (max {gates['max_mean_rgb_delta']}) "
        f"dstd={st['dstdev']:.2f} (max {gates['max_luma_std_delta']}) "
        f"region_mean={st['mean_a']} surround_mean={st['mean_b']} n={st['n_a']}/{st['n_b']}"
    )
    if st["dmean"] > float(gates["max_mean_rgb_delta"]):
        fails.append(
            f"{label}: paint-out region mean RGB differs from surrounding floor by "
            f"{st['dmean']:.1f} (max {gates['max_mean_rgb_delta']})"
        )
    if st["dstdev"] > float(gates["max_luma_std_delta"]):
        fails.append(
            f"{label}: paint-out region luma std differs from surrounding floor by "
            f"{st['dstdev']:.1f} (max {gates['max_luma_std_delta']})"
        )

    lo, hi = int(gates["board_lag_lo"]), int(gates["board_lag_hi"])
    ac = y_autocorr_at_lags(img, eroded, lo, hi)
    print(f"  {label} board autocorr lag {lo}-{hi}: {ac:.3f} (min {gates['min_board_autocorr']})")
    if ac < float(gates["min_board_autocorr"]):
        fails.append(
            f"{label}: floorboard autocorrelation {ac:.3f} in the paint-out region "
            f"(min {gates['min_board_autocorr']} at lag {lo}-{hi})"
        )
    return fails


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--art", type=Path, default=ROOT / "assets/game/locations/office.png",
                    help="Office location art (office.png)")
    ap.add_argument("--dir", type=Path, default=None,
                    help="Directory of rendered office-1230.png and office-2200.png")
    ap.add_argument("--frames", nargs="*", type=Path, default=None,
                    help="Explicit rendered Office frames")
    ap.add_argument("--gates", type=Path, default=GATES_PATH)
    args = ap.parse_args()

    gates = load_gates(args.gates)
    mask_path = ROOT / gates["mask"]
    mask = load_mask(mask_path)
    desk = None
    if gates.get("desk_mask"):
        desk = load_mask(ROOT / gates["desk_mask"])
    print(f"check_office_bed: art={args.art} mask={mask_path} "
          f"({int(mask.sum())} px, frozen from {gates.get('source_commit', '?')})")

    fails: list[str] = []
    if not args.art.exists():
        fails.append(f"missing Office art {args.art}")
    else:
        fails.extend(check_image(args.art, mask, gates, args.art.name, desk))

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
        fails.extend(check_image(fr, mask, gates, fr.name, desk))

    if fails:
        print("FAIL: Office still has a bed, or the paint-out does not match the floor")
        for f in fails:
            print(f"  - {f}")
        return 1
    print("PASS: no bed-palette pixels in the Office; paint-out matches surrounding floor")
    return 0


if __name__ == "__main__":
    sys.exit(main())
