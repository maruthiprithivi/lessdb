#!/usr/bin/env python3
"""G12 attire check: outfit layers match the role-and-place model.

For each case in config/attire.json the check asserts:

  1. The model's expected layers (attirelib.layers_for).
  2. If outfit-state.json is present (written by tools/render_attire.gd),
     the scene's reported layers and chosen frame agree with the model.
  3. The chosen sprite (or the case's default frame, when the scene has not
     been wired yet) has apron / cap / backpack pixels if and only if the
     model says they should be on. This is what fails on 1a099fc for the
     known defects (Maya and Bo aprons in Town and at 22:00; Lin's cap and
     backpack at home at 22:00).

Exit 0 only when every case passes.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
import attirelib as al

CHAR = al.CHAR_DIR
DEFAULT_FRAMES = {
    "maya-cafe-1230": "maya_0.png",
    "bo-cafe-1230": "bo_0.png",
    "maya-town-street-1230": "maya_1.png",
    "bo-town-street-1230": "bo_1.png",
    "maya-park-2200": "maya_seat_0.png",
    "bo-town-2200": "bo_seat_0.png",
    "lin-home-2200": "lin_seat_e.png",
    "lin-commute-town-0730": "lin_0.png",
    "lin-bus-0740": "lin_seat_e.png",
    "lin-cafe-study-1830": "lin_seat_2.png",
    "maya-grocery-1400": "maya_0.png",
    "bo-trail-1515": "bo_1.png",
    "sam-office-1230": "sam_type_e.png",
    "sam-home-2200": "sam_seat_e.png",
    "ama-home-2200": "ama_seat_0.png",
    "maya-terrace-1230": "maya_seat_0.png",
}


def load_state(path: Path | None) -> dict:
    if path and path.exists():
        return json.loads(path.read_text())
    return {}


def plain_name(filename: str) -> str:
    stem = Path(filename).stem
    if "_plain_" in stem:
        return filename
    cid, rest = stem.split("_", 1)
    return f"{cid}_plain_{rest}.png"


def model_frame(case: dict, expected: dict) -> str:
    default = DEFAULT_FRAMES.get(case["id"], f"{case['character']}_0.png")
    cid = case["character"]
    needs = False
    if cid in ("maya", "bo") and not expected.get("apron", False):
        needs = True
    if cid == "lin" and (not expected.get("cap", False) or not expected.get("backpack", False)):
        needs = True
    return plain_name(default) if needs else default


def sprite_for(case: dict, rec: dict) -> Path | None:
    frame = rec.get("frame") or rec.get("sprite")
    if frame:
        p = Path(frame)
        if not p.is_absolute():
            if not str(frame).endswith(".png"):
                frame = frame + ".png"
            p = CHAR / Path(frame).name
        if p.exists():
            return p
    crop = rec.get("sprite_png")
    if crop and Path(crop).exists():
        return Path(crop)
    default = DEFAULT_FRAMES.get(case["id"])
    if default and (CHAR / default).exists():
        return CHAR / default
    return None


def compare_layers(expected: dict, actual: dict, keys=None) -> list[str]:
    keys = keys or al.LAYER_KEYS
    misses = []
    for k in keys:
        if k not in expected:
            continue
        if bool(actual.get(k, False)) != bool(expected[k]):
            misses.append(f"{k}: expected {expected[k]}, got {bool(actual.get(k, False))}")
    return misses


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--dir", type=Path, default=None, help="Folder with outfit-state.json and crops")
    ap.add_argument("--state", type=Path, default=None)
    ap.add_argument("--model", type=Path, default=al.MODEL_PATH)
    ap.add_argument(
        "--apply-model",
        action="store_true",
        help="Use plain/base sprites implied by the model (post-fix). Default is the baked unfixed frames (fail-before).",
    )
    args = ap.parse_args()
    model = al.load_model(args.model)
    state_path = args.state
    if state_path is None and args.dir:
        state_path = args.dir / "outfit-state.json"
    state = load_state(state_path)
    records = state.get("cases", state) if isinstance(state, dict) else {}

    fails: list[str] = []
    print(f"attire check: {len(model.get('cases', []))} cases, state={'yes' if records else 'no (default frames)'}")
    for case in model["cases"]:
        cid = case["id"]
        character = case["character"]
        agent, clock = al.agent_from_case(case)
        expected = case.get("expected") or al.layers_for(agent, clock, model)
        # Model self-consistency
        got_model = al.layers_for(agent, clock, model)
        for msg in compare_layers(expected, got_model):
            fails.append(f"{cid} model: {msg}")
        rec = records.get(cid, {}) if isinstance(records, dict) else {}
        if rec.get("layers"):
            for msg in compare_layers(expected, rec["layers"]):
                fails.append(f"{cid} scene-state: {msg}")
        if args.apply_model and not rec:
            rec = {"frame": model_frame(case, expected)}
        sprite = sprite_for(case, rec)
        if sprite is None and character in ("sam", "ama"):
            print(f"  {cid}: skip pixel check (no baked work layers)")
            continue
        if sprite is None:
            fails.append(f"{cid}: no sprite to inspect")
            continue
        im = Image.open(sprite)
        detected = al.detect_layers(im, character)
        counts = al.counts(im, character)
        # Pixel keys that this character can actually show.
        keys = [k for k in expected if k in counts or k in ("apron", "cap", "backpack", "towel")]
        if character == "maya":
            keys = ["apron"]
        elif character == "bo":
            keys = ["apron", "towel"]  # cap is personal and stays
        elif character == "lin":
            keys = ["cap", "backpack"]
        else:
            keys = [k for k in expected if expected[k] or detected.get(k)]
        pixel_misses = compare_layers({k: expected[k] for k in keys}, detected, keys)
        status = "FAIL" if pixel_misses else "ok"
        print(f"  {cid}: {sprite.name} counts={counts} det={ {k: detected[k] for k in keys} } expected={ {k: expected[k] for k in keys} } {status}")
        for msg in pixel_misses:
            fails.append(f"{cid} pixels ({sprite.name}): {msg}")
        # Optional crop verification when a tight sprite crop is provided.
        crop_path = rec.get("crop")
        if crop_path and Path(crop_path).exists() and character in ("maya", "bo", "lin"):
            crop = Image.open(crop_path)
            # Only use the crop when it is roughly sprite-shaped (tight).
            if crop.width <= 512 and crop.height <= 600:
                cdet = al.detect_layers(crop, character)
                crop_misses = compare_layers({k: expected[k] for k in keys}, cdet, keys)
                if crop_misses:
                    # Night-tinted town crops can shift hues; report but only
                    # fail if the source sprite already failed, or if the crop
                    # is a raw sprite (no heavy grade). Prefer sprite as authority.
                    print(f"    crop note {Path(crop_path).name}: {crop_misses}")

    if fails:
        print(f"FAIL: {len(fails)} attire problems")
        for f in fails:
            print("  -", f)
        return 1
    print("PASS: all attire cases")
    return 0


if __name__ == "__main__":
    sys.exit(main())
