#!/usr/bin/env bash
# Package the npm wrapper for the Cloudflare-hosted npm distribution.
#
# LessDB's npm package is NOT published to registry.npmjs.org — it is
# served from lessdb.dev/npm/ (a Pages Function backed by the
# lessdb-downloads R2 bucket). This script produces the two artifacts
# npm needs and prints the upload commands:
#
#   dist/lessdb-<ver>.tgz       the packed package (installable by URL)
#   dist/npm-lessdb.json        the packument (registry metadata)
#
# Installation (after upload + deploy):
#   npm install -g lessdb --registry https://lessdb.dev/npm/
#   npm install -g https://lessdb.dev/npm/lessdb/-/lessdb-<ver>.tgz
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
[ -n "$VERSION" ] || { echo "could not read version from Cargo.toml" >&2; exit 1; }

# package.json must track the workspace version (bump-version.sh keeps
# them in lockstep).
PKG_VER="$(node -p "require('./packages/npm/package.json').version" 2>/dev/null || true)"
if [ -n "$PKG_VER" ] && [ "$PKG_VER" != "$VERSION" ]; then
  echo "npm package.json is at $PKG_VER but workspace is at $VERSION — run scripts/bump-version.sh first" >&2
  exit 1
fi

DIST="$ROOT/dist"
mkdir -p "$DIST"
TARBALL="$DIST/lessdb-$VERSION.tgz"

# Pack the wrapper (respects package.json "files"). A workspace-local
# npm cache avoids the root-owned ~/.npm cache problem on shared boxes.
( cd packages/npm && npm pack --pack-destination "$DIST" \
    --cache "$ROOT/.npm-cache" --logs-dir "$ROOT/.npm-logs" >/dev/null )
mv "$DIST/lessdb-$VERSION.tgz" "$TARBALL"

python3 - "$VERSION" "$TARBALL" <<'PY'
import base64, hashlib, json, sys

version, tarball = sys.argv[1], sys.argv[2]
raw = open(tarball, "rb").read()
sha1 = hashlib.sha1(raw).hexdigest()
sha512 = base64.b64encode(hashlib.sha512(raw).digest()).decode()
pkg = json.load(open("packages/npm/package.json"))

url = f"https://lessdb.dev/npm/lessdb/-/lessdb-{version}.tgz"
packument = {
    "name": pkg["name"],
    "description": pkg["description"],
    "license": pkg["license"],
    "homepage": pkg["homepage"],
    "repository": pkg["repository"],
    "keywords": pkg.get("keywords", []),
    "dist-tags": {"latest": version},
    "versions": {
        version: {
            **{k: v for k, v in pkg.items() if k in (
                "name", "version", "description", "license", "homepage",
                "repository", "keywords", "bin", "engines", "scripts")},
            "dist": {
                "tarball": url,
                "shasum": sha1,
                "integrity": f"sha512-{sha512}",
            },
        }
    },
}
with open("dist/npm-lessdb.json", "w") as f:
    json.dump(packument, f, indent=2)
    f.write("\n")
print(f"packed {tarball} (shasum {sha1[:12]}…)")
print(f"packument dist/npm-lessdb.json -> https://lessdb.dev/npm/lessdb")
PY

echo
echo "Upload to R2:"
echo "  wrangler r2 object put lessdb-downloads/npm/lessdb-$VERSION.tgz $TARBALL"
echo "  wrangler r2 object put lessdb-downloads/npm/lessdb            dist/npm-lessdb.json"
