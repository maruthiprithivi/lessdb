#!/usr/bin/env bash
# Bootstrap LessDB's Cloudflare side: R2 buckets for the shared storage
# tier + the public downloads host, and the exact commands to point a
# database node at R2.
#
# Requires: wrangler authenticated (`wrangler whoami`).
# For the live node test you need an R2 API token (S3 credentials) scoped
# to the lessdb-shared bucket: Dashboard -> R2 -> Manage R2 API Tokens.
set -euo pipefail

WRANGLER="${WRANGLER:-wrangler}"
ACCOUNT_ID="${CLOUDFLARE_ACCOUNT_ID:-$(grep -oE '"account_id"[^,]*' /dev/null 2>/dev/null || true)}"

echo "== 1/4: buckets =="
"$WRANGLER" r2 bucket create lessdb-shared 2>/dev/null \
  || echo "lessdb-shared already exists (ok)"
"$WRANGLER" r2 bucket create lessdb-downloads 2>/dev/null \
  || echo "lessdb-downloads already exists (ok)"

echo
echo "== 2/4: R2 API token =="
echo "Create one in the dashboard: R2 -> Manage R2 API Tokens -> Create API Token,"
echo "permission: Object Read & Write, bucket: lessdb-shared."
echo
echo "== 3/4: point a node at R2 =="
cat <<'EOF'
export AWS_ACCESS_KEY_ID=<access_key_id>
export AWS_SECRET_ACCESS_KEY=<secret_access_key>
export AWS_ENDPOINT_URL=https://<ACCOUNT_ID>.r2.cloudflarestorage.com
export AWS_ALLOW_HTTP=true

lessdb init --dir /var/lib/lessdb --shared s3://lessdb-shared/lessdb
lessdb create --dir /var/lib/lessdb \
  "CREATE TABLE events (ts Timestamp, host Utf8, cpu Float64)
   ENGINE=FireflyCloud ORDER BY (host, ts) TTL ts INTERVAL 30 DAY"
lessdb insert --dir /var/lib/lessdb events --csv events.csv
lessdb sql --dir /var/lib/lessdb "SELECT host, avg(cpu) FROM events GROUP BY host"
EOF

echo
echo "== 4/4: verify from a second node =="
cat <<'EOF'
# any other machine, same credentials, same bucket prefix:
lessdb init --dir /tmp/node-b --shared s3://lessdb-shared/lessdb
lessdb tables --dir /tmp/node-b        # discovers `events` by listing the bucket
lessdb sql --dir /tmp/node-b "SELECT count(*) FROM events"
EOF
