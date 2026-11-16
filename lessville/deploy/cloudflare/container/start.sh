#!/bin/sh
# Reassemble the release binary from its image-layer chunks and verify it
# against the sha256 recorded in the release marker before starting.
set -eu
BIN=/tmp/lessville-agentd
cat /opt/lessville/part-* > "$BIN"
WANT=$(sed -n 's/^binary sha256=\([0-9a-f]\{64\}\)$/\1/p' /etc/lessville-release)
GOT=$(sha256sum "$BIN" | cut -d' ' -f1)
if [ -z "$WANT" ] || [ "$WANT" != "$GOT" ]; then
  echo "lessville-start: binary checksum mismatch; refusing to start" >&2
  exit 1
fi
chmod 0755 "$BIN"
exec "$BIN"
