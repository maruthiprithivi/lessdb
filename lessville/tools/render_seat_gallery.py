#!/usr/bin/env python3
"""Per-scene seating gallery for BL-0148 step 1. Not a self-certification."""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

sys.path.insert(0, str(Path(__file__).resolve().parent))
import seatlib as sl
from check_seat_render import blit_sprite, apply_fg, load_bg

DEFAULT_OUT = Path("/workspace/shared/ai-hq/results/lessdb/evidence/BL-0148/gallery-step1-r3")
BEFORE_GALLERY = Path("/workspace/shared/ai-hq/results/lessdb/evidence/BL-0148/gallery-step1-r2")
FIXED_SEATS = [
    "town.cafe.table_b",
    "home.dining.east",
    "school.lecture.r1c0",
    "school.lecture.r1c1",
    "school.lecture.r1c2",
    "school.lecture.r2c0",
    "school.lecture.r2c1",
    "school.lecture.r2c2",
]
OUT = DEFAULT_OUT
AGENTS = ["maya", "sam", "lin", "ama", "bo"]
SCENE_FILE = {
    "office": "office", "home": "home", "cafe": "cafe", "grocery": "grocery",
    "school": "school", "gym": "gym", "trail": "trail", "bus": "bus", "town": "town",
}
SCENE_TITLE = {
    "office": "Office", "home": "Home", "cafe": "Café", "grocery": "FreshMart",
    "school": "University", "gym": "Gym", "trail": "Trail", "bus": "On the bus", "town": "Town",
}
# Clock labels and who is where. Standing spots come from interior STATIONS.
SCHEDULE = {
    "0730": {
        "label": "07:30",
        "minute": 450,
        "agents": [
            {"id": "ama", "name": "Ama", "location": "home", "station": "living", "pose": "seat", "activity": "Breakfast at home"},
            {"id": "sam", "name": "Sam", "location": "home", "station": "dining", "pose": "seat", "activity": "Morning coffee"},
            {"id": "maya", "name": "Maya", "location": "cafe", "station": "till", "pose": "stand", "activity": "Opening the café"},
            {"id": "bo", "name": "Bo", "location": "cafe", "station": "espresso", "pose": "stand", "activity": "Warming the machine"},
            {"id": "lin", "name": "Lin", "location": "bus", "station": "seat", "pose": "seat", "activity": "Riding to campus"},
        ],
    },
    "1230": {
        "label": "12:30",
        "minute": 750,
        "agents": [
            {"id": "sam", "name": "Sam", "location": "office", "station": "desk", "pose": "type", "activity": "Midday report work"},
            {"id": "lin", "name": "Lin", "location": "school", "station": "lecture", "pose": "seat", "activity": "Lecture"},
            {"id": "maya", "name": "Maya", "location": "cafe", "station": "till", "pose": "stand", "activity": "Lunch rush till"},
            {"id": "bo", "name": "Bo", "location": "cafe", "station": "espresso", "pose": "stand", "activity": "Pulling espresso shots"},
            {"id": "ama", "name": "Ama", "location": "cafe", "station": "table1", "pose": "seat", "activity": "Lunch at Less Café"},
        ],
    },
    "1830": {
        "label": "18:30",
        "minute": 1110,
        "agents": [
            {"id": "sam", "name": "Sam", "location": "office", "station": "desk", "pose": "type", "activity": "Finishing the quarterly report"},
            {"id": "ama", "name": "Ama", "location": "home", "station": "living", "pose": "seat", "activity": "Evening at home"},
            {"id": "maya", "name": "Maya", "location": "cafe", "station": "till", "pose": "stand", "activity": "Closing the till"},
            {"id": "bo", "name": "Bo", "location": "cafe", "station": "espresso", "pose": "stand", "activity": "Last coffees"},
            {"id": "lin", "name": "Lin", "location": "cafe", "station": "table2", "pose": "seat", "activity": "Studying at a table"},
        ],
    },
    "2200": {
        "label": "22:00",
        "minute": 1320,
        "agents": [
            {"id": "sam", "name": "Sam", "location": "home", "station": "dining", "pose": "seat", "activity": "Late dinner"},
            {"id": "ama", "name": "Ama", "location": "home", "station": "living", "pose": "seat", "activity": "Winding down"},
            {"id": "lin", "name": "Lin", "location": "home", "station": "dining", "pose": "seat", "activity": "Back at the house"},
            {"id": "maya", "name": "Maya", "location": "town", "station": "park_bench", "pose": "seat", "activity": "Sitting by the fountain"},
            {"id": "bo", "name": "Bo", "location": "town", "station": "bus_bench", "pose": "seat", "activity": "Waiting at the stop"},
        ],
    },
}

