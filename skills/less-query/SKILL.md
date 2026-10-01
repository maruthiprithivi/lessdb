---
name: less-query
description: Author, explain, and tune SQL queries against LessDB (via its MCP server, CLI, or embedded SDKs). Covers Firefly/FireflyCloud semantics, uniqueness constraints, part pruning, compression, and query tuning. Use whenever the user asks to query, analyze, or tune LessDB tables.
---

# LessDB Query Skill

LessDB is a SQL-first analytical database with immutable columnar parts,
DataFusion SQL, and MCP access for agents.

## Access

- **MCP tools** (preferred for agents): `less_query`, `less_explain`,
  `less_schema`, `less_stats`, `less_tables`, `less_optimize`,
  `less_cypher` (graph queries over the context store).
- **CLI**: `less sql "SELECT ..."`, `less bench`, `less parts <table>`.
- **Embedded**: Python `lessdb.open(...)`, Node `@lessdb/node`.

## Authoring queries

- Standard SQL: joins, CTEs, window functions, subqueries, `EXPLAIN` — all
  supported (DataFusion engine).
- Always `EXPLAIN` expensive queries first; check how many parts the plan
  scans. Fewer parts scanned = better pruning.
- Use the sort key for range filters (`ORDER BY (kind, id)` ⇒ filter on
  `kind` and `id` ranges scan sequentially).
- Prefer `=` filters on `UNIQUE` columns: bloom filters in part metadata
  skip almost every irrelevant part without I/O.
- Prefer `WHERE` over post-filtering in the client; predicates push into
  the parquet reader.
- Aggregations with `GROUP BY` on low-cardinality columns are hash
  aggregations — fast; add `ORDER BY` only when needed.

## Uniqueness semantics

`UNIQUE (a, b)` requires `(a, b)` to be a prefix of the sort key and means
*replacing* semantics (keep the last row per key), enforced at flush and
merge:

- duplicates within an insert collapse immediately;
- duplicates across parts collapse at merge (`less optimize <table>` or
  automatic background merge);
- reads between merges may transiently see duplicate keys — `OPTIMIZE`
  before reporting exact-unique results.

## Tuning

- Compression: `COMPRESSION='lz4'` trades ratio for speed; `zstd` (default)
  is the balanced choice. Set per table at `CREATE TABLE`.
- Parts: `less parts <table>` shows part count and levels; many small parts
  ⇒ run `less optimize <table>`.
- Scale-out: `ENGINE = FireflyCloud` puts parts in shared object
  storage so additional compute nodes see the same data.
- GPU: `less gpu` benchmarks GPU kernels; the query engine falls back to
  CPU automatically.

## Vector search (SQL)

`vector_search('space', [vector], k [, nprobe])` is a table function —
query it like a table, join it, filter its payloads:

```sql
SELECT * FROM vector_search('docs', [0.1, 0.2, 0.3], 5);
SELECT id, payload FROM vector_search('docs', [0.1, 0.2, 0.3], 10)
 WHERE payload LIKE '%rust%';
```

* Columns: `id` (u32), `score` (f32, **smaller = closer** in every metric
  space), `payload` (JSON string).
* Spaces are created outside SQL (`less vector create` or MCP
  `vector_create`) with a metric (`l2` | `cosine` | `dot`) and an index
  (`flat` exact or `ivf_pq` approximate); `nprobe` tunes IVF-PQ recall.
* Embedding functions: MCP `vector_embed` / `less vector embed` use the
  registered embedder (built-in `trigram` is lexical-only; production
  models are registered per deployment).

## Graph querying (openCypher)

The in-memory context graph (`context_put`/`context_link` or `less context`)
is queryable with an openCypher subset via MCP `less_cypher` or
`less cypher "<query>"`:

```cypher
MATCH (t:task)-[:depends_on*1..2]->(p)
WHERE t.title CONTAINS 'gpu'
RETURN DISTINCT t.key, t.title, p.title
ORDER BY t.key SKIP 0 LIMIT 20

MATCH (n) RETURN count(*) AS nodes
```

* Supported: `MATCH` (typed/undirected/variable-length patterns
  `-[:r*1..3]->`), `WHERE` (`=`, `<>`, comparisons, `CONTAINS`,
  `STARTS WITH`, `AND/OR/NOT`, `IS NULL`), `RETURN` with `AS` aliases and
  aggregates `count/collect/sum/avg/min/max`, `ORDER BY`, `SKIP`, `LIMIT`,
  `DISTINCT`; mutations `CREATE` (nodes/edges), `DELETE`, `SET`.
* `id(n)` returns the LessDB node key. Deleting a node cascades its edges.
* Not supported (documented subset): `MERGE`, `WITH`, `UNION`, subqueries,
  `shortestPath` — use `context_path` MCP/CLI for shortest paths.

## Workflow for data questions

1. `less_tables` → find the table.
2. `less_schema` on it → columns, types, sort key, unique columns.
3. Compose the SQL, preferring sort-key ranges and unique-column
   equalities.
4. `less_explain` → confirm pruning (few parts).
5. `less_query` → present results; mention row counts and any plan notes.
