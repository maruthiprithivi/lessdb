# LessDB

⚗️ **Experimental** — LessDB is built with coding agents: a DBMS for AI,
with AI. Human review at every step; expect rough edges and rapid change.

LessDB is an experimental Rust analytical database using DataFusion and
immutable columnar parts. It targets lightweight deployment alongside agents.

**Current maturity:** this clean source baseline does not include the private
in-progress durability, concurrent visibility or access-boundary fixes.
Transaction isolation, crash recovery, tenant isolation, replication and mesh
sharing require further implementation and validation. Feature descriptions
below describe intended interfaces, not a production-readiness certification.
Do not use this baseline for irreplaceable data or untrusted remote access.
There is no measured claim of universal performance superiority over DuckDB.

It combines:

* **Columnar compression** (zstd/lz4 per column) and immutable Firefly
  parts — including **FireflyCloud**, with parts in shared object storage;
* **DuckDB's** embeddability: one binary / one library, Arrow-native,
  works with your existing pandas/pyarrow/JS tooling;
* **StarRocks/Doris-class joins** via the DataFusion SQL engine (hash
  joins, cost-based optimizer, full SQL surface);
* **Uniqueness constraints** with bloom-filter pruning and
  replacing-merge deduplication;
* **Native AI-agent access** through an MCP server, plus installable agent
  skills.

## Quickstart

```bash
cargo build --release
alias lessdb="$PWD/target/release/lessdb"

lessdb init
lessdb create "CREATE TABLE events (id Int64, kind String, amount Float64, ts DateTime) \
             ENGINE = Firefly ORDER BY (kind, id) UNIQUE (kind)"
lessdb insert events --csv events.csv
lessdb sql "SELECT kind, count(*), sum(amount) FROM events GROUP BY kind ORDER BY kind"
less                # rich interactive TUI (also: lessdb sql)
lessdb bench --rows 5000000
```

The bare `lessdb` command opens a full-screen TUI: table sidebar, scrollable
results with column types, query editor with history, and psql-style meta
commands (`\t` tables, `\d` describe, `\p` parts, `\o` optimize, `\h`
help, `\q` quit). Piped stdout falls back to a plain line-based shell.

FireflyCloud tables (scale-out storage) work with the same SQL — parts
go to the shared object store instead of local disk:

```sql
CREATE TABLE events_shared (id Int64, kind String, amount Float64)
ENGINE = FireflyCloud ORDER BY (kind, id) UNIQUE (kind);
```

### Cloud-native: compute and storage separated

Point FireflyCloud at object storage and compute nodes become stateless —
durable state (data parts *and* table manifests) lives in the shared store,
local disk is only ephemeral buffers/caches:

```bash
cargo build --release --features cloud    # S3/GCS/Azure backends
lessdb init --shared s3://my-bucket/lessdb  # config.json remembers it
lessdb create "CREATE TABLE t (...) ENGINE = FireflyCloud ORDER BY (...)"
# any other node with the same --shared URL sees the table immediately:
lessdb tables --dir /another/compute/node
```

Backends: `s3://bucket/prefix`, `gcs://bucket/prefix`,
`az://account/container/prefix`, `file:///path`, `memory://` (credentials
from standard AWS/GOOGLE/AZURE env vars). Reads never touch local disk for
shared tables; killed nodes are replaced, not recovered.

Multiple writers on the same shared store coordinate through a CAS
metadata layer: part publication is a conditional create of `meta.json`,
and merges claim their input parts first — two nodes can flush and
`OPTIMIZE` concurrently without losing or double-merging rows.

A block cache makes repeated shared reads local: immutable part objects
fetched from object storage are served from a bounded LRU
(`block_cache_bytes`, default 256 MiB) and, with `block_cache_dir` set, a
persistent disk tier that survives restarts. `lessdb cache` prints live
hits/misses/bytes.

## SQL surface

Everything DataFusion supports: joins (hash/sort-merge), CTEs, window
functions, subqueries, `EXPLAIN`, aggregations. LessDB adds immutable-part
`CREATE TABLE` with `ENGINE`, `ORDER BY` / `PRIMARY KEY`, `UNIQUE`, and
`COMPRESSION` clauses. Query planning prunes data parts using typed
min/max statistics and bloom filters before any I/O.

## Integrations

```bash
# HTTP API (JSON or Arrow IPC)
lessdb server &  curl -s localhost:7080/v1/sql -d '{"sql":"SELECT 1"}' -H 'content-type: application/json'

# MCP server for AI agents (Claude Desktop, etc.)
lessdb mcp --dir mydb                     # default agent-memory tenant "default"
lessdb mcp --dir mydb --tenant agent-1    # per-agent context/memory/vector namespace
```

