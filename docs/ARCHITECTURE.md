# LessDB Architecture

LessDB is a SQL-first analytical database in Rust that scales **vertically**
(one machine, many cores, optional GPU) and **horizontally** (stateless
compute over shared object storage), with per-column compression,
DuckDB-grade embeddability, and native MCP access for AI agents.

```
┌────────────────────────────────────────────────────────────────────────┐
│  Integrations                                                            │
│  CLI (`lessdb`)  │  Python SDK  │  Node SDK  │  HTTP/Arrow API  │  MCP     │
└───────────────────────────────┬────────────────────────────────────────┘
                                │ SQL + Arrow
┌───────────────────────────────▼────────────────────────────────────────┐
│  less-query — DataFusion 55 (SQL parser, optimizer, hash joins,         │
│  aggregations, window functions, EXPLAIN, partitioning)                 │
│                                                                         │
│  LessTableProvider: part pruning (bloom + typed min/max) →              │
│  one parallel ParquetSource scan per surviving part                     │
└───────────────────────────────┬────────────────────────────────────────┘
                                │ parts, metadata
┌───────────────────────────────▼────────────────────────────────────────┐
│  less-engine — table engines                                            │
│                                                                         │
│  Firefly (local disk)         FireflyCloud (object storage)        │
│  • insert buffer → flush       • same immutable parts, but in S3-       │
│    → immutable parts            compatible object storage               │
│  • merge → dedup (UNIQUE)     • catalog + part metadata shared:        │
│  • parts under <data>/parts/    any compute node serves any table       │
│  • single-node vertical scale  • horizontal scale = add compute         │
└───────────────────────────────┬────────────────────────────────────────┘
                                │ data.parquet + meta.json
┌───────────────────────────────▼────────────────────────────────────────┐
│  less-storage — part format, codecs, bloom filters, object store        │
│  less-catalog — schemas, table manifests                                │
└────────────────────────────────────────────────────────────────────────┘
```

## 1. The data part

A **part** is the unit of immutability in the Firefly engine. Each part is a directory (or object-store prefix) containing:

* `data.parquet` — columnar data, zstd/lz4-compressed per column;
* `meta.json` — `PartMeta`: per-column **typed** min/max statistics, null
  counts, and **bloom filters** on uniqueness columns.

Parts are written once, never mutated. Inserts accumulate in an in-memory
buffer and flush into a new part; deletes/updates (v2) are handled by
replacing-merges: parts are merged into bigger parts, with uniqueness
deduplication applied during the merge.

Why parquet under the hood: it gives us battle-tested columnar encoding,
predicate/statistics-based row-group pruning inside every part, and
interoperability with the entire Arrow ecosystem (DuckDB, pandas, pyarrow,
Spark) — the DuckDB-style "your existing tools just work" property. The part
metadata layer (`meta.json`) stores pruning statistics and bloom filters; keeping it outside the parquet file means the
planner prunes parts *without opening them*.

## 2. Firefly (vertical scale)

```
INSERT ──► buffer (Arrow batches, in memory)
              │ flush at flush_rows (default 256k)
              ▼
        write_part(): sort by sort key → dedup UNIQUE (keep last) →
        column stats + blooms → data.parquet + meta.json
              │
              ▼
        parts: p0 … pn  (immutable)
              │ merge when parts ≥ auto_merge_parts (or OPTIMIZE)
              ▼
        merge_all(): bounded passes — each pass merges one size-tiered
        selection capped at max_merge_rows (default 4M) input rows →
        sort → dedup → write level+1 part → delete inputs; repeats to
        convergence (parts larger than the cap wait for streaming merges)
```

* **Sort key** orders data within a part (range scans become sequential
  reads).
* **UNIQUE constraint** = a prefix of the sort key. Dedup happens at flush
  (within the new part) and at merge (across parts), keeping the **last**
  row per key. Between merges, reads may transiently see duplicate
  keys; `OPTIMIZE` enforces exact uniqueness.
* **Pruning** at three levels: part metadata (typed min/max + bloom, before
  any I/O), parquet row-group statistics, and row-level predicate
  pushdown inside the parquet reader.

## 3. FireflyCloud (horizontal scale, cloud-native)

This architecture uses **no replicas or quorum protocol** — the shared
source of truth is object storage.

