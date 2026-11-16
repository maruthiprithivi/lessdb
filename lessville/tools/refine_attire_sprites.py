#!/usr/bin/env python3
import sys
from pathlib import Path
from collections import defaultdict
from PIL import Image

sys.path.insert(0, "/workspace/lessville-godot-src/work-attire/tools")
import attirelib as al
import derive_attire_sprites as d

CHAR = Path("/workspace/lessville-godot-src/work-attire/assets/game/characters")
OUT = Path("/tmp/attire-refine")
OUT.mkdir(exist_ok=True)


def is_skin(p):
    r, g, b, a = p
    return a > 160 and r >= 220 and 110 <= g <= 200 and 40 <= b <= 160


def is_hair_color(p):
    r, g, b, a = p
    return a > 160 and 8 <= r <= 80 and 4 <= g <= 60 and 10 <= b <= 80 and r >= g


def reshape_maya_blouse(px, orig, w, h, palette):
    upper = (*al.snap((253, 244, 213), palette), 255)
    lower = (*al.snap((80, 77, 54), palette), 255)
    outline = (*al.snap((18, 10, 18), palette), 255)
    waist = 84
    apron = {
        (x, y)
        for y in range(46, 100)
        for x in range(w)
        if al.is_maya_apron(orig[x, y], x, y, w, h)
    }
    if not apron:
        return
    ax = [x for x, _ in apron]
    ay = [y for _, y in apron]
    x0, x1 = min(ax), max(ax)
    y0, y1 = min(ay), max(ay)
    extra = set()
    for y in range(max(48, y0 - 2), min(100, y1 + 3)):
        for x in range(max(0, x0 - 2), min(w, x1 + 3)):
            r, g, b, a = px[x, y]
            if a < 160 or is_skin(px[x, y]) or is_hair_color(px[x, y]):
                continue
            if al.is_maya_apron(px[x, y], x, y, w, h) or (r + g + b < 140 and r < 80 and y < 100):
                if any(d.original_transparent(orig, nx, ny, w, h) for nx, ny in d.neighbors(x, y)):
                    continue
                extra.add((x, y))
    for x, y in apron | extra:
        if is_skin(orig[x, y]) and y < 58:
            continue
        px[x, y] = upper if y < waist else lower
    for y in range(max(56, y0), waist):
        xs = []
        for x in range(w):
            r, g, b, a = px[x, y]
            if a < 160 or is_skin(px[x, y]) or is_hair_color(px[x, y]):
                continue
            if (x, y) in apron or (x, y) in extra or (r >= 200 and g >= 190 and b >= 160):
                xs.append(x)
        if len(xs) < 2:
            continue
        lo, hi = min(xs), max(xs)
        if any(d.original_transparent(orig, x, y, w, h) and (x, y) not in apron for x in range(lo, hi + 1)):
            if any(orig[x, y][3] < 32 and (x, y) not in apron for x in range(lo, hi + 1)):
                continue
        for x in range(lo, hi + 1):
            if is_skin(orig[x, y]) and y < 62:
                continue
            if is_hair_color(orig[x, y]):
                continue
            if d.original_transparent(orig, x, y, w, h) and (x, y) not in apron:
                continue
            if any(d.original_transparent(orig, nx, ny, w, h) for nx, ny in d.neighbors(x, y)):
                if px[x, y][3] > 160 and px[x, y][0] + px[x, y][1] + px[x, y][2] < 80:
                    continue
            px[x, y] = upper
    for y in range(46, 100):
        for x in range(w):
            if px[x, y][3] < 32:
                continue
            if any(d.original_transparent(orig, nx, ny, w, h) for nx, ny in d.neighbors(x, y)):
                if any(not (0 <= nx < w and 0 <= ny < h) or px[nx, ny][3] < 32 for nx, ny in d.neighbors(x, y)):
                    px[x, y] = outline