```python
import lessdb
db = lessdb.open("mydb")          # in-process, DuckDB style
db.create_table("CREATE TABLE t (x Int64) ENGINE=Firefly ORDER BY (x)")
db.insert_json("t", [{"x": 1}, {"x": 2}])
rows = db.sql("SELECT sum(x) FROM t")
```

```js
const { open } = require("@lessdb/node");
const db = open("mydb");
const ipc = db.queryArrow("SELECT count(*) FROM events"); // Arrow IPC bytes
```

## In-memory contexts & graphs (for AI agents)

LessDB includes a RAM-resident tier that replaces Obsidian-style vaults and
standalone graph databases: a property graph of titled, tagged notes with
typed links, BFS traversal, shortest paths and ranked search, plus
RAM tables with primary-key point lookups and full SQL. Everything
persists under `<data_dir>/memory/` and is exposed to agents through the
same MCP server (`context_*` and `memory_*` tools) and the CLI.

The MCP server additionally namespaces each agent's memory per tenant:
`lessdb mcp --tenant <name>` keeps that agent's contexts, memory tables and
vector spaces under `<data_dir>/tenants/<name>/`, so multiple agents can
share one server without colliding (`less_*` SQL tools stay on the shared
engine):

```bash
lessdb context put proj/lessdb "LessDB" "Analytical DB in Rust" --tag db --kind project
lessdb context put task/123 "GPU kernels" "wgpu filtered-sum + dot" --tag gpu --kind task
lessdb context link task/123 proj/lessdb depends_on
lessdb context find gpu && lessdb context neighbors task/123 && lessdb context path task/123 proj/lessdb

# openCypher subset over the same graph (MATCH/WHERE/RETURN/ORDER BY/SKIP/LIMIT/DISTINCT,
# count/collect/sum/avg/min/max, variable-length paths, CREATE/DELETE/SET)
lessdb cypher "MATCH (t)-[:depends_on*1..2]->(p) RETURN DISTINCT t.title, p.title"
lessdb cypher "MATCH (t:task) WHERE t.title CONTAINS 'gpu' RETURN t.key, t.title ORDER BY t.key SKIP 0 LIMIT 10"

lessdb memory create people --field id:Int64 --field name:Utf8 --pk id
lessdb memory insert people '[{"id":1,"name":"ada"},{"id":2,"name":"grace"}]'
lessdb memory sql "SELECT count(*) FROM people"
```

## Security: the control plane (agents + humans)

One identity model, one role model, one audit trail — both front doors.

```bash
# Agent credentials for the MCP door (plaintext shown once; only the
# SHA-256 hash is stored under <data_dir>/auth/tokens.json):
lessdb token create claude --role read --tenant default      # read|write|admin
lessdb token list

# Fail-closed MCP door: tokens required at initialize, every tool call
# role-checked (admin > write > read) before it executes:
lessdb mcp --require-auth

# Every call — caller, tool, role, outcome, SQL, duration — lands in an
# append-only NDJSON trail under <data_dir>/audit/:
lessdb audit --since 24h --caller claude --outcome denied
```

Tool→permission map: `lessdb_query/explain/schema/stats/tables`, `vector_*`
reads, `context_*` reads, `memory_sql/get` = **read**; `lessdb_optimize`,
`context_put/link/unlink/delete`, `memory_insert/compact`, `vector_put` =
**write**; `vector_create/drop`, `memory_create` = **admin**. Unknown tools
fail safe to read-only. See [docs/AGENT-GOVERNANCE.md](docs/AGENT-GOVERNANCE.md).

## Security: LDAP / Active Directory

Remote interfaces (HTTP server) authenticate against LDAP/AD with
role-based authorization — fail-closed group→role mapping:

```bash
lessdb init --auth '{
  "ldap": {
    "url": "ldaps://ad.corp.example.com:636",
    "base_dn": "DC=corp,DC=example,DC=com",
    "bind_dn": "CN=lessdb-svc,OU=Services,DC=corp,DC=example,DC=com",
    "bind_password": "…",
    "role_mapping": { "DB-Admins": "admin", "DB-Users": "read", "DB-Writers": "write" }
  }
}'
lessdb server   # now requires HTTP Basic auth; admins can POST /v1/admin/optimize

# HTTPS with rustls — no proxy needed:
lessdb server --tls-cert cert.pem --tls-key key.pem
# (or "tls_cert"/"tls_key" in config.json)
```

