#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
project_root="$PWD"
godot_bin="${GODOT_BIN:-$project_root/.local/Godot.app/Contents/MacOS/Godot}"
if [[ ! -x "$godot_bin" ]]; then
  godot_bin="$(command -v godot || true)"
fi
if [[ ! -x "$godot_bin" ]]; then
  echo "Set GODOT_BIN to Godot 4.7.2, or install it in .local/Godot.app." >&2
  exit 1
fi
if [[ ! -f "$project_root/vendor/lessdb/PROVENANCE.json" ]]; then
  echo "Vendored LessDB source missing. Restore vendor/lessdb from this repository." >&2
  exit 1
fi
"$godot_bin" --headless --path "$project_root" --editor --import
export LESSVILLE_PORT="${LESSVILLE_PORT:-8790}"
export LESSVILLE_DATA="${LESSVILLE_DATA:-$project_root/.local/world}"
export LESSVILLE_BIND=127.0.0.1 LESSVILLE_PUBLIC=0 LESSVILLE_LIVE_WEATHER=0
export LESSVILLE_LLM_URL=""
export LESSVILLE_INTERNAL_KEY=""
export LESSVILLE_CKPT_URL=""
export LESSVILLE_API="http://127.0.0.1:$LESSVILLE_PORT"
if curl --silent --fail --max-time 2 "$LESSVILLE_API/api/health" >/dev/null; then
  echo "A service already owns port $LESSVILLE_PORT. Set LESSVILLE_PORT to an unused port." >&2
  exit 1
fi
(cd backend/agentd && cargo +1.96 build --locked)
mkdir -p .local
backend/agentd/target/debug/lessville-agentd >.local/backend.log 2>&1 &
backend_pid=$!
cleanup() { kill -TERM "$backend_pid" 2>/dev/null || true; wait "$backend_pid" || true; }
trap cleanup EXIT
trap 'exit 130' INT TERM
ready=0
for ((attempt=0; attempt<300; attempt++)); do
  if ! kill -0 "$backend_pid" 2>/dev/null; then cat .local/backend.log >&2; exit 1; fi
  if curl --silent --fail --max-time 2 "$LESSVILLE_API/api/health" >/dev/null; then ready=1; break; fi
  sleep 0.2
done
if [[ "$ready" != 1 ]]; then echo "Backend did not become ready; see .local/backend.log" >&2; exit 1; fi
"$godot_bin" --path "$project_root" --rendering-driver opengl3
