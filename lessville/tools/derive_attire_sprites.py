#!/usr/bin/env python3
"""Derive off-duty / at-home character frames from the approved pixel art.

The approved art bakes work and commute gear into every frame:

* Maya and Bo always wear a café apron (Bo also a white bar towel).
* Lin always wears a teal cap and a mustard backpack.

This tool leaves every original frame untouched (they remain the on-duty
and commuting variants) and writes `<id>_plain_<rest>.png` siblings with
those layers removed. Fill colours and outlines are snapped to each
character's existing palette, so the output has 0 off-palette pixels.

Deterministic: the same input pixels always give the same output.
"""
from __future__ import annotations

import sys
from collections import Counter, deque
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
CHAR = ROOT / "assets" / "game" / "characters"
sys.path.insert(0, str(Path(__file__).resolve().parent))
import attirelib as al  # noqa: E402

DEBUG = Path("/tmp/attire-audit/derived")


def dark(p) -> bool:
    r, g, b, a = p
    if a <= 128:
        return False
    return (r < 70 and g < 70 and b < 80) or (max(r, g, b) < 125 and max(r, g, b) - min(r, g, b) < 22)


def opaque(p) -> bool:
    return p[3] > 128


def collect_frames(cid: str) -> list[Path]:
    out = []
    for p in sorted(CHAR.glob(f"{cid}_*.png")):
        if "_plain_" in p.name or "_free_" in p.name:
            continue
        out.append(p)
    return out


def plain_name(path: Path) -> str:
    # maya_0.png -> maya_plain_0.png; lin_seat_e.png -> lin_plain_seat_e.png
    stem = path.stem
    cid, rest = stem.split("_", 1)
    return f"{cid}_plain_{rest}.png"


def neighbors(x, y):
    return ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1))


def neighbors8(x, y):
    for dx in (-1, 0, 1):
        for dy in (-1, 0, 1):
            if dx or dy:
                yield (x + dx, y + dy)


def flood(pred, w, h, seeds=None, clip=None) -> set[tuple[int, int]]:
    if seeds is None:
        x0, y0, x1, y1 = clip or (0, 0, w, h)
        seeds = [(x, y) for y in range(y0, y1) for x in range(x0, x1) if pred(x, y)]
    mask: set[tuple[int, int]] = set()
    q = deque(seeds)
    while q:
        x, y = q.popleft()
        if (x, y) in mask or not (0 <= x < w and 0 <= y < h):
            continue
        if clip and not (clip[0] <= x < clip[2] and clip[1] <= y < clip[3]):
            continue
        if not pred(x, y):
            continue
        mask.add((x, y))
        q.extend(neighbors(x, y))
    return mask


def absorb_outline(mask, px, w, h, rounds=3) -> set[tuple[int, int]]:
    """Grow into the prop's own outline, but keep outline that borders remaining body."""
    def is_body(n):
        x, y = n
        return 0 <= x < w and 0 <= y < h and n not in mask and opaque(px[x, y]) and not dark(px[x, y])

    for _ in range(rounds):
        grow = set()
        for x, y in mask:
            for n in neighbors8(x, y):
                nx, ny = n
                if n in mask or not (0 <= nx < w and 0 <= ny < h):
                    continue
                if not dark(px[nx, ny]):
                    continue
                if any(is_body(m) for m in neighbors(nx, ny)):
                    continue
                grow.add(n)
        mask |= grow
    return mask


def mode_color(samples, palette) -> tuple[int, int, int, int]:
    if not samples:
        return (20, 12, 20, 255)
    rgb = Counter((p[0], p[1], p[2]) for p in samples).most_common(1)[0][0]
    rgb = al.snap(rgb, palette)
    return (rgb[0], rgb[1], rgb[2], 255)


