#!/usr/bin/env bash
# Regenerate every checksum manifest from the artifacts in dist/.
#
# Writes:
#   website/downloads/manifest.json   downloads page + install.sh checksums
#   website/downloads/latest.json     triple -> file (version-agnostic install)
#   packages/npm/platforms.json       triple -> sha256 (npm wrapper verify)
#
# Run once AFTER both platform builds have populated dist/ (see
# scripts/package-release.sh), before uploading to R2 and deploying.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
DIST="$ROOT/dist"

PLATFORM_LABELS='{
  "aarch64-apple-darwin": "macOS (Apple Silicon)",
  "x86_64-apple-darwin": "macOS (Intel)",
  "x86_64-unknown-linux-gnu": "Linux x86_64",
  "aarch64-unknown-linux-gnu": "Linux aarch64"
}'

python3 - "$DIST" "$PLATFORM_LABELS" <<'PY'
import json, os, sys

dist = sys.argv[1]
labels = json.loads(sys.argv[2])

entries = []   # downloads manifest
latest = {}    # triple -> base file
platforms = {} # triple -> base sha256
for sha_file in sorted(os.listdir(dist)):
    if not sha_file.endswith(".tar.gz.sha256"):
        continue
    tgz = sha_file.removesuffix(".sha256")
    path = os.path.join(dist, tgz)
    if not os.path.exists(path):
        continue
    sha = open(os.path.join(dist, sha_file)).read().split()[0]
    size = os.path.getsize(path)
    # lessdb-v0.2.0-aarch64-apple-darwin[-cloud].tar.gz
    rest = tgz.removeprefix("lessdb-v").removesuffix(".tar.gz")
    _version, _, rest = rest.partition("-")
    if "-cloud" in rest:
        triple = rest.replace("-cloud", "")
        cloud = True
    else:
        triple = rest
        cloud = False
    label = labels.get(triple, triple)
    entries.append({
        "platform": f"{label} \u2014 {'cloud' if cloud else 'base'}",
        "file": tgz,
        "url": f"/dl/{tgz}?v={sha[:12]}",
        "sha256": sha,
        "size": size,
    })
    if not cloud:
        latest[triple] = tgz
        platforms[triple] = sha

# deterministic order: base before cloud, then by file name
entries.sort(key=lambda e: (not e["file"].endswith("-cloud.tar.gz"), e["file"]))

os.makedirs("website/downloads", exist_ok=True)
with open("website/downloads/manifest.json", "w") as f:
    json.dump(entries, f, indent=2, ensure_ascii=False)
    f.write("\n")
with open("website/downloads/latest.json", "w") as f:
    json.dump({"version": entries[0]["file"].removeprefix("lessdb-v").split("-")[0],
               "files": dict(sorted(latest.items()))}, f, indent=2)
    f.write("\n")
os.makedirs("packages/npm", exist_ok=True)
with open("packages/npm/platforms.json", "w") as f:
    json.dump(dict(sorted(platforms.items())), f, indent=2)
    f.write("\n")

print(f"manifest.json: {len(entries)} artifacts")
print(f"latest.json: {sorted(latest)}")
print(f"platforms.json: {sorted(platforms)}")
PY
