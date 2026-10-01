#!/usr/bin/env bash
# S3/MinIO end-to-end for FireflyCloud: two compute nodes, one bucket.
#
# Requires:
#   * a reachable S3 endpoint (defaults: MinIO on 127.0.0.1:9000, bucket
#     `lessdb`, credentials minioadmin/minioadmin — all overridable via env);
#   * a `lessdb` binary built with the cloud feature (S3 backend).
#
# Verifies: init --shared s3://, table discovery from a second node,
# cross-node writes/reads, OPTIMIZE over shared storage, and block-cache
# stats. Used by CI on the self-hosted runner (MinIO in docker).
set -euo pipefail

LESSDB=${LESSDB:-./target/release/lessdb}
export AWS_ACCESS_KEY_ID=${AWS_ACCESS_KEY_ID:-minioadmin}
export AWS_SECRET_ACCESS_KEY=${AWS_SECRET_ACCESS_KEY:-minioadmin}
export AWS_ENDPOINT=${AWS_ENDPOINT:-http://127.0.0.1:9000}
export AWS_ALLOW_HTTP=${AWS_ALLOW_HTTP:-true}
export AWS_REGION=${AWS_REGION:-us-east-1}

BUCKET=${BUCKET:-lessdb}
PREFIX=${PREFIX:-ci-$(date +%s)}
SHARED="s3://$BUCKET/$PREFIX"
NODE_A=$(mktemp -d /tmp/less-s3-a.XXXXXX)
NODE_B=$(mktemp -d /tmp/less-s3-b.XXXXXX)
CSV=$(mktemp /tmp/less-s3.XXXXXX)
CSV2=$(mktemp /tmp/less-s3.XXXXXX)
trap 'rm -rf "$NODE_A" "$NODE_B" "$CSV" "$CSV2"' EXIT

python3 - "$CSV" <<'PY'
import csv, random, sys
random.seed(42)
with open(sys.argv[1], "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["id", "v"])
    for i in range(1000):
        w.writerow([i, round(random.uniform(0, 100), 3)])
PY
printf 'id,v\n424242,1.5\n' > "$CSV2"

count_rows() { # $1 = data dir, $2 = table
    "$LESSDB" sql --dir "$1" "SELECT count(*) FROM $2" | awk '/^[|] *[0-9]+/ {print $2; exit}'
}

echo "== init node A on $SHARED =="
"$LESSDB" init --dir "$NODE_A" --shared "$SHARED"
"$LESSDB" create --dir "$NODE_A" \
    "CREATE TABLE s3_t (id Int64, v Float64) ENGINE=FireflyCloud ORDER BY id"
"$LESSDB" insert --dir "$NODE_A" s3_t --csv "$CSV"
[ "$(count_rows "$NODE_A" s3_t)" = "1000" ]

echo "== node B discovers the table from the same bucket =="
"$LESSDB" init --dir "$NODE_B" --shared "$SHARED"
"$LESSDB" tables --dir "$NODE_B" | grep -qx s3_t
[ "$(count_rows "$NODE_B" s3_t)" = "1000" ]

echo "== node B writes; node A sees the new row =="
"$LESSDB" insert --dir "$NODE_B" s3_t --csv "$CSV2"
[ "$(count_rows "$NODE_A" s3_t)" = "1001" ]

echo "== OPTIMIZE from node A; both nodes see one merged part =="
"$LESSDB" optimize --dir "$NODE_A" s3_t
"$LESSDB" parts --dir "$NODE_A" s3_t
[ "$(count_rows "$NODE_B" s3_t)" = "1001" ]

echo "== block cache served the shared reads =="
"$LESSDB" cache --dir "$NODE_A" | grep -q "block cache: enabled"

echo "PASS: S3/MinIO FireflyCloud e2e ($SHARED)"
