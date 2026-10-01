#!/usr/bin/env python3
"""Design + preview the LUMO firefly pixel-art mascot map for the LessDB website."""
import sys

# 21 wide x 24 tall. Keys:
#   D dark frame, H head, h head shade, E eye, L antenna tip, S scarf,
#   W wing (translucent), B body, G glow abdomen, g glow core, . empty
MAP = [
    ".....................",
    ".......L.....L.......",
    ".......D.....D.......",
    "......D.......D......",
    ".....DDDDDDDDDDD.....",
    "....DHHHHHHHHHHHD....",
    "....DHHHEHHHEHHHD....",
    "....DHHHEHHHEHHHD....",
    "....DHHHHHHHHHHHD....",
    ".....DhHHHHHHHHhD....",
    "......DDDDDDDDD......",
    ".......SSSSSSS.......",
    "......SSSSSSSSS......",
    ".WWW..DDDDDDDDD..WWW.",
    "WWWW.WDDDDDDDDDW.WWWW",
    "WWWWW.BBBBBBBBB.WWWWW",
    ".WWWW.BBBBBBBBB.WWWW.",
    "..WWW.BBBBBBBBB.WWW..",
    "......BBBBBBBBB......",
    ".....BBGGGGGGGBB.....",
    ".....BGgGGGGGgGB.....",
    ".....BGGgGGGgGGB.....",
    "......GGGGGGGGG......",
    "........ggg..........",
]

W = max(len(r) for r in MAP)
H = len(MAP)
for i, r in enumerate(MAP):
    if len(r) != W:
        print(f"ROW {i} LEN {len(r)} != {W}")
        sys.exit(1)

COLORS = {
    "D": "#2e2e26", "H": "#e8e6d8", "h": "#9a988a", "E": "#f6f09c",
    "L": "#ffe14a", "S": "#f25533", "W": "#6fd7c0", "B": "#3a3a32",
    "G": "#ffe14a", "g": "#fff8b0",
}
ANSI = {
    "D": "\033[90m", "H": "\033[97m", "h": "\033[37m", "E": "\033[93m",
    "L": "\033[93m", "S": "\033[91m", "W": "\033[96m", "B": "\033[90m",
    "G": "\033[33m", "g": "\033[103m\033[93m",
}

print(f"grid {W}x{H}\n")
for row in MAP:
    for ch in row:
        if ch == ".":
            print("  ", end="")
        else:
            print(ANSI[ch] + "██" + "\033[0m", end="")
    print()
print("\nlegend:", " ".join(f"{k}={COLORS[k]}" for k in COLORS))

# counts for sanity
from collections import Counter
c = Counter("".join(MAP))
print("\ncells:", dict(c))
