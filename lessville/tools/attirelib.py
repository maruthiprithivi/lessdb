#!/usr/bin/env python3
"""Shared Lessville attire model and layer detectors (BL-0148 step 3)."""
from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
MODEL_PATH = ROOT / "config" / "attire.json"
CHAR_DIR = ROOT / "assets" / "game" / "characters"

LAYER_KEYS = ("apron", "cap", "backpack", "towel")

# Sprite row bands (native 96x112). Cap lives in the head; apron/bag in the torso.
BAND_HEAD = (0, 48)
BAND_TORSO = (50, 96)


def load_model(path: Path | None = None) -> dict:
    return json.loads((path or MODEL_PATH).read_text())


def minute_of_day(clock: dict[str, Any] | None, fallback: int | None = None) -> int:
    clock = clock or {}
    if "minute_of_day" in clock:
        return int(clock["minute_of_day"]) % 1440
    if fallback is not None:
        return int(fallback) % 1440
    if "minute" in clock:
        return int(clock["minute"]) % 1440
    return 0


def _is_terrace(station: str, model: dict) -> bool:
    s = (station or "").strip().lower()
    if not s:
        return False
    for t in model.get("terrace_stations", []):
        if s == t.lower() or s.endswith("." + t.lower()) or t.lower().endswith(s):
            return True
    if "cafe" in s and "table" in s:
        return True
    return False


def layers_for(agent: dict[str, Any], clock: dict[str, Any] | None = None, model: dict | None = None) -> dict[str, bool]:
    """Expected outfit layers for a character at a place, activity and time."""
    model = model or load_model()
    cid = str(agent.get("id") or agent.get("character") or "")
    chars = model.get("characters", {})
    if cid not in chars:
        return {k: False for k in LAYER_KEYS}
    spec = chars[cid]
    layers = {k: bool(spec.get("base_layers", {}).get(k, False)) for k in LAYER_KEYS}
    loc = str(agent.get("location") or "")
    station = str(agent.get("station") or "")
    travel = agent.get("travel")
    travelling = isinstance(travel, dict) and bool(travel)
    md = minute_of_day(clock, agent.get("minute"))

    if cid in ("maya", "bo"):
        shift = spec.get("shift") or {}
        start = int(shift.get("start_minute", 0))
        end = int(shift.get("end_minute", 0))
        on_shift = start <= md <= end
        at_terrace = loc == "town" and _is_terrace(station, model)
        if on_shift and (loc == "cafe" or at_terrace):
            for k, v in (spec.get("work_layers") or {}).items():
                if k in layers:
                    layers[k] = bool(v)

    if cid == "lin":
        indoor = loc in set(model.get("indoor_settled_locations", []))
        settled = indoor and not travelling
        if not settled:
            for k, v in (spec.get("commute_layers") or {}).items():
                if k in layers:
                    layers[k] = bool(v)

    return layers


def agent_from_case(case: dict) -> tuple[dict, dict]:
    agent = {
        "id": case["character"],
        "location": case.get("location", ""),
        "station": case.get("station", ""),
        "activity": case.get("activity", ""),
        "activity_kind": case.get("activity_kind", ""),
        "travel": case.get("travel"),
        "minute": case.get("minute", 0),
    }
    clock = {
        "minute_of_day": int(case.get("minute", 0)) % 1440,
        "minute": int(case.get("minute", 0)),
        "label": case.get("label", ""),
    }
    return agent, clock


# ---------------------------------------------------------------------------
# Pixel detectors. Spatial first (head vs torso) so Lin's teal jacket is not
# counted as a cap, and Maya's hair scrunchie is not counted as an apron.
# ---------------------------------------------------------------------------

def _px(im: Image.Image) -> tuple[Image.Image, Any, int, int]:
    im = im.convert("RGBA")
    return im, im.load(), im.width, im.height


def _count(im: Image.Image, pred, band: tuple[int, int] | None = None) -> int:
    im, px, w, h = _px(im)
    y0, y1 = band if band else (0, h)
    y0 = max(0, y0)
    y1 = min(h, y1)
    n = 0
    for y in range(y0, y1):
        for x in range(w):
            if pred(px[x, y], x, y, w, h):
                n += 1
    return n


def is_maya_apron(p, x, y, w, h) -> bool:
    r, g, b, a = p
    if a < 160 or y < 50:
        return False
    # Rust bib / straps: darker than peach skin, not cream blouse.
    if r >= 140 and 35 <= g <= 160 and 20 <= b <= 130 and r > g + 40 and r > b + 50:
        if r >= 235 and g >= 125:  # skin
            return False
        if g >= 180 and b >= 150:  # blouse
            return False
        return True
    # Cream pocket stitched on the bib (yellower/darker than the blouse).
    if 70 <= y <= 92 and r >= 220 and 170 <= g <= 235 and 130 <= b <= 200 and (g - b) >= 20 and g <= 235:
        return True
    return False


