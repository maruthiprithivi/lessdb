# Architecture

LessDB is a SQL-first analytical database in Rust. It scales **vertically**
(one machine, many cores, optional GPU) and **horizontally** (stateless
compute over shared object storage), and it ships an agent-native memory
tier on the same engine.

```
┌──────────────────────────────────────────────────────────────────────┐
│  Integrations                                                          │
│  CLI (`lessdb`) │ Python SDK │ Node SDK │ HTTP/Arrow API │ MCP           │
└───────────────────────────────┬──────────────────────────────────────┘
                                │ SQL + Arrow
┌───────────────────────────────▼──────────────────────────────────────┐
│  Query layer — SQL parser, cost-based optimizer, hash joins,          │
│  aggregates, window functions, EXPLAIN                                │
│                                                                       │
│  Table provider: prune parts from metadata (typed min/max + bloom)    │
│  → one parallel scan per surviving part                               │
└───────────────────────────────┬──────────────────────────────────────┘
                                │ parts, metadata
┌───────────────────────────────▼──────────────────────────────────────┐
│  Engine — table engines                                               │
│                                                                       │
│  Firefly (local disk)          FireflyCloud (object storage)     │
│  • buffer → flush → parts       • same parts, in a shared bucket      │
│  • bounded size-tiered merges   • any node serves any table           │
└───────────────────────────────┬──────────────────────────────────────┘
                                │ data.parquet + meta.json
┌───────────────────────────────▼──────────────────────────────────────┐
│  Storage — part format, zstd/lz4 codecs, bloom filters, object store  │
│  Catalog — schemas, table manifests, CAS metastore                    │
└──────────────────────────────────────────────────────────────────────┘
```

## The data part

A **part** is the unit of immutability: a directory (or object-store
prefix) holding two files:

* `data.parquet` — columnar data, zstd/lz4-compressed per column, with
  per-row-group statistics. Using the parquet format means every Arrow
  tool can read LessDB files directly and vice versa.
* `meta.json` — per-column **typed** min/max, null counts, and bloom
  filters on uniqueness columns. This metadata layer is what lets the
  planner prune parts **without opening them**.

Parts are written once and never mutated. Inserts accumulate in an
in-memory buffer and flush into a new part; deletes and updates replace
parts with rewritten ones.

## Firefly (vertical scale)

```
INSERT ──► buffer (Arrow batches, in memory)
             │ flush at flush_rows (default 256k)
             ▼
         write_part(): sort by sort key → dedup UNIQUE (keep last) →
         column stats + blooms → data.parquet + meta.json
             ▼
         parts p0…pn (immutable)
             │ merge when parts ≥ auto_merge_parts (or OPTIMIZE)
             ▼
         merge_all(): bounded passes — each pass merges one size-tiered
         selection capped at max_merge_rows (default 4M) input rows;
         repeats to convergence
```

* **Sort key** orders data within a part (range scans become sequential
  reads).
* **UNIQUE** columns (a prefix of the sort key) dedup keep-last at flush
  and merge — an upsert key. Between merges, reads may transiently see
  duplicate keys; `OPTIMIZE` enforces exact uniqueness.
* **Pruning** at three levels: part metadata (typed min/max + bloom,
  before any I/O), row-group statistics inside the parquet file, and
  row-level predicate pushdown into the reader.
* **Bounded merges**: a table with hundreds of equal-sized parts
  converges over many small passes instead of one giant in-memory merge.

## FireflyCloud (horizontal scale)

No replicas, no quorum protocol: the shared source of truth is object
storage.

```
        shared object storage (s3:// gcs:// az:// file://)
        catalog/<table>.json          table manifests (shared)
        tables/<t>/parts/<part>/*     immutable data parts
                   ▲
   compute A   compute B   compute C   …   (stateless)
   local disk: buffers, caches, local tables
```

* A node's durable state for shared tables is **zero** — a fresh node
  with an empty local directory sees every table by listing the bucket.
* **Multi-writer safety via CAS**: part publication is a conditional
  create of `meta.json` (readers only see parts whose metadata exists);
  merges first claim their inputs under `merge-claims/<table>/<part>`
  (10-minute TTL), so two writers can flush and optimize concurrently
  without double-merging.
* **Block cache**: shared reads go through a bounded in-memory LRU
  (default 256 MiB) plus an optional persistent disk tier; immutable
  part objects are fetched once and sliced from cache afterwards.

## Query layer

The SQL front end (parser, type coercion, cost-based optimizer, hash
joins, aggregations, window functions, subqueries) is a battle-tested,
audited engine; the piece LessDB owns is the table provider:

1. **Prune** parts from `meta.json` alone — bloom "definitely absent"
   skips for `col = X` on UNIQUE columns; typed min/max comparisons skip
   provably-disjoint parts.
2. **Plan** one parallel scan per surviving part.
3. **Push down** row predicates into the reader.

`EXPLAIN` shows everything, including pushdown decisions. A `memory_limit`
caps the query memory pool: over-limit queries fail with a clean
`ResourcesExhausted` error instead of exhausting the machine.

## Mutations, TTL, and durability

* `DELETE … WHERE` / `UPDATE … SET` rewrite affected parts into fresh
  immutable parts; buffered rows flush first so mutations cover the
  whole table, and rewritten parts carry the old part's WAL watermark so
  recovery can never resurrect deleted rows.
* `TTL <col> INTERVAL <n> DAY` drops expired rows at flush/optimize.
* The WAL (`<data_dir>/wal/<table>.wal`) makes accepted inserts durable
  before they return; replay is idempotent (records newer than the
  newest part's watermark re-buffer and flush — exactly once). Torn part
  directories from a crash are swept at open.

## The agent tier (same engine)

* **Context store** — titled, tagged, linked notes with ranked search,
  BFS neighborhoods, and shortest paths; atomic snapshots.
* **Memory tables** — Arrow-backed RAM tables with a hash primary-key
  index, O(1) latest-row point lookups, append-log semantics.
* **Cypher** — a documented openCypher subset over the same graph store.
* **Vector search** — named spaces (L2/cosine/dot) with exact flat and
  IVF-PQ ANN indexes, queried through the SQL table function
  `vector_search('space', [v…], k)`.

All of it is exposed through the same MCP server as the database,
tenant-scoped per agent.

## Security & observability

* **LDAP/AD** — service bind → user rebind → group membership → role
  (`admin`/`read`/`write`), fail-closed; a dev file authenticator covers
  local accounts.
* **TLS** on the HTTP server via rustls.
* **Prometheus** at `/metrics`: query counts + durations, rows, parts
  written/merged/scanned/pruned, HTTP routes, auth failures, and the
  memory-pool gauge.

See the [interactive diagram](/docs/architecture-interactive) for the
clickable component map and the flow walkthroughs.