```
                ┌───────────────────────────────┐
                │  shared object storage (S3/… ) │
                │  catalog/<table>.json          │  ◄── table manifests (shared)
                │  tables/<t>/parts/<part>/*     │  ◄── immutable data parts
                └───────────────▲───────────────┘
                                │ same objects for every node
   compute A        compute B        compute C   …
   (stateless)      (stateless)      (stateless)
   local disk:      local disk:      local disk:
   buffers, caches, buffers, caches, buffers, caches,
   local tables     local tables     local tables
```

**Compute and storage are cleanly separated.** A node's durable state for
FireflyCloud tables is *zero*: parts and table manifests both live in
the shared store, discovered by listing — a fresh node with an empty local
directory sees every table immediately (proven by the multi-node tests in
`less-engine` and `less-query`). Local disk is ephemeral compute scratch:
insert buffers, caches, and any node-local Firefly tables. Kill a node,
start a replacement — nothing is lost, nothing needs rebalancing.

* Storage URL: `lessdb init --shared s3://bucket/prefix` (also `gcs://`,
  `az://`, `file:///path`, `memory://`) — persisted to `config.json`.
  S3/GCS/Azure backends build behind the `cloud` feature
  (`cargo build --features cloud`); credentials come from standard
  AWS/GOOGLE/AZURE environment variables.
* Manifests live at `catalog/<table>.json` in the shared store (atomic
  object put = last-write-wins). `DROP` deletes part objects first and the
  manifest last, so table visibility changes atomically.
* Reads never touch local disk for shared tables: scan planning lists
  parts from the store and DataFusion reads them through the registered
  object store — the same code path for `file://` and `s3://`.
* **Block cache** (`less-storage/block_cache.rs`): shared-store reads go
  through a `CachingObjectStore` that serves immutable part objects from a
  bounded in-memory LRU (`block_cache_bytes`, default 256 MiB) plus an
  optional persistent disk tier (`block_cache_dir` — FNV-1a-keyed files
  that survive restarts). The parquet reader's byte-range requests fetch a
  part once and slice from cache thereafter. Only `…/parts/…` keys are
  cached and conditional (etag/versioned) reads bypass it, so mutable
  control-plane objects are never stale. `lessdb cache` prints live stats.
* **Merges are size-tiered**: parts are grouped by
`log2(rows)`; OPTIMIZE merges the parts of the smallest tier holding at
least two parts (else the two smallest overall), so similar-sized parts
combine and big parts are never dragged into small rewrites. The merge
itself is a chunked k-way merge: each part is sorted into a run and runs
are merged in 64k-row chunks streamed straight to parquet with
incrementally accumulated stats/blooms and cross-chunk uniqueness dedup
(`write_part_sorted`) — none of the concat-everything + global-sort
memory blowup. Spill-to-disk external merges remain on the roadmap.

**Multi-writer via CAS coordination** (`less-catalog/metastore.rs`):
  a pluggable `MetaStore` with `put_if_absent` + `compare_and_swap` removes
  the single-writer assumption. Part publication is a conditional create of
  `meta.json` (readers only see parts whose metadata exists; a concurrent
  publication of the same uuid-named part fails loudly). Merges first claim
  their input part set under `metastore/merge-claims/<table>/<part>` (TTL
  10 min, expired claims are taken over where update-CAS exists), so two
  writers can flush and OPTIMIZE concurrently without double-merging rows.
  Implementations: `FileMetaStore` (flock-serialized local files) and
  `ObjectMetaStore` (claims as objects in the same S3/GCS/Azure bucket —
  insert-CAS via conditional copy); etcd transactions are the roadmap
  backend for full update-CAS across machines. Proven by the
  `multi_writer_concurrent_flush_and_optimize` test (two nodes, concurrent
  flushes + concurrent OPTIMIZE → every row exactly once).

### 3.5 Distributed fan-out (P2 v1)

`less-fanout` adds a query coordinator over the HTTP SQL API:

* the SQL hook is `lessdb_shard(col, i, n)` — an always-true marker the
  table provider intercepts to scan only parts whose names hash to shard
  `i` of `n` (stable FNV-1a, identical on every node);
