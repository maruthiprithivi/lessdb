#!/usr/bin/env bash
# LessDB workspace cleanup — keep the local disk healthy without losing
# fast incremental builds.
#
# Tiers:
#   scripts/cleanup.sh                safe clean (release artifacts,
#                                      incremental caches, bench/SDK targets,
#                                      stale registry versions, /tmp junk);
#                                      debug build deps stay for iteration.
#   scripts/cleanup.sh --aggressive   full `cargo clean` on top of the safe
#                                      tier (frees everything; next build is
#                                      a from-scratch ~3 min rebuild).
#   scripts/cleanup.sh --check N      no-op (exit 0) while the workspace
#                                      volume has >= N GB free; otherwise run
#                                      the safe tier and exit 1 (for cron/CI
#                                      gating).
#
# Idempotent; safe to run while nothing is building.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

MODE=safe
CHECK_GB=0
while [ $# -gt 0 ]; do
    case "$1" in
        --aggressive) MODE=aggressive; shift ;;
        --check)
            CHECK_GB="${2:-0}"
            shift 2
            ;;
        *) shift ;;
    esac
done

free_gb() {
    # Free space on the volume holding the workspace, in whole GiB.
    local avail
    avail=$(df -Pk "$ROOT" | awk 'NR==2 {print $4}')
    echo $((avail / 1024 / 1024))
}

BEFORE=$(free_gb)

if [ "$CHECK_GB" -gt 0 ] && [ "$BEFORE" -ge "$CHECK_GB" ]; then
    echo "disk ok: ${BEFORE}G free (threshold ${CHECK_GB}G) — nothing to clean"
    exit 0
fi

echo "cleanup: ${BEFORE}G free before, mode=$MODE"

# 1. Release artifacts (debug deps survive: they're what makes iteration fast).
if command -v cargo >/dev/null 2>&1; then
    cargo clean --release 2>/dev/null || true
fi

# 2. Incremental compilation cache (safe; rebuilt on demand).
rm -rf target/debug/incremental

# 2b. DuckDB's bundled C++ build outputs (the largest regenerable chunk;
#     rebuilt in ~2-3 min on next need).
rm -rf target/debug/build/libduckdb-sys-*

# 3. Bench and SDK build dirs (rebuilt on demand).
rm -rf bench/target sdks/python/target sdks/node/target

# 4. Cargo registry: drop crate versions no Cargo.lock references.
python3 - "$ROOT" <<'PY'
import os, re, shutil, sys
root = sys.argv[1]
wanted = set()
for lock in ["Cargo.lock", "bench/Cargo.lock", "sdks/python/Cargo.lock", "sdks/node/Cargo.lock"]:
    p = os.path.join(root, lock)
    if not os.path.exists(p):
        continue
    text = open(p).read()
    for m in re.finditer(r'\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"', text):
        wanted.add(f'{m.group(1)}-{m.group(2)}')
removed = 0
for base, sub in [("src", ""), ("cache", ".crate")]:
    idx = f'{root}/.cargo/registry/{base}/index.crates.io-1949cf8c6b5b557f'
    if not os.path.isdir(idx):
        continue
    for entry in os.listdir(idx):
        name = entry[: -len(sub)] if sub and entry.endswith(sub) else entry
        if name not in wanted:
            p = os.path.join(idx, entry)
            (shutil.rmtree(p, ignore_errors=True) if os.path.isdir(p) else os.remove(p))
            removed += 1
print(f"  registry: removed {removed} stale crate version(s)")
PY

# 5. This workspace's test/smoke leftovers in /tmp.
rm -rf /tmp/less-* /tmp/ci /tmp/ci.zip /tmp/ci-log /tmp/ci-log.zip \
       /tmp/ci2 /tmp/ci2.zip /tmp/ci3 /tmp/ci3.zip /tmp/mc /tmp/mc-config \
       /tmp/rt_test /tmp/rt_test.rs /tmp/footer8.bin /tmp/footer8b.bin \
       /tmp/oldpart.parquet /tmp/newpart.parquet /tmp/wps.txt 2>/dev/null || true

# 6. Aggressive tier: the whole target dir.
CRITICAL=0
if [ "$CHECK_GB" -gt 0 ]; then
    NOW=$(free_gb)
    if [ "$NOW" -lt $((CHECK_GB / 2)) ]; then
        CRITICAL=1
    fi
fi
if [ "$MODE" = aggressive ] || [ "$CRITICAL" = 1 ]; then
    echo "cleanup: critical space — full cargo clean"
    if command -v cargo >/dev/null 2>&1; then
        cargo clean 2>/dev/null || true
    else
        rm -rf target
    fi
fi

AFTER=$(free_gb)
echo "cleanup: ${AFTER}G free after (freed ~$((AFTER > BEFORE ? AFTER - BEFORE : 0))G)"
if [ "$CHECK_GB" -gt 0 ]; then
    exit 1 # was low and needed cleaning
fi