def restore_outline(px, mask, w, h, outline, outside):
    add = []
    for x, y in mask:
        if (x, y) not in outside:
            continue
        if any(
            0 <= nx < w and 0 <= ny < h and opaque(px[nx, ny]) and not dark(px[nx, ny])
            for nx, ny in neighbors(x, y)
        ):
            add.append((x, y))
    for x, y in add:
        px[x, y] = outline


def drop_orphaned_outline(px, w, h, clip, radius=2):
    x0, y0, x1, y1 = clip
    for _ in range(3):
        drop = []
        for y in range(max(0, y0), min(h, y1)):
            for x in range(max(0, x0), min(w, x1)):
                if not dark(px[x, y]):
                    continue
                near = any(
                    0 <= x + dx < w and 0 <= y + dy < h
                    and opaque(px[x + dx, y + dy])
                    and not dark(px[x + dx, y + dy])
                    for dx in range(-radius, radius + 1)
                    for dy in range(-radius, radius + 1)
                )
                if not near:
                    drop.append((x, y))
        for x, y in drop:
            px[x, y] = (0, 0, 0, 0)


# ---------------------------------------------------------------------------
# Maya / Bo: remove apron (and Bo's towel), refill blouse / shirt / trousers.
# ---------------------------------------------------------------------------

def maya_apron_pred(px, y_min=50):
    def pred(x, y):
        if y < y_min:
            return False
        return al.is_maya_apron(px[x, y], x, y, 96, 112)
    return pred


def bo_apron_pred(px, y_min=50):
    def pred(x, y):
        if y < y_min:
            return False
        p = px[x, y]
        return al.is_bo_apron(p, x, y, 96, 112) or al.is_bo_towel(p, x, y, 96, 112)
    return pred



# Canonical garment colours sampled from the standing frames (maya_0 / bo_0 / lin_0).
# Per-frame sampling is too easy to contaminate with skin, outline or props.
GARMENTS = {
    "maya": {
        "upper": (253, 244, 213),
        "lower": (80, 77, 54),
        "outline": (18, 10, 18),
        "waist": 84,
    },
    "bo": {
        "upper": (253, 194, 56),
        "lower": (49, 49, 55),
        "outline": (5, 4, 9),
        "waist": 82,
    },
}


def original_transparent(orig, x, y, w, h) -> bool:
    if not (0 <= x < w and 0 <= y < h):
        return True
    return orig[x, y][3] <= 128


