# Local data work — DuckDB-style, one binary

**What**: The "I have a CSV/Parquet/JSON mess and I need an answer"
workflow — exploring, cleaning, joining, and exporting files from the
terminal, the way DuckDB users love, with a columnar analytical engine
underneath.

**Why LessDB**:

- Files become tables in one expression: `read_parquet()`,
  `read_csv()`, `read_json()` — no imports, no staging.
- Real tables, real SQL: joins, CTEs, window functions, `UPDATE`/
  `DELETE` — and everything persists so you can come back tomorrow.
- The interactive shell feels like home: `.tables`, `.schema`, `.mode
  box`, `.import`, `\g`, timers, "did you mean?" hints.
- When the one-off becomes a pipeline, it's already a database —
  schedule it, serve it, or hand it to an agent.

**How, step by step**:

```sh
lessdb sql                       # the REPL
```

```sql
-- 1. peek at a file without importing anything
SELECT * FROM read_parquet('downloads/2026-08.parquet') LIMIT 5;

-- 2. make it a real table (persisted, indexed by ORDER BY)
CREATE TABLE orders AS SELECT * FROM read_parquet('downloads/2026-08.parquet');
.import customers.csv            -- dot-command: file → table in one line

-- 3. the actual question — joins across three files
SELECT c.region, count(*) AS orders, sum(o.amount) AS revenue
FROM orders o JOIN customers c USING (customer_id)
WHERE o.ts >= '2026-08-01'
GROUP BY c.region ORDER BY revenue DESC;

-- 4. export the answer (and keep the database for next time)
COPY (SELECT * FROM regional_summary) TO 'regional_summary.parquet';
```

```sh
lessdb sql --format json "SELECT …" > report.json   # one-shot, scriptable
```

**Value**: exploratory analysis that used to live in throwaway scripts
now leaves behind a reusable, queryable database — and the file-parsing
time is zero.
