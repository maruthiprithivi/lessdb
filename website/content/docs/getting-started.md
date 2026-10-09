# Getting started with LessDB

One binary, zero daemons. Install it, point it at a directory, and you
have a database your team queries with SQL and your agents work over
MCP — the same engine, the same data.

## 1. Install

```sh
# macOS (Apple Silicon) and Linux x64
curl -fsSL https://lessdb.dev/install.sh | sh

# or via npm — the package is served from lessdb.dev (Cloudflare), not
# the public registry; it downloads + verifies the same binary
npm install -g lessdb --registry https://lessdb.dev/npm/

# or via Homebrew
brew tap lessdb/lessdb && brew install lessdb
```

The installer puts `lessdb` in `~/.local/bin` **and adds that directory
to your `~/.zshrc` / `~/.bashrc` PATH automatically**, so a new terminal
just works — no PATH fiddling.

Check it:

```sh
lessdb --version     # lessdb 0.6.0
```

**Stay current without brew/npm**: `lessdb upgrade` downloads the latest
release, verifies its SHA-256, and replaces itself in place
(`lessdb upgrade --check` just reports; `--force` reinstalls the same
version). After the first install from any channel, upgrades are
one command.

Building from source: `cargo install --path crates/less-cli --features
cloud,gpu` (the `cloud` feature adds S3/GCS/Azure object-storage
backends, `gpu` adds the wgpu kernels).

## 2. Sixty-second tour (optional, recommended)

```sh
lessdb demo        # seeds 50k rows of event data and runs showcase queries
```

One command builds a database, loads a seeded `events` table, and runs
three real analytics queries with timings on screen — revenue by event
kind, daily active users, top pages. It prints the next steps at the
end. Delete it afterwards with `rm -rf .less`.

## 3. Create a database

```sh
lessdb init                 # creates .less/ here (or: lessdb init --dir /data/mydb)
lessdb create "CREATE TABLE events (
    id Int64,
    kind Utf8,
    amount Float64,
    ts Timestamp
) ENGINE=Firefly ORDER BY (kind, ts)"
```

The `ORDER BY` is the sort key: rows inside every data part are stored
sorted by it, so range scans over `(kind, ts)` read sequentially.

## 4. Load data

```sh
lessdb insert events --csv events.csv        # headers map to columns, any order
lessdb insert events --jsonl events.jsonl
lessdb insert events --parquet events.parquet
lessdb sql "INSERT INTO events VALUES (2, 'view', 1.5, now())"
```

Inserts buffer in memory and flush into immutable parts automatically
(256k rows by default). The parquet loader streams batches rather than
loading the complete file into memory.

## 5. Query — the interactive shell

```sh
lessdb sql                    # interactive SQL REPL: prompt, multi-line, history
```

The shell is a proper client, not a toy: a `lessdb :) ` prompt with
`:-] ` continuation, multi-line statements terminated by `;` (or `\G`
for vertical output), line editing and command history (up/down arrows,
saved to `~/.lessdb_history`). Explore fast with dot- and backslash-commands:

```
.tables [p%]         list tables          .mode table|pretty|box|csv|tsv|json|jsoneachrow|vertical
.schema [t]          show CREATE TABLE    .timer on|off
.import f.csv [t]    load a file          .read file.sql · .demo · .license
.mcp                 how to open the agent door     .help
\G  run as vertical   \g  run now          \t \d \p \o \h \c   .quit
```

`\G` runs the current statement with vertical (row-wise) output, `\g`
sends it immediately, and near-miss names get "did you mean?" hints —
for both tables and dot-commands.

One-shot queries pick formats with `--format`:

```sh
lessdb sql --format json "SELECT …"       # table|pretty|box|csv|tsv|json|jsoneachrow|vertical
```

The full-screen TUI (panes, mouse, SQL editor) is still there as
`lessdb tui`.

SQL itself covers the catalog too: `SHOW DATABASES`, `SHOW TABLES [FROM
default] [LIKE 'p%']`, `SHOW CREATE TABLE t`, `SHOW COLUMNS FROM t` /
`DESCRIBE t`, `USE default`, `SHOW PROCESSLIST`, `PRAGMA version`, and
`VACUUM` (an alias for OPTIMIZE across tables — parts are immutable).

DDL is first-class and durable — not just in the REPL, but everywhere SQL
runs (SDKs, MCP, HTTP):

```sql
CREATE TABLE IF NOT EXISTS pageviews (page String, hits Int64)
    ENGINE=Firefly ORDER BY (page);
ALTER TABLE pageviews ADD COLUMN score Float64;   -- rewrites parts
ALTER TABLE pageviews DROP COLUMN score;
CREATE TABLE agg AS SELECT page, count(*) FROM pageviews GROUP BY page;
TRUNCATE TABLE pageviews;                          -- empty, keep schema
DROP TABLE IF EXISTS pageviews;
```

Files work as tables, DuckDB-style — `read_parquet()`, `read_csv()`,
`read_json()` register the file(s) for the duration of the statement, so
`CREATE TABLE t AS SELECT * FROM read_parquet('x.parquet')` persists it.