def is_bo_apron(p, x, y, w, h) -> bool:
    r, g, b, a = p
    if a < 160 or y < 50:
        return False
    # Teal/green bib and straps. Navy cap is above the torso and has B >> G.
    if g >= 40 and b >= 40 and g >= r + 15 and b >= r + 15 and abs(g - b) <= 36:
        return True
    return False


def is_bo_towel(p, x, y, w, h) -> bool:
    r, g, b, a = p
    if a < 160 or y < 50 or y > 88:
        return False
    # Near-white towel. Shirt is yellow (B much lower); shoes sit below y=88.
    if r > 200 and g > 190 and b > 170 and max(r, g, b) - min(r, g, b) < 55:
        return True
    return False


def is_lin_cap(p, x, y, w, h) -> bool:
    r, g, b, a = p
    # Head band only — the jacket uses the same teal in the torso.
    if a < 160 or y >= 48:
        return False
    # Teal crown / brim. Seated back-of-cap frames use a very dark teal (R as low as 1).
    if r <= 90 and 70 <= g <= 200 and 70 <= b <= 200 and abs(g - b) <= 36 and g > r + 25:
        return True
    # White cap logo, only in the upper crown.
    if y < 36 and r > 200 and g > 200 and b > 170 and abs(r - g) < 30:
        return True
    return False


def is_lin_backpack(p, x, y, w, h) -> bool:
    r, g, b, a = p
    if a < 160 or y < 48:
        return False
    # Mustard/gold bag body (not peach skin: bag has a much larger G-B).
    if r >= 180 and 90 <= g <= 210 and b <= 110 and (g - b) >= 80:
        return True
    # Tan/brown bag shadow.
    if 90 <= r <= 200 and 50 <= g <= 160 and 20 <= b <= 110 and (g - b) >= 30 and r > g + 10:
        return True
    return False


def detect_layers(im: Image.Image, character: str, thresholds: dict[str, int] | None = None) -> dict[str, bool]:
    """Detect baked outfit layers in a 96x112 sprite or a cropped character render."""
    thresholds = thresholds or {}
    cid = character
    out = {k: False for k in LAYER_KEYS}
    if cid == "maya":
        out["apron"] = _count(im, is_maya_apron, BAND_TORSO) >= thresholds.get("apron", 40)
    elif cid == "bo":
        out["apron"] = _count(im, is_bo_apron, BAND_TORSO) >= thresholds.get("apron", 40)
        out["towel"] = _count(im, is_bo_towel, BAND_TORSO) >= thresholds.get("towel", 20)
        # Cap is navy and stays on; we still record it when the head is visible.
        # A simple navy-in-head count, not required for the known defects.
        out["cap"] = _count_navy_cap(im) >= thresholds.get("cap", 25)
    elif cid == "lin":
        out["cap"] = _count(im, is_lin_cap, BAND_HEAD) >= thresholds.get("cap", 30)
        out["backpack"] = _count(im, is_lin_backpack, BAND_TORSO) >= thresholds.get("backpack", 20)
    return out


def _count_navy_cap(im: Image.Image) -> int:
    def pred(p, x, y, w, h):
        r, g, b, a = p
        if a < 160 or y >= 40:
            return False
        # Navy: B dominant, mid, not teal (teal has G≈B and G high).
        if b >= 60 and b > r + 20 and b > g + 10 and g < 90 and r < 80:
            return True
        return False
    return _count(im, pred, (0, 40))


def counts(im: Image.Image, character: str) -> dict[str, int]:
    cid = character
    if cid == "maya":
        return {"apron": _count(im, is_maya_apron, BAND_TORSO)}
    if cid == "bo":
        return {
            "apron": _count(im, is_bo_apron, BAND_TORSO),
            "towel": _count(im, is_bo_towel, BAND_TORSO),
            "cap": _count_navy_cap(im),
        }
    if cid == "lin":
        return {
            "cap": _count(im, is_lin_cap, BAND_HEAD),
            "backpack": _count(im, is_lin_backpack, BAND_TORSO),
        }
    return {}


def palette_of(character: str) -> set[tuple[int, int, int]]:
    colors: set[tuple[int, int, int]] = set()
    for p in CHAR_DIR.glob(f"{character}_*.png"):
        if "_plain_" in p.name:
            continue
        im = Image.open(p).convert("RGBA")
        for r, g, b, a in im.getdata():
            if a >= 32:
                colors.add((r, g, b))
    return colors


def off_palette_count(im: Image.Image, palette: set[tuple[int, int, int]]) -> int:
    n = 0
    for r, g, b, a in im.convert("RGBA").getdata():
        if a >= 32 and (r, g, b) not in palette:
            n += 1
    return n


def snap(rgb: tuple[int, int, int], palette: set[tuple[int, int, int]]) -> tuple[int, int, int]:
    if rgb in palette:
        return rgb
    r, g, b = rgb
    best = None
    best_d = 1 << 30
    for pr, pg, pb in palette:
        d = (r - pr) * (r - pr) + (g - pg) * (g - pg) + (b - pb) * (b - pb)
        if d < best_d:
            best_d = d
            best = (pr, pg, pb)
    return best if best else rgb
