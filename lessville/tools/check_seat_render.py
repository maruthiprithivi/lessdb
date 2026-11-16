#!/usr/bin/env python3
"""Headless seating check. Fails the build if a character is not visibly
seated on the furniture seat surface.

This asserts the property the reviewer inspects, not a proxy:
  1. The catalog hip lies inside a seat_surface mask derived from the actual
     chair / bench / stool / log pixels in the location art (not a rectangle
     drawn around the hip).
  2. After compositing the exact gallery frame and furniture overlay, a
     minimum number of sprite-palette pixels remain visible in the TORSO
     band of the sprite (rows 56–77, chest/waist above the sit line).
  3. For ordinary sit poses, a minimum number of sprite-palette pixels
     remain visible in the LEGS band (rows 88–111). Sit-type poses at a
     desk (`pose == "type"`) and seats marked `legs_hidden` (a front-facing
     desk that is supposed to hide the lower body) do not require legs;
     they still require a visible torso.
  4. A furniture overlay may not cover more than MAX_TORSO_OCCLUSION of the
     torso-band character pixels (a small table-edge overlap is allowed;
     painting out the chest is not).
  5. Expected facing is derived from the furniture, independent of the
     catalog `facing` field: opposite the recorded chair_back (the side of
     the seat_surface where the chair/sofa/bench back sits in the location
     art); otherwise toward the centroid of faces_furniture (an explicit
     overlay, not seat["fg"]); otherwise the recorded open_side on
     backless furniture. The catalog facing and the rendered frame
     direction (_e/_w/front/back) must both match that derived value.

The check composites each catalog seat with the same frame and crop the
gallery publishes.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from PIL import Image, ImageDraw

sys.path.insert(0, str(Path(__file__).resolve().parent))
import seatlib as sl

REF_AGENT = "sam"
GALLERY_CROP_PAD = 24

# Native 96×112 sprite rows. Hip row is 78 (see seatlib / Godot HIP_ROW).
BAND_HEAD = (0, 55)
BAND_TORSO = (56, 77)  # chest and waist above the sit line
BAND_LAP = (78, 87)
BAND_LEGS = (88, 111)

MIN_VISIBLE_TORSO = 40
MIN_VISIBLE_LEGS = 25
MAX_TORSO_OCCLUSION = 0.65  # side sitters face into the table, so the front of
                             # the jacket may sit under the rim; 0.65 still fails a
                             # painted-out torso (attempt-2 dining.east was 98%).

FAIL: list[str] = []
FAIL_IDS: list[str] = []


def fail(msg: str, sid: str | None = None) -> None:
    FAIL.append(msg)
    if sid and sid not in FAIL_IDS:
        FAIL_IDS.append(sid)
    print("FAIL:", msg)


def load_bg(loc: str) -> Image.Image:
    name = "town.png" if loc == "town" else f"{loc}.png"
    return Image.open(sl.LOC_DIR / name).convert("RGBA")


def blit_sprite(canvas: Image.Image, sprite: Image.Image, rect: tuple[float, float, float, float], flip: bool) -> None:
    x0, y0, x1, y1 = rect
    w, h = int(round(x1 - x0)), int(round(y1 - y0))
    if w <= 0 or h <= 0:
        return
    sp = sprite.resize((w, h), Image.Resampling.NEAREST)
    if flip:
        sp = sp.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
    canvas.paste(sp, (int(round(x0)), int(round(y0))), sp)


def apply_fg(canvas: Image.Image, fg_name: str | None) -> Image.Image | None:
    if not fg_name:
        return None
    path = sl.FG_DIR / f"{fg_name}.png"
    if not path.exists():
        fail(f"missing foreground overlay {fg_name}")
        return None
    fg = Image.open(path).convert("RGBA")
    if fg.size != canvas.size:
        fail(f"fg {fg_name} size {fg.size} != canvas {canvas.size}")
        return fg
    canvas.alpha_composite(fg)
    return fg


def gallery_frame(agent: str, seat: dict) -> str:
    return sl.frame_for(agent, seat)


def gallery_crop_box(canvas_size: tuple[int, int], rect: tuple[float, float, float, float], pad: int = GALLERY_CROP_PAD):
    x0, y0, x1, y1 = [int(round(v)) for v in rect]
    w, h = canvas_size
    return (
        max(0, x0 - pad),
        max(0, y0 - pad),
        min(w, x1 + pad),
        min(h, y1 + pad),
    )


def composite_seat(cat: dict, seat: dict, agent: str = REF_AGENT):
    """Return (canvas, char_layer, sprite, rect, frame_name, flipped). Same path as the gallery."""
    bg = load_bg(seat["location"])
    canvas = bg.copy()
    hip = (float(seat["hip"][0]), float(seat["hip"][1]))
    rect = sl.sprite_rect_from_hip(hip, cat, seat)
    frame = gallery_frame(agent, seat)
    sprite_path = sl.CHAR_DIR / frame
    if not sprite_path.exists():
        fail(f"{seat['id']}: gallery frame {frame} is missing", seat["id"])
        empty = Image.new("RGBA", bg.size, (0, 0, 0, 0))
        return canvas, empty, Image.new("RGBA", (96, 112), (0, 0, 0, 0)), rect, frame, False
    sprite = Image.open(sprite_path).convert("RGBA")
    flip = seat.get("facing") == "w" and not (frame.endswith("_w.png") or "_w." in frame)
    if frame.endswith("_e.png") or "_e." in frame:
        flip = False
    char_layer = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    blit_sprite(char_layer, sprite, rect, flip)
    canvas.alpha_composite(char_layer)
    if seat.get("fg"):
        apply_fg(canvas, seat["fg"])
    return canvas, char_layer, sprite, rect, frame, flip


def sprite_palette(sprite: Image.Image) -> set[tuple[int, int, int]]:
    pal = set()
    px = sprite.load()
    w, h = sprite.size
    for y in range(h):
        for x in range(w):
            r, g, b, a = px[x, y]
            if a >= 32:
                pal.add((r, g, b))
    return pal


def is_side_frame(frame: str) -> bool:
    stem = Path(frame).stem
    return stem.endswith("_e") or stem.endswith("_w") or "_side_" in stem or stem.endswith("_side")


def band_of_row(sy: int) -> str:
    if sy <= BAND_HEAD[1]:
        return "head"
    if sy <= BAND_TORSO[1]:
        return "torso"
    if sy <= BAND_LAP[1]:
        return "lap"
    return "legs"


def legs_required(seat: dict) -> bool:
    """Ordinary sit poses must show legs. Typists at a desk, and seats
    explicitly marked legs_hidden (a front-facing desk that is supposed
    to hide the lower body), do not."""
    if str(seat.get("pose", "seat")) == "type":
        return False
    if seat.get("legs_hidden"):
        return False
    return True




OPP_DIR = {"n": "s", "s": "n", "e": "w", "w": "e"}


def overlay_centroid(overlay: dict) -> tuple[float, float]:
    if "ellipse" in overlay:
        cx, cy, _rx, _ry = overlay["ellipse"]
        return float(cx), float(cy)
    xs, ys = [], []
    for poly in overlay.get("polygons") or []:
        for x, y in poly:
            xs.append(float(x))
            ys.append(float(y))
    if not xs:
        raise ValueError("overlay has no ellipse or polygons")
    return sum(xs) / len(xs), sum(ys) / len(ys)


def cardinal_toward(hip: tuple[float, float], dest: tuple[float, float]) -> str:
    dx = dest[0] - hip[0]
    dy = dest[1] - hip[1]
    if dx == 0 and dy == 0:
        raise ValueError("hip coincides with furniture centroid")
    if abs(dx) >= abs(dy):
        return "e" if dx > 0 else "w"
    return "s" if dy > 0 else "n"


def furniture_centroid(cat: dict, ref) -> tuple[float, float]:
    if isinstance(ref, str):
        overlays = cat.get("overlays") or {}
        if ref not in overlays:
            raise KeyError(f"faces_furniture overlay {ref!r} is not in the catalog")
        return overlay_centroid(overlays[ref])
    if isinstance(ref, dict) and "centroid" in ref:
        c = ref["centroid"]
        return float(c[0]), float(c[1])
    if isinstance(ref, dict):
        return overlay_centroid(ref)
    raise TypeError(f"faces_furniture must be an overlay id or geometry, got {type(ref)}")


def derived_facing(cat: dict, seat: dict) -> tuple[str | None, str]:
    """Return (expected_facing, source). Furniture first, never seat['facing']."""
    sid = seat["id"]
    back = seat.get("chair_back")
    faces = seat.get("faces_furniture")
    open_side = seat.get("open_side")
    derived = None
    source = None
    if back is not None:
        if back not in OPP_DIR:
            fail(f"{sid}: chair_back={back!r} is not n/e/s/w", sid)
            return None, "invalid chair_back"
        derived = OPP_DIR[back]
        source = f"opposite chair_back={back}"
    if faces is not None:
        try:
            centroid = furniture_centroid(cat, faces)
            toward = cardinal_toward((float(seat["hip"][0]), float(seat["hip"][1])), centroid)
        except (KeyError, ValueError, TypeError) as exc:
            fail(f"{sid}: cannot derive facing from faces_furniture={faces!r}: {exc}", sid)
            return derived, source or "faces_furniture error"
        if derived is not None and toward != derived:
            fail(
                f"{sid}: chair_back implies facing={derived} but faces_furniture={faces} "
                f"(centroid {centroid[0]:.1f},{centroid[1]:.1f}) implies {toward}",
                sid,
            )
        if derived is None:
            derived = toward
            source = f"toward faces_furniture={faces} centroid=({centroid[0]:.1f},{centroid[1]:.1f})"
        else:
            source = f"opposite chair_back={back}, agrees with faces_furniture={faces}"
    if derived is None and open_side is not None:
        if open_side not in OPP_DIR:
            fail(f"{sid}: open_side={open_side!r} is not n/e/s/w", sid)
            return None, "invalid open_side"
        derived = open_side
        source = f"open_side={open_side} (backless furniture)"
    if derived is None:
        fail(
            f"{sid}: no facing derivation (need chair_back, faces_furniture, or open_side)",
            sid,
        )
        return None, "missing derivation"
    return derived, source


def frame_direction(frame: str) -> str | None:
    """Cardinal implied by the rendered filename, or None if it is a front/back frame."""
    stem = Path(frame).stem
    if stem.endswith("_e") or "_e." in Path(frame).name:
        return "e"
    if stem.endswith("_w") or "_w." in Path(frame).name:
        return "w"
    return None  # front or back; n vs s is selected by frame_for from facing

def count_bands(canvas: Image.Image, char_layer: Image.Image, rect, palette) -> dict[str, dict[str, int]]:
    """Count character vs still-visible sprite pixels per body band."""
    x0, y0, x1, y1 = rect
    scale = (y1 - y0) / 112.0 if (y1 - y0) else 1.0
    stats = {k: {"char": 0, "vis": 0, "occ": 0} for k in ("head", "torso", "lap", "legs")}
    w, h = canvas.size
    ymin, ymax = max(0, int(y0)), min(h, int(round(y1)) + 1)
    xmin, xmax = max(0, int(x0)), min(w, int(round(x1)) + 1)
    for y in range(ymin, ymax):
        sy = int((y - y0) / scale) if scale else 0
        if sy < 0 or sy > 111:
            continue
        band = band_of_row(sy)
        for x in range(xmin, xmax):
            cc = char_layer.getpixel((x, y))
            if cc[3] < 32 or cc[:3] not in palette:
                continue
            stats[band]["char"] += 1
            pc = canvas.getpixel((x, y))
            visible = pc[3] >= 32 and sum((int(pc[i]) - int(cc[i])) ** 2 for i in range(3)) <= 27
            if visible:
                stats[band]["vis"] += 1
            else:
                stats[band]["occ"] += 1
    return stats


def check_one(cat: dict, seat: dict, preview_dir: Path | None) -> None:
    sid = seat["id"]
    hip = (float(seat["hip"][0]), float(seat["hip"][1]))
    surface = seat.get("seat_surface")
    if not surface:
        fail(f"{sid}: missing seat_surface (furniture-derived seat mask)", sid)
        return
    if not sl.point_in_surface(hip[0], hip[1], surface):
        fail(
            f"{sid}: hip {hip} is not inside the furniture seat_surface {surface}",
            sid,
        )

    canvas, char_layer, sprite, rect, frame, flip = composite_seat(cat, seat)
    pal = sprite_palette(sprite)
    stats = count_bands(canvas, char_layer, rect, pal)

    torso_vis = stats["torso"]["vis"]
    torso_char = stats["torso"]["char"]
    legs_vis = stats["legs"]["vis"]
    if torso_vis < MIN_VISIBLE_TORSO:
        fail(
            f"{sid}: only {torso_vis} visible torso-band pixels "
            f"(need >= {MIN_VISIBLE_TORSO}); frame={frame} bands={ {k: v['vis'] for k, v in stats.items()} }",
            sid,
        )
    if legs_required(seat) and legs_vis < MIN_VISIBLE_LEGS:
        fail(
            f"{sid}: only {legs_vis} visible legs-band pixels "
            f"(need >= {MIN_VISIBLE_LEGS} for pose={seat.get('pose')}); frame={frame}",
            sid,
        )
    if seat.get("fg") and torso_char > 0:
        occ = stats["torso"]["occ"] / torso_char
        if occ > MAX_TORSO_OCCLUSION:
            fail(
                f"{sid}: furniture overlay covers {occ:.0%} of the torso band "
                f"(max {MAX_TORSO_OCCLUSION:.0%}); vis={torso_vis} char={torso_char}",
                sid,
            )

    expected, source = derived_facing(cat, seat)
    facing = str(seat.get("facing", "s"))
    if expected is not None and facing != expected:
        fail(
            f"{sid}: catalog facing={facing} but furniture derives facing={expected} ({source})",
            sid,
        )
    check_facing = expected if expected is not None else facing
    frame_dir = frame_direction(frame)
    if check_facing in ("e", "w"):
        if not is_side_frame(frame):
            fail(
                f"{sid}: expected facing={check_facing} ({source}) but used non-side frame {frame}",
                sid,
            )
        elif frame_dir != check_facing:
            fail(
                f"{sid}: expected facing={check_facing} ({source}) but used mismatched side frame {frame}",
                sid,
            )
    elif frame_dir in ("e", "w"):
        fail(
            f"{sid}: expected facing={check_facing} ({source}) but used side frame {frame}",
            sid,
        )

    if preview_dir is not None:
        preview_dir.mkdir(parents=True, exist_ok=True)
        vis = canvas.convert("RGB")
        d = ImageDraw.Draw(vis)
        _draw_surface(d, surface, (0, 255, 0))
        hx, hy = int(round(hip[0])), int(round(hip[1]))
        d.ellipse([hx - 2, hy - 2, hx + 2, hy + 2], fill=(255, 255, 0))
        box = gallery_crop_box(canvas.size, rect)
        crop = vis.crop(box)
        crop.resize((crop.size[0] * 2, crop.size[1] * 2), Image.Resampling.NEAREST).save(preview_dir / f"{sid}.png")


def _draw_surface(d: ImageDraw.ImageDraw, surface: dict, color) -> None:
    typ = surface.get("type")
    if typ == "rect":
        x0, y0, x1, y1 = surface["xyxy"]
        d.rectangle([x0, y0, x1, y1], outline=color)
    elif typ == "polygon":
        d.polygon([tuple(p) for p in surface["points"]], outline=color)


def load_catalog(path: Path | None) -> dict:
    if path is None:
        return sl.load_catalog()
    return json.loads(Path(path).read_text())


def merge_surfaces(seats: list[dict], surfaces_from: Path) -> None:
    src = json.loads(Path(surfaces_from).read_text())
    by_id = {s["id"]: s for s in src["seats"]}
    for s in seats:
        other = by_id.get(s["id"])
        if other and other.get("seat_surface"):
            s["seat_surface"] = other["seat_surface"]
            if other.get("legs_hidden"):
                s["legs_hidden"] = other["legs_hidden"]


def main() -> int:
    FAIL.clear()
    FAIL_IDS.clear()
    ap = argparse.ArgumentParser()
    ap.add_argument("--preview-dir", type=Path, default=None)
    ap.add_argument("--only", default="", help="comma-separated seat ids to check")
    ap.add_argument("--catalog", type=Path, default=None, help="seats.json to use (default: work/config/seats.json)")
    ap.add_argument("--assets-root", type=Path, default=None, help="project root for backgrounds, sprites, overlays")
    ap.add_argument(
        "--surfaces-from",
        type=Path,
        default=None,
        help="another seats.json from which to take seat_surface / legs_hidden by id",
    )
    args = ap.parse_args()
    if args.assets_root is not None:
        sl.ROOT = Path(args.assets_root)
        sl.CATALOG_PATH = sl.ROOT / "config" / "seats.json"
        sl.CHAR_DIR = sl.ROOT / "assets" / "game" / "characters"
        sl.LOC_DIR = sl.ROOT / "assets" / "game" / "locations"
        sl.FG_DIR = sl.ROOT / "assets" / "game" / "seats" / "fg"

    cat = load_catalog(args.catalog)
    seats = cat["seats"]
    if args.surfaces_from:
        merge_surfaces(seats, args.surfaces_from)
    if args.only:
        want = {s.strip() for s in args.only.split(",") if s.strip()}
        seats = [s for s in seats if s["id"] in want]
    if len(seats) < 10 and not args.only:
        fail("catalog has too few seats")
    ids = [s["id"] for s in seats]
    if len(ids) != len(set(ids)):
        fail("duplicate seat ids")
    for s in seats:
        check_one(cat, s, args.preview_dir)
    if not args.only:
        have = {s["location"] for s in seats}
        for loc in ["office", "home", "cafe", "school", "bus", "gym", "trail", "town"]:
            if loc not in have:
                fail(f"no seats catalogued for {loc}")
    if not FAIL:
        print(
            f"PASS: {len(seats)} seats, seat-surface, visible torso"
            + ("/legs" if any(legs_required(s) for s in seats) else "")
            + ", occlusion and furniture-derived facing checks ok"
        )
        return 0
    print(f"FAIL: {len(FAIL)} problems on {len(FAIL_IDS)} seats: {', '.join(FAIL_IDS)}")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
