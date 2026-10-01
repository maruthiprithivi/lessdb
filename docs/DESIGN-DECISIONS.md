# Design Decisions (ADRs)

## D1: Rust + Arrow as the foundation
Rust for memory safety without GC pauses, fearless concurrency, and a single
binary that embeds everywhere (DuckDB's distribution story). Apache Arrow is
the in-memory format: zero-copy between storage, compute, and every
integration (pandas/pyarrow/JS). Alternatives considered: C++ (slower iteration and a memory-unsafety surface), Zig (ecosystem immaturity).

## D2: DataFusion for SQL, not a from-scratch engine
Writing a competitive SQL engine is a multi-year project; DataFusion gives a
production-grade parser, optimizer, and **hash join** machinery (the
StarRocks/Doris-style joins the project wants) with Apache-grade review.
LessDB's differentiation lives below it: the storage engine, pruning, merge
pipeline, compression, GPU kernels, and integrations. Alternatives: fork DuckDB (C++, GPL-ish
license questions, hard to restructure around shared storage), build on
GlueSQL (too small).

## D3: FireflyCloud — compute/storage separated
The shared-storage architecture uses: parts in shared
object storage, no replica sets, no quorum replication protocol. Writes
become "upload an object + publish metadata", reads are listings, and
scale-out means adding stateless compute. Replication comes from object
storage itself.

v0.2 of this project completes the separation: **table manifests moved into
the shared store** (`catalog/<table>.json`), so a compute node keeps *no*
durable state for shared tables — local disk is buffers/caches only, and a
replacement node discovers every table by listing. The storage backend is a
URL (`s3://`, `gcs://`, `az://`, `file://`, `memory://`) selected at
`lessdb init --shared <url>`, with cloud backends compiled behind the `cloud`
feature. Trade-off: metadata coordination for multi-writer needs a
CAS-capable metastore (roadmap), and listing/object-API latency replaces
local-disk latency (mitigated by a persistent block cache later).

## D4: Parquet data files + JSON sidecar metadata
Custom file formats get you the last few percent of compression, but parquet
buys correctness (stats pruning, page indexes, bloom filters inside files),
interop (DuckDB/pandas/Spark can read parts directly), and Arrow-grade
readers/writers. Additional codecs (delta/Gorilla/dictionaries) plug
in as parquet encodings or post-v0.1 custom pages — the sidecar `meta.json`
(typed min/max, blooms, sort key) is independent of that evolution.

## D5: Typed pruning, never string comparison
Early on, pruning min/max values were stored as strings. `9 < 10` compares
wrong lexicographically — a silent-correctness trap. All statistics are
typed (`StatValue`), comparisons return `None` (→ keep the part) when types
don't match, and bloom filters only ever produce false positives. Pruning
must be sound by construction.

## D6: Uniqueness = sort-key prefix + keep-last (Replacing semantics)
`UNIQUE (a, b)` requires `(a, b)` to be a prefix of the sort key; a stable
sort then makes "keep the last row per unique key" exact at flush and merge
time.
This is an *eventually-exact* uniqueness model (deduplication occurs at flush and merge); strict transactional uniqueness is a v1.0 option
behind a CAS in the shared metastore. Bloom filters on unique columns make
the common `WHERE key = X` skip almost all parts.

## D7: MCP transport is hand-rolled JSON-RPC
The MCP spec is newline-delimited JSON-RPC 2.0 over stdio — small enough to
implement directly and stable against SDK churn. Tools return text tables
(and JSON), which agents handle well. No HTTP/SSE transport yet.

## D8: The io runtime is owned by the engine, and dropped on a plain thread
Object-store I/O is async, but the CLI/embedding APIs are synchronous.
`LessEngine` owns a small current-thread tokio runtime for those sync calls;
it is handed to a plain OS thread on drop because tokio panics if a runtime
is dropped inside an async context. Async callers (server, MCP, query
planner) use the `*_async` engine methods and never block.
