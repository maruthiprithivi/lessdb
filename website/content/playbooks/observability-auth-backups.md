# Observability, auth, and backups

## Prometheus

`lessdb server` exposes `GET /metrics` in Prometheus text format (also
`lessdb metrics` for one-shot CLI use). Instrumented end to end: query
counts + duration histograms, rows returned/inserted, parts
written/merged/scanned/**pruned** (pruning counters make bloom/index
effectiveness visible), HTTP route/status, auth failures, and the
memory-pool gauge.

```yaml
scrape_configs:
  - job_name: lessdb
    static_configs:
      - targets: ["db-host:7080"]
    metrics_path: /metrics
```

Handy dashboard lines:

```sh
curl -s db-host:7080/metrics | grep -E "lessdb_(queries_total|parts_pruned|memory_pool_bytes)"
```

## LDAP / Active Directory for the HTTP API

```sh
lessdb init --auth '{
  "ldap": {
    "url": "ldaps://ad.example.com", "base_dn": "DC=example,DC=com",
    "bind_dn": "svc-lessdb@example.com", "bind_password": "…",
    "role_mapping": {"DB Admins": "admin", "Analysts": "read"}
  }}'
lessdb server --dir /var/lib/lessdb --listen 0.0.0.0:7080
```

The handshake: service bind → locate user DN → **rebind as the user**
(password verification happens in the directory) → group membership →
role. Fail-closed: an ungrouped user is denied unless `default_role` is
set. A dev-mode file authenticator covers local accounts and tests.

## TLS

```sh
lessdb server --dir /var/lib/lessdb --tls-cert cert.pem --tls-key key.pem
```

axum-server + rustls; an e2e test generates a self-signed cert and
completes a client handshake against `/health`.

## Backups

Immutable parts make backups a copy job — and with FireflyCloud the
bucket *is* the durable copy:

```sh
# local Firefly: rsync the data dir (parts are immutable, so a live copy is safe)
rsync -a /var/lib/lessdb/ backup-host:/backups/lessdb-$(date +%F)/

# FireflyCloud: replicate the bucket (rclone, s5cmd, or a second R2
# bucket with object-level versioning), or rely on R2's versioning/retention.
rclone sync s3:lessdb-shared s3:lessdb-backup --s3-endpoint https://<ACCOUNT_ID>.r2.cloudflarestorage.com
```

Restore = put the directory back (local) or point a new node at the
restored bucket prefix (shared). The WAL replays any tail, and torn
part directories are swept at open — a crash mid-write never corrupts
the table.
