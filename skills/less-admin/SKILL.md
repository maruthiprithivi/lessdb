---
name: less-admin
description: Operate LessDB: initialize databases, create/manage tables, load data, run merges, benchmark performance, start servers, and wire the MCP endpoint for agents. Use whenever the user asks to set up, administer, or troubleshoot LessDB.
---

# LessDB Admin Skill

## Bootstrap

```bash
cargo build --release --features cloud   # cloud = S3/GCS/Azure backends
alias less="$PWD/target/release/less"
less init                                  # local dev mode
less init --shared s3://bucket/prefix      # cloud-native: compute/storage split
```

With `--shared`, all durable FireflyCloud state (parts + table
manifests) lives in the shared store; local disk is ephemeral compute
scratch, and any node initialized against the same URL sees the same
tables immediately. Local Firefly tables stay node-local by design.

## Tables

```sql
-- local storage engine (vertical scale)
CREATE TABLE t (id Int64, kind String, amount Float64, ts DateTime)
ENGINE = Firefly ORDER BY (kind, id) UNIQUE (kind) COMPRESSION = 'zstd';

-- shared object storage engine (horizontal scale)
CREATE TABLE t2 (...) ENGINE = FireflyCloud ORDER BY (...) UNIQUE (...);
```

Rules: `UNIQUE` columns must be a prefix of `ORDER BY` columns; table and
column names are `[A-Za-z_][A-Za-z0-9_]*`.

## Loading data

```bash
less insert events --csv events.csv          # header must name every column
less insert events --jsonl events.jsonl      # or NDJSON/JSON array
less tables && less describe events && less parts events
```

CSV cells that are empty, `\N` or `NULL` become NULLs; values coerce to the
column types.

## Maintenance & tuning

```bash
less optimize events        # merge parts, enforce UNIQUE (all tables if omitted)
less bench --rows 5000000   # insert + scan/filter/group-by throughput
less gpu                    # GPU vs CPU kernel benchmark (--features gpu build)
```

Auto-merge triggers at 8 parts by default (`auto_merge_parts` in config);
`flush_rows` controls insert buffer size (default 262,144).

## Serving

```bash
less server --addr 0.0.0.0:7080      # GET /health, /metrics; POST /v1/sql (json|arrow)
less mcp --dir /data/mydb            # MCP over stdio for AI agents
```

### Authentication (LDAP / Active Directory)

```bash
less init --auth @auth.json   # or --auth '<inline json>'
# auth.json:
# { "ldap": { "url": "ldaps://ad.corp.example.com:636",
#             "base_dn": "DC=corp,DC=example,DC=com",
#             "bind_dn": "CN=lessdb-svc,OU=Services,DC=corp,DC=example,DC=com",
#             "bind_password": "...",
#             "role_mapping": { "DB-Admins": "admin", "DB-Users": "read" } } }
```

* Roles: `admin`, `read`, `write`. Unmapped users are denied (fail closed)
  unless `default_role` is set.
* `POST /v1/admin/optimize` requires `admin` or `write`.
* Dev without a directory: `{"file": {"users": {"alice": {"password":
  "pw", "role": "admin"}}}}` (plaintext — dev only).
* Client: `curl -u alice:secret -H 'content-type: application/json'
  -d '{"sql":"SELECT 1"}' http://host:7080/v1/sql`. Put TLS in front for
  production (Basic auth over plaintext HTTP is not secure).

### Prometheus

```yaml
scrape_configs:
  - job_name: lessdb
    static_configs: [{ targets: ["db-host:7080"] }]
```

Watch `lessdb_parts_pruned_total` vs `lessdb_parts_scanned_total` for
pruning effectiveness, `lessdb_query_duration_seconds` for latency, and
`lessdb_auth_failures_total` for abuse. `less metrics` prints the same
exposition for a CLI process (e.g. after `less bench`).

Claude Desktop config example:

```json
{ "mcpServers": { "lessdb": { "command": "less", "args": ["mcp", "--dir", "/data/mydb"] } } }
```

## Troubleshooting

- Query returns duplicate rows for a UNIQUE column: run `less optimize`.
- Query slower than expected: `EXPLAIN` it — many parts scanned means the
  filter doesn't match the sort key / unique columns.
- Disk usage: `less describe <table>` shows rows, parts and size.
- Shared tables: parts live under `<data_dir>/shared/tables/<t>/parts/`;
  data_dir defaults to `.less` (`--dir` / `LESSDB_DIR` to override).
