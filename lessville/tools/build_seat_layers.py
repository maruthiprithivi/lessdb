#!/usr/bin/env python3
"""Extract furniture-foreground PNGs from location backgrounds using config/seats.json."""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

from PIL import Image, ImageDraw

sys.path.insert(0, str(Path(__file__).resolve().parent))
import seatlib as sl


def extract_overlay(bg: Image.Image, ov: dict) -> Image.Image:
    fg = Image.new("RGBA", bg.size, (0, 0, 0, 0))
    mask = Image.new("L", bg.size, 0)
    d = ImageDraw.Draw(mask)
    if "ellipse" in ov:
        cx, cy, rx, ry = ov["ellipse"]
        d.ellipse([cx - rx, cy - ry, cx + rx, cy + ry], fill=255)
    for poly in ov.get("polygons", []):
        d.polygon([tuple(p) for p in poly], fill=255)
    src = bg.convert("RGBA")
    fg.paste(src, (0, 0), mask)
    return fg


def tight_crop_copy(im: Image.Image) -> Image.Image:
    """Keep full canvas size (so blit is at 0,0) — required for alignment."""
    return im


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--preview-dir", type=Path, default=None)
    args = ap.parse_args()
    cat = sl.load_catalog()
    sl.FG_DIR.mkdir(parents=True, exist_ok=True)
    loc_cache: dict[str, Image.Image] = {}
    written = []
    for name, ov in cat["overlays"].items():
        loc = ov["location"]
        loc_file = "town.png" if loc == "town" else f"{loc}.png"
        if loc not in loc_cache:
            loc_cache[loc] = Image.open(sl.LOC_DIR / loc_file).convert("RGBA")
        fg = extract_overlay(loc_cache[loc], ov)
        out = sl.FG_DIR / f"{name}.png"
        # Only write if there is at least one opaque pixel
        extrema = fg.getchannel("A").getextrema()
        if extrema[1] == 0:
            print(f"WARN empty overlay {name}", file=sys.stderr)
        fg.save(out)
        written.append(out)
        print(f"wrote {out.relative_to(sl.ROOT)} {fg.size} alpha_max={extrema[1]}")
    if args.preview_dir:
        args.preview_dir.mkdir(parents=True, exist_ok=True)
    print(f"overlays {len(written)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