def fill_mask_with_clothes(px, orig, w, h, mask, cid, palette) -> None:
    """Replace every masked pixel with shirt or trousers. Never punch a hole
    in the body. Only write an outline pixel where the *original* sprite
    already met the transparent background (the true silhouette)."""
    g = GARMENTS[cid]
    upper = (*al.snap(g["upper"], palette), 255)
    lower = (*al.snap(g["lower"], palette), 255)
    outline = (*al.snap(g["outline"], palette), 255)
    waist = g["waist"]
    # A slightly tighter waist for sitting frames, which draw the body lower.
    opaque_ys = [y for y in range(h) for x in range(w) if orig[x, y][3] > 128]
    if opaque_ys and min(opaque_ys) > 16:
        waist = min(h - 8, waist + max(0, min(opaque_ys) - 8) // 2)
    for x, y in mask:
        px[x, y] = upper if y < waist else lower
    # Silhouette outline only, 1 px, only against original transparency.
    for x, y in mask:
        if any(original_transparent(orig, nx, ny, w, h) for nx, ny in neighbors(x, y)):
            # Don't outline interior waist seams — only the outer edge.
            px[x, y] = outline


def strip_apron(path: Path, cid: str, palette: set, debug: bool) -> dict:
    im = Image.open(path).convert("RGBA")
    w, h = im.size
    orig = im.copy().load()
    px = im.load()
    # Only treat skin in the head as the face, so arms and hands don't push
    # the apron start down over the bib.
    face_bottom = 48
    for y in range(0, 58):
        for x in range(w):
            r, g, b, a = px[x, y]
            if a > 160 and r >= 230 and 120 <= g <= 200 and 50 <= b <= 150:
                face_bottom = max(face_bottom, y + 1)
    y_min = min(max(face_bottom, 48), 58)
    pred = maya_apron_pred(px, y_min) if cid == "maya" else bo_apron_pred(px)
    mask = flood(pred, w, h, clip=(0, 46, w, h))
    # Grow at most 1 pixel into the prop's own outline, never into remaining
    # clothing (those dark pixels that already touch a non-mask body pixel).
    def is_body(n):
        x, y = n
        return 0 <= x < w and 0 <= y < h and n not in mask and opaque(px[x, y]) and not dark(px[x, y])

    grow = set()
    for x, y in list(mask):
        for n in neighbors(x, y):
            nx, ny = n
            if n in mask or not (0 <= nx < w and 0 <= ny < h):
                continue
            if dark(px[nx, ny]) and not any(is_body(m) for m in neighbors(nx, ny)):
                grow.add(n)
    mask |= grow
    if debug:
        DEBUG.mkdir(parents=True, exist_ok=True)
        dbg = Image.open(path).convert("RGBA")
        dpx = dbg.load()
        for x, y in mask:
            dpx[x, y] = (255, 0, 80, 180)
        dbg.save(DEBUG / f"mask-{path.stem}.png")
    if mask:
        fill_mask_with_clothes(px, orig, w, h, mask, cid, palette)
    # Second pass: any rust bib pixels the first flood missed (narrow strap
    # edges), but never the shoes (y >= 100) or the face.
    if cid == "maya":
        garment = GARMENTS[cid]
        upper = (*al.snap(garment["upper"], palette), 255)
        lower = (*al.snap(garment["lower"], palette), 255)
        extra = set()
        for y in range(48, 100):
            for x in range(w):
                pr, pg, pb, pa = px[x, y]
                if pa < 160 or (pr >= 230 and pg >= 120):  # leave face/arm skin alone
                    continue
                if al.is_maya_apron(px[x, y], x, y, w, h):
                    extra.add((x, y))
        for x, y in extra:
            px[x, y] = upper if y < garment["waist"] else lower
        mask |= extra
    off = 0
    for y in range(h):
        for x in range(w):
            r, g, b, a = px[x, y]
            if a >= 32 and (r, g, b) not in palette:
                rgb = al.snap((r, g, b), palette)
                px[x, y] = (rgb[0], rgb[1], rgb[2], a)
                off += 1
    out_path = CHAR / plain_name(path)
    im.save(out_path)
    return {"file": out_path.name, "removed": len(mask), "snapped": off}


# ---------------------------------------------------------------------------
# Lin: remove cap (repaint hair) and backpack / strap. Keep activity props
# (book, laptop, headphones) and the teal jacket.
# ---------------------------------------------------------------------------

def teal_like(p) -> bool:
    r, g, b, a = p
    if a < 160:
        return False
    return r <= 90 and 70 <= g <= 200 and 70 <= b <= 200 and abs(g - b) <= 36 and g > r + 25


def connected_components(pred, w, h, clip=None):
    seen = set()
    comps = []
    x0, y0, x1, y1 = clip or (0, 0, w, h)
    for y in range(y0, y1):
        for x in range(x0, x1):
            if (x, y) in seen or not pred(x, y):
                continue
            comp = set()
            q = deque([(x, y)])
            while q:
                u = q.popleft()
                if u in seen:
                    continue
                ux, uy = u
                if not (x0 <= ux < x1 and y0 <= uy < y1) or not pred(ux, uy):
                    continue
                seen.add(u)
                comp.add(u)
                q.extend(neighbors(ux, uy))
            if comp:
                comps.append(comp)
    return comps


def lin_cap_mask(px, w, h) -> set:
    """The cap is the *upper* teal connected component, separated from the
    jacket by hair and face. This does not use a fixed y cutoff, so sitting
    frames (where the head sits lower in the 96x112 canvas) still work."""
    def pred(x, y):
        return teal_like(px[x, y])

    comps = connected_components(pred, w, h)
    if not comps:
        return set()
    # Character opaque bbox to judge "upper".
    ys = [y for y in range(h) for x in range(w) if opaque(px[x, y])]
    xs = [x for y in range(h) for x in range(w) if opaque(px[x, y])]
    if not ys:
        return set()
    top, bot = min(ys), max(ys)
    mid = top + 0.45 * (bot - top)
    # Cap = teal component whose centroid is in the upper 45% of the body
    # and which is not the largest torso component (the jacket).
    scored = []
    for comp in comps:
        cy = sum(y for _, y in comp) / len(comp)
        scored.append((cy, len(comp), comp))
    scored.sort()
    cap = set()
    for cy, n, comp in scored:
        if cy <= mid:
            cap |= comp
    if not cap and scored:
        cap = scored[0][2]
    # White logo attached to the cap.
    for x, y in list(cap):
        for nx, ny in neighbors8(x, y):
            if not (0 <= nx < w and 0 <= ny < h):
                continue
            r, g, b, a = px[nx, ny]
            if a > 160 and r > 200 and g > 200 and b > 170 and abs(r - g) < 30:
                cap.add((nx, ny))
    # 1 px dark brim / cap outline, but never flood into the hair mass:
    # only dark pixels that touch the cap and have at most 3 opaque dark
    # neighbours (a thin edge, not a hair volume).
    thin = set()
    for x, y in list(cap):
        for nx, ny in neighbors8(x, y):
            if (nx, ny) in cap or not (0 <= nx < w and 0 <= ny < h):
                continue
            p = px[nx, ny]
            if not dark(p):
                continue
            dark_n = sum(
                1
                for mx, my in neighbors8(nx, ny)
                if 0 <= mx < w and 0 <= my < h and dark(px[mx, my])
            )
            if dark_n <= 5:
                thin.add((nx, ny))
    cap |= thin
    return cap


def lin_bag_mask(px, w, h) -> set:
    """Gold/mustard bag body and its tan strap only. Never the teal jacket,
    the cream shirt, the blue study book, the laptop or the headphones."""
    def is_bag_body(x, y):
        return y >= 48 and al.is_lin_backpack(px[x, y], x, y, w, h)

    mask = flood(is_bag_body, w, h, clip=(0, 46, w, h))
    if not mask:
        return mask
    # Grow through tan/brown strap pixels that are clearly warmer than the
    # white shirt, and not teal, not skin.
    def is_strap(x, y):
        if y < 48 or not opaque(px[x, y]):
            return False
        r, g, b, a = px[x, y]
        if teal_like(px[x, y]):
            return False
        # white shirt
        if r > 220 and g > 200 and b > 180 and abs(r - g) < 25:
            return False
        # skin
        if r > 180 and 80 <= g <= 200 and 40 <= b <= 140 and r > g + 40 and (g - b) < 80:
            return False
        # tan / gold / brown strap
        if r > 140 and g > 80 and b < 180 and (r - b) >= 25 and r >= g:
            return True
        return False

    for _ in range(10):
        grow = set()
        for x, y in mask:
            for nx, ny in neighbors8(x, y):
                if (nx, ny) in mask or not (0 <= nx < w and 0 <= ny < h):
                    continue
                if is_strap(nx, ny):
                    grow.add((nx, ny))
        if not grow:
            break
        mask |= grow
    # 1 px outline of the bag, not of the jacket.
    grow = set()
    for x, y in list(mask):
        for n in neighbors(x, y):
            nx, ny = n
            if n in mask or not (0 <= nx < w and 0 <= ny < h):
                continue
            if dark(px[nx, ny]) and not teal_like(px[nx, ny]):
                # Don't swallow hair or shorts outlines that merely touch the bag.
                body_like = 0
                for mx, my in neighbors(nx, ny):
                    if not (0 <= mx < w and 0 <= my < h) or (mx, my) in mask:
                        continue
                    if opaque(px[mx, my]) and not dark(px[mx, my]) and not is_strap(mx, my) and not is_bag_body(mx, my):
                        body_like += 1
                if body_like == 0:
                    grow.add(n)
    mask |= grow
    return mask


def paint_lin_hair(px, orig, w, h, cap_mask, palette) -> None:
    """Clear the brim; fill the crown with a modest mop of existing hair
    colours. Never overwrite hair that was already visible."""
    # Existing hair (not in the cap mask).
    hair_px = []
    for y in range(h):
        for x in range(w):
            if (x, y) in cap_mask or not opaque(orig[x, y]) or dark(orig[x, y]):
                continue
            r, g, b, a = orig[x, y]
            # Dark purple / burgundy curls only (Lin's side hair), not grey,
            # not dusty brown, not teal.
            if 8 <= r <= 80 and 4 <= g <= 60 and 10 <= b <= 70 and r >= g and b >= g and (r + b - 2 * g) >= 8:
                if y < 70:
                    hair_px.append((x, y, r, g, b))
    # Always paint with Lin's known curl colours so a stray brown sample
    # cannot tint the crown like a cap.
    fill = al.snap((38, 20, 27), palette)
    dark_h = al.snap((15, 8, 16), palette)
    if hair_px:
        left = min(x for x, _, _, _, _ in hair_px)
        right = max(x for x, _, _, _, _ in hair_px)
        hairline = min(y for _, y, _, _, _ in hair_px)
    else:
        cap_xs = [x for x, _ in cap_mask]
        left, right = min(cap_xs) + 6, max(cap_xs) - 6
        hairline = min(y for _, y in cap_mask) + 16
    outline = al.snap((7, 6, 13), palette)
    cx = (left + right) / 2.0
    # Modest dome: a little above the visible hair, narrower than the brim.
    top = max(min(y for _, y in cap_mask) + 2, hairline - 14)
    rx = max(6.0, (right - left) / 2.0 + 1.5)
    ry = max(6.0, (hairline - top) + 4.0)
    cy = top + ry * 0.85
    for x, y in cap_mask:
        nx = (x + 0.5 - cx) / rx
        ny = (y + 0.5 - cy) / ry
        inside = (nx * nx + ny * ny) <= 1.02
        # Always keep a connection down to the existing hairline.
        if inside or (abs(x - cx) <= rx * 0.95 and hairline - 2 <= y <= hairline + 2):
            edge = (nx * nx + ny * ny) >= 0.80 or y <= top + 1
            rgb = dark_h if edge else fill
            # Outline only against original (or already cleared) transparency
            # on the upper sides, so we don't draw a helmet brim.
            px[x, y] = (rgb[0], rgb[1], rgb[2], 255)
        else:
            px[x, y] = (0, 0, 0, 0)
    # Outline the new hair where it meets transparency.
    for x, y in cap_mask:
        r, g, b, a = px[x, y]
        if a < 32:
            continue
        if any(
            not (0 <= nx < w and 0 <= ny < h) or px[nx, ny][3] < 32
            for nx, ny in neighbors(x, y)
        ):
            px[x, y] = (outline[0], outline[1], outline[2], 255)
    # Brim / logo leftovers: any remaining cap-teal or white badge in the
    # head, outside the hair dome, is cleared; inside the dome it becomes hair.
    face_right = right
    face_left = left
    for y in range(0, min(h, hairline + 8)):
        for x in range(w):
            if (x, y) in cap_mask:
                continue
            p = px[x, y]
            leftover = teal_like(p) or (p[3] > 160 and p[0] > 200 and p[1] > 200 and p[2] > 170 and y < hairline)
            if not leftover:
                continue
            if x > face_right + 1 or x < face_left - 1:
                px[x, y] = (0, 0, 0, 0)
            else:
                px[x, y] = (fill[0], fill[1], fill[2], 255)

def fill_behind_bag(px, orig, w, h, mask, palette) -> None:
    shirt = al.snap((252, 241, 216), palette)
    jacket = al.snap((26, 143, 146), palette)
    shorts = al.snap((53, 53, 58), palette)
    outline = al.snap((7, 6, 13), palette)
    for x, y in mask:
        # If this bag pixel sat outside the body (original transparency as a
        # neighbour AND no remaining body on the other side), clear it.
        trans_n = any(original_transparent(orig, nx, ny, w, h) for nx, ny in neighbors(x, y))
        body_n = any(
            0 <= nx < w and 0 <= ny < h and (nx, ny) not in mask and orig[nx, ny][3] > 128 and not dark(orig[nx, ny])
            for nx, ny in neighbors(x, y)  # 4-neighbour: hanging bags only touch via a corner or strap
        )
        if trans_n and not body_n:
            px[x, y] = (0, 0, 0, 0)
            continue
        # Choose clothing from the nearest remaining body pixel, else by band.
        best = None
        for dist in range(1, 10):
            cands = []
            for nx, ny in ((x + dist, y), (x - dist, y), (x, y + dist), (x, y - dist)):
                if 0 <= nx < w and 0 <= ny < h and (nx, ny) not in mask and orig[nx, ny][3] > 128 and not dark(orig[nx, ny]):
                    p = orig[nx, ny]
                    if teal_like(p) or (p[0] > 220 and p[1] > 200) or (abs(p[0] - p[1]) < 16 and 25 <= p[0] <= 80 and ny >= 70):
                        cands.append((p[0], p[1], p[2]))
            if cands:
                best = Counter(cands).most_common(1)[0][0]
                break
        if best is None:
            if y >= 84:
                best = shorts
            elif y < 62:
                best = shirt
            else:
                best = jacket
        best = al.snap(best, palette)
        px[x, y] = (best[0], best[1], best[2], 255)
    # Outline only at the original silhouette.
    for x, y in mask:
        if px[x, y][3] < 32:
            continue
        if any(original_transparent(orig, nx, ny, w, h) for nx, ny in neighbors(x, y)):
            if any(
                0 <= nx < w and 0 <= ny < h and px[nx, ny][3] < 32
                for nx, ny in neighbors(x, y)
            ):
                px[x, y] = (outline[0], outline[1], outline[2], 255)



def cover_side_scalp(px, orig, w, h, palette) -> None:
    """Grow existing profile hair a few pixels into the bald scalp. Does not
    paint a new hat or brim."""
    fill = al.snap((38, 20, 27), palette)
    dark_h = al.snap((15, 8, 16), palette)
    def is_hair(x, y):
        if not (0 <= x < w and 0 <= y < h):
            return False
        r, g, b, a = px[x, y]
        return a > 160 and 8 <= r <= 80 and 4 <= g <= 60 and 10 <= b <= 80 and r >= g
    def is_skin(x, y):
        if not (0 <= x < w and 0 <= y < h):
            return False
        r, g, b, a = px[x, y]
        return a > 160 and r > 180 and 80 <= g <= 200 and 40 <= b <= 150
    hair = [(x, y) for y in range(h) for x in range(w) if is_hair(x, y)]
    skin = [(x, y) for y in range(h) for x in range(w) if is_skin(x, y)]
    if not hair or not skin:
        return
    face_top = min(y for _, y in skin)
    hx = [x for x, _ in hair]
    left, right = min(hx), max(hx)
    for _ in range(5):
        grow = []
        for x, y in hair:
            for nx, ny in neighbors(x, y):
                if not (0 <= nx < w and 0 <= ny < h):
                    continue
                if nx < left - 1 or nx > right + 1:
                    continue
                if ny >= face_top:  # don't paint the forehead
                    continue
                r, g, b, a = px[nx, ny]
                if is_hair(nx, ny) or is_skin(nx, ny):
                    continue
                # Empty scalp, leftover teal, leftover badge.
                if a < 32 or teal_like(px[nx, ny]) or (a > 160 and r > 200 and g > 200):
                    grow.append((nx, ny))
        if not grow:
            break
        for x, y in grow:
            edge = y <= min(yy for _, yy in hair) or x <= left or x >= right
            rgb = dark_h if edge else fill
            px[x, y] = (rgb[0], rgb[1], rgb[2], 255)
            hair.append((x, y))
    # Isolated white badge above the head.
    for y in range(0, face_top):
        for x in range(w):
            r, g, b, a = px[x, y]
            if a > 160 and r > 200 and g > 200 and b > 170 and not any(
                is_hair(x + dx, y + dy) for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))
            ):
                px[x, y] = (0, 0, 0, 0)