* `lessdb fanout --nodes http://a:7080,http://b:7080 "<sql>"` parses the
  query against the fan-out v1 subset (single table; group columns plus
  bare `count`/`sum`/`min`/`max`), pushes a partial aggregate per node with
  the shard marker, then re-aggregates the Arrow IPC partials locally —
  `count` merges by summing counts, `sum`/`min`/`max` re-apply;
* joins, CTEs, unions, subqueries, windows, `HAVING`, `DISTINCT`, `avg`
  and `OFFSET` are rejected with clear errors (documented subset).
  gRPC/DataFusion-distributed execution is the roadmap next step.

## 4. Query engine (DataFusion)

DataFusion provides the battle-tested SQL front end — parser, type coercion,
cost-based optimizer, **hash joins** (the StarRocks-style join machinery),
aggregations, window functions, subqueries — so LessDB inherits a huge,
audited SQL surface instead of reimplementing one.

**TTL retention**: `CREATE TABLE … TTL <col> INTERVAL <n> DAY|HOUR|MONTH`
adds part retention on a Timestamp/Date column; expired rows are dropped
by the same replace-parts machinery (whole expired parts removed, mixed
parts rewritten) at flush and OPTIMIZE.

**DELETE/UPDATE (replace-parts mutations)**: DataFusion plans
`DELETE FROM t WHERE …` / `UPDATE t SET … WHERE …` as DML against the
table provider; `LessTableProvider` compiles the predicates/assignments to
physical expressions and the engine rewrites affected parts (filter out
matching rows, or apply assignments) into fresh immutable parts, then
swaps. Buffered rows are flushed first so mutations cover the whole table,
and rewritten parts carry the old part's `wal_lsn_max` so WAL replay can
never resurrect deleted rows. FireflyCloud mutations claim each part in
the metastore exactly like merges (a claim conflict fails loudly instead of
partially applying). Mutation marks + background rewrite is the roadmap
replacement for hot paths.

The piece LessDB owns is [`LessTableProvider`](../crates/less-query/src/provider.rs):

1. **Prune** parts with `PartMeta` only (no data I/O):
   * `col = X` on a UNIQUE column → bloom filter says "definitely absent"?
     skip the part.
   * any `col <op> literal` → compare the literal against the part's typed
     min/max; skip when provably disjoint. Comparisons are **typed**
     (`StatValue`), never lexicographic-on-numbers.
2. **Plan** one `ParquetSource` scan per surviving part, each in its own
   file group → parallel execution across parts and cores.
3. **Push down** the row predicate into the parquet reader when all its
   columns are projected (parquet does its own stats pruning + row filter).

`EXPLAIN` shows everything, including the provider's pushdown decisions.

**Memory limits**: with `memory_limit` (bytes, `lessdb init --memory-limit N`)
the DataFusion session runs on a bounded `GreedyMemoryPool` wrapped in a
tracking pool that mirrors reserved bytes into the `lessdb_memory_pool_bytes`
Prometheus gauge. Reservations beyond the cap fail the query with
`ResourcesExhausted` instead of exhausting the machine — the first line of
admission control. `0` (default) keeps DataFusion's unbounded pool.

## 5. GPU acceleration

`less-gpu` (optional, wgpu — Metal on macOS, Vulkan on Linux) provides the
kernels that dominate scan cost: **filtered sum/dot product** (the shape of
`WHERE … AND SUM(…)` and of join/projection cost math), with per-workgroup
parallel reduction and a trivial CPU final reduction. The dispatch policy
(roadmap): route an operator to the GPU only above a data-size threshold,
keep a CPU fallback, and never block the pipeline on PCIe round-trips for
small batches. `lessdb gpu` benchmarks GPU vs CPU on the same data.

## 5.5 The in-memory tier: contexts & graphs (agent-native memory)

For agentic workloads LessDB ships a RAM-resident tier that replaces
Obsidian-style context vaults *and* standalone graph databases with one
embedded store (`less-graph` + `less-memory`, persisted under
`<data_dir>/memory/`):

* **Context store** — titled, tagged notes (`context_put`) with stable
  string keys (`proj/lessdb`, `task/123`), ranked substring search
  (`context_find`), typed links (`context_link`, directed or not), BFS
  neighborhoods (`context_neighbors`, depth-limited), and unweighted
  shortest paths (`context_path`). Missing link endpoints are auto-created,
  deletes cascade edges, and every mutation snapshots to `graph.json`
  atomically (tmp + rename) — agents can crash and resume.
