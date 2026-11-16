#!/usr/bin/env python3
"""G12 atmosphere check: sky luminance/hue bands per clock time, and wind displacement.

Measures the exact Town PNGs the gallery publishes (1536x1024, map pixels = image
pixels). Thresholds live in config/atmosphere_gates.json and are absolute.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from PIL import Image

import scene_lighting as sl

ROOT = Path(__file__).resolve().parents[1]
GATES_PATH = ROOT / "config" / "atmosphere_gates.json"


def luma(r: int, g: int, b: int) -> float:
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def band_stats(im: Image.Image, box: tuple[int, int, int, int]) -> dict:
    x0, y0, x1, y1 = box
    x0, y0 = max(0, x0), max(0, y0)
    x1, y1 = min(im.width, x1), min(im.height, y1)
    if x1 <= x0 or y1 <= y0:
        return {"lum": 0.0, "r": 0.0, "g": 0.0, "b": 0.0, "n": 0}
    crop = im.convert("RGB").crop((x0, y0, x1, y1))
    px = list(crop.get_flattened_data() if hasattr(crop, "get_flattened_data") else crop.getdata())
    n = len(px)
    rs = sum(p[0] for p in px) / n
    gs = sum(p[1] for p in px) / n
    bs = sum(p[2] for p in px) / n
    return {"lum": luma(rs, gs, bs), "r": rs, "g": gs, "b": bs, "n": n}


def mae_luma(a: Image.Image, b: Image.Image, box: tuple[int, int, int, int]) -> float:
    x0, y0, x1, y1 = box
    ca = a.convert("RGB").crop((x0, y0, x1, y1))
    cb = b.convert("RGB").crop((x0, y0, x1, y1))
    pa = list(ca.getdata())
    pb = list(cb.getdata())
    if not pa or len(pa) != len(pb):
        return 0.0
    return sum(abs(luma(*x) - luma(*y)) for x, y in zip(pa, pb)) / len(pa)


def load_gates(path: Path) -> dict:
    return json.loads(path.read_text())



def check_lighting(folder: Path, gates: dict) -> list[str]:
    """(a) glass containment, (b) glow-on-lamp, (c) every lamp lit, (d) dusk warmth.

    Glass and lamp sprites are derived from town.png (the location art) and
    compared to the rendered pixels. The hand-authored TOWN_WINDOWS / LAMPS
    lists are never read.
    """
    fails: list[str] = []
    lighting = gates.get("lighting") or {}
    if not lighting:
        return fails
    night_key = lighting.get("night_key", "2200")
    dusk_key = lighting.get("dusk_key", "1830")
    night_path = folder / f"town-{night_key}.png"
    dusk_path = folder / f"town-{dusk_key}.png"
    if not night_path.exists():
        fails.append(f"missing {night_path.name} for lighting checks")
        return fails
    scene = sl.load_scene()
    town, glass, lamps = scene["rgb"], scene["glass"], scene["lamps"]
    print(f"  scene: {int(glass.sum())} glass pixels, {len(lamps)} art-derived lamps")
    darkness = float(lighting.get("night_darkness", 0.74))
    render = sl._as_rgb(night_path)

    # (a) glass-mask containment
    contain = sl.glass_containment(
        render, town, glass, lamps, darkness,
        glow_radius=float(lighting.get("glow_extent_max_dist", 48)),
    )
    max_frac = float(lighting.get("max_spill_frac", 0.03))
    max_px = int(lighting.get("max_spill_pixels", 400))
    print(
        f"  (a) glass containment {night_path.name}: lit={contain['n_lit']} "
        f"inside={contain['n_inside']} spill={contain['n_spill']} "
        f"frac={contain['spill_frac']:.3f} (max frac {max_frac} or {max_px} px)"
    )
    if contain["n_spill"] > max_px and contain["spill_frac"] > max_frac:
        fails.append(
            f"(a) lit-window spill {contain['n_spill']} px ({contain['spill_frac']:.1%}) "
            f"outside the art-derived glass mask (limits {max_px} px, {max_frac:.1%})"
        )

    # (b) glow-on-lamp, including the café-front / terrace ROI
    glow = sl.glow_mask(render, town, glass, darkness, contain)
    g = sl.glow_on_lamp(
        glow,
        lamps,
        max_centroid_dist=float(lighting.get("glow_centroid_max_dist", 28)),
        max_extent=float(lighting.get("glow_extent_max_dist", 48)),
        roi=sl.CAFE_FRONT,
        max_far_pixels=int(lighting.get("glow_extent_max_far_pixels", 200)),
        max_far_frac=float(lighting.get("glow_extent_max_far_frac", 0.08)),
    )
    print(
        f"  (b) glow-on-lamp: {g['n_glow']} glow px, {g['n_components']} blobs, "
        f"{g['far_centroids']} far centroids, extent far={g['n_far_extent']} ({g['far_frac']:.1%})"
        + (f", terrace ROI n={g['roi']['n']} dist={g['roi']['dist']:.1f}" if g.get("roi") and g["roi"].get("dist") is not None else "")
    )
    far_frac_lim = float(lighting.get("glow_extent_max_far_frac", 0.08))
    far_px_lim = int(lighting.get("glow_extent_max_far_pixels", 200))
    for msg in g["fails"]:
        # The extent fail message is gated by both pixel count and fraction in glow_on_lamp;
        # re-state all fails that the helper already decided.
        fails.append(f"(b) {msg}")

    # (b2) same glow-on-lamp rule on the WHOLE building band, using a glow
    # mask that does not trust a leaky glass mask (wall-coloured source pixels
    # outside a real window pane must sit on a lamp).
    wall_glow = sl.whole_frame_glow(render, town, glass, lamps, darkness, contain)
    wg = sl.glow_on_lamp(
        wall_glow,
        lamps,
        max_centroid_dist=float(lighting.get("glow_centroid_max_dist", 28)),
        max_extent=float(lighting.get("glow_extent_max_dist", 48)),
        roi=None,
        max_far_pixels=int(lighting.get("wall_glow_max_far_pixels", 80)),
        max_far_frac=float(lighting.get("wall_glow_max_far_frac", 0.08)),
    )
    print(
        f"  (b) whole-frame wall glow: {wg['n_glow']} px, {wg['n_components']} blobs, "
        f"{wg['far_centroids']} far centroids, extent far={wg['n_far_extent']} ({wg['far_frac']:.1%})"
    )
    for msg in wg["fails"]:
        fails.append(f"(b) whole-frame {msg}")

    # (c) every lamp lit after dark
    lit = sl.lamps_lit(
        glow,
        lamps,
        radius=float(lighting.get("lamp_annulus_radius", 28)),
        min_pixels=int(lighting.get("lamp_min_pixels", 40)),
        min_sectors=int(lighting.get("lamp_min_sectors", 10)),
    )
    print(f"  (c) lamps lit after dark: {lit['n_lit']}/{lit['n_lamps']}")
    for m in lit["missing"]:
        print(f"      unlit {m['kind']} ({m['x']:.1f},{m['y']:.1f}) n={m['n']} sectors={m.get('sectors', 0)}")
        fails.append(
            f"(c) {m['kind']} at ({m['x']:.1f},{m['y']:.1f}) is not lit after dark "
            f"(halo n={m['n']}, sectors={m.get('sectors', 0)})"
        )

    # (d) 18:30 body warmth / saturation, not just the sky-band luminance
    if not dusk_path.exists():
        fails.append(f"missing {dusk_path.name} for dusk warmth gate")
        return fails
    dusk = sl.dusk_warmth(sl._as_rgb(dusk_path))
    body = dusk["body"]
    warm_min = float(lighting.get("dusk_body_warm_min", 16.0))
    sat_min = float(lighting.get("dusk_body_sat_min", 0.14))
    print(
        f"  (d) {dusk_path.name} body lum={body['lum']:.1f} R-B={body['warm']:.1f} "
        f"sat={body['sat']:.3f} (floor R-B>={warm_min}, sat>={sat_min})"
    )
    if body["warm"] < warm_min or body["sat"] < sat_min:
        fails.append(
            f"(d) 18:30 town body is not a warm golden dusk: R-B={body['warm']:.1f} "
            f"sat={body['sat']:.3f} (need R-B>={warm_min}, sat>={sat_min})"
        )

    # (e) every lamp is OFF in daylight (07:30, 12:30): no lit-lamp sprite.
    day_keys = lighting.get("day_keys") or ["0730", "1230"]
    warm_max = float(lighting.get("day_lamp_warm_max", 28.0))
    sat_max = float(lighting.get("day_lamp_sat_max", 0.16))
    head_r = float(lighting.get("day_lamp_head_r", 6.0))
    for key in day_keys:
        path = folder / f"town-{key}.png"
        if not path.exists():
            fails.append(f"missing {path.name} for daylight lamp-off check")
            continue
        off = sl.lamps_off_daylight(sl._as_rgb(path), lamps, head_r=head_r,
                                    warm_max=warm_max, sat_max=sat_max)
        print(
            f"  (e) lamps off in daylight {path.name}: {off['n_off']}/{off['n_lamps']} off "
            f"(head R-B<{warm_max}, sat<{sat_max})"
        )
        for m in off["lit"]:
            print(
                f"      lit {m['kind']} ({m['x']:.1f},{m['y']:.1f}) "
                f"R-B={m['warm']:.1f} sat={m['sat']:.2f} lum={m['lum']:.1f}"
            )
        for msg in off["fails"]:
            fails.append(f"(e) {path.name}: {msg}")
    return fails


def check_dir(folder: Path, gates: dict) -> list[str]:
    fails: list[str] = []
    sx, sy, ex, ey = gates["sky_band"]
    sky_box = (sx, sy, ex, ey)
    for key, spec in gates["times"].items():
        path = folder / f"town-{key}.png"
        if not path.exists():
            fails.append(f"missing {path.name}")
            continue
        im = Image.open(path)
        st = band_stats(im, sky_box)
        lo, hi = spec["lum_min"], spec["lum_max"]
        print(
            f"  {spec['label']} {path.name}: sky lum={st['lum']:.1f} "
            f"rgb=({st['r']:.1f},{st['g']:.1f},{st['b']:.1f}) want lum {lo}-{hi} hue={spec.get('hue')}"
        )
        if not (lo <= st["lum"] <= hi):
            fails.append(
                f"{spec['label']}: sky luminance {st['lum']:.1f} not in [{lo}, {hi}]"
            )
        hue = spec.get("hue", "any")
        if hue == "night" and not (st["b"] > st["r"] + 4):
            fails.append(
                f"{spec['label']}: night sky should be blue (B>R+4), got R={st['r']:.1f} B={st['b']:.1f}"
            )
        if hue == "warm" and not (st["r"] > st["b"] + 4):
            fails.append(
                f"{spec['label']}: {spec['label']} sky should be warm (R>B+4), got R={st['r']:.1f} B={st['b']:.1f}"
            )

    fa, fb = folder / "wind-a.png", folder / "wind-b.png"
    if not fa.exists() or not fb.exists():
        fails.append("missing wind-a.png or wind-b.png")
    else:
        fx0, fy0, fw, fh = gates["flag_roi"]
        # flag_roi is [x, y, w, h] if 4 nums starting with 748,488,72,72
        # also accept [x0,y0,x1,y1] if x1>x0+20 and looks like a corner
        if fw > fx0 + 20 and fh > fy0 + 20:
            flag_box = (fx0, fy0, fw, fh)  # already xyxy?
            # our json is [748, 488, 820, 560] as xyxy in the comment, wait:
            # I wrote "flag_roi": [748, 488, 820, 560] in the first draft then
            # changed notes. Check actual file. We'll normalize below.
        flag_box = _as_xyxy(gates["flag_roi"])
        d = mae_luma(Image.open(fa), Image.open(fb), flag_box)
        need = float(gates["wind_min_mae"])
        print(f"  wind flag_roi={flag_box} mae={d:.2f} want >= {need}")
        if d < need:
            fails.append(f"wind displacement mae {d:.2f} < {need} in flag_roi {flag_box}")
    return fails


def _as_xyxy(roi) -> tuple[int, int, int, int]:
    x, y, a, b = [int(v) for v in roi]
    # If a,b look like a size (small) rather than a corner, treat as xywh.
    if a <= 256 and b <= 256 and a < x and b < y:
        return (x, y, x + a, y + b)
    if a > x and b > y:
        return (x, y, a, b)
    return (x, y, x + a, y + b)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", type=Path, required=True, help="directory of town-*.png and wind-*.png")
    ap.add_argument("--gates", type=Path, default=GATES_PATH)
    args = ap.parse_args()
    gates = load_gates(args.gates)
    print(f"check_atmosphere: {args.dir} (gates {args.gates})")
    fails = check_dir(args.dir, gates)
    fails += check_lighting(args.dir, gates)
    if fails:
        print(f"FAIL: {len(fails)} atmosphere problems")
        for f in fails:
            print("  -", f)
        return 1
    print("PASS: sky, wind, glass-masked windows, lamp glows, dusk warmth, and daylight lamps-off ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
