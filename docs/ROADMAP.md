# LessDB Roadmap

Status: **v0.1 — working core**: Firefly + FireflyCloud engines,
compression, uniqueness with bloom pruning, DataFusion SQL, GPU kernels,
CLI, HTTP server, MCP server (database + context + memory tools), Python &
Node SDKs (built and smoke-tested), in-memory context graph + RAM tables,
skills, docs. **v0.2 compute/storage separation**: shared catalog (table
manifests in object storage), `lessdb init --shared s3://gcs://az://file://`,
stateless compute nodes (multi-node tests), cloud backends behind the
`cloud` feature. **P0 hardening (in progress)**: WAL crash-safe inserts
(len-prefixed + lsn-stamped records, idempotent replay via per-part
`wal_lsn_max`), SQL `INSERT INTO … VALUES/SELECT`, session.register_file +
CLI `--parquet`/`--arrow` ingestion, `COPY TO` (parquet/csv/arrow),
Decimal/UUID/Array/Map types, `rust-toolchain.toml` pin, self-hosted CI. **v0.3 security & telemetry**: LDAP/AD authentication
(`less-auth`, Basic auth + roles on the HTTP server, fail-closed group
mapping), Prometheus `/metrics` (queries, durations, rows, parts
scanned/pruned, auth failures, process metrics). **v0.4 vector search**:
`less-vector` registry (multi-space, l2/cosine/dot, flat + IVF-PQ with ADC
and exact re-ranking), embedding-function registry, `vector_search` SQL
table function, MCP/CLI tools, persistence. **v0.5 openCypher**: `less-cypher`
openCypher subset over the context graph (pattern matching incl.
variable-length paths, aggregation, CREATE/DELETE/SET), exposed via
`lessdb cypher` and MCP `less_cypher`. **P1 (in progress)**: shared-store
block cache (bounded LRU + optional disk tier, `lessdb cache`, cache-aware
`CachingObjectStore` for part reads), sync-engine-API safety inside async
runtimes (`block_on_owned`), query memory limits (`memory_limit` →
bounded DataFusion pool + `lessdb_memory_pool_bytes` gauge, fail-fast
`ResourcesExhausted`), TLS on the HTTP server (rustls,
`lessdb server --tls-cert/--tls-key`, rcgen-based e2e handshake test), CAS
multi-writer FireflyCloud (MetaStore + conditional part publication +
merge claims, `FileMetaStore`/`ObjectMetaStore`, concurrency test). Next:
S3/MinIO e2e CI. **P2**: MCP **agent tenancy** (done) — per-agent
namespaces for the agent-memory tier (`lessdb mcp --tenant <name>`, default
`default`): contexts/memory tables under
`<data_dir>/tenants/<name>/memory/` and vector spaces under
`<data_dir>/tenants/<name>/vectors/`, while `less_*` SQL tools stay on
the shared engine.

## v0.2 — Context/graph depth (agent memory)
- [x] openCypher subset over the context graph (`less-cypher`: MATCH/WHERE/
      RETURN/aggregates/ORDER BY/SKIP/LIMIT/DISTINCT, variable-length
      patterns, CREATE/DELETE/SET; `lessdb cypher`, MCP `less_cypher`)
- [ ] Inverted index over context text/properties (ranked search at scale)
- [ ] Weighted shortest paths (Dijkstra) + edge weights
- [ ] SQL graph table-functions (`neighbors(key, depth)`,
      `shortest_path(a, b)`) over the context store
- [ ] Embeddings + vector search on context text (HNSW), hybrid ranking
- [ ] Context subscriptions/change streams for agent orchestration

## v0.3 — Correctness & storage depth
- [x] WAL for the insert buffer (crash-safe inserts)
- [x] Size-tiered merge policy (merge *similar-sized* parts,
      instead of merge-everything) + chunked k-way
      merge: per-part sorted runs merge in 64k-row chunks streamed
      straight to parquet (no concat-everything blowup); spill-to-disk
      external merges remain
- [ ] Sparse primary index per part (granule-level key ranges) for fast
      range scans on the sort key
- [x] DELETE/UPDATE via replace-parts (mutation marks + background rewrite)
- [x] TTL / part retention (`TTL <col> INTERVAL <n> DAY|HOUR|MONTH`,
      enforced at flush and OPTIMIZE via replace-parts)
