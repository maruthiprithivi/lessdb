#!/usr/bin/env bash
# Install a launchd agent that runs scripts/cleanup.sh twice a day, so the
# LessDB workspace keeps itself clear of stale build artifacts and caches.
#
# Usage: scripts/install-cleanup-agent.sh   (or: uninstall-cleanup-agent.sh)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LABEL="com.lessdb.cleanup"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
LOG="/tmp/lessdb-cleanup.log"

if [ "${1:-}" = "uninstall" ]; then
    launchctl unload "$PLIST" 2>/dev/null || true
    rm -f "$PLIST"
    echo "removed $PLIST"
    exit 0
fi

mkdir -p "$HOME/Library/LaunchAgents"
cat > "$PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$LABEL</string>
  <key>ProgramArguments</key>
  <array>
    <string>/bin/bash</string>
    <string>$ROOT/scripts/cleanup.sh</string>
    <string>--check</string>
    <string>30</string>
  </array>
  <key>StartInterval</key><integer>43200</integer>
  <key>RunAtLoad</key><true/>
  <key>StandardOutPath</key><string>$LOG</string>
  <key>StandardErrorPath</key><string>$LOG</string>
</dict>
</plist>
EOF

launchctl unload "$PLIST" 2>/dev/null || true
launchctl load "$PLIST"
echo "installed: $PLIST"
echo "runs scripts/cleanup.sh --check 30 every 12h (log: $LOG)"
echo "remove with: scripts/install-cleanup-agent.sh uninstall"
