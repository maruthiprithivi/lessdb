#!/usr/bin/env python3
"""Shared seat catalog math. Must stay in lockstep with scripts/lessville/seat_catalog.gd."""
from __future__ import annotations

import json
from pathlib import Path
from typing import Iterable

ROOT = Path(__file__).resolve().parents[1]
CATALOG_PATH = ROOT / "config" / "seats.json"
LOC_DIR = ROOT / "assets" / "game" / "locations"
CHAR_DIR = ROOT / "assets" / "game" / "characters"
FG_DIR = ROOT / "assets" / "game" / "seats" / "fg"


def load_catalog(path: Path = CATALOG_PATH) -> dict:
    return json.loads(path.read_text())


# Must match WorldScale.TOWN_ACTOR; the seat contact calibration is unchanged.
TOWN_SCALE = 0.62 / 0.677


def sprite_params(cat: dict, seat: dict | None = None) -> tuple[float, float, float, float]:
    sp = cat["sprite"]
    scale = float(sp["scale"])
    if seat and str(seat.get("map", "")) == "town":
        scale = TOWN_SCALE
    return float(sp["width"]), float(sp["height"]), scale, float(sp["hip_row"])


def hip_offset_y(cat: dict, seat: dict | None = None) -> float:
    _w, h, scale, hip_row = sprite_params(cat, seat)
    return (h - hip_row) * scale


def feet_from_hip(hip: tuple[float, float], cat: dict, seat: dict | None = None) -> tuple[float, float]:
    return hip[0], hip[1] + hip_offset_y(cat, seat)


def sprite_rect_from_hip(hip: tuple[float, float], cat: dict, seat: dict | None = None) -> tuple[float, float, float, float]:
    """Return (x0, y0, x1, y1) of the drawn sprite, hip-anchored."""
    w, h, scale, hip_row = sprite_params(cat, seat)
    dw, dh = w * scale, h * scale
    x0 = hip[0] - dw / 2
    y0 = hip[1] - hip_row * scale
    return x0, y0, x0 + dw, y0 + dh


def point_in_mask(x: float, y: float, mask: dict) -> bool:
    t = mask.get("type", "rect")
    if t == "rect":
        x0, y0, x1, y1 = mask["xyxy"]
        return x0 <= x <= x1 and y0 <= y <= y1
    if t == "polygon":
        return _pnpoly(x, y, mask["points"])
    if t == "ellipse":
        cx, cy, rx, ry = mask["xyxy"] if "xyxy" in mask else mask["ellipse"]
        if rx == 0 or ry == 0:
            return False
        return ((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2 <= 1.0
    raise ValueError(f"unknown mask type {t}")


def _pnpoly(x: float, y: float, pts: list) -> bool:
    inside = False
    n = len(pts)
    j = n - 1
    for i in range(n):
        xi, yi = pts[i]
        xj, yj = pts[j]
        if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi + 1e-9) + xi:
            inside = not inside
        j = i
    return inside


def point_in_overlay(x: float, y: float, ov: dict) -> bool:
    if "ellipse" in ov:
        cx, cy, rx, ry = ov["ellipse"]
        if rx == 0 or ry == 0:
            return False
        return ((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2 <= 1.0
    for poly in ov.get("polygons", []):
        if _pnpoly(x, y, poly):
            return True
    return False


def seats_for(cat: dict, location: str, station: str | None = None) -> list[dict]:
    out = [s for s in cat["seats"] if s["location"] == location]
    if station is not None:
        out = [s for s in out if s["station"] == station]
    out.sort(key=lambda s: s["index"])
    return out


def all_locations(cat: dict) -> list[str]:
    return sorted({s["location"] for s in cat["seats"]})


def side_frame_candidates(agent_id: str, facing: str, pose: str) -> list[str]:
    """Preferred profile frames for an east/west seat."""
    out = []
    if pose == "type":
        out += [f"{agent_id}_type_{facing}.png", f"{agent_id}_typing_{facing}.png"]
    out += [f"{agent_id}_seat_{facing}.png", f"{agent_id}_type_{facing}.png"]
    # unique, preserve order
    seen = set()
    uniq = []
    for c in out:
        if c not in seen:
            seen.add(c)
            uniq.append(c)
    return uniq


def frame_for(agent_id: str, seat: dict) -> str:
    """Pick a sprite key for a seated/typing agent at this seat."""
    facing = seat.get("facing", "s")
    pose = seat.get("pose", "seat")
    if facing in ("e", "w"):
        for cand in side_frame_candidates(agent_id, facing, pose):
            if (CHAR_DIR / cand).exists():
                return cand
    if pose == "type" or facing == "n":
        typ = f"{agent_id}_typing_0.png"
        if (CHAR_DIR / typ).exists():
            return typ
        back = f"{agent_id}_back_0.png"
        if facing == "n" and (CHAR_DIR / back).exists() and pose == "type":
            # Prefer a sit frame if present; back is standing.
            sit = f"{agent_id}_seat_0.png"
            if (CHAR_DIR / sit).exists() and agent_id != "sam":
                return sit
        if pose == "type" and (CHAR_DIR / f"{agent_id}_seat_0.png").exists():
            return f"{agent_id}_seat_0.png"
        return back if (CHAR_DIR / back).exists() else f"{agent_id}_0.png"
    sit = f"{agent_id}_seat_0.png"
    return sit if (CHAR_DIR / sit).exists() else f"{agent_id}_0.png"


def point_in_surface(x: float, y: float, surface: dict) -> bool:
    """True if (x, y) lies inside a furniture-derived seat_surface mask."""
    if not surface:
        return False
    typ = surface.get("type")
    if typ == "rect":
        x0, y0, x1, y1 = surface["xyxy"]
        return x0 <= x <= x1 and y0 <= y <= y1
    if typ == "polygon":
        pts = surface["points"]
        n = len(pts)
        if n < 3:
            return False
        # Even-odd ray cast. Edges are inclusive via a small tolerance on y.
        odd = False
        for i in range(n):
            x1, y1 = pts[i]
            x2, y2 = pts[(i + 1) % n]
            if y1 == y2:
                if y == y1 and min(x1, x2) <= x <= max(x1, x2):
                    return True
                continue
            crosses = (y1 > y) != (y2 > y) or y == y1 or y == y2
            if not crosses:
                continue
            xint = x1 + (x2 - x1) * (y - y1) / (y2 - y1)
            if abs(x - xint) < 1e-6:
                return True
            if x < xint and (y1 > y) != (y2 > y):
                odd = not odd
        return odd
    return False