- [ ] Low-cardinality dictionaries, delta/double-delta/Gorilla codecs
- [x] Decimal, UUID, Array/Map/JSON column types

## v0.4 — Scale-out
- [x] S3/GCS/Azure backends (`cloud` feature) + `lessdb init --shared <url>`
- [x] Shared catalog: table manifests in object storage; stateless compute
      nodes discover tables by listing
- [x] Pluggable `MetaStore` for FireflyCloud with compare-and-swap part
      publication (single-writer assumption removed): CAS `meta.json`
      publication, merge ownership claims with TTL + takeover,
      `FileMetaStore` (flock) + `ObjectMetaStore` (conditional copy);
      multi-writer concurrency test. etcd = roadmap backend for
      cross-machine update-CAS
- [x] Multi-node `SELECT` fan-out v1 (`less-fanout`): coordinator splits
      scans across HTTP nodes via `lessdb_shard(col, i, n)` part-hash
      sharding + partial aggregation, then re-aggregates locally
      (count/sum/min/max subset; e2e test over two nodes)
- [ ] gRPC transport + full DataFusion distributed execution
- [ ] Background merge workers on dedicated nodes (shared storage makes
      merges movable)

## v0.5 — Vector search depth
- [ ] Vector columns inside Firefly tables (FixedSizeList<Float32>) with
      per-part ANN indexes + filtered (pre-filter) search
- [ ] HNSW index; PQ for cosine/dot; GPU-accelerated brute-force kernels
- [ ] Multi-space federated search + hybrid keyword/vector ranking

## v0.6 — GPU & performance
- [x] Persistent block cache for shared-storage reads (bounded LRU +
      optional disk tier; `block_cache_bytes`/`block_cache_dir`, `lessdb cache`)
- [ ] Operator-level GPU dispatch in the query plan (filter/aggregate
      kernels with cost-based routing to GPU, CPU fallback)
- [ ] f64 kernels behind device feature detection
- [ ] SIMD/vectorized kernels in the engine hot paths
- [ ] Persistent block cache for shared-storage reads (NVMe + memory)
- [ ] `lessdb bench` controlled cross-engine comparison harness

## v0.7 — Security & observability depth
- [x] **Control plane for both doors (P0, see docs/AGENT-GOVERNANCE.md):**
      append-only audit log (`lessdb audit`; `GET /v1/admin/audit` pending),
      MCP auth with agent tokens (`lessdb token create`,
      `lessdb mcp --require-auth`), tool→permission map
- [ ] **Approval gates for agent writes (P1):** `lessdb mcp --approval`,
      `less approvals list/accept/reject` — propose→approve→commit
- [ ] Table-level grants (catalog field + pre-exec check)
- [ ] Quotas per caller (max rows scanned, concurrency) + schema-governance
      flags (`allow_agent_ddl`)
- [ ] JWT/bearer tokens and mTLS for service-to-service access
- [ ] Per-table metrics and query-level tracing (OpenTelemetry)
- [x] LDAP connection pooling/caching (avoid per-request directory binds)
- [ ] P2: row-level security (ROW POLICY predicates) + column masking,
      OpenLineage events, OAuth 2.1/PKCE resource-server flow on MCP

## v0.8 — Integrations
- [x] Rich interactive TUI for the CLI (ratatui: table sidebar, scrollable
      typed results, editor with history, \t/\d/\p/\o/\h meta commands,
      running indicator, TTY fallback to the line shell)
- [ ] Arrow Flight SQL endpoint (JDBC/ADBC/DBAPI clients)
- [ ] Python SDK: pyarrow/polars DataFrame interop, `to_pandas`, lazy
      insert API
- [ ] Node SDK: Apache Arrow JS interop, TypeScript types
- [ ] MCP: resources for tables, sampling tool, `less_tune` prompt
- [ ] Skills: query-tuning playbook, schema-design playbook

## v1.0 — Production hardening
- [ ] Transactions on inserts (batch atomicity), idempotent retries
- [ ] Auth (mTLS/JWT), TLS everywhere, audit log
- [ ] Query cancellation, memory limits, admission control
- [ ] Prometheus metrics + OpenTelemetry traces
- [ ] Point-in-time recovery from shared storage snapshots
- [ ] Chaos + fuzz test suites; differential testing vs DuckDB
