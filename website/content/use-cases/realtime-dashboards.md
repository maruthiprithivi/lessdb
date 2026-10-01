# Realtime dashboards & ops

**What**: Operational views that refresh every few seconds — request
rates, error counts, queue depths — backed by the same database that
stores the history, with Prometheus metrics for the database itself.

**Why LessDB**:

- The HTTP API (`POST /v1/sql`) answers in milliseconds; dashboards
  just poll it. No Kafka, no Redis, no refresh pipeline.
- Upsert tables (`UNIQUE` keys) hold hot state — counters, sessions,
  config — with "latest write wins" semantics, right next to history.
- `/metrics` exports the engine's own Prometheus gauges — rows
  inserted, parts pruned, query durations, memory pool — so the
  dashboard can monitor itself.
- Basic-auth users on the SQL API, agent tokens on `/mcp` — humans and
  bots both get controlled access. (Agents can keep their own hot state
  in `memory_*` RAM tables over MCP.)

**How, step by step**:

```sh
# 1. serve the database (auth: config.json users, or open on localhost)
lessdb server --dir opsdb --addr 127.0.0.1:7080

# 2. hot state as an upsert table — UNIQUE means "latest write wins"
lessdb create --dir opsdb "CREATE TABLE counters (name String, value Int64)
    ENGINE = Firefly ORDER BY (name) UNIQUE (name)"
```

```sh
# 3. the dashboard polls: current error rate over the last minute
curl -s localhost:7080/v1/sql -d '{
  "sql": "SELECT count(*) FROM app_logs
          WHERE level = 'ERROR' AND ts > now() - INTERVAL '1 minute'"}'

# 4. and Prometheus scrapes the engine itself
curl -s localhost:7080/metrics | grep lessdb_
```

```json
// 5. Grafana/any chart polls the same URL every 5s — done.
```

**Value**: you stop building a separate realtime stack — the dashboard
reads live state and history from one audited endpoint, and your agents
can watch the same numbers through MCP.
