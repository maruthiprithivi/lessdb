#!/usr/bin/env bash
# Run the incremental CI gate on the build box (ssh 192.168.0.2) without
# touching GitHub Actions. Syncs this working tree to the box, then runs
# scripts/ci-local.sh there (the box keeps its own target/ for incremental
# builds). GitHub's ci.yml is manual-only and reserved for final batches.
#
# Usage (from this repo on the Mac):
#   scripts/ci-on-box.sh            # fast gate: fmt/clippy/test/sql-logic-test
#   scripts/ci-on-box.sh --full     # + release build, MinIO e2e, bench smoke
set -euo pipefail
cd "$(dirname "$0")/.."
BOX="${LESSDB_BOX:-lessdb@192.168.0.2}"
DIR="/sloth/lessdb-bench/ci"
EXTRA="${1:-}"

echo "ci-on-box: syncing working tree -> $BOX:$DIR (excl. target/.git/dist/.cargo)"
rsync -az --delete \
  --exclude .git --exclude target --exclude dist --exclude .cargo \
  --exclude bench/target --exclude bench/results \
  ./ "$BOX:$DIR/"

echo "ci-on-box: running scripts/ci-local.sh${EXTRA:+ $EXTRA}"
ssh -o ConnectTimeout=15 "$BOX" "cd '$DIR' && scripts/ci-local.sh $EXTRA"