def paint_lin_hair(px, orig, w, h, cap_mask, palette, stem):
    fill = al.snap((38, 20, 27), palette)
    dark_h = al.snap((15, 8, 16), palette)
    outline = al.snap((7, 6, 13), palette)
    if not cap_mask:
        return
    profile = stem.endswith("_e") or stem.endswith("_w")
    facing_e = stem.endswith("_e")
    facing_w = stem.endswith("_w")
    skin = [(x, y) for y in range(h) for x in range(w) if orig[x, y][3] > 160 and is_skin(orig[x, y]) and y < 70]
    cap_ys = [y for _, y in cap_mask]
    top_y = min(cap_ys)
    brow_y = top_y + 12
    forehead_x = None
    if skin:
        skin_by_y = defaultdict(list)
        for x, y in skin:
            skin_by_y[y].append(x)
        for y in sorted(skin_by_y):
            if len(skin_by_y[y]) >= 4:
                brow_y = y
                forehead_x = max(skin_by_y[y]) if facing_e else (min(skin_by_y[y]) if facing_w else max(skin_by_y[y]))
                break
    crown_xs = [x for x, y in cap_mask if y <= top_y + 6]
    crown_min = min(crown_xs) if crown_xs else 0
    crown_max = max(crown_xs) if crown_xs else w

    for x, y in cap_mask:
        is_brim = False
        if profile and forehead_x is not None and y >= brow_y - 1:
            if facing_e and x >= forehead_x:
                is_brim = True
            if facing_w and x <= forehead_x:
                is_brim = True
        elif not profile:
            if x > crown_max + 1 or x < crown_min - 1:
                is_brim = True
        if is_brim:
            px[x, y] = (0, 0, 0, 0)
            continue
        edge = y <= top_y + 1
        rgb = dark_h if edge else fill
        px[x, y] = (rgb[0], rgb[1], rgb[2], 255)

    for y in range(0, brow_y):
        for x in range(w):
            if not is_skin(px[x, y]):
                continue
            if y >= brow_y - 1 and (x, y) not in cap_mask:
                continue
            if profile and forehead_x is not None:
                if facing_e and x >= forehead_x:
                    continue
                if facing_w and x <= forehead_x:
                    continue
            px[x, y] = (fill[0], fill[1], fill[2], 255)

    for y in range(0, 50):
        for x in range(w):
            p = px[x, y]
            leftover = d.teal_like(p) or (p[3] > 160 and p[0] > 200 and p[1] > 200 and p[2] > 170)
            if not leftover:
                continue
            near_hair = any(is_hair_color(px[nx, ny]) for nx, ny in d.neighbors8(x, y) if 0 <= nx < w and 0 <= ny < h)
            on_brim_side = False
            if profile and forehead_x is not None:
                on_brim_side = (facing_e and x >= forehead_x) or (facing_w and x <= forehead_x)
            if not profile and (x > crown_max + 1 or x < crown_min - 1):
                on_brim_side = True
            if near_hair and not on_brim_side:
                px[x, y] = (fill[0], fill[1], fill[2], 255)
            else:
                px[x, y] = (0, 0, 0, 0)

    # Front/back only: also drop visor *outlines* (dark pixels the cap mask missed)
    # in the visor band, so a brim ghost does not remain. Never run this on
    # profile frames — it would eat the face.
    if not profile:
        band_bot = top_y + 20
        for y in range(0, min(h, band_bot)):
            for x in range(w):
                if x > crown_max + 1 or x < crown_min - 1:
                    if px[x, y][3] >= 32:
                        px[x, y] = (0, 0, 0, 0)

    for y in range(0, 60):
        for x in range(w):
            if not is_hair_color(px[x, y]):
                continue
            if any(not (0 <= nx < w and 0 <= ny < h) or px[nx, ny][3] < 32 for nx, ny in d.neighbors(x, y)):
                px[x, y] = (outline[0], outline[1], outline[2], 255)