def strip_lin(path: Path, palette: set, debug: bool) -> dict:
    im = Image.open(path).convert("RGBA")
    w, h = im.size
    orig_im = im.copy()
    orig = orig_im.load()
    px = im.load()
    cap = lin_cap_mask(px, w, h)
    bag = lin_bag_mask(px, w, h)
    if debug:
        DEBUG.mkdir(parents=True, exist_ok=True)
        dbg = orig_im.copy()
        dpx = dbg.load()
        for x, y in cap:
            dpx[x, y] = (0, 180, 255, 200)
        for x, y in bag:
            dpx[x, y] = (255, 160, 0, 200)
        dbg.save(DEBUG / f"mask-{path.stem}.png")
    if cap:
        paint_lin_hair(px, orig, w, h, cap, palette)
    if path.stem.endswith("_e") or path.stem.endswith("_w"):
        cover_side_scalp(px, orig, w, h, palette)
    if bag:
        fill_behind_bag(px, orig, w, h, bag, palette)
    off = 0
    for y in range(h):
        for x in range(w):
            r, g, b, a = px[x, y]
            if a >= 32 and (r, g, b) not in palette:
                rgb = al.snap((r, g, b), palette)
                px[x, y] = (rgb[0], rgb[1], rgb[2], a)
                off += 1
    out_path = CHAR / plain_name(path)
    im.save(out_path)
    det = al.detect_layers(im, "lin")
    return {
        "file": out_path.name,
        "cap_removed": len(cap),
        "bag_removed": len(bag),
        "snapped": off,
        "det": det,
    }



