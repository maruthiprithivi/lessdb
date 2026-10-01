# Configuration reference

Everything LessDB can be told, where it lives, and how to change it.
Configuration is validated on load and save — a bad value fails fast with
a clear message instead of misbehaving later.

## Where config lives

`<data_dir>/config.json` — written by `lessdb init`, read by every command
and the embedded SDKs. All knobs have sane defaults; you only set what
you care about.

```sh
lessdb init --dir /data/mydb \
  --shared s3://bucket/prefix \
  --memory-limit 40GiB \
  --compression lz4 \
  --flush-rows 1000000 \
  --max-merge-rows 8388608 \
  --block-cache-bytes 1073741824 \
  --no-wal
```

Every `lessdb init` flag also has a `LESSDB_*` environment variable (see
`lessdb init --help`). Flags/env write `config.json` once; the file stays
the source of truth afterwards — edit it directly and restart.

## Storage & durability

| key | default | what it does |
|---|---|---|
| `data_dir` | `.less` | root of the database directory |
| `shared_url` | none | object-storage URL for FireflyCloud tables: `s3://bucket/prefix`, `gcs://…`, `az://…`, `file:///path`, `memory://` |
| `wal_enabled` | `true` | write-ahead log for insert durability (`--no-wal` disables) |
| `wal_fsync` | `true` | fsync every WAL append before acknowledging |

## Parts & merging

| key | default | what it does |
|---|---|---|
| `flush_rows` | `262144` | buffered rows per table before flushing into an immutable part |
| `max_buffer_rows` | `1048576` | hard cap; flush even mid-batch |
| `target_part_bytes` | `268435456` | target part size for background merging |
| `auto_merge_parts` | `8` | run an optimize once a table has this many parts |
| `max_merge_rows` | `4194304` | cap on input rows per merge pass — bounds merge memory (a 100M-row table converges in bounded passes) |

## Compression

| key | default | what it does |
|---|---|---|
| `compression` | `zstd` | `zstd` \| `lz4` \| `none`, per column |
| `zstd_level` | `3` | 0 = library default, 1..=22 |

## Query engine

| key | default | what it does |
|---|---|---|
| `memory_limit` | `0` | query memory pool cap in bytes (`0` = unbounded). Over-limit queries fail with `ResourcesExhausted` instead of OOM-ing the box |
| `parquet_page_index` | `true` | page-level skipping toggle (off = row-group statistics only) |
| `gpu_enabled` | `false` | enable the wgpu kernels |

## Shared-storage cache

| key | default | what it does |
|---|---|---|
| `block_cache_bytes` | `268435456` | in-memory LRU for shared parts |
| `block_cache_dir` | none | optional persistent disk tier (survives restarts) |

## Security

| key | default | what it does |
|---|---|---|
| `auth` | none | LDAP/AD and/or dev-file authenticator JSON (`lessdb init --auth @auth.json`) |
| `tls_cert` / `tls_key` | none | PEM files for HTTPS on `lessdb server` |
| `node_id` | random | stable compute-node identity for multi-writer merge claims |

## Logging

`lessdb server --log-level <off|error|warn|info|debug|trace> --log-dir <dir>`:

* **stdout** — human-readable lines: `2026-08-28T16:37:49Z INFO request
  method=POST path=/v1/sql status=200 latency_ms=18.94`.
* **`--log-dir`** — the same records written to
  `lessdb.YYYY-MM-DD.log`, rotated daily, 7 files kept.
* Filter with `LESSDB_LOG` (e.g. `LESSDB_LOG=info,datafusion=warn` to
  silence the planner's debug chatter).
* The audit trail is separate: `<data_dir>/audit/audit-YYYY-MM-DD.ndjson`
  — one NDJSON line per call (caller, action, outcome, duration), day-
  rotated, flush-on-write.

## Validation rules

Invalid config is rejected at load/save time: `compression` must be
zstd/lz4/none, `zstd_level` 0..=22, `flush_rows` ≥ 1,
`max_buffer_rows` ≥ `flush_rows`, `max_merge_rows` ≥ 1, and
`bloom_fp_rate` in (0, 1).

## Per-table settings

Compression is also per-table DDL: `COMPRESSION='lz4'`. Sort key, UNIQUE
columns, and TTL live in the table definition, not the config file — see
[Getting started](/docs/getting-started).
