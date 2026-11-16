#!/usr/bin/env python3
"""Assemble BL-0148 step-2 gallery from Godot atmosphere frames."""
from __future__ import annotations

import shutil
from pathlib import Path

from PIL import Image, ImageDraw

FRAMES = Path("/tmp/lessville-layout/step2/after")
OUT = Path("/workspace/shared/ai-hq/results/lessdb/evidence/BL-0148/gallery-step2")

TIMES = [
    ("0730", "07:30", "dawn"),
    ("1230", "12:30", "day"),
    ("1830", "18:30", "dusk"),
    ("2200", "22:00", "night"),
]
INTERIORS = {"0730": "home", "1230": "office", "1830": "cafe", "2200": "home"}
CROPS = [
    ("park-flag", (680, 420, 1000, 700), "Fountain and park flag"),
    ("cafe-terrace", (1180, 180, 1536, 520), "Café terrace"),
    ("bus-stop", (160, 580, 620, 920), "Bus stop and street lamps"),
]


def cap(im: Image.Image, text: str) -> Image.Image:
    im = im.convert("RGB")
    bar = 28
    out = Image.new("RGB", (im.width, im.height + bar), (12, 14, 18))
    out.paste(im, (0, 0))
    d = ImageDraw.Draw(out)
    d.text((8, im.height + 8), text, fill=(240, 230, 200))
    return out


def crop4x(im: Image.Image, box) -> Image.Image:
    x0, y0, x1, y1 = box
    x0, y0 = max(0, x0), max(0, y0)
    x1, y1 = min(im.width, x1), min(im.height, y1)
    c = im.crop((x0, y0, x1, y1))
    return c.resize((c.size[0] * 4, c.size[1] * 4), Image.Resampling.NEAREST)


def main() -> None:
    if not FRAMES.exists():
        raise SystemExit(f"missing frames at {FRAMES}")
    if OUT.exists():
        shutil.rmtree(OUT)
    frames_dest = OUT / "frames"
    shutil.copytree(FRAMES, frames_dest)

    index = [
        "# BL-0148 step 2 — sky, lighting and wind",
        "",
        "The in-game clock drives a day/night sky gradient and ambient curve (dawn, day, dusk, night) across Town, and window-only lighting in interiors. Wind strength comes from a deterministic schedule seeded by date; a park flag and tree canopies move when wind > 0. Not a self-certification — Chief of Staff reviews these frames.",
        "",
        "Day 2 of the seeded schedule is used (12:30 and 22:00 are windy at 28 km/h). Automated check: `tools/check_atmosphere.py` (absolute luminance/hue gates in `config/atmosphere_gates.json`, plus wind displacement).",
        "",
        "Proof the check failed on the pre-step-2 commit `da7c3d2` (22:00 still too bright, dusk not warm, no wind motion): [`../proof-step2-fail-before.log`](../proof-step2-fail-before.log). Pass-after: [`checker.txt`](checker.txt).",
        "",
    ]
    for key, label, phase in TIMES:
        slot = OUT / key
        slot.mkdir(parents=True)
        town = Image.open(frames_dest / f"town-{key}.png")
        cap(town.resize((768, 512), Image.Resampling.NEAREST), f"Town · {label} · {phase}").save(slot / "town.png")
        index += [f"## {label} ({phase})", "", f"![Town wide]({key}/town.png)", ""]
        index += ["| Crop | Frame |", "| --- | --- |"]
        for name, box, title in CROPS:
            fname = f"{name}@4x.png"
            cap(crop4x(town, box), f"{title} · {label}").save(slot / fname)
            index.append(f"| {title} | ![]({key}/{fname}) |")
        loc = INTERIORS[key]
        interior = Image.open(frames_dest / f"{loc}-{key}.png")
        cap(interior, f"{loc} · {label}").save(slot / f"{loc}.png")
        index += ["", f"**Interior:** {loc}", "", f"![]({key}/{loc}.png)", ""]

    # Wind proof: side-by-side + GIF of the flag
    wind = OUT / "wind"
    wind.mkdir()
    a = Image.open(frames_dest / "wind-a.png")
    b = Image.open(frames_dest / "wind-b.png")
    box = (720, 460, 860, 620)
    ca = crop4x(a, box)
    cb = crop4x(b, box)
    cap(ca, "wind t=0.00s · 12:30 · 28 km/h").save(wind / "flag-a@4x.png")
    cap(cb, "wind t=0.40s · 12:30 · 28 km/h").save(wind / "flag-b@4x.png")
    pair = Image.new("RGB", (ca.width * 2 + 16, ca.height + 28), (12, 14, 18))
    pair.paste(ca, (0, 0))
    pair.paste(cb, (ca.width + 16, 0))
    d = ImageDraw.Draw(pair)
    d.text((8, ca.height + 8), "Park flag at 12:30, 28 km/h wind: t=0.00s (left) and t=0.40s (right)", fill=(240, 230, 200))
    pair.save(wind / "flag-compare.png")
    ca.save(wind / "flag-a-raw.png")
    cb.save(wind / "flag-b-raw.png")
    ca.convert("P", palette=Image.Palette.ADAPTIVE).save(
        wind / "flag.gif",
        save_all=True,
        append_images=[cb.convert("P", palette=Image.Palette.ADAPTIVE)],
        duration=400,
        loop=0,
    )
    index += [
        "## Wind proof",
        "",
        "Two frames of the park flag 0.40 s apart at 12:30 on day 2 (scheduled wind 28 km/h). The checker requires mean absolute luma displacement ≥ 2 in this ROI.",
        "",
        "![flag compare](wind/flag-compare.png)",
        "",
        "![flag gif](wind/flag.gif)",
        "",
        "## Checker",
        "",
        "See [`checker.txt`](checker.txt).",
        "",
    ]
    (OUT / "index.md").write_text("\n".join(index) + "\n")
    print("wrote", OUT / "index.md")


if __name__ == "__main__":
    main()
