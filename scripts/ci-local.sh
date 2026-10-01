#!/usr/bin/env bash
# Incremental CI gate — runs on the build box (ssh 192.168.0.2), NOT on
# GitHub Actions. Invoke it through scripts/ci-on-box.sh from the Mac
# (which syncs the tree over), or directly once a checkout lives there:
#
#   ssh lessdb@192.168.0.2 \
#     'cd /sloth/lessdb-bench/ci && scripts/ci-local.sh'      # fast gate
#   ... scripts/ci-local.sh --full        # + release build, MinIO e2e, bench
#
# GitHub's ci.yml is manual-only (workflow_dispatch) and reserved for
# accumulated "final" runs.
set -euo pipefail
cd "$(dirname "$0")/.."
# Rust toolchain comes from rust-toolchain.toml (1.96 + rustfmt + clippy),
# exactly as GitHub's ci.yml resolves it.
FULL=0
[ "${1:-}" = "--full" ] && FULL=1

echo "== fmt =="
cargo fmt --all --check

echo "== clippy (warnings denied) =="
cargo clippy --workspace --all-targets -- -D warnings

echo "== tests =="
cargo test --workspace

echo "== sqllogictest corpus =="
cargo run --release -p less-sqllogictest -- tests/sqllogictest

if [ "$FULL" = 1 ]; then
  echo "== release build (cloud + gpu) =="
  cargo build --release --features less-cli/cloud,less-cli/gpu

  echo "== MinIO e2e (S3 shared storage) =="
  MINIO_NAME="lessdb-minio-local-$$-$RANDOM"
  MINIO_PORT=$((19000 + RANDOM % 2000))
  docker run -d --rm --name "$MINIO_NAME" -p "127.0.0.1:${MINIO_PORT}:9000" \
    -e MINIO_ROOT_USER=minioadmin -e MINIO_ROOT_PASSWORD=minioadmin \
    minio/minio:latest server /data --console-address :9001
  for _ in $(seq 1 30); do
    if curl -sf "http://127.0.0.1:${MINIO_PORT}/minio/health/live"; then break; fi
    sleep 1
  done
  curl -fsSL https://dl.min.io/client/mc/release/linux-amd64/mc -o /tmp/mc
  chmod +x /tmp/mc
  /tmp/mc alias set local "http://127.0.0.1:${MINIO_PORT}" minioadmin minioadmin
  /tmp/mc mb -p local/lessdb
  LESSDB=./target/release/lessdb \
    AWS_ENDPOINT="http://127.0.0.1:${MINIO_PORT}" AWS_ALLOW_HTTP=true \
    scripts/minio-e2e.sh
  docker rm -f "$MINIO_NAME"

  echo "== benchmark smoke (SF 0.001, no MCP) =="
  cargo build --release --manifest-path bench/Cargo.toml
  ./bench/target/release/less-bench --sf 0.001 --skip-mcp
fi

echo "ci-local: OK"
