#!/usr/bin/env python3
"""Derive hands-free character frames from the approved pixel art.

The approved art bakes props into the base frames (Ama's red leash, Sam's
phone, Bo's take-away cup), which made characters look like they hold them
all the time. This tool keeps every original untouched (they remain the
prop-holding variants) and writes `<id>_free_<n>.png` siblings with the prop
removed. The renderer chooses the variant from the backend's situated
presence state (`presence.hands`).

Method, per frame: flood from a seed through prop-coloured pixels inside a
clip rectangle, absorb the dark outline around the prop, then either clear the
pixels (prop hung outside the silhouette) or refill them with the covered
clothing colour (prop held against the body), and finally restore a two-pixel
outline wherever cleared space now touches the character. Deterministic: same
input pixels always give the same output.
"""
from __future__ import annotations

import sys
from collections import Counter, deque
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1] / "assets" / "game" / "characters"
OUTLINE = (20, 12, 20, 255)


def dark(p):
    r, g, b, a = p
    if a <= 128:
        return False
    # true outline, or the grey anti-aliased outline used around props
    return (r < 70 and g < 70 and b < 80) or (max(r, g, b) < 125 and max(r, g, b) - min(r, g, b) < 22)


def red(p):
    r, g, b, a = p
    return a > 128 and r > g + 45 and g < 115 and b < 125 and r > 80


def paper(p):
    r, g, b, a = p
    return a > 128 and r > 150 and g > 140 and b > 125 and max(r, g, b) - min(r, g, b) < 50


def cup(p):
    r, g, b, a = p
    lid = 70 <= r <= 175 and 40 <= g <= 115 and 20 <= b <= 80 and r > g > b
    return a > 128 and (paper(p) or lid)


PREDICATES = {"red": red, "paper": paper, "cup": cup}

# (frame, predicate, seed points or None for every match in clip, clip x0,y0,x1,y1)
JOBS = [
    ("ama_0", "red", None, (4, 74, 34, 104)),
    ("ama_1", "red", None, (2, 70, 34, 104)),
    ("ama_2", "red", None, (4, 72, 34, 104)),
    ("ama_3", "red", None, (4, 74, 34, 106)),
    ("sam_0", "paper", [(28, 70)], (18, 60, 40, 80), "clear-tight"),
    ("sam_1", "paper", [(27, 70)], (16, 60, 40, 84), "clear-tight"),
    ("sam_2", "paper", [(64, 76)], (56, 66, 78, 90)),
    ("sam_3", "paper", [(60, 70)], (56, 60, 74, 86), "jacket"),
    ("bo_3", "cup", [(76, 56), (77, 65)], (72, 48, 92, 72), "clear-tight"),
]


def inside(rect, x, y):
    return rect[0] <= x < rect[2] and rect[1] <= y < rect[3]