LessDB's model is explicit: every statement is atomic, so `BEGIN`/
`COMMIT` answer that no transaction is needed, `ATTACH` points you at
`read_*()` instead, and `CREATE VIEW` honestly says "use a CTE for now".

```sh
lessdb sql "SELECT kind, count(*), sum(amount)
          FROM events GROUP BY kind ORDER BY 3 DESC"

lessdb sql                    # interactive REPL (end statements with ;)
lessdb sql "EXPLAIN SELECT …" # the plan, including which parts were pruned
```

Full SQL surface: joins, CTEs, window functions, subqueries,
`DELETE … WHERE`, `UPDATE … SET`, and `COPY … TO 'file.parquet'`.

## 6. Keep it tidy

```sh
lessdb optimize events        # merge small parts (bounded memory, size-tiered)
lessdb parts events           # per-part rows/levels
less stats events           # row counts, compressed size
```

Tables can carry a TTL — `TTL ts INTERVAL 30 DAY` drops expired rows at
flush/optimize — and a UNIQUE prefix of the sort key, deduplicated
keep-last like an upsert key.

## 7. Vector search

```sh
lessdb vector create docs 3 --metric cosine                 # exact (flat)
lessdb vector create big 256 --index ivf_pq --nlist 64 --m 8 # ANN index
lessdb vector add docs --vectors '[[1,0,0],[0,1,0]]' \
                     --payloads '[{"title":"rust"},{"title":"python"}]'
lessdb vector search docs '[1,0,0]' --k 2

# SQL-first: search hits join and filter like any table
lessdb sql "SELECT id, payload
          FROM vector_search('docs', [1.0,0.0,0.0], 10)
          WHERE payload LIKE '%rust%'"
```

## 8. Knowledge graph & memory (agent tier)

```sh
lessdb context put proj/lessdb "LessDB" "Analytical DB in Rust" --tag db
lessdb context link task/123 proj/lessdb depends_on
lessdb context find gpu
lessdb context path task/123 proj/lessdb          # shortest path

lessdb memory create people --field id:Int64 --field name:Utf8 --pk id
lessdb memory insert people '[{"id":1,"name":"ada"}]'
lessdb memory sql "SELECT count(*) FROM people"
```

Everything persists under `.less/memory/`; every mutation snapshots
atomically, so agents can crash and resume.

## 9. Open the agent door (MCP)

```sh
lessdb mcp                 # stdio JSON-RPC on .less — 27 tools for any MCP client
```

Register it with your client:

```json
{ "mcpServers": { "lessdb": { "command": "lessdb", "args": ["mcp", "--dir", "/abs/path/to/mydb"] } } }
```

Tools: `lessdb_query`, `lessdb_explain`, `lessdb_schema`, `lessdb_stats`,
`lessdb_tables`, `lessdb_optimize`, `context_*`, `memory_*`, `vector_*`,
`lessdb_cypher`. Lock the door with `lessdb token create <name> --role read`
+ `lessdb mcp --require-auth`, and review everything in `lessdb audit`.

Run it hosted instead of locally with `lessdb server` — the same 27 tools
over authenticated HTTP at `/mcp` (fail-closed, agent tokens required).
Full walkthrough — local vs hosted, roles, tenants, troubleshooting — in
the [MCP guide](/docs/mcp). To make your coding agent fluent in LessDB
without reading the manual, install the one-file
[LessDB skill](/docs/skills).

For a situation-aware agent integration, start with the
[agent deployment blueprints](/docs/agent-blueprints) and run the
[adoption scenarios](/docs/agent-scenarios). They define identity, scope,
bounded ContextPackets, provenance, handoffs and approval boundaries without
pretending that an application policy predicate is native row ACL.

## 10. Serve it (HTTP + Prometheus)

```sh
lessdb server --addr 127.0.0.1:7080
curl -s localhost:7080/health
curl -s localhost:7080/v1/sql -d '{"sql":"SELECT count(*) FROM events"}'
curl -s localhost:7080/metrics              # Prometheus text format
```

TLS via `--tls-cert/--tls-key`; LDAP/AD roles via `lessdb init --auth
@auth.json` (fail-closed group→role mapping).

## 11. Share it (compute/storage separation)

```sh
lessdb init --shared s3://my-bucket/lessdb    # parts + catalog in object storage
```

Any node that points at the same URL sees the same tables instantly —
local disk stays ephemeral (buffers and caches only), so compute nodes
are disposable. `file://` works for local development.

## Next steps

* [Use cases](/use-cases/) — end-to-end blueprints: analytics, agent
  memory, vector search, embedded.
* [Agent blueprints](/docs/agent-blueprints) and [adoption scenarios](/docs/agent-scenarios)
  — grow from one assistant to a governed swarm.
* [Playbooks](/playbooks/) — operations: benchmarks, multi-node, auth,
  backups, secrets, and the manual Cloudflare tasks.
* [Architecture](/docs/architecture) — how the engine is put together,
  or the [interactive diagram](/docs/architecture-interactive).