def main() -> int:
    debug = "--debug" in sys.argv
    if debug:
        DEBUG.mkdir(parents=True, exist_ok=True)
    reports = []
    for cid in ("maya", "bo", "lin"):
        palette = al.palette_of(cid)
        print(f"{cid}: palette {len(palette)} colours")
        for path in collect_frames(cid):
            if cid in ("maya", "bo"):
                rec = strip_apron(path, cid, palette, debug)
            else:
                rec = strip_lin(path, palette, debug)
            rec["character"] = cid
            reports.append(rec)
            extra = {k: v for k, v in rec.items() if k not in ("file", "character")}
            print(f"  {rec['file']} {extra}")
    # Verify 0 off-palette and that work layers are gone / stay for the
    # characters we expect.
    failed = 0
    for rec in reports:
        cid = rec["character"]
        im = Image.open(CHAR / rec["file"]).convert("RGBA")
        off = al.off_palette_count(im, al.palette_of(cid))
        det = al.detect_layers(im, cid)
        rec["off_palette"] = off
        rec["det_after"] = det
        if off != 0:
            print(f"FAIL {rec['file']}: {off} off-palette pixels")
            failed += 1
        if cid in ("maya", "bo") and det.get("apron"):
            print(f"FAIL {rec['file']}: apron still detected {al.counts(im, cid)}")
            failed += 1
        if cid == "bo" and det.get("towel"):
            print(f"FAIL {rec['file']}: towel still detected {al.counts(im, cid)}")
            failed += 1
        if cid == "lin" and (det.get("cap") or det.get("backpack")):
            print(f"FAIL {rec['file']}: commute gear still detected {al.counts(im, cid)}")
            failed += 1
    print("PASS" if failed == 0 else f"FAIL {failed}")
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