def clear_bag(px, orig, w, h, bag, palette):
    jacket = al.snap((26, 143, 146), palette)
    shirt = al.snap((252, 241, 216), palette)
    outline = al.snap((7, 6, 13), palette)
    body_px = {(x, y) for x, y in bag if al.is_lin_backpack(orig[x, y], x, y, w, h)}
    strap_px = set(bag) - body_px
    torso = [
        (x, y)
        for y in range(48, h)
        for x in range(w)
        if (x, y) not in bag and orig[x, y][3] > 128 and not d.dark(orig[x, y])
    ]
    if torso:
        tx0, tx1 = min(x for x, _ in torso), max(x for x, _ in torso)
    else:
        tx0, tx1 = 0, w
    comps = d.connected_components(lambda x, y: (x, y) in body_px, w, h)
    hang, onbody = set(), set()
    for comp in comps:
        if not comp:
            continue
        xs = [x for x, _ in comp]
        cx = sum(xs) / len(xs)
        outside = sum(1 for x, y in comp if x < tx0 or x > tx1 or d.original_transparent(orig, x, y, w, h))
        if cx < tx0 + 1 or cx > tx1 - 1 or outside >= 0.45 * len(comp):
            hang |= set(comp)
        else:
            onbody |= set(comp)
    for x, y in hang:
        px[x, y] = (0, 0, 0, 0)
    for x, y in onbody:
        px[x, y] = (*(shirt if y < 62 else jacket), 255)
    for x, y in strap_px:
        trans_n = sum(1 for nx, ny in d.neighbors8(x, y) if d.original_transparent(orig, nx, ny, w, h))
        body_n = any(
            0 <= nx < w and 0 <= ny < h and (nx, ny) not in bag and orig[nx, ny][3] > 128 and not d.dark(orig[nx, ny])
            for nx, ny in d.neighbors(x, y)
        )
        if trans_n >= 3 or not body_n:
            px[x, y] = (0, 0, 0, 0)
        else:
            px[x, y] = (*(shirt if y < 62 else jacket), 255)
    for x, y in bag:
        if px[x, y][3] < 32:
            continue
        if any(d.original_transparent(orig, nx, ny, w, h) for nx, ny in d.neighbors(x, y)):
            if any(0 <= nx < w and 0 <= ny < h and px[nx, ny][3] < 32 for nx, ny in d.neighbors(x, y)):
                px[x, y] = (outline[0], outline[1], outline[2], 255)


def snap_im(px, w, h, palette):
    off = 0
    for y in range(h):
        for x in range(w):
            r, g, b, a = px[x, y]
            if a >= 32 and (r, g, b) not in palette:
                rgb = al.snap((r, g, b), palette)
                px[x, y] = (rgb[0], rgb[1], rgb[2], a)
                off += 1
    return off


def main():
    pal_m = al.palette_of("maya")
    pal_l = al.palette_of("lin")
    # Maya: strip_apron writes into CHAR, then we reshape from original and save to OUT
    # To avoid clobbering CHAR until the end, copy strip result from a temp.
    # We'll call strip_apron (writes CHAR) then immediately overwrite CHAR at the end from OUT.
    for src in sorted(p for p in CHAR.glob("maya_*.png") if "_plain_" not in p.name and "_free_" not in p.name):
        orig_im = Image.open(src).convert("RGBA")
        orig = orig_im.load()
        w, h = orig_im.size
        d.strip_apron(src, "maya", pal_m, False)
        im = Image.open(CHAR / d.plain_name(src)).convert("RGBA")
        px = im.load()
        reshape_maya_blouse(px, orig, w, h, pal_m)
        off = snap_im(px, w, h, pal_m)
        dest = OUT / d.plain_name(src)
        im.save(dest)
        print(f"maya {src.name:22} off={off} det={al.detect_layers(im,'maya')}")
    for src in sorted(p for p in CHAR.glob("lin_*.png") if "_plain_" not in p.name and "_free_" not in p.name):
        im = Image.open(src).convert("RGBA")
        w, h = im.size
        orig_im = im.copy()
        orig = orig_im.load()
        px = im.load()
        cap = d.lin_cap_mask(orig, w, h)
        bag = d.lin_bag_mask(orig, w, h)
        paint_lin_hair(px, orig, w, h, cap, pal_l, src.stem)
        if bag:
            clear_bag(px, orig, w, h, bag, pal_l)
        off = snap_im(px, w, h, pal_l)
        dest = OUT / d.plain_name(src)
        im.save(dest)
        print(f"lin  {src.name:22} off={off} det={al.detect_layers(im,'lin')}")
    print("wrote", OUT)


if __name__ == "__main__":
    main()