# Standing stations (mirrors interior_view.gd STATIONS, non-seat only)
STAND = {
    "home": {"kitchen": [(520, 178), (605, 188)]},
    "office": {"meeting": [(600, 405), (545, 420)], "coffee": [(300, 318)]},
    "cafe": {"till": [(610, 152)], "espresso": [(520, 148)], "fridge": [(668, 150)],
             "queue": [(520, 305), (455, 325), (585, 325)]},
    "grocery": {"aisle": [(330, 245), (240, 300), (560, 268)]},
    "gym": {"workout": [(290, 150), (390, 150)]},
    "trail": {"path": [(470, 476), (545, 424), (615, 372), (395, 496)]},
}


def assign(cat, agents):
    """Return list of {agent, location, feet, seat or None, pose}."""
    taken = {}
    out = []
    for a in agents:
        loc = a["location"]
        st = a["station"]
        key = (loc, st)
        n = taken.get(key, 0)
        taken[key] = n + 1
        placed = None
        seats = sl.seats_for(cat, loc, st)
        if seats:
            seat = seats[n % len(seats)]
            hip = (float(seat["hip"][0]), float(seat["hip"][1]))
            feet = sl.feet_from_hip(hip, cat, seat)
            placed = {"agent": a, "location": loc, "feet": feet, "seat": seat, "pose": seat.get("pose", "seat")}
        else:
            spots = STAND.get(loc, {}).get(st, [(384, 380)])
            feet = spots[n % len(spots)]
            placed = {"agent": a, "location": loc, "feet": (float(feet[0]), float(feet[1])), "seat": None, "pose": "stand"}
        out.append(placed)
    return out


def stand_rect(feet, cat, seat=None):
    # Standing still uses interior 1.2 unless town
    hip = (feet[0], feet[1] - sl.hip_offset_y(cat, seat or {"map": ""}))
    # For standing, hip_row is not right — feet-anchored at bottom of 112*scale
    w, h, scale, _hr = sl.sprite_params(cat, seat)
    dw, dh = w * scale, h * scale
    return (feet[0] - dw / 2, feet[1] - dh, feet[0] + dw / 2, feet[1])


def frame_for_placed(placed):
    a = placed["agent"]
    if placed["seat"]:
        return sl.frame_for(a["id"], placed["seat"])
    return f"{a['id']}_0.png"


def render_scene(cat, location, placed_here) -> tuple[Image.Image, list]:
    canvas = load_bg(location)
    # y-sort, two-pass around max fg_front_y
    def front_y(p):
        if p["seat"] and p["seat"].get("fg_front_y") is not None:
            return float(p["seat"]["fg_front_y"])
        return -1.0

    def rect_of(p):
        if p["seat"]:
            hip = (float(p["seat"]["hip"][0]), float(p["seat"]["hip"][1]))
            return sl.sprite_rect_from_hip(hip, cat, p["seat"])
        return stand_rect(p["feet"], cat, p["seat"])

    placed_here = sorted(placed_here, key=lambda p: p["feet"][1])
    crops = []

    def draw_one(p):
        fr = frame_for_placed(p)
        sprite = Image.open(sl.CHAR_DIR / fr).convert("RGBA")
        rect = rect_of(p)
        profile = fr.endswith("_e.png") or fr.endswith("_w.png")
        flip = bool(p["seat"] and p["seat"].get("facing") == "w" and not profile)
        blit_sprite(canvas, sprite, rect, flip)
        crops.append((p, rect))

    behind, front = [], []
    for p in placed_here:
        # Bound sitters always draw under their furniture overlay.
        if p["seat"] and p["seat"].get("fg"):
            behind.append(p)
            continue
        fy = front_y(p)
        if fy >= 0 and p["feet"][1] <= fy + 8:
            behind.append(p)
        else:
            front.append(p)
    for p in behind:
        draw_one(p)
    # overlays used by anyone in this scene, or all overlays for the location
    drawn = set()
    for p in placed_here:
        name = p["seat"].get("fg") if p["seat"] else None
        if name and name not in drawn:
            apply_fg(canvas, name)
            drawn.add(name)
    for p in front:
        draw_one(p)
    return canvas, crops


