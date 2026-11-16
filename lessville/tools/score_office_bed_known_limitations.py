#!/usr/bin/env python3
"""Score Office floor checks against the BL-0148 step-4 known limitations.

Runs check_office_bed.py, check_office_bed_patch.py and check_office_bed_a3.py
with their existing gates (nothing is loosened). Classifies each property:

  - Four documented limitations must still FAIL (XFAIL). If one passes,
    this scorer prints XPASS and exits 1, so the suite cannot silently flip.
  - Every other property must still hard-PASS. An unexpected failure exits 1.

The four xfails, tied to evidence/BL-0148/KNOWN-LIMITATIONS.md:

  a3.artefacts_and_chains  diagonal chains and blobs
  a2.seams_and_dark_band   seam rows and the 226 px band at x80-155 y300-303
  a2.brightness_windows    per-plank luma/hue windows
  a1.dstd_and_autocorr     texture spread (luma std) and board autocorr

Expected exit 0 only when there are exactly 4 XFAIL lines, 0 XPASS, and
0 unexpected failures.
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DOC = "evidence/BL-0148/KNOWN-LIMITATIONS.md"
DEFAULT_FRAMES = Path(
    "/workspace/shared/ai-hq/results/lessdb/evidence/BL-0148/gallery-step4-a3/frames/after"
)
# Documented 226 px band from proof-step4a3-result.log
KNOWN_DARK_BAND = re.compile(r"x80-155 y300-303")

LIMITATIONS = [
    "a3.artefacts_and_chains",
    "a2.seams_and_dark_band",
    "a2.brightness_windows",
    "a1.dstd_and_autocorr",
]


def run_checker(script: Path, art: Path, frames: Path | None) -> tuple[int, str]:
    args = [sys.executable, str(script), "--art", str(art)]
    if frames is not None:
        args.extend(["--dir", str(frames)])
    proc = subprocess.run(args, capture_output=True, text=True)
    out = (proc.stdout or "") + (proc.stderr or "")
    return proc.returncode, out


def fail_lines(text: str) -> list[str]:
    """Bullet lines under the checker's FAIL: block (not the PASS/FAIL header)."""
    lines = []
    in_fail = False
    for line in text.splitlines():
        if line.startswith("FAIL:"):
            in_fail = True
            continue
        if line.startswith("PASS:") or line.startswith("EXIT:"):
            in_fail = False
            continue
        if in_fail and line.startswith("  - "):
            lines.append(line[4:].strip())
    return lines


def classify(a1_fails: list[str], a2_fails: list[str], a3_fails: list[str]) -> dict[str, list[str]]:
    buckets = {
        "a1.bed_palette": [],
        "a1.dmean": [],
        "a1.dstd_and_autocorr": [],
        "a2.brightness_windows": [],
        "a2.seams_and_dark_band": [],
        "a2.desk_mismatch": [],
        "a2.nightstand_green": [],
        "a2.other": [],
        "a3.artefacts_and_chains": [],
        "a3.sliver": [],
        "a3.stamps": [],
        "a3.left_end": [],
        "a3.other": [],
        "other": [],
    }
    for f in a1_fails:
        if "bed-palette" in f:
            buckets["a1.bed_palette"].append(f)
        elif "mean RGB" in f:
            buckets["a1.dmean"].append(f)
        elif "luma std" in f or "autocorrelation" in f:
            buckets["a1.dstd_and_autocorr"].append(f)
        else:
            buckets["other"].append("a1: " + f)
    for f in a2_fails:
        if "window luma" in f or "window hue" in f:
            buckets["a2.brightness_windows"].append(f)
        elif "plank seams do not line up" in f:
            buckets["a2.seams_and_dark_band"].append(f)
        elif "unexplained hard-edged dark region" in f:
            if KNOWN_DARK_BAND.search(f):
                buckets["a2.seams_and_dark_band"].append(f)
            else:
                buckets["a2.other"].append(f)
        elif "left-desk silhouette" in f:
            buckets["a2.desk_mismatch"].append(f)
        elif "nightstand-green" in f:
            buckets["a2.nightstand_green"].append(f)
        else:
            buckets["a2.other"].append(f)
    for f in a3_fails:
        if "high-contrast artefact" in f or "diagonal thin-line chains" in f:
            buckets["a3.artefacts_and_chains"].append(f)
        elif "ragged dark sliver" in f:
            buckets["a3.sliver"].append(f)
        elif "repeated tiles" in f:
            buckets["a3.stamps"].append(f)
        elif "left desk end is incomplete" in f:
            buckets["a3.left_end"].append(f)
        else:
            buckets["a3.other"].append(f)
    return buckets


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--art", type=Path, default=ROOT / "assets/game/locations/office.png")
    ap.add_argument("--dir", type=Path, default=None)
    ap.add_argument(
        "--quiet-raw",
        action="store_true",
        help="do not echo the raw checker output (suite still prints it separately)",
    )
    args = ap.parse_args()
    frames = args.dir
    if frames is None and (DEFAULT_FRAMES / "office-1230.png").is_file():
        frames = DEFAULT_FRAMES

    print(f"score_office_bed_known_limitations: doc={DOC}")
    print(f"  art={args.art} dir={frames}")

    code1, out1 = run_checker(ROOT / "tools/check_office_bed.py", args.art, frames)
    code2, out2 = run_checker(ROOT / "tools/check_office_bed_patch.py", args.art, frames)
    code3, out3 = run_checker(ROOT / "tools/check_office_bed_a3.py", args.art, frames)
    if not args.quiet_raw:
        print("--- raw a1 ---")
        print(out1.rstrip())
        print("--- raw a2 ---")
        print(out2.rstrip())
        print("--- raw a3 ---")
        print(out3.rstrip())
        print("--- scores ---")

    buckets = classify(fail_lines(out1), fail_lines(out2), fail_lines(out3))
    hard = [
        "a1.bed_palette",
        "a1.dmean",
        "a2.desk_mismatch",
        "a2.nightstand_green",
        "a3.sliver",
        "a3.stamps",
        "a3.left_end",
    ]
    n_xfail = n_xpass = n_unexpected = n_hard = 0

    for name in hard:
        hits = buckets[name]
        if hits:
            n_unexpected += 1
            print(f"UNEXPECTED_FAIL {name}: " + " | ".join(hits[:6]))
        else:
            n_hard += 1
            print(f"HARD_PASS {name}")

    for name in ("a2.other", "a3.other", "other"):
        if buckets[name]:
            n_unexpected += 1
            print(f"UNEXPECTED_FAIL {name}: " + " | ".join(buckets[name][:8]))

    for name in LIMITATIONS:
        hits = buckets[name]
        if hits:
            n_xfail += 1
            print(f"XFAIL {name} still failing ({len(hits)} reports): " + " | ".join(hits[:4]))
        else:
            n_xpass += 1
            print(f"XPASS {name} unexpectedly passed; documented as a known limitation in {DOC}")

    print(
        f"SUMMARY: {n_xfail} expected failures, {n_xpass} xpass, "
        f"{n_unexpected} unexpected, {n_hard} hard passes; doc={DOC}"
    )
    ok = n_xfail == 4 and n_xpass == 0 and n_unexpected == 0
    # The raw checkers are still expected to fail overall (limitations remain).
    if ok and not (code1 != 0 and code2 != 0 and code3 != 0):
        print(
            "XPASS overall checker exit: a1/a2/a3 were expected to exit non-zero "
            f"while the four limitations remain (got a1={code1} a2={code2} a3={code3})"
        )
        ok = False
        n_xpass += 1
    print("EXIT:0" if ok else "EXIT:1")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