def derive(name, pred_name, seeds, rect, fill_from=None):
    im = Image.open(ROOT / f"{name}.png").convert("RGBA")
    w, h = im.size
    px = im.load()
    pred = PREDICATES[pred_name]
    if seeds is None:
        seeds = [(x, y) for y in range(rect[1], rect[3]) for x in range(rect[0], rect[2]) if pred(px[x, y])]
    mask = set()
    q = deque(s for s in seeds if pred(px[s[0], s[1]]))
    while q:
        x, y = q.popleft()
        if (x, y) in mask or not inside(rect, x, y) or not pred(px[x, y]):
            continue
        mask.add((x, y))
        q.extend(((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)))
    if not mask:
        raise SystemExit(f"{name}: no prop pixels found from seeds")
    def is_body(n):
        return 0 <= n[0] < w and 0 <= n[1] < h and n not in mask and px[n][3] > 128 and not dark(px[n])

    # absorb the prop's own outline, but keep dark pixels that border the body:
    # those are the hand/clothing outline and must survive
    for _ in range(4):
        grow = set()
        for x, y in mask:
            for dx in (-1, 0, 1):
                for dy in (-1, 0, 1):
                    n = (x + dx, y + dy)
                    if n in mask or not inside(rect, *n) or not dark(px[n]):
                        continue
                    if any(is_body((n[0] + ex, n[1] + ey)) for ex, ey in ((1, 0), (-1, 0), (0, 1), (0, -1))):
                        continue
                    grow.add(n)
        mask |= grow
    # removed pixels connected to the frame exterior through transparent or removed
    # pixels become transparent; pixels enclosed by the body are refilled
    outside = set()
    q = deque([(x, y) for x in range(w) for y in (0, h - 1)] + [(x, y) for y in range(h) for x in (0, w - 1)])
    while q:
        x, y = q.popleft()
        if (x, y) in outside or not (0 <= x < w and 0 <= y < h):
            continue
        if px[x, y][3] > 128 and (x, y) not in mask:
            continue
        outside.add((x, y))
        q.extend(((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)))
    if fill_from == "jacket":
        # the prop is held against the jacket: sample the jacket mid-tone
        tones = Counter(px[x, y] for x in range(w) for y in range(60, 80)
                        if px[x, y][3] > 128 and px[x, y][2] > px[x, y][1] + 40 and px[x, y][0] > 120)
        fill = tones.most_common(1)[0][0]
    else:
        body = Counter()
        for x, y in mask:
            for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                n = (x + dx, y + dy)
                if is_body(n):
                    body[px[n]] += 1
        fill = body.most_common(1)[0][0] if body else OUTLINE
    radius = 1 if fill_from == "clear-tight" else 2
    if fill_from in ("clear", "clear-tight"):
        outside |= mask
    for x, y in mask:
        px[x, y] = (0, 0, 0, 0) if (x, y) in outside else fill
    # drop orphaned prop outline: dark pixels in the clip with no non-dark
    # opaque pixel within two pixels belonged to the prop, not the character
    for _ in range(3):
        drop = []
        for y in range(rect[1], rect[3]):
            for x in range(rect[0], rect[2]):
                if not dark(px[x, y]):
                    continue
                near = any(
                    0 <= x + dx < w and 0 <= y + dy < h and px[x + dx, y + dy][3] > 128 and not dark(px[x + dx, y + dy])
                    for dx in range(-radius, radius + 1) for dy in range(-radius, radius + 1)
                )
                if not near:
                    drop.append((x, y))
        for q_ in drop:
            px[q_] = (0, 0, 0, 0)
            outside.add(q_)
            mask.add(q_)
    # drop tiny detached specks left inside the clip
    seen = set()
    for y in range(rect[1], rect[3]):
        for x in range(rect[0], rect[2]):
            if (x, y) in seen or px[x, y][3] <= 128:
                continue
            comp, q2, touches_out = [], deque([(x, y)]), False
            while q2 and len(comp) < 40:
                u = q2.popleft()
                if u in seen or not (0 <= u[0] < w and 0 <= u[1] < h) or px[u][3] <= 128:
                    continue
                seen.add(u)
                comp.append(u)
                q2.extend((u[0] + dx, u[1] + dy) for dx in (-1, 0, 1) for dy in (-1, 0, 1) if dx or dy)
            if len(comp) < 12 and not q2:
                for u in comp:
                    px[u] = (0, 0, 0, 0)
    # close outline gaps: a cleared pixel touching bare body colour (no outline
    # between them) becomes outline
    add = []
    for x, y in mask:
        if (x, y) not in outside:
            continue
        nb = [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]
        if any(is_body(n) for n in nb):
            add.append((x, y))
    for p in add:
        px[p] = OUTLINE
    out = ROOT / (name.replace("_", "_free_", 1) + ".png")
    im.save(out)
    refilled = sum(1 for m in mask if m not in outside)
    return out.name, len(mask), refilled


def main():
    for job in JOBS:
        print(*derive(*job))
    return 0


if __name__ == "__main__":
    sys.exit(main())
