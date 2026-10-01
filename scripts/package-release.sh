#!/usr/bin/env bash
# Package LessDB release binaries for the CURRENT host platform.
#
# Builds two variants of the `lessdb` CLI — base, and cloud (S3/GCS/Azure
# object-store backends for FireflyCloud on R2 etc.) — tars them with
# LICENSE + RELEASE notes, and writes SHA-256 files plus a manifest
# fragment into dist/. Run once per target platform (e.g. on the macOS
# dev box and on the Linux benchmark instance), then upload dist/* to the
# lessdb-downloads R2 bucket (see upload step below).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
# Version comes from the workspace Cargo.toml (single source of truth);
# an explicit argument still overrides for one-off builds.
VERSION="v${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}"
TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
DIST="$ROOT/dist"
mkdir -p "$DIST"

export CARGO_HOME="${CARGO_HOME:-$ROOT/.cargo}"
RUSTUP_TOOLCHAIN=stable cargo build --release -p less-cli
RUSTUP_TOOLCHAIN=stable cargo build --release -p less-cli --features less-cli/cloud

BASE="target/release/lessdb"
CLOUD="target/release/lessdb"
NAME="lessdb-${VERSION}-${TRIPLE}"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/$NAME" "$STAGE/${NAME}-cloud"

GIT_SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"

{
  echo "LessDB $VERSION"
  echo "git: $GIT_SHA"
  echo "platform: $TRIPLE"
  echo "built: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
} > "$STAGE/RELEASE"

for f in "$STAGE/$NAME" "$STAGE/${NAME}-cloud"; do
  cp "$ROOT/LICENSE" "$f/LICENSE"
  cp "$STAGE/RELEASE" "$f/RELEASE"
done
cp "$BASE" "$STAGE/$NAME/lessdb"
cp "$CLOUD" "$STAGE/${NAME}-cloud/lessdb"

for d in "$NAME" "${NAME}-cloud"; do
  tar -C "$STAGE" -czf "$DIST/$d.tar.gz" "$d"
  ( cd "$DIST" && shasum -a 256 "$d.tar.gz" > "$d.tar.gz.sha256" )
  echo "packaged $DIST/$d.tar.gz ($(du -h "$DIST/$d.tar.gz" | cut -f1))"
done

echo
echo "Upload to R2 (from any machine with wrangler auth):"
echo "  wrangler r2 object put lessdb-downloads/$NAME.tar.gz        $DIST/$NAME.tar.gz"
echo "  wrangler r2 object put lessdb-downloads/$NAME.tar.gz.sha256 $DIST/$NAME.tar.gz.sha256"
echo "  wrangler r2 object put lessdb-downloads/${NAME}-cloud.tar.gz        $DIST/${NAME}-cloud.tar.gz"
echo "  wrangler r2 object put lessdb-downloads/${NAME}-cloud.tar.gz.sha256 $DIST/${NAME}-cloud.tar.gz.sha256"
