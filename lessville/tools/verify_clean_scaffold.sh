#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCAFFOLD_DIR="$(mktemp -d "${TMPDIR:-/tmp}/generic-scaffold.XXXXXX")"
trap 'rm -rf -- "$SCAFFOLD_DIR"' EXIT
rsync -a --exclude '.git/' --exclude '.godot/' --exclude 'dist/' \
  --exclude 'site/' --exclude '.manus-logs/' --exclude 'test-output/' \
  "$ROOT/" "$SCAFFOLD_DIR/"
# The first import is already strict; do not mask a failing bootstrap import.
bash "$SCAFFOLD_DIR/tools/verify.sh" "$@"
echo "[clean-scaffold] PASS"
