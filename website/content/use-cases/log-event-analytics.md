# Logs & event streams

**What**: Application logs, clickstreams, IoT readings — millions of
rows a day that need ingest, retention policy, and instant queries,
without running an ELK cluster.

**Why LessDB**:

- Immutable parts + columnar compression (zstd/lz4) make logs cheap:
  the same storage that keeps large analytical data in gigabytes, not
  terabytes.
- `TTL` is the retention policy: rows expire at flush/optimize time,
  no cron jobs, no manual deletes.
- `UNIQUE` turns "replayed" log streams into idempotent upserts —
  re-running a loader never double-counts.
- One CLI ingests and queries; the same tables are queryable over HTTP
  and MCP when you want alerts or agents on the logs.

**How, step by step**:

```sh
# 1. a log table with retention + dedupe
lessdb init --dir logs
lessdb create --dir logs "CREATE TABLE app_logs (
    ts Timestamp, level String, service String, message String, trace_id String
) ENGINE = Firefly
  ORDER BY (service, level, ts)
  UNIQUE (trace_id)
  TTL ts INTERVAL 30 DAY"

# 2. stream logs in (CSV/JSONL — re-runs are safe thanks to UNIQUE)
tail -f app.log | lessdb insert app_logs --csv --dir logs   # one row per line
# or batched: lessdb insert app_logs --csv logs-2026-08-30.csv --dir logs

# 3. error forensics, seconds later
lessdb sql --dir logs "
  SELECT service, level, count(*) AS n, max(ts) AS last_seen
  FROM app_logs WHERE level = 'ERROR' AND ts > now() - INTERVAL '1 hour'
  GROUP BY service, level ORDER BY n DESC"

# 4. correlate a trace across services
lessdb sql --dir logs "
  SELECT service, message, ts FROM app_logs
  WHERE trace_id = 'abc123' ORDER BY ts"
```

Alert on it: `lessdb server` exposes the same tables over `/v1/sql`,
so a cron/CI job can poll "errors in the last 5 minutes" and page you.

**Value**: log search stops being "grep, if the file is still there"
and becomes a queryable, self-cleaning stream — with agents able to
pull their own evidence from the same source.
