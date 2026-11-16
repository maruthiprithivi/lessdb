#!/usr/bin/env python3
"""Debug overlays: each seat_surface drawn on the location art, hip marked."""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

sys.path.insert(0, str(Path(__file__).resolve().parent))
import seatlib as sl

DEFAULT_OUT = Path("/workspace/shared/ai-hq/results/lessdb/evidence/BL-0148/gallery-step1-r3/surfaces")


def loc_image(name: str) -> Image.Image:
    fn = "town.png" if name == "town" else f"{name}.png"
    return Image.open(sl.LOC_DIR / fn).convert("RGBA")


def draw_surface(d: ImageDraw.ImageDraw, surface: dict, fill, outline) -> None:
    typ = surface.get("type")
    if typ == "rect":
        x0, y0, x1, y1 = surface["xyxy"]
        d.rectangle([x0, y0, x1, y1], fill=fill, outline=outline)
    elif typ == "polygon":
        d.polygon([tuple(p) for p in surface["points"]], fill=fill, outline=outline)


def render_one(bg: Image.Image, seat: dict, scale: int = 3) -> Image.Image:
    hx, hy = int(seat["hip"][0]), int(seat["hip"][1])
    surface = seat["seat_surface"]
    # pad around hip and surface
    xs, ys = [hx], [hy]
    if surface["type"] == "rect":
        x0, y0, x1, y1 = surface["xyxy"]
        xs += [x0, x1]; ys += [y0, y1]
    else:
        for x, y in surface["points"]:
            xs.append(x); ys.append(y)
    pad = 48
    box = (
        max(0, min(xs) - pad),
        max(0, min(ys) - pad),
        min(bg.size[0], max(xs) + pad),
        min(bg.size[1], max(ys) + pad),
    )
    overlay = bg.crop(box).convert("RGBA")
    # draw surface in the crop
    layer = Image.new("RGBA", overlay.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    # translate
    def tx(p):
        return (p[0] - box[0], p[1] - box[1])
    if surface["type"] == "rect":
        x0, y0, x1, y1 = surface["xyxy"]
        d.rectangle([tx((x0, y0)), tx((x1, y1))], fill=(0, 220, 80, 90), outline=(0, 255, 80, 255))
    else:
        d.polygon([tx(tuple(p)) for p in surface["points"]], fill=(0, 220, 80, 90), outline=(0, 255, 80, 255))
    hx2, hy2 = tx((hx, hy))
    d.ellipse([hx2 - 2, hy2 - 2, hx2 + 2, hy2 + 2], fill=(255, 220, 0, 255), outline=(255, 80, 0, 255))
    overlay = Image.alpha_composite(overlay, layer)
    big = overlay.resize((overlay.size[0] * scale, overlay.size[1] * scale), Image.Resampling.NEAREST)
    try:
        font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 13)
    except Exception:
        font = ImageFont.load_default()
    out = Image.new("RGB", (big.size[0], big.size[1] + 22), (24, 24, 28))
    out.paste(big.convert("RGB"), (0, 0))
    ImageDraw.Draw(out).text(
        (6, big.size[1] + 4),
        f"{seat['id']}  hip=({hx},{hy})  on_surface={sl.point_in_surface(hx, hy, surface)}",
        fill=(230, 230, 230),
        font=font,
    )
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    cat = sl.load_catalog()
    cache = {}
    for s in cat["seats"]:
        loc = s["location"]
        if loc not in cache:
            cache[loc] = loc_image(loc)
        im = render_one(cache[loc], s)
        dest = args.out / f"{s['id']}.png"
        im.save(dest)
        print("wrote", dest.name, im.size)
    print(f"surfaces {len(cat['seats'])} -> {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
