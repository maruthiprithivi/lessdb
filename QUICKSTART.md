# LessDB Quickstart

Get from zero to queries (and agents) in ~2 minutes.

## 1. Install

```bash
# Build + install the `less` binary (Rust ≥ 1.85 required):
cd /path/to/less
export CARGO_HOME="$PWD/.cargo"          # keeps the toolchain local
cargo install --path crates/less-cli --features cloud,gpu --locked

# Put it on your PATH (pick one):
export PATH="$PWD/.cargo/bin:$PATH"                       # this shell
echo "export PATH=\"$PWD/.cargo/bin:\$PATH\"" >> ~/.zshrc  # permanent
```

Verify: `less version` → `lessdb 0.1.0`

## 2. Your first database

```bash
less init                      # creates .less/ in the current dir (or --dir /data/mydb)
```

## 3. Tables + data

```bash
less create "CREATE TABLE events (
    id Int64, kind String, amount Float64, ts DateTime
) ENGINE = Firefly ORDER BY (kind, id) UNIQUE (kind) COMPRESSION = 'zstd'"

# CSV header names must match columns (any order); empty/`\N` cells are NULL:
less insert events --csv events.csv          # or --jsonl / --parquet / --arrow
less sql "INSERT INTO events VALUES (2, 'view', 1.5, '2025-01-01 10:01:00')"
less sql "COPY (SELECT * FROM events) TO 'export.parquet'"
```

## 4. Query (SQL-first)

```bash
less sql "SELECT kind, count(*) AS c, sum(amount) AS total
          FROM events GROUP BY kind ORDER BY total DESC"

less sql "SELECT * FROM events WHERE kind = 'click' AND ts > '2025-01-01'"

less sql                                      # interactive REPL (end with ;)
less sql "EXPLAIN SELECT ..."                 # plan + part-pruning info
```

DataFusion SQL surface: joins, CTEs, window functions, subqueries.
`UNIQUE` columns dedup keep-last at flush/merge; run `less optimize events`
to enforce across parts. `less describe events` / `less parts events`
show storage state.

## 5. Vector search

```bash
less vector create docs 3 --metric cosine                    # flat exact
less vector create big 256 --index ivf_pq --nlist 64 --m 8   # ANN
less vector add docs --vectors '[[1,0,0],[0,1,0],[0.9,0.1,0]]' \
                    --payloads '[{"title":"rust"},{"title":"python"},{"title":"rust-like"}]'
less vector search docs '[1,0,0]' --k 2
less vector embed "rust database"        # built-in trigram embedder (demo)

# SQL, too — and you can join/filter the hits:
less sql "SELECT id, payload FROM vector_search('docs', [1.0,0.0,0.0], 10)
          WHERE payload LIKE '%rust%'"
```

## 6. Agent memory: contexts + graph (no Obsidian/Neo4j needed)

```bash
less context put proj/lessdb "LessDB" "Analytical DB in Rust" --tag db --kind project
less context put task/123 "GPU kernels" "wgpu filtered-sum + dot" --tag gpu --kind task
less context link task/123 proj/lessdb depends_on
less context find gpu
less context neighbors task/123 && less context path task/123 proj/lessdb

less memory create people --field id:Int64 --field name:Utf8 --pk id
less memory insert people '[{"id":1,"name":"ada"},{"id":2,"name":"grace"}]'
less memory sql "SELECT count(*) FROM people"
```

Everything persists under `.less/memory/`.

## 7. MCP server → Claude Code / Codex / any MCP client

```bash
less mcp            # stdio JSON-RPC, uses --dir .less by default
```

Claude Code: `claude mcp add lessdb -- less mcp --dir /abs/path/to/mydb`
Claude Desktop `claude_desktop_config.json`:

```json
{ "mcpServers": { "lessdb": { "command": "less", "args": ["mcp", "--dir", "/abs/path/to/mydb"] } } }
```

Agents get 27 tools: `less_query/explain/schema/stats/tables/optimize`,
`context_*`, `memory_*`, `vector_*`.

## 8. Disk hygiene (dev machines)

Build artifacts are regenerable — keep the disk healthy:

```bash
scripts/cleanup.sh               # safe: release artifacts, incremental
                                 # caches, bench/SDK targets, stale registry
                                 # versions, /tmp leftovers (debug deps stay)
scripts/cleanup.sh --aggressive  # + full cargo clean
scripts/install-cleanup-agent.sh # launchd: auto-runs --check 30 twice a day
```

CI runs the same guard before every job (cleans caches when the runner
drops below 20G free).

## 9. HTTP server (+ Prometheus, LDAP)

```bash
less server --addr 127.0.0.1:7080
curl -s localhost:7080/health
curl -s localhost:7080/v1/sql -H 'content-type: application/json' \
     -d '{"sql":"SELECT count(*) FROM events"}'          # JSON
curl -s localhost:7080/metrics                           # Prometheus text

# LDAP/AD auth (fail-closed group→role mapping):
less init --auth @auth.json   # see docs/ARCHITECTURE.md §9 for the schema
curl -u alice:secret -H 'content-type: application/json' \
     -d '{"sql":"SELECT 1"}' localhost:7080/v1/sql
```

Prometheus:

```yaml
scrape_configs:
  - job_name: lessdb
    static_configs: [{ targets: ["localhost:7080"] }]
```

## 9. Cloud-native: compute/storage separation

```bash
less init --shared s3://my-bucket/lessdb        # parts + catalog in S3
# any node with the same --shared URL sees the same tables instantly;
# local disk stays ephemeral (buffers/caches only). file:// for local dev.
```

## 10. Python / Node (embedded, DuckDB-style)

```bash
# Python (wheel already built in sdks/python/target/wheels):
pip install sdks/python/target/wheels/lessdb-0.1.0-cp39-abi3-*.whl
python -c "
import lessdb
db = lessdb.open('mydb')
db.create_table('CREATE TABLE t (x Int64) ENGINE = Firefly ORDER BY (x)')
db.insert_json('t', [{'x': 1}, {'x': 2}])
print(db.sql('SELECT sum(x) FROM t'))"

# Node (sdks/node):
cd sdks/node && npm install && npm run build   # or use the prebuilt .node
node -e "const {open}=require('./index.js'); const db=open('mydb');
db.queryJson('SELECT 40+2').then(console.log)"
```

## Benchmarks & GPU

```bash
less bench --rows 5000000     # insert + scan/filter/group-by throughput
less gpu                      # GPU vs CPU kernels (Metal/Vulkan)
less metrics                  # this process's Prometheus metrics
```

More: [README.md](README.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md),
[docs/ROADMAP.md](docs/ROADMAP.md), and the agent skills in `skills/`.
