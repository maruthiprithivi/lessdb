#!/bin/sh
# LessDB installer — fetches the right release archive for this machine,
# verifies its SHA-256, and installs `lessdb` to ~/.local/bin.
#
# Usage:  curl -fsSL https://lessdb.dev/install.sh | sh
# Env:    LESSDB_DOWNLOAD_BASE  override the artifact base URL
#         LESSDB_VERSION        pin a version (default: latest release)
set -eu

BASE="${LESSDB_DOWNLOAD_BASE:-https://lessdb.dev/dl}"
DEST="${LESSDB_INSTALL_DIR:-$HOME/.local/bin}"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)  TRIPLE="aarch64-apple-darwin" ;;
  Darwin-x86_64) TRIPLE="x86_64-apple-darwin" ;;
  Linux-x86_64)  TRIPLE="x86_64-unknown-linux-gnu" ;;
  Linux-aarch64) TRIPLE="aarch64-unknown-linux-gnu" ;;
  *) echo "lessdb: unsupported platform $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

# Version: explicit pin wins; otherwise take the file the latest release
# manifest maps to this triple, so the installer never hardcodes a version.
if [ -n "${LESSDB_VERSION:-}" ]; then
  FILE="lessdb-${LESSDB_VERSION}-${TRIPLE}.tar.gz"
else
  LATEST_URL="${LESSDB_MANIFEST_URL:-https://lessdb.dev/downloads/latest.json}"
  FILE=$(curl -fsSL "$LATEST_URL" 2>/dev/null \
    | tr -d '\n ' \
    | sed -n "s/.*\"$TRIPLE\":\"\(lessdb-v[0-9.]*-$TRIPLE.tar.gz\)\".*/\1/p" \
    | head -1 || true)
  if [ -z "$FILE" ]; then
    echo "lessdb: no prebuilt release for $TRIPLE (see https://lessdb.dev/downloads/)" >&2
    exit 1
  fi
fi

# Fetch the release manifest (served fresh by Pages) and take the
# checksum + cache-busting query from it, so re-uploaded artifacts are
# never served stale from an edge cache.
WANT=""
MANIFEST_URL="${LESSDB_MANIFEST_URL:-https://lessdb.dev/downloads/manifest.json}"
MANIFEST=$(curl -fsSL "$MANIFEST_URL" 2>/dev/null || true)
if [ -n "$MANIFEST" ]; then
  WANT=$(echo "$MANIFEST" | tr -d '\n ' | sed -n "s/.*\"file\":\"$FILE\",\"url\":\"[^\"]*\",\"sha256\":\"\([0-9a-f]*\)\".*/\1/p" | head -1)
fi
VQ=""
[ -n "$WANT" ] && VQ="?v=${WANT%${WANT#????????????}}"
URL="$BASE/$FILE$VQ"

echo "lessdb: downloading $URL"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if command -v curl >/dev/null 2>&1; then
  curl -fsSL "$URL" -o "$TMP/$FILE"
else
  wget -q "$URL" -O "$TMP/$FILE"
fi

# sha256 verification
SUM=""
if command -v sha256sum >/dev/null 2>&1; then
  SUM=$(sha256sum "$TMP/$FILE" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then
  SUM=$(shasum -a 256 "$TMP/$FILE" | awk '{print $1}')
fi
if [ -n "$SUM" ]; then
  WANT=""
  if command -v curl >/dev/null 2>&1; then
    WANT=$(curl -fsSL "$BASE/$FILE.sha256$VQ" | awk '{print $1}')
  fi
  if [ -n "$WANT" ] && [ "$SUM" != "$WANT" ]; then
    echo "lessdb: checksum mismatch (got $SUM, want $WANT)" >&2
    exit 1
  fi
  echo "lessdb: sha256 ok ($SUM)"
fi

mkdir -p "$DEST"
tar -xzf "$TMP/$FILE" -C "$TMP" --strip-components=1
chmod +x "$TMP/lessdb"
mv "$TMP/lessdb" "$DEST/lessdb"
echo "lessdb: installed $DEST/lessdb"
"$DEST/lessdb" --version 2>/dev/null || true

# Add $DEST to PATH in the user's shell rc files so `lessdb` works in any
# new terminal without manual setup. Idempotent; skipped for a custom
# LESSDB_INSTALL_DIR (explicit destinations are the caller's to manage).
if [ -z "${LESSDB_INSTALL_DIR:-}" ] || [ "$DEST" = "$HOME/.local/bin" ]; then
  add_to_path() {
    rc="$1"
    [ -f "$rc" ] || : > "$rc"
    line="export PATH=\"$DEST:\$PATH\"  # lessdb"
    if ! grep -qF "lessdb" "$rc" 2>/dev/null; then
      printf '\n%s\n' "$line" >> "$rc"
      echo "lessdb: added $DEST to PATH in $rc"
    fi
  }
  for rc in "$HOME/.zshrc" "$HOME/.bashrc" "$HOME/.zprofile" "$HOME/.bash_profile"; do
    add_to_path "$rc"
  done
fi
echo "lessdb: done — open a new terminal (or: source ~/.zshrc), then run: lessdb demo"
echo "lessdb: upgrade anytime with:  lessdb upgrade"
