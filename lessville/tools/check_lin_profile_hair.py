#!/usr/bin/env python3
"""G12 profile hair-coverage check for Lin's no-cap plains.

The reviewer inspects temple and forehead coverage, hairline shape, and
curl texture against the accepted front frames — not cap-palette absence.

Authority is Lin's plain, no-gear standing front frames (lin_plain_0..3).
The seated-front sheet frame lin_plain_seat_0 still shows the teal cap, so
it is not used as the ratio reference. Ratios from the standing fronts are
applied to the four side-profile plains, and to Lin's head in the rendered
home 22:00 and café 18:30 scenes (located from the seat catalog anchor and
InteriorView's 1.2 draw scale — not a mask fitted to the output).

The sprite-file check must FAIL on the f423a7e skullcap plains and PASS on
the painted plains. The render check must FAIL on the gallery-step3-a3
home/café views at c9a2484 (stale import of those plains) and PASS once
the same path draws the current PNGs.
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
CHAR_DIR = ROOT / "assets/game/characters"

FRONT_STANDING = [
    "lin_plain_0.png",
    "lin_plain_1.png",
    "lin_plain_2.png",
    "lin_plain_3.png",
]
FRONT_SEATED = [
    "lin_plain_seat_0.png",
    "lin_plain_seat_1.png",
    "lin_plain_seat_2.png",
    "lin_plain_seat_3.png",
]
BACK = [
    "lin_plain_back_0.png",
    "lin_plain_back_1.png",
]
PROFILES = [
    ("lin_plain_seat_e.png", "e"),
    ("lin_plain_seat_w.png", "w"),
    ("lin_plain_type_e.png", "e"),
    ("lin_plain_type_w.png", "w"),
]

# Coverage in the forward/center crown-to-eye slab must be at least this
# fraction of the weakest accepted seated-front frame.
MIN_COVER_OF_REF = 0.90
# Bare-skin fraction in that slab may not exceed this multiple of the
# highest accepted front-frame fraction.
MAX_SKIN_OF_REF = 1.25
# A hard vertical hair/skin wall (rows) with no hair in front of it.
MAX_WALL_RUN = 4
# Curl-highlight pixels / slab silhouette, as a floor vs seated fronts.
MIN_CURL_OF_REF = 0.70

Y_LIM = 80


def classify(p: tuple[int, int, int, int]) -> str:
    r, g, b, a = p
    if a < 32:
        return "empty"
    if r <= 120 and g <= 90 and b <= 90 and a >= 160 and (r + g + b) <= 220:
        if r + g + b <= 30:
            return "outline"
        if g > r + 15 and b > r + 10:
            return "teal"
        return "hair"
    if r >= 180 and 80 <= g <= 200 and 40 <= b <= 160 and r > g + 20 and r > b + 40:
        return "skin"
    return "other"


def is_dark(p: tuple[int, int, int, int]) -> bool:
    r, g, b, a = p
    return a >= 160 and r <= 40 and g <= 35 and b <= 35


def is_white(p: tuple[int, int, int, int]) -> bool:
    r, g, b, a = p
    return a >= 160 and r >= 200 and g >= 200 and b >= 200


def luma(p: tuple[int, int, int, int]) -> float:
    r, g, b, _a = p
    return 0.3 * r + 0.59 * g + 0.11 * b


def grid(im: Image.Image) -> list[list[str]]:
    w, _h = im.size
    return [[classify(im.getpixel((x, y))) for x in range(w)] for y in range(Y_LIM)]


def head_box(cls: list[list[str]]) -> tuple[int, int, int, int] | None:
    pts = [
        (x, y)
        for y in range(Y_LIM)
        for x in range(len(cls[0]))
        if cls[y][x] in ("hair", "skin", "outline")
    ]
    if not pts:
        return None
    xs = [x for x, _y in pts]
    ys = [y for _x, y in pts]
    return min(xs), min(ys), max(xs), max(ys)


def eye_y(im: Image.Image, cls: list[list[str]], x0: int, x1: int, y_top: int, y_bot: int) -> int | None:
    """Row of the eye: dark or white pixels that sit in the face (skin neighbours)."""
    w = im.size[0]
    cands: list[int] = []
    for y in range(y_top + 8, min(y_bot + 8, Y_LIM)):
        for x in range(x0, x1 + 1):
            p = im.getpixel((x, y))
            if not (is_dark(p) or is_white(p)):
                continue
            skin_n = 0
            for dy in range(-2, 3):
                for dx in range(-2, 3):
                    xx, yy = x + dx, y + dy
                    if 0 <= xx < w and 0 <= yy < Y_LIM and cls[yy][xx] == "skin":
                        skin_n += 1
            if skin_n >= 2:
                cands.append(y)
    if not cands:
        return None
    cands.sort()
    return cands[len(cands) // 2]


def forward_band(x0: int, x1: int, facing: str) -> tuple[int, int]:
    mid = (x0 + x1) / 2
    if facing == "e":
        return int(mid), x1
    if facing == "w":
        return x0, int(mid)
    # Front / back: central half, excluding hanging side curls.
    return int(x0 + (x1 - x0) * 0.25), int(x0 + (x1 - x0) * 0.75)


def measure(path: Path, facing: str) -> dict:
    im = Image.open(path).convert("RGBA")
    if im.size != (96, 112):
        return {"file": path.name, "error": f"size {im.size} != 96x112"}
    cls = grid(im)
    box = head_box(cls)
    if box is None:
        return {"file": path.name, "error": "no head silhouette"}
    x0, y_top, x1, _y1 = box
    y_bot = min(Y_LIM - 1, y_top + 45)
    ey = eye_y(im, cls, x0, x1, y_top, y_bot)
    fx0, fx1 = forward_band(x0, x1, facing)
    rec: dict = {
        "file": path.name,
        "facing": facing,
        "y_top": y_top,
        "eye_y": ey,
        "fwd": [fx0, fx1],
        "issues": [],
    }
    if ey is None:
        # Back views have no face. Skull must still be hair, not bare skin.
        hair = skin = sil = 0
        for y in range(y_top, y_bot + 1):
            for x in range(x0, x1 + 1):
                k = cls[y][x]
                if k == "empty":
                    continue
                sil += 1
                if k in ("hair", "outline"):
                    hair += 1
                elif k == "skin":
                    skin += 1
        rec.update({
            "has_face": False,
            "cover": hair / sil if sil else 0.0,
            "skin_frac": skin / sil if sil else 0.0,
            "curl_frac": 0.0,
            "wall_run": 0,
            "sil": sil,
            "hair": hair,
            "skin": skin,
        })
        return rec

    hair = skin = sil = hi = 0
    for y in range(y_top, ey):
        for x in range(fx0, fx1 + 1):
            k = cls[y][x]
            if k == "empty":
                continue
            sil += 1
            if k in ("hair", "outline"):
                hair += 1
                if luma(im.getpixel((x, y))) >= 42:
                    hi += 1
            elif k == "skin":
                skin += 1

    max_run = 0
    for x in range(x0, x1):
        run = 0
        for y in range(y_top, ey + 1):
            a, b = cls[y][x], cls[y][x + 1]
            if facing == "e":
                wall = a in ("hair", "outline") and b == "skin"
                if wall:
                    wall = not any(cls[y][xx] == "hair" for xx in range(x + 2, x1 + 1))
            elif facing == "w":
                wall = a == "skin" and b in ("hair", "outline")
                if wall:
                    wall = not any(cls[y][xx] == "hair" for xx in range(x0, x))
            else:
                wall = False
            run = run + 1 if wall else 0
            if run > max_run:
                max_run = run

    rec.update({
        "has_face": True,
        "cover": hair / sil if sil else 0.0,
        "skin_frac": skin / sil if sil else 0.0,
        "curl_frac": hi / sil if sil else 0.0,
        "wall_run": max_run,
        "sil": sil,
        "hair": hair,
        "skin": skin,
        "highlights": hi,
        "span": ey - y_top,
    })
    return rec


def judge(rec: dict, ref: dict, kind: str) -> dict:
    issues: list[str] = []
    if rec.get("error"):
        issues.append(rec["error"])
        rec["issues"] = issues
        rec["ok"] = False
        return rec
    if kind == "back":
        if rec["skin_frac"] > 0.05:
            issues.append(f"back skull has skin_frac={rec['skin_frac']:.3f} (expected hair coverage)")
        if rec["cover"] < 0.80:
            issues.append(f"back skull cover={rec['cover']:.3f} < 0.80")
        rec["issues"] = issues
        rec["ok"] = not issues
        return rec

    # Profiles are judged against seated-front ratios; fronts against their own group.
    min_cover = MIN_COVER_OF_REF * ref["min_cover"]
    max_skin = MAX_SKIN_OF_REF * ref["max_skin"]
    min_curl = MIN_CURL_OF_REF * ref["min_curl"]
    rec["thresholds"] = {
        "min_cover": min_cover,
        "max_skin": max_skin,
        "min_curl": min_curl,
        "max_wall": MAX_WALL_RUN,
        "ref_min_cover": ref["min_cover"],
        "ref_max_skin": ref["max_skin"],
        "ref_min_curl": ref["min_curl"],
    }
    if rec["cover"] < min_cover:
        issues.append(
            f"forehead/temple hair cover={rec['cover']:.3f} < {min_cover:.3f} "
            f"({MIN_COVER_OF_REF:.0%} of front min {ref['min_cover']:.3f})"
        )
    if rec["skin_frac"] > max_skin:
        issues.append(
            f"bare skin in skull region skin_frac={rec['skin_frac']:.3f} > {max_skin:.3f} "
            f"({MAX_SKIN_OF_REF:.0%} of front max {ref['max_skin']:.3f})"
        )
    if rec.get("has_face") and rec["wall_run"] > MAX_WALL_RUN:
        issues.append(
            f"hard vertical hairline wall_run={rec['wall_run']} > {MAX_WALL_RUN}"
        )
    if rec.get("has_face") and rec["curl_frac"] < min_curl:
        issues.append(
            f"curl-highlight fraction={rec['curl_frac']:.3f} < {min_curl:.3f} "
            f"({MIN_CURL_OF_REF:.0%} of front min {ref['min_curl']:.3f})"
        )
    rec["issues"] = issues
    rec["ok"] = not issues
    return rec


def ref_from(rows: list[dict]) -> dict:
    covers = [r["cover"] for r in rows]
    skins = [r["skin_frac"] for r in rows]
    curls = [r["curl_frac"] for r in rows if r.get("has_face")]
    return {
        "min_cover": min(covers),
        "max_skin": max(skins),
        "min_curl": min(curls) if curls else 0.0,
        "n": len(rows),
    }



# --- Rendered-scene check -------------------------------------------------

INTERIOR_SCALE = 1.2
HIP_ROW = 78
SEATS_JSON = ROOT / "config/seats.json"

# The two review scenes CoS inspects. Hips come from seats.json; the frame is
# the one InteriorView._draw_agent_row selects: _frame_key returns lin_seat_e/w
# (interior_view.gd _side_frame / _frame_key), then Attire.apply_key
# (attire.gd) rewrites it to lin_plain_seat_e/w when Lin is settled indoors.
RENDER_CASES = [
    {
        "id": "lin-home-2200",
        "seat_id": "home.dining.west",
        "facing": "e",
        "frame": "lin_plain_seat_e.png",
        "views": ["lin-home-2200/view.png", "lin-home-2200-view.png"],
    },
    {
        "id": "lin-cafe-study-1830",
        "seat_id": "cafe.table2.right",
        "facing": "w",
        "frame": "lin_plain_seat_w.png",
        "views": ["lin-cafe-study-1830/view.png", "lin-cafe-study-1830-view.png"],
    },
]


def _seat_hip(seat_id: str) -> tuple[float, float]:
    import json
    data = json.loads(SEATS_JSON.read_text())
    for s in data.get("seats", []):
        if s.get("id") == seat_id:
            h = s["hip"]
            return float(h[0]), float(h[1])
    raise KeyError(f"seat {seat_id} not in {SEATS_JSON}")


def _find_view(root: Path, names: list[str]) -> Path | None:
    for n in names:
        p = root / n
        if p.is_file():
            return p
    return None


def extract_head_from_view(view: Image.Image, hip: tuple[float, float], sprite: Image.Image) -> Image.Image:
    """Sample the 96x112 sprite dest from an interior view.

    Matches InteriorView._draw_sprite (s=1, scale 1.2, feet-anchored):
    dest origin = feet - (96*1.2/2, 112*1.2), feet = hip + (112-78)*1.2.
    A 1px west snap is applied because the dest x origin sits on a half-pixel
    at this scale; without it the café head is sampled one texel off and the
    wall-run metric flickers. Alpha is taken from the expected plain frame
    (the asset apply_key should load), not from the render, so background
    wall/furniture in the dest rect is dropped.
    """
    import math

    view_rgb = view.convert("RGB")
    feet = (hip[0], hip[1] + (112 - HIP_ROW) * INTERIOR_SCALE)
    x0 = feet[0] - 96 * INTERIOR_SCALE / 2
    y0 = feet[1] - 112 * INTERIOR_SCALE
    pix = view_rgb.load()
    vw, vh = view_rgb.size
    out = Image.new("RGBA", (96, 112), (0, 0, 0, 0))
    for sy in range(112):
        for sx in range(96):
            vx = int(round(x0 - 1 + (sx + 0.5) * INTERIOR_SCALE))
            vy = int(round(y0 + (sy + 0.5) * INTERIOR_SCALE))
            sa = sprite.getpixel((sx, sy))[3]
            if 0 <= vx < vw and 0 <= vy < vh:
                r, g, b = pix[vx, vy]
                out.putpixel((sx, sy), (r, g, b, sa))
    return out


def measure_render_case(view_path: Path, case: dict, char: Path, scratch: Path) -> dict:
    hip = _seat_hip(case["seat_id"])
    sprite_path = char / case["frame"]
    if not sprite_path.is_file():
        return {"file": case["id"], "error": f"missing sprite {sprite_path}", "issues": [f"missing sprite {sprite_path}"], "ok": False}
    sprite = Image.open(sprite_path).convert("RGBA")
    view = Image.open(view_path).convert("RGB")
    head = extract_head_from_view(view, hip, sprite)
    scratch.parent.mkdir(parents=True, exist_ok=True)
    head.save(scratch)
    rec = measure(scratch, case["facing"])
    rec["file"] = f"{case['id']} ({view_path.name}, {case['frame']} @ {case['seat_id']} hip={hip})"
    rec["view"] = str(view_path)
    rec["frame"] = case["frame"]
    rec["seat_id"] = case["seat_id"]
    return rec


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", default=str(CHAR_DIR), help="character PNG directory")
    ap.add_argument("--renders", default="", help="directory of rendered interior views (home 22:00, café 18:30)")
    ap.add_argument("--renders-only", action="store_true", help="skip the on-disk sprite-file check")
    ap.add_argument("--scratch", default="/tmp/lin-hair-render-heads", help="where to write extracted 96x112 heads")
    args = ap.parse_args()
    char = Path(args.dir)

    def load(name: str, facing: str) -> dict:
        return measure(char / name, facing)

    standing = [load(n, "front") for n in FRONT_STANDING]
    seated = [load(n, "front") for n in FRONT_SEATED]
    backs = [load(n, "front") for n in BACK]
    profiles = [load(n, f) for n, f in PROFILES]
    # Profiles (and rendered heads) are judged against the plain, no-gear
    # standing fronts. lin_plain_seat_0 still shows the cap in the sheet.
    ref_front = ref_from(standing)
    ref_seated = ref_from(seated)

    print("Lin profile hair-coverage check")
    print(f"dir: {char}")
    print(
        "plain no-gear front reference (standing lin_plain_0..3): "
        f"min_cover={ref_front['min_cover']:.3f} max_skin={ref_front['max_skin']:.3f} "
        f"min_curl={ref_front['min_curl']:.3f}"
    )
    print()

    fails: list[str] = []
    n = 0

    def show(kind: str, rec: dict, ref: dict | None) -> None:
        nonlocal n
        n += 1
        if ref is not None and kind != "back":
            judge(rec, ref, kind)
        else:
            judge(rec, {"min_cover": 0, "max_skin": 1, "min_curl": 0}, kind)
        tag = "PASS" if rec["ok"] else "FAIL"
        extra = (
            f"cover={rec.get('cover', 0):.3f} skin={rec.get('skin_frac', 0):.3f} "
            f"curl={rec.get('curl_frac', 0):.3f} wall={rec.get('wall_run', 0)} "
            f"eye_y={rec.get('eye_y')} sil={rec.get('sil')}"
        )
        print(f"[{tag}] {kind:8s} {rec['file']:22s} {extra}")
        for iss in rec["issues"]:
            print(f"       - {iss}")
            fails.append(f"{rec['file']}: {iss}")

    if not args.renders_only:
        for rec in standing:
            show("standing", rec, ref_front)
        for rec in seated:
            # Still measured, but not used as the profile authority.
            show("seated", rec, ref_seated)
        for rec in backs:
            show("back", rec, None)
        for rec in profiles:
            show("profile", rec, ref_front)
        print()

    render_recs: list[dict] = []
    if args.renders:
        rend = Path(args.renders)
        print(f"rendered scenes: {rend}")
        scratch_root = Path(args.scratch)
        for case in RENDER_CASES:
            view = _find_view(rend, case["views"])
            if view is None:
                rec = {
                    "file": case["id"],
                    "error": f"missing view, tried {case['views']} under {rend}",
                    "issues": [f"missing view, tried {case['views']} under {rend}"],
                    "ok": False,
                    "cover": 0, "skin_frac": 0, "curl_frac": 0, "wall_run": 0, "sil": 0, "eye_y": None,
                }
            else:
                rec = measure_render_case(view, case, char, scratch_root / f"{case['id']}.png")
            render_recs.append(rec)
            show("render", rec, ref_front)
        print()

    if fails:
        profile_fails = [r for r in profiles if not r.get("ok", False)] if not args.renders_only else []
        render_fails = [r for r in render_recs if not r.get("ok", False)]
        print(
            f"FAIL: {len(fails)} issue(s); "
            + (f"profile failed={len(profile_fails)}/4, " if not args.renders_only else "")
            + (f"render failed={len(render_fails)}/{len(render_recs)}" if render_recs else "no renders")
        )
        return 1
    if args.renders and not render_recs:
        print("FAIL: --renders given but no cases were checked")
        return 1
    bits = []
    if not args.renders_only:
        bits.append(f"{n - len(render_recs)} sprite frames")
    if render_recs:
        bits.append(f"{len(render_recs)} rendered scenes")
    print(f"PASS: {' and '.join(bits)} meet hair-coverage ratios from the plain no-gear front frames")
    return 0


if __name__ == "__main__":
    sys.exit(main())
