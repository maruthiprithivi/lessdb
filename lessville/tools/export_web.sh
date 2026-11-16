#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
GODOT_BIN="${GODOT_BIN:-$PWD/.local/Godot.app/Contents/MacOS/Godot}"
if [[ ! -x "$GODOT_BIN" ]]; then GODOT_BIN="$(command -v godot || true)"; fi
if [[ ! -x "$GODOT_BIN" ]]; then echo "Set GODOT_BIN to Godot 4.7.2." >&2; exit 1; fi
if [[ "$("$GODOT_BIN" --version)" != 4.7.2.* ]]; then echo "Godot 4.7.2 and matching Web templates are required." >&2; exit 1; fi
mkdir -p dist
"$GODOT_BIN" --headless --path . --editor --import
"$GODOT_BIN" --headless --path . --export-release Web dist/index.html
test -s dist/index.html && test -s dist/index.js && test -s dist/index.wasm && test -s dist/index.pck
