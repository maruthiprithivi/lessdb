#!/usr/bin/env bash
# Bump the LessDB version everywhere it lives.
#
# Usage: scripts/bump-version.sh 0.3.0
#
# The workspace Cargo.toml is the single source of truth (every crate
# inherits `version.workspace = true`, and `less_common::VERSION` is
# `env!("CARGO_PKG_VERSION")`). The npm wrapper tracks the same number;
# everything else (tarball names, manifests, brew formula) derives from
# it at release time.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

NEW="${1:?usage: bump-version.sh <x.y.z>}"
case "$NEW" in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) echo "version must look like x.y.z (got '$NEW')" >&2; exit 2 ;;
esac

OLD="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
if [ "$OLD" = "$NEW" ]; then
  echo "already at $NEW"
  exit 0
fi

# 1. Workspace (crates inherit it).
sed -i '' -e "s/^version = \"$OLD\"/version = \"$NEW\"/" Cargo.toml 2>/dev/null \
  || sed -i    -e "s/^version = \"$OLD\"/version = \"$NEW\"/" Cargo.toml

# 2. npm wrapper package.
sed -i '' -e "s/\"version\": \"$OLD\"/\"version\": \"$NEW\"/" packages/npm/package.json 2>/dev/null \
  || sed -i    -e "s/\"version\": \"$OLD\"/\"version\": \"$NEW\"/" packages/npm/package.json

# 3. Static version mentions that would otherwise drift.
sed -i '' -e "s/lessdb $OLD/lessdb $NEW/g" website/content/docs/getting-started.md 2>/dev/null \
  || sed -i    -e "s/lessdb $OLD/lessdb $NEW/g" website/content/docs/getting-started.md
sed -i '' -e "s/brand-version mono\">v$OLD</brand-version mono\">v$NEW</" website/index.html 2>/dev/null \
  || sed -i    -e "s/brand-version mono\">v$OLD</brand-version mono\">v$NEW</" website/index.html
# PRAGMA version golden in the sqllogictest corpus.
sed -i '' -e "s/^$OLD\$/$NEW/" tests/sqllogictest/11_show.slt 2>/dev/null \
  || sed -i    -e "s/^$OLD\$/$NEW/" tests/sqllogictest/11_show.slt

# 4. Stale generated artifacts (regenerated on the next release).
rm -f dist/lessdb-v"$OLD"-*.tar.gz dist/lessdb-v"$OLD"-*.tar.gz.sha256 \
      packages/npm/lessdb-"$OLD".tgz

echo "bumped $OLD -> $NEW"
echo "next: build + package (scripts/package-release.sh), then upload + manifests"
