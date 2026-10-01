#!/usr/bin/env python3
"""Update the Homebrew tap formula (lessdb/homebrew-lessdb) from
the artifacts in dist/.

Regex-based, so a stale placeholder can never silently keep an old
checksum: version, both download URLs (?v= cache-busters) and both
sha256 lines are replaced from the dist/*.sha256 files of the current
workspace version. Also keeps the caveats text on the current engine
branding (FireflyCloud).

Usage: python3 scripts/update-brew-formula.py
Requires: gh CLI authenticated, dist/ populated by package-release.sh.
"""
import base64
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DIST = ROOT / "dist"

VERSION = re.search(
    r'^version = "([^"]+)"', (ROOT / "Cargo.toml").read_text(), re.M
).group(1)


def sha_of(triple: str) -> str:
    return (DIST / f"lessdb-v{VERSION}-{triple}.tar.gz.sha256").read_text().split()[0]


ARM = sha_of("aarch64-apple-darwin")
X64 = sha_of("x86_64-unknown-linux-gnu")

out = subprocess.run(
    ["gh", "api", "repos/lessdb/homebrew-lessdb/contents/Formula/lessdb.rb"],
    capture_output=True,
    text=True,
    check=True,
)
meta = json.loads(out.stdout)
content = base64.b64decode(meta["content"]).decode()
original = content

content = re.sub(r'version "[0-9.]+"', f'version "{VERSION}"', content)
content = re.sub(
    r'(url "https://lessdb\.pages\.dev/dl/lessdb-v)[0-9.]+(-aarch64-apple-darwin\.tar\.gz\?v=)[0-9a-f]+(")',
    rf"\g<1>{VERSION}\g<2>{ARM[:12]}\g<3>",
    content,
)
content = re.sub(
    r'(url "https://lessdb\.pages\.dev/dl/lessdb-v)[0-9.]+(-x86_64-unknown-linux-gnu\.tar\.gz\?v=)[0-9a-f]+(")',
    rf"\g<1>{VERSION}\g<2>{X64[:12]}\g<3>",
    content,
)
# The two sha256 lines right after each url block.
content = re.sub(
    r'(url "https://lessdb\.pages\.dev/dl/lessdb-v[0-9.]+-aarch64-apple-darwin\.tar\.gz\?v=[0-9a-f]+"\n\s*sha256 ")[0-9a-f]+(")',
    rf"\g<1>{ARM}\g<2>",
    content,
)
content = re.sub(
    r'(url "https://lessdb\.pages\.dev/dl/lessdb-v[0-9.]+-x86_64-unknown-linux-gnu\.tar\.gz\?v=[0-9a-f]+"\n\s*sha256 ")[0-9a-f]+(")',
    rf"\g<1>{X64}\g<2>",
    content,
)

if content == original:
    print(f"formula already current at {VERSION}")
    sys.exit(0)

assert ARM in content and X64 in content, "checksum replacement failed — aborting"
payload = {
    "message": f"lessdb {VERSION}: refresh tarballs + checksums",
    "content": base64.b64encode(content.encode()).decode(),
    "sha": meta["sha"],
}
subprocess.run(
    ["gh", "api", "-X", "PUT",
     "repos/lessdb/homebrew-lessdb/contents/Formula/lessdb.rb",
     "--input", "-"],
    input=json.dumps(payload),
    text=True,
    check=True,
)
print(f"formula bumped to {VERSION} (arm {ARM[:12]}…, x64 {X64[:12]}…)")
