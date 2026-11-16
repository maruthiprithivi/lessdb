#!/usr/bin/env python3
"""Cut Lessville source sheets into runtime frames (bottom-aligned, real alpha).

Sources: recovered approved art (<recovered-assets-root>/assets) and
newly generated companions (<recovered-assets-root>/generated).
"""
from collections import deque
import os
from pathlib import Path
from PIL import Image
SRC = Path(os.environ['LESSVILLE_SOURCE_ASSETS'])
OUT = Path(__file__).resolve().parent.parent / 'assets' / 'game'
FRAME_W, FRAME_H, BODY_H = 96, 112, 104


def white_to_alpha(im: Image.Image, tol: int = 38) -> Image.Image:
    """Flood-fill near-white background from the borders into transparency."""
    im = im.convert('RGBA')
    w, h = im.size
    px = im.load()
    seen = bytearray(w * h)
    q = deque()
    for x in range(w):
        q.append((x, 0)); q.append((x, h - 1))
    for y in range(h):
        q.append((0, y)); q.append((w - 1, y))
    while q:
        x, y = q.popleft()
        if x < 0 or y < 0 or x >= w or y >= h or seen[y * w + x]:
            continue
        seen[y * w + x] = 1
        r, g, b, a = px[x, y]
        if a == 0 or (r > 255 - tol and g > 255 - tol and b > 255 - tol):
            px[x, y] = (0, 0, 0, 0)
            q.extend(((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)))
    return im


def harden(im: Image.Image) -> Image.Image:
    px = im.load()
    for y in range(im.size[1]):
        for x in range(im.size[0]):
            r, g, b, a = px[x, y]
            px[x, y] = (r, g, b, 255 if a >= 128 else 0)
    return im


