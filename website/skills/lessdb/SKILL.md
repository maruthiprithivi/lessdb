---
name: lessdb
description: Use when working with LessDB — SQL analytics (Firefly/FireflyCloud tables), durable agent memory (context notes and links), RAM tables, vector search, and graph queries — either through the lessdb CLI, its SQL, or its MCP tools (lessdb_*, context_*, memory_*, vector_*, lessdb_cypher). Triggers: "query the database", "store this for later", "remember this", "search my notes", "vector search", "find related", "analytics on ...", "lessdb".
---

# LessDB

LessDB is one Rust binary that is simultaneously an analytical SQL
database, an agent-memory store, a vector database, and a graph engine.
Everything persists under a data directory (`.less` by default). The
CLI is `lessdb`; agents can also use the MCP tools below.

## First principles

1. **Explore before you write.** Always start with `lessdb_schema` /
   `lessdb_tables` (or `lessdb tables` / `lessdb describe` on the CLI),
   then `lessdb_explain` before expensive queries.
2. **SQL is the analytics language.** Joins, CTEs, window functions,
   `DELETE … WHERE`, `UPDATE … SET`, `COPY … TO 'file.parquet'` all work.
   Files become tables with `read_parquet()` / `read_csv()` /
   `read_json()`.
3. **Durable memory is explicit.** Notes (`context_put`) survive
   restarts; RAM tables (`memory_*`) are fast but in-memory. Use notes
   for facts you must not lose, RAM tables for hot lookup state.
4. **Respect roles.** Tools/calls may be denied by token role — if a
   write is denied, ask the user for a higher-role token instead of
   retrying.
5. **LessDB statements are atomic.** No BEGIN/COMMIT; each statement
   commits immediately. Views don't exist yet — use CTEs (`WITH`).

## CLI quick reference

```sh
lessdb init [--dir DIR]                  # create a database directory
lessdb tables | describe | stats | parts # explore
lessdb sql "SELECT …"                    # one-shot query
lessdb sql                               # interactive REPL (.tables .schema .import)
lessdb create "CREATE TABLE …"           # DDL (ENGINE = Firefly | FireflyCloud)
lessdb insert events --csv rows.csv      # bulk load
lessdb optimize events                   # merge parts / enforce UNIQUE
lessdb audit                             # who did what (MCP + HTTP)
```

## DDL essentials

```sql
CREATE TABLE events (
  id Int64, kind String, amount Float64, ts Timestamp
) ENGINE = Firefly              -- local disk
  ORDER BY (kind, ts)           -- sort key: pruning + fast range scans
  UNIQUE (id);                  -- optional upsert key (prefix of ORDER BY)

CREATE TABLE cloud_events (...) ENGINE = FireflyCloud  -- shared object storage
  ORDER BY (kind, ts);
```

`ORDER BY` is how LessDB prunes parts — put the columns you filter on
first. `TTL ts INTERVAL 30 DAY` auto-expires old rows.

## MCP tools (when running inside an agent)

- **SQL**: `lessdb_query` (SQL), `lessdb_explain` (plan + pruned parts),
  `lessdb_schema`, `lessdb_stats`, `lessdb_tables`, `lessdb_optimize`.
- **Context (durable notes)**: `context_put` (key, title, tags, text),
  `context_find` (full-text over notes), `context_link` (typed edges),
  `context_neighbors`, `context_path` (shortest path).
- **Memory (RAM tables)**: `memory_create`, `memory_insert`,
  `memory_get` (O(1) latest row by PK), `memory_sql` (SQL over RAM
  tables), `memory_compact`.
- **Vectors**: `vector_*` tools plus the `vector_search('space', [...], k)`
  SQL table function — join hits with payloads in plain SQL.
- **Graph**: `lessdb_cypher` — `MATCH (a)-[:depends_on*1..3]->(b) RETURN …`.

## Patterns

**Remember something for later**
```
context_put { "key": "decision/2026-08-30", "title": "Chose Firefly for hot data",
              "tags": ["architecture"], "text": "…" }
context_link { "from": "decision/2026-08-30", "to": "issue/142",
               "type": "resolves" }
```

**Semantic search then SQL join**
```sql
SELECT id, payload FROM vector_search('docs', [0.1, 0.9, …], 5)
WHERE payload LIKE '%postmortem%';
```

**Hot lookup state** — `memory_create sessions(id Int64, user String,
ts Timestamp)` then `memory_get` by `sessions/id`. RAM tables die with
the process; notes don't.

**Event analytics** — `SELECT kind, count(*), approx_percentile_cont(amount, 0.95)
FROM events WHERE ts > now() - INTERVAL '7 days' GROUP BY kind` — check
`lessdb_explain` to see which parts were pruned.

## Safety

- Prefer read-only exploration; never `DROP TABLE` or `TRUNCATE` without
  the user asking.
- Never print tokens or secrets; tokens are shown once at creation.
- Large writes: batch `lessdb insert --csv` instead of row-by-row.