def crop4x(canvas: Image.Image, rect, pad=16) -> Image.Image:
    x0, y0, x1, y1 = [int(round(v)) for v in rect]
    x0, y0 = max(0, x0 - pad), max(0, y0 - pad)
    x1, y1 = min(canvas.size[0], x1 + pad), min(canvas.size[1], y1 + pad)
    c = canvas.crop((x0, y0, x1, y1)).convert("RGB")
    return c.resize((c.size[0] * 4, c.size[1] * 4), Image.Resampling.NEAREST)


def caption(im: Image.Image, text: str) -> Image.Image:
    bar = 28
    out = Image.new("RGB", (im.size[0], im.size[1] + bar), (18, 22, 30))
    out.paste(im.convert("RGB"), (0, 0))
    d = ImageDraw.Draw(out)
    d.text((8, im.size[1] + 8), text, fill=(242, 181, 74))
    return out


def main() -> int:
    global OUT
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    args = ap.parse_args()
    OUT = args.out
    OUT.mkdir(parents=True, exist_ok=True)
    cat = sl.load_catalog()
    OUT.mkdir(parents=True, exist_ok=True)
    index_lines = []
    index_lines.append("# BL-0148 step 1 gallery r3 — seat anchors and z-order")
    index_lines.append("")
    index_lines.append("For Chief of Staff review under G4. The builder does not self-certify these visuals.")
    index_lines.append("client-v1 remains **HELD**. No deploy. This gallery is seating only (acceptance point 1).")
    index_lines.append("")
    index_lines.append("Composites use the same location backgrounds, character sprites, hip row (78 of 112),")
    index_lines.append("draw scale (1.2 interior, ~0.916 town), seat catalog and furniture overlays as the Godot client.")
    index_lines.append("Each character crop is 4× nearest-neighbour. Yellow is not drawn on these shots.")
    index_lines.append("")
    index_lines.append("Please inspect every zoomed crop: hips on the seat, body facing the desk/table where the art allows,")
    index_lines.append("character in front of the chair back, and behind a table when the table is closer to the camera.")
    index_lines.append("Side-facing seats should use dedicated profile sit/type frames (`{id}_seat_e` / `_seat_w` / `_type_e` / `_type_w`).")
    index_lines.append("")

    # --- seat atlas: every catalog seat ---
    atlas = OUT / "atlas"
    atlas.mkdir(exist_ok=True)
    index_lines.append("## Seat atlas (every seat, reference typist/sitter = Sam)")
    index_lines.append("")
    index_lines.append("| Seat | Scene | Facing | Pose | Hip | Crop |")
    index_lines.append("| --- | --- | --- | --- | --- | --- |")
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from check_seat_render import composite_seat
    for seat in cat["seats"]:
        canvas, _char, _sprite, rect, _frame, _flip = composite_seat(cat, seat, "sam")
        crop = crop4x(canvas, rect, pad=24)
        fname = f"{seat['id']}.png"
        crop.save(atlas / fname)
        hip = seat["hip"]
        index_lines.append(
            f"| `{seat['id']}` | {SCENE_TITLE.get(seat['location'], seat['location'])} | {seat['facing']} | {seat['pose']} | ({hip[0]:.0f}, {hip[1]:.0f}) | [atlas/{fname}](atlas/{fname}) |"
        )
    index_lines.append("")

    # --- 4 times of day ---
    for key, slot in SCHEDULE.items():
        slot_dir = OUT / key
        slot_dir.mkdir(exist_ok=True)
        placed = assign(cat, slot["agents"])
        index_lines.append(f"## {slot['label']} ({key})")
        index_lines.append("")
        # group by location
        by_loc = {}
        for p in placed:
            by_loc.setdefault(p["location"], []).append(p)
        # Render all standard scenes
        for loc in ["town", "home", "office", "cafe", "grocery", "school", "gym", "trail", "bus"]:
            here = by_loc.get(loc, [])
            canvas, crops = render_scene(cat, loc, here)
            title = f"{SCENE_TITLE[loc]} · {slot['label']}"
            scene = caption(canvas.convert("RGB"), title + (f" · {len(here)} here" if here else " · empty"))
            scene_name = f"{loc}.png"
            scene.save(slot_dir / scene_name)
            index_lines.append(f"### {title}")
            index_lines.append(f"![scene]({key}/{scene_name})")
            index_lines.append("")
            if not crops:
                index_lines.append("_No one from Lessville is in this scene at this time._")
                index_lines.append("")
                continue
            index_lines.append("| Character | Station | Crop |")
            index_lines.append("| --- | --- | --- |")
            for p, rect in crops:
                a = p["agent"]
                cname = f"{loc}-{a['id']}@4x.png"
                crop4x(canvas, rect, pad=20).save(slot_dir / cname)
                st = p["seat"]["id"] if p["seat"] else p["agent"]["station"]
                index_lines.append(f"| {a['name']} · {a['activity']} | `{st}` | [{cname}]({key}/{cname}) |")
            index_lines.append("")

    # Before/after crops for seats fixed in step-1 attempt 3
    cmp = OUT / "compare"
    cmp.mkdir(exist_ok=True)
    index_lines.append("## Before / after (seats fixed in attempt 3)")
    index_lines.append("")
    index_lines.append("Left: crop from `gallery-step1-r2` (attempt 2). Right: the same seat after the attempt-3 seat-surface and z-order fixes.")
    index_lines.append("")
    index_lines.append("| Seat | Before (attempt 2) | After (attempt 3) |")
    index_lines.append("| --- | --- | --- |")
    for sid in FIXED_SEATS:
        src_before = BEFORE_GALLERY / "atlas" / f"{sid}.png"
        src_after = atlas / f"{sid}.png"
        b_name = f"{sid}-before.png"
        a_name = f"{sid}-after.png"
        if src_before.exists():
            Image.open(src_before).save(cmp / b_name)
            b_cell = f"![](compare/{b_name})"
        else:
            b_cell = "_missing from gallery-step1-r2_"
        if src_after.exists():
            Image.open(src_after).save(cmp / a_name)
            a_cell = f"![](compare/{a_name})"
        else:
            a_cell = "_missing_"
        index_lines.append(f"| `{sid}` | {b_cell} | {a_cell} |")
    index_lines.append("")

    # Seat-surface debug overlays for every catalog seat
    surf_dir = OUT / "surfaces"
    if surf_dir.exists() and any(surf_dir.glob("*.png")):
        index_lines.append("## Seat-surface masks (debug, all 36 seats)")
        index_lines.append("")
        index_lines.append("Green is the furniture-derived `seat_surface` (chair seat, bench top, stool, or log ridge) from the location art. Yellow is the catalog hip, which must lie inside that surface. These are for review, not a self-certification.")
        index_lines.append("")
        index_lines.append("| Seat | Surface overlay |")
        index_lines.append("| --- | --- |")
        for _s in sl.load_catalog()["seats"]:
            _fn = f"{_s['id']}.png"
            if (surf_dir / _fn).exists():
                index_lines.append(f"| `{_s['id']}` | ![](surfaces/{_fn}) |")
        index_lines.append("")

    # Compare the original defect crop if present
    before = Path("/workspace/shared/ai-hq/results/lessdb/evidence/BL-0132/screenshots/client-local/after/110-1752_office.png")
    if before.exists():
        cmp = OUT / "compare"
        cmp.mkdir(exist_ok=True)
        shot = Image.open(before)
        shot.crop((420, 230, 580, 430)).resize((640, 800), Image.Resampling.NEAREST).save(cmp / "sam-office-1752-before@4x.png")
        index_lines.append("## Original defect reference")
        index_lines.append("")
        index_lines.append("Crop from the client-v1 Office shot at 17:52 (Sam floating behind the center stool),")
        index_lines.append("for comparison with the 18:30 Office Sam crop above and `atlas/office.desk.center.png`.")
        index_lines.append("")
        index_lines.append("![before](compare/sam-office-1752-before@4x.png)")
        index_lines.append("")

    index_lines.append("## Automated check")
    index_lines.append("")
    index_lines.append("`tools/check_seat_render.py` (furniture-derived seat_surface, visible torso and legs bands, overlay occlusion cap, side-frame facing) is part of the Godot test suite and fails the build on any violation. This gallery is for Chief of Staff review; it is not a self-certification.")
    index_lines.append("")
    (OUT / "index.md").write_text("\n".join(index_lines) + "\n")
    print(f"wrote gallery under {OUT}")
    return 0



if __name__ == "__main__":
    raise SystemExit(main())
