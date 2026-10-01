# Product & growth analytics

**What**: Events, sessions, funnels, retention — the classic
analytical SQL job — on data your whole team shares, with no cluster
to run.

**Why LessDB**:

- One binary, no daemon: `lessdb init` in a folder gives you an
  analytical engine that runs large analytical queries.
- Columnar storage with `ORDER BY` pruning: billion-row scans skip whole
  parts instead of reading them.
- `UNIQUE` keys deduplicate during flush and merge; `TTL` retires old events
  automatically.
- The same tables feed dashboards (HTTP), agents (MCP), and humans
  (SQL/REPL) — one source of truth, no ETL copies.

**How, step by step**:

```sh
# 1. create the database and an events table
lessdb init --dir analytics
lessdb create --dir analytics "CREATE TABLE events (
    ts Timestamp, user_id Int64, event String, props String
) ENGINE = Firefly ORDER BY (event, ts) TTL ts INTERVAL 90 DAY"

# 2. load events (the streaming loader keeps memory flat on huge files)
lessdb insert events --csv events.csv --dir analytics

# 3. ask product questions
lessdb sql --dir analytics "
  SELECT event, count(*) AS n, count(DISTINCT user_id) AS users
  FROM events WHERE ts > now() - INTERVAL '7 days'
  GROUP BY event ORDER BY n DESC"

# 4. retention: how many day-1 users come back on day 7?
lessdb sql --dir analytics "
  WITH d1 AS (SELECT DISTINCT user_id FROM events
              WHERE ts >= '2026-08-01' AND ts < '2026-08-02')
  SELECT count(*) FROM d1 JOIN events e USING (user_id)
  WHERE e.ts >= '2026-08-07' AND e.ts < '2026-08-08'"
```

Scale up: recreate the table with `ENGINE = FireflyCloud` and every
analyst's laptop queries the same parts from object storage —
compute/storage separated, no replication lag, no warehouse bill.

**Value**: a folder replaces "warehouse + ETL + dashboard cache", and
answers go from a daily batch to sub-second ad-hoc — while your agents
query the very same tables through MCP.
