#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT/backend/agentd"

. "${HOME}/.cargo/env" 2>/dev/null || true
export CARGO_HOME="${CARGO_HOME:-${HOME}/.cargo}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target-agentd}"

FILTER="${1:-scenario}"
cargo test --offline "$FILTER"