* **Memory tables** — Arrow-backed RAM tables (`memory_create/insert/sql`)
  with a hash index on the primary key for O(1) latest-row point lookups
  (`memory_get`), append-log semantics + `memory_compact` dedup (keep-last,
  the same semantics as the engine's UNIQUE merge), and full DataFusion SQL
  over the in-memory set.
* All of it is exposed through the same MCP server as the database
  (`context_*`, `memory_*` tools), so Claude Code / Codex / any MCP client
  gets durable, queryable, *linked* memory out of the box. The `lessdb`
  CLI has `lessdb context` and `lessdb memory` command groups.
* **Agent tenancy** — the MCP server scopes each agent's memory to a
  tenant (`lessdb mcp --tenant <name>`, default `default`): contexts and
  memory tables live under `<data_dir>/tenants/<name>/memory/` and vector
  spaces under `<data_dir>/tenants/<name>/vectors/`, so multiple agents on
  one server never collide. The `less_*` SQL tools intentionally stay on
  the shared engine (not tenant-scoped); the tenant is reported in the MCP
  `initialize` response.
* **openCypher (`less-cypher`)** — a documented subset of openCypher over
  the same `less-graph` store: `MATCH` with variable-length patterns
  (`-[:depends_on*1..3]->`), `WHERE` (property/type/label predicates,
  `CONTAINS`/`STARTS WITH`), `RETURN` with aggregation
  (`count`/`collect`/`sum`/`avg`/`min`/`max`), `ORDER BY`/`SKIP`/`LIMIT`/
  `DISTINCT`, and mutations `CREATE`/`DELETE`/`SET`. `id(n)` maps to the
  LessDB node key; deletes cascade edges. Exposed as `lessdb cypher "<q>"`
  and the MCP `less_cypher` tool. Explicitly out of scope for now:
  `MERGE`, `WITH`, `UNION`, subqueries, `shortestPath` (use `context_path`).

Design notes: adjacency lists give O(degree) traversal; search is a ranked
scan (fine at context scale, an inverted index is on the roadmap); graph
query surface is the Rust API + openCypher + MCP/CLI tools, with SQL graph
table-functions (`neighbors()`, `shortest_path()`) planned on top of the
same store.

## 6. Compression

* **zstd** (default, level 3) and **lz4** per column, written through
  parquet's codec framing (LZ4_RAW to skip Hadoop overhead). Both codecs are
  configurable per table (`COMPRESSION='lz4'`).
* Roadmap encodings: delta / double-delta / gorilla / dictionary
  encodings and low-cardinality dictionaries for time series and categorical data.

## 7. Integrations

| Surface | What | How |
|---|---|---|
| CLI | `lessdb init/create/insert/sql/optimize/bench/server/mcp/cypher` | `less-cli` |
| HTTP | `POST /v1/sql` → JSON or Arrow IPC stream | `less-server` (axum) |
| Python | in-process engine (DuckDB style), pyarrow/pandas interop | PyO3/maturin (abi3-py39+) |
| Node | in-process engine, Arrow IPC buffers | napi-rs |
| MCP | database tools (`less_query`, `less_explain`, `less_schema`, `less_stats`, `less_tables`, `less_optimize`) + context/graph tools (`context_*`) + memory-table tools (`memory_*`) | `less-mcp`, stdio JSON-RPC |
| Skills | `less-query`, `less-admin`, `less-context` agent skills | `skills/` |

## 8. Directory layout

```
crates/
  less-common   errors, config
  less-storage  codecs, bloom, part format, object store
  less-catalog  TypeSpec schemas, table manifests
  less-engine   Firefly + FireflyCloud, buffer/flush/merge
  less-query    DataFusion session + table provider + pruning
  less-server   HTTP API
  less-mcp      MCP server over stdio
  less-gpu      wgpu kernels (optional)
  less-graph    in-memory property graph + context store
  less-cypher   openCypher subset over less-graph
  less-memory   in-memory SQL tables with PK index
  less-fanout   distributed query coordinator (sharded scans + merge)
  less-cli      the `lessdb` binary
sdks/python     PyO3 binding (maturin)
sdks/node       napi-rs binding
skills/         installable agent skills
docs/           this document set
```

## 9. Security: LDAP / Active Directory

`less-auth` plugs LDAP/Active Directory authentication into every remote
interface (HTTP today; Flight SQL and networked MCP later):

* config lives in `config.json` (`lessdb init --auth '<json>'`):
  `url` (ldap/ldaps, optional StartTLS), `base_dn`, service-account
  `bind_dn`/`bind_password`, `user_filter` (default `sAMAccountName={user}`),
  `group_filter` (default `member={dn}`), `group_attribute` (default `cn`),
  and `role_mapping` — group name → role (`admin` | `read` | `write`).
* the flow is the standard AD handshake: service bind → locate the user DN →
  **rebind as the user** (password verification happens in the directory) →
  resolve group membership → map to a role. Usernames are RFC 4515-escaped
  before filter interpolation (no LDAP injection).
* fail closed: an authenticated user in no mapped group is denied unless
  `default_role` is set. Directory connections are pooled across requests
  (dropped on error / when anonymous binds are used), so a request costs
  no TCP+bind handshake. HTTP uses Basic auth (put TLS/ingress in front);
  failures are counted (`lessdb_auth_failures_total`).
* a dev-mode file authenticator (`{"file": {"users": {...}}}`) serves
  local accounts and tests without a directory; both can be chained.

**TLS on the HTTP server**: `lessdb server --tls-cert cert.pem --tls-key
key.pem` (or `tls_cert`/`tls_key` in `config.json`) serves HTTPS via
axum-server + rustls — no separate proxy needed. An e2e test in
`less-server` generates a self-signed cert with rcgen and completes a
rustls client handshake against `/health`.

## 9.5 Vector search (LanceDB-style)

`less-vector` adds native embedding search to the query layer:

* **Registry of vector spaces** — each space is an independent collection
  with its own dimension, metric space (L2 / cosine / dot; "space" in the
  Qdrant/LanceDB sense) and index flavor. One database hosts many spaces.
* **Indexes** — exact `flat` scan, and **IVF-PQ** ANN (k-means inverted
  lists over residuals + product quantization; ADC lookup tables per probed
  list; exact re-ranking of finalists). Cosine/dot spaces use IVF-flat
  (PQ for those metrics is roadmap). Indexes train lazily (k-means++
  init) and snapshot to `index.bin`.
* **Embedding registry** — named embedding functions; a deterministic
  `trigram` lexical embedder ships built-in, production models register
  their own (`register_embedder`).
* **SQL-first access** — `SELECT * FROM vector_search('space', [v…], k)`
  is a DataFusion table function (the exact LanceDB integration pattern),
  so hits join and filter like any table. Persistence:
  `<data_dir>/vectors/<space>/{meta.json,data.bin,index.bin}`.

Roadmap: vector columns inside Firefly tables with per-part ANN indexes,
filtered (pre-filtering) search, PQ for cosine/dot, HNSW.

## 10. Observability: Prometheus

`less-telemetry` is a small, dependency-light registry rendered in the
Prometheus text format at `GET /metrics` (also `lessdb metrics` for the CLI
process). Instrumented end to end:

* queries (`lessdb_queries_total{status}`, `lessdb_query_duration_seconds`
  histogram), rows returned/inserted;
* storage: parts written, merged, scanned and **pruned** — pruning
  counters make index/bloom effectiveness visible on a dashboard;
* HTTP: `lessdb_http_requests_total{route,status}`, auth failures;
* process: uptime, resident memory; build info gauge.

Sample scrape config:

```yaml
scrape_configs:
  - job_name: lessdb
    static_configs:
      - targets: ["db-host:7080"]
    metrics_path: /metrics
```

## 11. Correctness & concurrency notes

* Pruning is *sound by construction*: every skip must be provable from
  metadata; unknown/uncomparable types conservatively keep the part.
* Part writes are atomic (write to temp dir / upload object, then publish
  metadata), and merges delete inputs only after the new part is durable.
* The engine's synchronous API (insert/flush/optimize) targets CLI/embedded
  use; async variants (`parts_async`, `part_file_async`, …) serve concurrent
  server/agent contexts.
