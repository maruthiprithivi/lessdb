#!/usr/bin/env python3
"""Terrace sitter layers: furniture FG and window-glass must not cover a sitter's head/face.

12:30 pale bar: town.cafe_table_a FG west rim is drawn on top of Maya
(agent y <= cafe_front=510) and paints a 5 px wood strip on her cheek and torso.

18:30 forehead band: town_glass_mask.png, auto-derived, leaks onto the
courtyard (a smear at Maya's forehead and the table top). Dusk window
wash is drawn after agents, so the leak reads as a translucent pale band.

The chair in the town background is a separate empty seat, not the bar
on her silhouette. Overlays are fully opaque (not a premultiplied fringe).

Fail-first: current assets at f423a7e (before the a3 punch) fail for
table_a on both checks. After the punch they must pass. Lap/table
occlusion (source row >= 64) is allowed for the furniture FG.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
SEATS = ROOT / "config" / "seats.json"
CHAR = ROOT / "assets" / "game" / "characters"
FG_DIR = ROOT / "assets" / "game" / "seats" / "fg"
GLASS = ROOT / "assets" / "game" / "locations" / "town_glass_mask.png"

# town_view.gd _draw_agent at native map size (s=1): scale := 0.62 * s / 0.677
SCALE = 0.62 / 0.677
SPRITE = (96, 112)
HIP_ROW = 78
HEAD_ROW = 64  # chin / collar: rows above this are head and face

# Seat id -> default sitter sprite used on the terrace (Maya table_a, Sam table_b).
SITTERS = {
    "town.cafe.table_a": "maya_seat_e.png",
    "town.cafe.table_b": "sam_seat_w.png",
}


def _seats() -> list[dict]:
    data = json.loads(SEATS.read_text())
    seats = data["seats"] if isinstance(data, dict) else data
    return [s for s in seats if s.get("id") in SITTERS]


def _dest(hip: tuple[int, int]) -> tuple[int, int, int, int, int, int]:
    sw = int(round(SPRITE[0] * SCALE))
    sh = int(round(SPRITE[1] * SCALE))
    feet_x = hip[0]
    feet_y = hip[1] + (SPRITE[1] - HIP_ROW) * SCALE
    ox = int(round(feet_x - sw / 2))
    oy = int(round(feet_y - sh))
    return ox, oy, sw, sh, feet_x, int(round(feet_y))


def _src_row(sy: int) -> int:
    return min(SPRITE[1] - 1, max(0, int(sy / SCALE)))


def _overlaps(sprite: Image.Image, ox: int, oy: int, sw: int, sh: int, layer: Image.Image, max_row: int) -> list[tuple[int, int, int]]:
    hits: list[tuple[int, int, int]] = []
    lw, lh = layer.size
    for sy in range(sh):
        row = _src_row(sy)
        if row >= max_row:
            continue
        for sx in range(sw):
            if sprite.getpixel((sx, sy))[3] < 32:
                continue
            gx, gy = ox + sx, oy + sy
            if 0 <= gx < lw and 0 <= gy < lh and layer.getpixel((gx, gy))[3] >= 32:
                hits.append((gx, gy, row))
    return hits


def main() -> int:
    seats = _seats()
    if not seats:
        print("FAIL: town cafe terrace seats not found in seats.json")
        return 1
    glass = Image.open(GLASS).convert("RGBA")
    fails = 0
    checks = 0
    print("Terrace sitter layer check")
    print(f"scale={SCALE:.6f} head_row<{HEAD_ROW}")
    for seat in seats:
        sid = seat["id"]
        hip = (int(seat["hip"][0]), int(seat["hip"][1]))
        sprite_name = SITTERS[sid]
        path = CHAR / sprite_name
        if not path.exists():
            print(f"[FAIL] {sid}: missing sprite {sprite_name}")
            fails += 1
            continue
        ox, oy, sw, sh, fx, fy = _dest(hip)
        sprite = Image.open(path).convert("RGBA").resize((sw, sh), Image.Resampling.NEAREST)
        fg_name = str(seat.get("fg", ""))
        fg_path = FG_DIR / f"{fg_name}.png"
        print(f"\n{sid} hip={hip} feet=({fx},{fy}) dest=({ox},{oy},{ox+sw},{oy+sh}) sprite={sprite_name}")

        checks += 1
        if not fg_path.exists():
            print(f"  [FAIL] missing FG {fg_path}")
            fails += 1
        else:
            fg = Image.open(fg_path).convert("RGBA")
            hits = _overlaps(sprite, ox, oy, sw, sh, fg, HEAD_ROW)
            # Also flag a vertical bar on the torso (rows 64-77) — the 12:30 pale strip.
            torso = _overlaps(sprite, ox, oy, sw, sh, fg, HIP_ROW)
            torso_only = [h for h in torso if h[2] >= HEAD_ROW]
            ok = (len(hits) == 0 and len(torso_only) == 0)
            status = "PASS" if ok else "FAIL"
            if not ok:
                fails += 1
            print(
                f"  [{status}] FG {fg_name}: head_hits={len(hits)} torso_bar={len(torso_only)}"
                + (f" head_rows={sorted({h[2] for h in hits})}" if hits else "")
                + (f" torso_rows={sorted({h[2] for h in torso_only})}" if torso_only else "")
            )

        checks += 1
        g_hits = _overlaps(sprite, ox, oy, sw, sh, glass, HEAD_ROW)
        ok = len(g_hits) == 0
        status = "PASS" if ok else "FAIL"
        if not ok:
            fails += 1
        print(
            f"  [{status}] glass-mask on head: hits={len(g_hits)}"
            + (f" rows={sorted({h[2] for h in g_hits})} bbox="
               f"{min(h[0] for h in g_hits)},{min(h[1] for h in g_hits)}-"
               f"{max(h[0] for h in g_hits)},{max(h[1] for h in g_hits)}" if g_hits else "")
        )

    if fails:
        print(f"\nFAIL: {fails} of {checks} terrace sitter-layer checks")
        return 1
    print(f"\nPASS: {checks} terrace sitter-layer checks (FG and glass clear of heads)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