Standard AD flow: service-account bind → locate user DN → rebind as the
user (the directory verifies the password) → map groups to roles. Usernames
are filter-escaped (no LDAP injection), and auth failures are counted. A
dev file authenticator (`{"file": {"users": {"alice": {"password": "…",
"role": "admin"}}}}`) works without a directory.

## Observability: Prometheus

`GET /metrics` exposes Prometheus text metrics (also `lessdb metrics`):
queries + duration histogram, rows, parts written/merged/**scanned/pruned**
(pruning effectiveness on a dashboard), HTTP requests, auth failures,
uptime, memory, build info.

```yaml
scrape_configs:
  - job_name: lessdb
    static_configs: [{ targets: ["db-host:7080"] }]
```

## Native vector search (LanceDB-style)

Embedded vector search with a registry of named **vector spaces** (each with
its own dimension, metric space — `l2` / `cosine` / `dot` — and index),
exact **flat** search and **IVF-PQ** approximate nearest neighbors
(k-means inverted lists + product quantization, ADC lookup tables, exact
re-ranking), an **embedding-function registry**, and snapshot persistence
under `<data_dir>/vectors/`.

```bash
lessdb vector create docs 3 --metric cosine          # flat exact
lessdb vector create big 256 --index ivf_pq --nlist 64 --m 8
lessdb vector add docs --vectors '[[1,0,0],[0,1,0],[0.9,0.1,0]]'                     --payloads '[{"title":"rust"},{"title":"python"},{"title":"rust-like"}]'
lessdb vector search docs '[1,0,0]' --k 2             # ranked hits + payloads
lessdb vector embed "rust database"                   # built-in trigram embedder
```

SQL-first, too — the `vector_search` table function works like LanceDB's,
including joins over the hits:

```sql
SELECT * FROM vector_search('docs', [1.0, 0.0, 0.0], 2);
SELECT id, payload FROM vector_search('docs', [1.0, 0.0, 0.0], 10)
 WHERE payload LIKE '%rust%';
```

Agents get `vector_create` / `vector_put` / `vector_search` / `vector_list` /
`vector_embed` / `vector_drop` through the same MCP server, and the Python &
Node SDKs get it through SQL. Production embedding models plug into the
embedder registry (`VectorRegistry::register_embedder`).

## Writing data

```bash
lessdb insert events --csv events.csv        # header names map to columns
lessdb insert events --jsonl events.jsonl    # NDJSON or JSON array
lessdb insert events --parquet events.parquet
lessdb insert events --arrow events.arrow    # Arrow IPC stream
lessdb sql "INSERT INTO events VALUES (1, 'click', 1.5, '2025-01-01')"
lessdb sql "INSERT INTO events SELECT * FROM other_table"
lessdb sql "COPY (SELECT * FROM events) TO 'export.parquet'"   # parquet | csv | arrow
```

Embedded sessions: `session.register_file("staging", "file.parquet")` then
`INSERT INTO t SELECT * FROM staging`. Inserts are **WAL-protected**: every
accepted row is durable before the insert returns, and a crash mid-write
recovers exactly-once on reopen (replay skips rows already sealed into
parts). Types: integers, floats, bool, string, date, datetime,
`Decimal(p,s)`, `UUID`, `Array(T)`, `Map(K,V)`.

## GPU acceleration

`cargo build --release --features gpu` and `lessdb gpu` benchmarks the wgpu
kernels (Metal on macOS, Vulkan on Linux) against CPU — filtered
sums/dot products, the shapes behind `WHERE`+`SUM` and join costing. CPU
fallback is always available.

## Repository layout

```
crates/    engine, storage, query, catalog, server, mcp, gpu,
           graph (contexts), memory (RAM tables), vector (k-NN/IVF-PQ), cli
sdks/      python (PyO3), node (napi-rs)
skills/    less-query, less-admin, less-context agent skills
docs/      architecture, roadmap, design decisions
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full design,
[docs/ROADMAP.md](docs/ROADMAP.md) for what's next, and
[docs/DESIGN-DECISIONS.md](docs/DESIGN-DECISIONS.md) for the reasoning
behind each choice.

## CI

The checked-in CI and website deployment workflows run only when manually
dispatched. No hosted validation is implied by a push or pull request.
Toolchain: Rust 1.96, pinned in `rust-toolchain.toml`.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, validation, architecture,
benchmark standards and the issue/PR workflow.

## License

MIT

Dependency license responsibilities and snapshot scope are described in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