def quadrants(im):
    w, h = im.size
    return [im.crop((cx * w // 2, cy * h // 2, (cx + 1) * w // 2, (cy + 1) * h // 2))
            for cy in range(2) for cx in range(2)]


def tight(im):
    im = im.copy()
    w, h = im.size
    px = im.load()
    label = [0] * (w * h)
    sizes = {}
    cur = 0
    for sy in range(h):
        for sx in range(w):
            if label[sy * w + sx] or px[sx, sy][3] <= 40:
                continue
            cur += 1
            stack = [(sx, sy)]
            label[sy * w + sx] = cur
            n = 0
            while stack:
                x, y = stack.pop(); n += 1
                for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
                    if 0 <= nx < w and 0 <= ny < h and not label[ny * w + nx] and px[nx, ny][3] > 40:
                        label[ny * w + nx] = cur
                        stack.append((nx, ny))
            sizes[cur] = n
    if sizes:
        keep = {k for k, v in sizes.items() if v >= max(sizes.values()) * 0.03}
        for y in range(h):
            for x in range(w):
                lab = label[y * w + x]
                if px[x, y][3] and (lab == 0 or lab not in keep):
                    px[x, y] = (0, 0, 0, 0)
    box = im.getchannel('A').point(lambda a: 255 if a > 40 else 0).getbbox()
    return im.crop(box) if box else im


def place(frames, body_h, fw, fh, name):
    scale = body_h / max(f.size[1] for f in frames)
    for i, f in enumerate(frames):
        f = f.resize((max(1, round(f.size[0] * scale)), max(1, round(f.size[1] * scale))), Image.Resampling.BOX)
        f = harden(f)
        canvas = Image.new('RGBA', (fw, fh), (0, 0, 0, 0))
        canvas.alpha_composite(f, ((fw - f.size[0]) // 2, fh - f.size[1] - 2))
        canvas.save(OUT / 'characters' / f'{name}_{i}.png')


def sheet(path, white):
    im = Image.open(path).convert('RGBA')
    if white:
        im = white_to_alpha(im)
    return [tight(q) for q in quadrants(im)]


def main():
    (OUT / 'characters').mkdir(parents=True, exist_ok=True)
    (OUT / 'locations').mkdir(parents=True, exist_ok=True)
    (OUT / 'props').mkdir(parents=True, exist_ok=True)
    # 0 idle, 1 walk A, 2 walk B, 3 talk/gesture (right-facing)
    place(sheet(SRC / 'assets/personal-poses-v1.png', True), BODY_H, FRAME_W, FRAME_H, 'lin')
    place(sheet(SRC / 'assets/coworker-poses-v1.png', True), BODY_H, FRAME_W, FRAME_H, 'sam')
    for n in ('maya', 'ama', 'bo'):
        place(sheet(SRC / f'generated/{n}-poses.png', False), BODY_H, FRAME_W, FRAME_H, n)
    rear = sheet(SRC / 'assets/rear-walk-and-typing-poses-v1.png', True)
    place(rear[:2], BODY_H, FRAME_W, FRAME_H, 'lin_back')
    place(rear[2:], BODY_H - 16, FRAME_W, FRAME_H, 'sam_typing')
    corgi = sheet(SRC / 'assets/corgi-four-poses.png', True)
    place(corgi, 44, 72, 52, 'mochi')
    props = sheet(SRC / 'assets/four-city-life-props-v2.png', True)
    for name, p in zip(('coffee', 'milk', 'grocery_bag', 'backpack'), props):
        s = 40 / max(p.size)
        p = harden(p.resize((round(p.size[0] * s), round(p.size[1] * s)), Image.Resampling.BOX))
        c = Image.new('RGBA', (44, 44), (0, 0, 0, 0)); c.alpha_composite(p, ((44 - p.size[0]) // 2, (44 - p.size[1]) // 2))
        c.save(OUT / 'props' / f'{name}.png')
    bus = tight(Image.open(SRC / 'generated/bus-side.png').convert('RGBA'))
    s = 300 / bus.size[0]
    harden(bus.resize((300, round(bus.size[1] * s)), Image.Resampling.BOX)).save(OUT / 'props' / 'bus.png')
    Image.open(SRC / 'generated/neighborhood-nobus.png').convert('RGB').resize((1536, 1024), Image.Resampling.LANCZOS).save(OUT / 'locations' / 'town.png')
    interiors = {'cafe': SRC / 'assets/cafe-background-v2.png', 'office': SRC / 'assets/office-background-v1.png',
                 'grocery': SRC / 'assets/grocery-background-v1.png', 'trail': SRC / 'assets/hiking-background-v1.png',
                 'home': SRC / 'generated/home-background.png', 'school': SRC / 'generated/school-background.png',
                 'gym': SRC / 'generated/gym-background.png'}
    for name, p in interiors.items():
        Image.open(p).convert('RGB').resize((768, 512), Image.Resampling.LANCZOS).save(OUT / 'locations' / f'{name}.png')
    # Back-walk (2) + seated (2) frames per character; Lin has 4 seated frames.
    for n in ('maya', 'bo', 'ama', 'sam'):
        fr = sheet(SRC / f'generated/{n}-back-seat.png', False)
        place(fr[:2], BODY_H, FRAME_W, FRAME_H, f'{n}_back')
        place(fr[2:], BODY_H - 18, FRAME_W, FRAME_H, f'{n}_seat')
    place(sheet(SRC / 'generated/lin-seat.png', False), BODY_H - 18, FRAME_W, FRAME_H, 'lin_seat')
    cw = SRC / 'generated/coworkers-typing.png'
    if cw.exists():
        fr = sheet(cw, False)
        place(fr[:2], BODY_H - 16, FRAME_W, FRAME_H, 'priya_typing')
        place(fr[2:], BODY_H - 16, FRAME_W, FRAME_H, 'tom_typing')
    # Bus interior: magenta windows become transparent so the street scrolls behind them.
    bus_in = Image.open(SRC / 'generated/bus-interior.png').convert('RGBA').resize((768, 512), Image.Resampling.LANCZOS)
    px = bus_in.load()
    for y in range(bus_in.size[1]):
        for x in range(bus_in.size[0]):
            r, g, b, a = px[x, y]
            if r > 150 and b > 150 and g < 120:
                px[x, y] = (0, 0, 0, 0)
    bus_in.save(OUT / 'locations' / 'bus.png')
    Image.open(SRC / 'generated/bus-window-scenery.png').convert('RGB').resize((1194, 512), Image.Resampling.LANCZOS).save(OUT / 'locations' / 'bus_scenery.png')
    print('wrote', sum(1 for _ in OUT.rglob('*.png')), 'runtime images to', OUT)


if __name__ == '__main__':
    main()
