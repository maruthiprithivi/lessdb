#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ $# -gt 1 || ( $# -eq 1 && "$1" != "--export" ) ]]; then
  echo "Usage: tools/verify.sh [--export]" >&2
  exit 2
fi
cd "$PROJECT_DIR"
GODOT_BIN="${GODOT_BIN:-$PROJECT_DIR/.local/Godot.app/Contents/MacOS/Godot}"
if [[ ! -x "$GODOT_BIN" ]]; then GODOT_BIN="$(command -v godot || true)"; fi
if [[ ! -x "$GODOT_BIN" ]]; then echo "Set GODOT_BIN to Godot 4.7.2." >&2; exit 1; fi
"$GODOT_BIN" --headless --path . --editor --import
"$GODOT_BIN" --headless --path . res://test/test_suite.tscn
"$GODOT_BIN" --headless --path . --script res://test/weather_consistency.gd
if [[ "${1:-}" == "--export" ]]; then
  GODOT_BIN="$GODOT_BIN" ./tools/export_web.sh
fi
