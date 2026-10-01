# LessDB Test-Suite Research

How mature database projects structure their test suites, and what LessDB
should copy. Researched against the reference projects' GitHub repos and
docs; mapped to LessDB's actual architecture (`docs/ARCHITECTURE.md`,
`docs/DESIGN-DECISIONS.md`, `docs/ROADMAP.md`).

## Implementation status (2026-09)

The recommendations below are being implemented incrementally:

* **Golden SQL tests** — `crates/less-sqllogictest` (sqllogictest 0.29
  harness over an in-process engine + DataFusion session) with a growing
  corpus in `tests/sqllogictest/` (basics, tables, joins/CTEs, windows,
  types incl. Decimal/UUID/Array/Map, UNIQUE semantics, functions,
  FireflyCloud + fan-out marker). Runs in CI.
* **Property testing** — pruning-soundness proptest in
  `crates/less-query/tests/` (random parts × random predicates; pruning
  must never drop a part containing matching rows).
* **Differential testing** — `crates/less-difftest`: identical data +
  queries on LessDB and embedded DuckDB must agree (relative-epsilon
  floats, NULL semantics, GROUP BY/HAVING/DISTINCT).
* **Crash & corruption** — `crates/less-engine/tests/corruption.rs`:
  corrupt `data.parquet`/`meta.json`, torn part writes, WAL garbage and
  bogus length prefixes, multi-cycle kill -9 recovery, stray files.
  Found and fixed a WAL replay bug (torn tail blocked engine open;
  huge bogus length prefix panicked on allocation).
* **Vector recall** — `crates/less-vector/tests/{recall,registry}.rs`:
  recall@k vs flat ground truth (IVF-PQ L2, IVF-flat cosine/dot),
  persistence roundtrips, multi-space isolation, edge cases, rebuild.
* **S3/MinIO e2e** — `scripts/minio-e2e.sh` in CI (MinIO in docker).
* **Fuzzing (cargo-fuzz)** — planned; targets: SQL parser, meta.json
  decode, Arrow→parquet→Arrow roundtrip.

---

## 1. Executive summary

The reference projects converge on the same shape, which LessDB should adopt
as-is:

1. **Golden SQL tests** are the backbone. DuckDB, DataFusion, and
   SQLite all drive SQL correctness through `sqllogictest`-style input/expected
   files. This is the single highest-leverage harness LessDB can add, and a
   Rust crate (`sqllogictest`) already exists.
2. **Unit tests live next to the code** (gtest/catch2 for C++, `#[cfg(test)]`
   mods + `tests/` dirs for Rust). Storage/format/index logic is unit-tested
   exhaustively before any integration layer.
3. **Fuzzing is a first-class CI stage**, not an afterthought: libFuzzer for
   SQLSmith for DuckDB, seeded equivalence fuzz for DataFusion,
   dbsqlfuzz for SQLite.
4. **Differential testing** against an independent engine is how the best
   projects catch semantic bugs (SQLite vs Postgres/MySQL; DuckDB re-runs
   SQLite's entire sqllogictest corpus). LessDB has a natural oracle in
   DuckDB via the `duckdb` Rust crate.
5. **Crash-safety and concurrency** are validated with fault injection
   (kill -9, corrupt files) and isolation/restart tests, not just happy-path
   integration.
6. **Vector search** is tested with recall@k regression (index vs. brute-force
   ground truth), index rebuild, and persistence roundtrips — exactly what
   LanceDB/Qdrant do.
7. **Slow/cloud/chaos tests are tiered**: fast unit+SQL tests on normal CI,
   S3/MinIO + LDAP + multi-node + fuzz + chaos on dedicated/self-hosted
   runners.

---

## 3. DuckDB

Repo: https://github.com/duckdb/duckdb

### Test directory structure (`test/`)

```
test/
  sql/           sqllogictest .test files, split by component (~4,692 files)
  sqlite/        sqllogictest runner + SQLite's own select1-4.test_slow corpus
  api/           C API + binding tests (also test/common, test/...)
  fuzzer/        SQLSmith repros (duckfuzz/, pedro/, sqlsmith/, public/)
  ossfuzz/       OSS-Fuzz harnesses
  optimizer/     plan/optimizer correctness
  parquet/       parquet reader tests (data + golden files)
  extension/     loadable-extension tests
  ...
```

Unit and SQL-level tests run through a single **Catch2** `unittest` binary
(`test/unittest.cpp`). The split is: C++ unit tests colocated in `src/` (and
`test/api`, `test/common`, …), while `test/sql/*.test` sqllogictest files are
each registered as a Catch2 test case by `test/sqlite/test_sqllogictest.cpp`.

### sqllogictest usage

DuckDB adopted SQLite's `sqllogictest` format (`query`, `query I`, `statement
ok`, `----` expected results). Files in `test/sql/` are organized by feature
(`test/sql/aggregate/`, `test/sql/window/`, `test/sql/join/`). The runner is
built into the `unittest`/`shell` binaries. This is the format LessDB should
use via the Rust `sqllogictest` crate.

### The SQLite-corpus suite (`test/sqlite/`)

DuckDB runs SQLite's entire public sqllogictest corpus against itself as a
**differential compatibility check** — the `select1.test` … `select5.test`
SQLite files (plus slow variants like `select4.test_slow`) — millions of
queries, caught as wrong-answer diffs. This is the archetype for LessDB's
"compare query results vs DuckDB on shared data" requirement: embed DuckDB
(the `duckdb` crate), run the same SQL over the same parquet/CSV data, and
diff sorted results.

### Fuzzing (`test/fuzzer/`)

SQLSmith (the CockroachDB/Postgres random-query generator) is adapted into a
DuckDB extension; `duckdb-fuzzer` drives random SQL and checks for
crashes/wrong answers, and there is an `ossfuzz/` harness for OSS-Fuzz.
LessDB's equivalent: an `arbitrary`-driven SQL generator feeding both the
DataFusion parser and the engine, checking for panics and for differential
agreement with DuckDB.

### CI

GitHub Actions (`.github/workflows/Main.yml`) runs fmt/clippy/unit tests,
then the sqllogictest, SQLite-corpus, and extension suites as separate jobs.

---

## 4. Apache DataFusion

Repo: https://github.com/apache/datafusion

This is the single most important reference because **LessDB's query layer
is DataFusion** — LessDB inherits DataFusion's SQL semantics and can reuse
its test harness patterns verbatim.

### sqllogictest harness (`datafusion/sqllogictest/`)

- `.slt` files under `datafusion/sqllogictest/test_files/`, split by feature
  (`select.slt`, `aggregate.slt`, `window.slt`, `join.slt`, `explain.slt`, …):
  **~186 top-level `.slt` files** plus subdirectories, with opt-in
  `tpch`/`sqlite`/`postgres`/`substrait` suites.
- `datafusion/sqllogictest/` is a **thin driver** (crate
  `datafusion-sqllogictest`) over the upstream `sqllogictest` crate
  (https://github.com/risinglightdb/sqllogictest-rs, v0.29.x) — it implements
  the `AsyncDB` trait against a DataFusion `SessionContext` and shells out to
  `sqllogictest::Runner`.
- LessDB should depend on the `sqllogictest` crate directly and write its own
  thin `SessionContext`-building harness (see §9.3).

### Fuzzing (seeded equivalence fuzz, not libFuzzer)

The historical `datafusion/fuzz-utils` was a `rand`-based helper crate, and it
was **removed in PR #2081 (Feb 2023)**. There are no `cargo-fuzz`/libFuzzer
targets in the tree today. Current fuzzing = **seeded, in-process
equivalence/differential harnesses** at `datafusion/core/tests/fuzz_cases/`,
gated behind the `extended_tests` cargo feature: a fixed seed generates random
inputs and asserts a specialized operator implementation matches a naive
reference (or that results are stable). LessDB should adopt this pattern for
"optimized == reference" checks and additionally run `cargo-fuzz` +
`arbitrary` for panic-free SQL/parse/data decode (bloom filter, `meta.json`,
parquet part reads).

### Benchmarks (`datafusion/benchmarks/`)

TPC-H-inspired harnesses (`cargo run --release --bin tpch -- ...`) that
also serve as **semantic regression tests**: CI job `verify-benchmark-results`
runs `benchmark-plan` + `benchmark-sqllogic` and fails on any `git diff` in the
generated plans/results. LessDB's roadmap already lists `lessdb bench` vs
DuckDB; the TPC-H-inspired workloads corpus doubles as a correctness harness
and should produce plan files that are committed and diffed in CI.

### Unit tests

- `#[cfg(test)]` mods and `tests/` dirs per crate.
- Note: DataFusion does **not** use `proptest`/`arbitrary`/`datadriven`; it
  uses `rand` + `rstest` + `test-case` for parametrized tests, with `.slt`
  golden files as the data-driven layer. (LessDB should still use `proptest`
  for codec/stat/pruning round-trips — LanceDB and Qdrant both lean on it.)

### CI

`.github/workflows/rust.yml` runs fmt/clippy/test, then the sqllogictest
suite and `extended_tests` (fuzz cases). Benchmarks and their
`verify-benchmark-results` diff gate are separate jobs.

---

## 5. SQLite

Repos/sources: https://sqlite.org/testing.html, https://github.com/sqlite/sqlite

SQLite is the gold standard for database testing and the origin of the
sqllogictest format LessDB will use.

### The TCL test suite (`test/*.test`)

~1,400 TCL files (`test/alter.test`, `test/select1.test`, `test/where.test`,
`test/join.test`, …) yielding ~51,000 distinct cases, driven by a TCL harness
(`test/tester.tcl`) with `do_test`/`execsql` helpers and `do_catchsql_test`
for asserting an error is raised — the model for LessDB's "expected error"
cases. Runs in modes `quick`, `veryquick`, `all`, and `permutations` (the
latter re-runs the suite under different compile-time options).

### TH3 (Test Harness #3)

A commercial-grade test generator: **hundreds of millions of tests** generated
from templates that emit SQL + expected results + crash injection, checking
wrong answers, leaks, and crashes. The harness source is public domain
(available to SQLite Consortium members), the test *data* is private. The
copyable idea is not TH3 itself but its **shape**: generate parameterized SQL
over schema/data templates, and check correctness + crash-resistance in one
loop.

### Other vectors

- **sqllogictest corpus** — the `*.test`-style SLT files (now public) that
  DuckDB/DataFusion/etc. consume. LessDB's golden tests are this format.
- **dbsqlfuzz** — differential fuzzing comparing SQLite vs Postgres/MySQL/
  SQL Server on random SQL, mutating the SQL and DB file together
  (~500M cases/day). Archetype for LessDB vs DuckDB differential.
- **fuzzcheck / jfuzz / OSS-Fuzz**, **mptester**, **threadtest** (concurrency),
  **releasetest** (release gate), **speedtest** (perf).

### Why it's famously thorough

100% branch and 100% MC/DC coverage claimed, enforced with `gcov -b`, plus
`testcase()`/`ALWAYS()`/`NEVER()` macros that make every branch reachable,
mutation testing, and OOM/IO-error/crash fault injection. The process lesson
for LessDB: **every bug fix must land with a sqllogictest/proptest regression
case in the same PR.** Two directly-copyable sqllogictest idioms: `rowsort`
/`valuesort` for order-independent result comparison, and running the same
query optimized vs. unoptimized to catch optimizer bugs.

---

## 6. PostgreSQL (brief)

Repo: https://github.com/postgres/postgres

- **Regression tests** — `src/test/regress/`: `pg_regress` runs `sql/*.sql`
  and diffs against `expected/*.out` (golden files, like sqllogictest but
  PostgreSQL-native). `make check`.
- **Isolation tests** — `src/test/isolation/`: `isolationtester` runs
  `specs/*.spec` + `expected/` to test concurrent transaction interleavings
  (races, deadlocks, blocking) with explicit permutation control. Direct model
  for LessDB multi-writer tests.
- **TAP tests** — `src/test/perl/`, `src/bin/*/t/`, `src/test/recovery/t/`:
  Perl `Test::More` + `PostgreSQL::Test::Cluster`, driving real server
  processes for **crash recovery, replication, WAL** scenarios. This is the
  archetype for LessDB's "kill -9 mid-write, restart, verify" tests.
- **SQLsmith** (out-of-tree, https://github.com/anse1/sqlsmith) — random query
  generator for differential/fuzz testing; **buildfarm**
  (buildfarm.postgresql.org) runs the full matrix across platforms/versions.

---

## 7. LanceDB (vector search)

Repos: https://github.com/lancedb/lance (now `lance-format/lance`; Rust core),
https://github.com/lancedb/lancedb (Python + server).

### Layout (Rust core is a workspace under `rust/`)

- **`rust/lance/`** — main crate (dataset, scanner, transactions, IVF/PQ/HNSW
  orchestration). Inline `#[cfg(test)] mod tests` at the bottom of source files
  is the dominant pattern (not one big `tests/` tree).
- **`rust/lance-index/`** — extracted low-level vector-index crate (graph,
  kmeans, HNSW, IVF storage, PQ/SQ/BQ, `flat/`). Brute-force `flat` is the
  ground-truth oracle.
- **`rust/lance/tests/`** — one integration-test binary per crate; modules
  (`query/`, `utils/`) gated behind a `slow_tests` cargo feature.
- **`rust/lance/src/dataset/tests/`** — 18 files: `dataset_index.rs`,
  `dataset_versioning.rs`, `dataset_merge_update.rs`, `dataset_transactions.rs`,
  `fragment_validate_tombstones.rs`, etc.

### Recall@k — the pattern to copy (`rust/lance/src/index/vector/ivf/v2.rs`)

All IVF-family recall/regression tests live here, structured as:

- **Ground truth** — `async fn ground_truth(...)` runs an **exhaustive scan**
  with `.use_index(false)` and returns the true top-k row-ids.
  (HNSW side: `brute_force_topk()` over flat storage.)
- **Recall assertion** — `async fn test_recall(params, nlist,
  recall_requirement, ...)` searches with `nprobes(nlist)` and asserts
  `recall = |index ∩ gt| / k >= recall_requirement`.
- **Parametrized thresholds via `rstest` `#[case(nlist, metric, recall)]`**:
  IVF_FLAT **1.0** (exact), IVF_SQ **0.85/0.85/0.75** (L2/Cosine/Dot),
  IVF_RQ **0.9**, IVF_HNSW **0.9**, IVF_PQ **0.5**, standalone HNSW
  **≥ 0.5** (a documented "project rule"), online/incremental HNSW **0.85**.

Canonical templates worth naming directly:

- **`test_pq_matrix_case`** — *persistence + recall*: build index →
  `drop(dataset)` → `Dataset::open(uri)` → recompute ground truth → search →
  assert recall ≥ 0.5 **and** distances finite & sorted.
- **`test_remap_impl`** — *index rebuild after compaction*: delete/update
  rows → `compact_files(...)` → reopen → re-assert recall with an epsilon
  tolerance (`recall_requirement - f32::EPSILON`), including the "delete down
  to 1 row to force remap" edge case.
- **`test_builder_write_load` / `test_loaded_search_parity_and_recall`**
  (HNSW) — build → serialize (`to_batch`) → `HNSW::load` → assert
  `builder_results == loaded_results` **and** recall ≥ 0.5.
- **`test_write_and_load` / `test_load_v1_format_ivf`** — index storage
  roundtrip + old-format compatibility.

### Property-based tests & data generation

- **`proptest` is a workspace dev-dependency**; the flagship is
  `rust/lance-encoding/src/encodings/fuzz_tests.rs` ("comprehensive fuzz
  testing for Lance 2.1 encoding coverage", 16 encoding permutations via
  `EncodingTestConfig`). Also used for distance kernels (`lance-linalg`),
  row-id bitmaps, and arrow-scalar/arrow-stats.
- **Deterministic synthetic data**: `rust/lance-testing/src/datagen.rs`
  (`ArrayGenerator`, `generate_random_array_with_seed`) and `rust/lance-datagen`
  (`gen_batch().with_seed(Seed(42)).col("vector", array::rand_vec::<T>(..))`
  with `RowCount`/`ByteCount`/`Dimension`). No reliance on real datasets in
  unit tests.

### Fault injection

No `cargo-fuzz`/libFuzzer — fault injection is a **store proxy**:
`rust/lance/src/utils/test/failing_store.rs` (`FailingProxyStore.fail_after_n
(method, path_substr, skip, error)`) exercises partial-write cleanup paths;
`throttle_store.rs` injects latency. Copy this for LessDB's object-store
write/cleanup fault tests.

### Persistence / versioning / compaction

- Manifest correctness — `rust/lance-table/src/{format,io}/manifest.rs`
  (`test_roundtrip_manifest`, `test_read_large_manifest`), `io/commit.rs`
  (`test_manifest_naming_migration`, `test_commit_lock_released_on_cancellation`).
- Versioning — `dataset_versioning.rs` (`test_restore`, `test_tag`,
  `test_branch`, `test_fragment_id_never_reset`).

### Integration vs. unit split

Rust core: inline unit tests + `rust/lance/tests/` binary; Python surface:
`python/python/tests/` (`test_index.py`, `test_query.py`, `test_table.py`).
Key point: **the Python layer does not assert recall@k — the Rust core owns
recall; Python asserts API surface and plan shape** (e.g. `assert "ANN" in
query.explain_plan(verbose=True)`, `test_search_after_merge`). Server/cloud is
tested separately (`test_remote_db.py`, `test_e2e_remote_db.py`). LessDB should
mirror this: recall owned by `less-vector` Rust tests; SDKs assert API + plan
shape only.

### Qdrant (brief)

Repo: https://github.com/qdrant/qdrant

- Vector index in **`lib/segment`**; unit tests colocated, integration in
  `lib/segment/tests/integration/` (`filtrable_hnsw_test.rs`,
  `hnsw_quantized_search_test.rs`, `hnsw_incremental_build.rs`).
- Recall measurement = **quantized vs. exact** search gap; a shared helper
  enforces a **40% recall floor** for mid-bit quantizers, and asserts the
  distance/error gap (not raw recall) for low-bit cases that can't clear it.
- proptest in `lib/segment` (score fusion, id-tracker versions, query
  discover/context/reco).
- Integration `tests/` (consensus, snapshot/restore, openapi).

### Milvus (brief)

Repo: https://github.com/milvus-io/milvus

- C++ core gtest in `internal/core/unittest/` (`test_indexing.cpp`,
  `test_storage.cpp`); vector index correctness delegated to the vendored
  **knowhere** library (`CheckVecIndexWithDataType`).
- Python per-index tests in `tests/python_client/testcases/indexes/`
  (`test_hnsw.py`, `test_hnsw_pq.py`, `test_diskann.py`, …).
- Explicit recall tests: `first_recall_test.py` (`recall >= 0.6`) and
  `test_search_recall_with_maxsim_ground_truth` (brute-force maxsim ground
  truth, `recall >= 0.8`, monotonic in `retrieval_ann_ratio`); range search
  asserts hit count ≥ `expected_limit * 0.8`.
- The copyable idea: keep a small curated recall dataset with a fixed
  threshold, and fail CI when recall regresses.

---

## 8. What LessDB should copy, mapped to our feature set

Direct mapping from the user's requested categories to concrete harnesses
and representative test names.

### 8.1 SQL correctness — sqllogictest-style golden tests
- Harness: `sqllogictest` crate + a `less-sqllogictest` wrapper around a
  DataFusion `SessionContext` registered with a `LessTableProvider`.
- Files: `tests/sqllogictest/select.slt`, `aggregate.slt`, `join.slt`,
  `window.slt`, `explain.slt`, `pruning.slt`, `vector_search.slt`.
- Copy DuckDB/DataFusion: expected-error cases (`statement error`), sorted
  output diffs, and `EXPLAIN` golden files (catch plan regressions).

Example `tests/sqllogictest/test_files/select.slt`:

```
statement ok
CREATE TABLE t (id BIGINT UNIQUE, ts TIMESTAMP, v DOUBLE) ENGINE = Firefly
  ORDER BY id

statement ok
INSERT INTO t VALUES (1, '2024-01-01 00:00:00', 1.5), (2, '2024-01-02 00:00:00', 2.5)

query IIR
SELECT id, id, v FROM t ORDER BY id
----
1 1 1.5
2 2 2.5

# pruning must skip the part entirely when the bloom filter proves absence
query I
SELECT count(*) FROM t WHERE id = 999
----
0

statement error
SELECT * FROM t WHERE nonexistent_col = 1
```

### 8.2 Storage engine — crash safety, WAL, merges, pruning
- Merge correctness: insert N batches → flush → merge → assert result ==
  dedup-by-`UNIQUE`-keep-last (differential vs an in-memory reference merge).
- WAL (roadmap): TAP-style recovery test — write, `kill -9`, restart, assert
  no lost/duplicated committed rows (Postgres `src/test/recovery/t/` model).
- Part atomicity: assert a killed write leaves old-or-new part, never torn.
- Pruning correctness (property test): generate random data + predicates;
  assert pruned-part query result == full-scan result, for every skip.
- Bloom false-positive safety: assert blooms never cause false negatives.

### 8.3 Vector search — recall@k, index rebuild, persistence
- Recall@k regression (LanceDB `ivf/v2.rs::test_recall` pattern): ground
  truth = exhaustive `flat` scan (`use_index(false)`), then
  `recall = |index ∩ gt| / k`; parametrize thresholds per index/metric via
  `rstest` — flat **1.0**, IVF-PQ **0.5**, IVF-flat for cosine/dot **1.0**,
  HNSW **0.9** (copy Lance's documented thresholds and tune for `less-vector`).
- Persistence roundtrip (Lance `test_pq_matrix_case`): build → `drop` →
  reopen from `<data_dir>/vectors/<space>/` → recompute ground truth →
  assert recall + sorted/finite distances; plus `index.bin`/`data.bin`/
  `meta.json` byte-level roundtrip.
- Index rebuild after compaction (Lance `test_remap_impl`): delete/update
  rows → compact → reopen → re-assert recall with an epsilon tolerance, and
  handle the "force remap down to empty partition" edge case.
- Exact-re-rank invariant: ANN re-ranked top-k == brute-force top-k for the
  finalists.
- Fault injection (Lance `FailingProxyStore`): object-store failures mid-write
  must clean up partial `index.bin`/`data.bin` and leave the space reopenable.

### 8.4 Graph/context store — traversal invariants, deletion cascades
- Traversal invariants: BFS/shortest-path distance == reference BFS
  (differential vs a naive implementation); `neighbors(k, d)` closure ==
  iterative expansion.
- Deletion cascades: delete node → all incident edges gone; auto-created
  endpoints cleaned up; no orphan edges (property + targeted tests).
- Persistence: `graph.json` atomic snapshot (tmp + rename) survives crash.

### 8.5 MCP protocol — conformance, error handling
- Conformance: spawn `lessdb mcp` over stdio, speak JSON-RPC 2.0, validate
  `initialize`, `tools/list`, `tools/call` for every tool, and response
  shapes (text/JSON tables).
- Error handling: unknown method → `-32601`; malformed JSON → parse error;
  tool errors → structured `isError` results; invalid params → `-32602`.
- Idempotency/edge cases: empty args, unexpected fields, concurrent requests.

### 8.6 Auth — LDAP flows, fail-closed
- LDAP flows: bind against a real directory (glauth or OpenLDAP in a
  testcontainer) — valid user, wrong password, group→role mapping.
- Fail-closed: user in no mapped group denied (unless `default_role`);
  directory-unreachable → deny, never allow.
- Injection: RFC 4515-escaped usernames (`*`, `(`, `)` in usernames) never
  break the filter.
- File authenticator parity: `{"file": ...}` behaves identically to LDAP.

### 8.7 Concurrency — multi-writer, parallel queries
- Parallel queries over immutable parts: N threads issue scans/aggregations,
  assert results equal single-threaded.
- Multi-writer (FireflyCloud, once the CAS `MetaStore` lands): concurrent
  part publication with CAS, assert no lost part / no torn manifest.
- Isolation-style: interleave flush + merge + read, assert reads always see a
  consistent part set.

### 8.8 Fault injection — corrupt files, kill -9 mid-write
- Corrupt parquet part / `meta.json` → graceful error (not panic), catalog
  skips/repairs.
- `kill -9` mid-flush/mid-merge → restart → no torn parts, consistency holds.
- Bloom/stat decode on truncated/malformed `meta.json` → error, never UB.

### 8.9 Differential testing — vs DuckDB
- Embed DuckDB via the `duckdb` crate; load identical parquet data into both;
  run a shared SQL corpus; diff sorted results (DuckDB's SQLite-corpus model).
- Include edge cases: NULLs, type casts, window frames, `UNIQUE` dedup
  semantics where they diverge (document expected divergences).

### 8.10 Fuzzing — SQL parser fuzz, data fuzz
- SQL parser fuzz: `cargo-fuzz` + `arbitrary`-generated/mutated SQL into
  DataFusion's parser, assert no panic (complements DataFusion's own seeded
  `fuzz_cases/` equivalence harnesses).
- Seeded equivalence fuzz: assert optimized/specialized operators (pruning,
  merge dedup, ANN re-rank) match a naive reference implementation
  (DataFusion `datafusion/core/tests/fuzz_cases/` model).
- Data fuzz: random Arrow batches → parquet write/read roundtrip → assert
  identical; random `meta.json` stats vs recomputed stats.
- Differential fuzz: random SQL run on both LessDB and DuckDB, diff results.

### 8.11 Upgrade / migration
- Manifest versioning: read a manifest written by an older code version
  (fixture files committed to the repo) and migrate in place.
- Part-format compatibility: old `meta.json`/`data.parquet` fixtures still
  readable after format evolution.

### 8.12 CI integration
- Fast tier (GitHub Actions / normal runner): unit, clippy, fmt, sqllogictest,
  proptest (short), CLI smoke, MCP conformance, auth file-authenticator.
  Workflow: `.github/workflows/ci.yml`.
- Slow tier (self-hosted runner): S3/MinIO, LDAP (testcontainers), multi-node,
  fault injection, differential vs DuckDB, fuzzing, TPC-H-inspired workloads.
  Workflow: `.github/workflows/ci-slow.yml` (runs on a `self-hosted` runner
  label; `nightly` cron for the long soak/differential jobs).

---

## 9. Recommended test-suite architecture for LessDB

### 9.1 Directory layout

```
crates/<crate>/
  src/...          #[cfg(test)] unit tests colocated with code
  tests/           crate-level integration tests
  benches/         per-crate criterion benches (optional)

tests/                     (workspace-level integration harnesses)
  sqllogictest/            .slt golden files + runner binary
    test_files/
      select.slt
      aggregate.slt
      join.slt
      window.slt
      explain.slt
      pruning.slt
      vector_search.slt
  differential/            LessDB vs DuckDB harness + corpus
  integration/
    ldap/                  glauth/OpenLDAP via testcontainers
    s3/                    MinIO via testcontainers
    multinode/             multi-node FireflyCloud
    fault/                 kill -9, corrupt-file injection
    upgrade/               old-format fixture migration
  fixtures/                committed parquet parts, old meta.json, recall dataset

fuzz/                      cargo-fuzz targets (top-level, cargo fuzz discovers it)
  fuzz_targets/
    sql_parser.rs          DataFusion SQL parser fuzz (no panic)
    data_roundtrip.rs      Arrow->parquet->Arrow roundtrip fuzz
    meta_json.rs           malformed meta.json decode fuzz
```

### 9.2 Harness choices (Rust crates)

| Concern | Crate | Notes |
|---|---|---|
| Golden SQL tests | `sqllogictest` (risinglightdb/sqllogictest-rs) | Same crate DataFusion uses; supports async, expected-error, sorted output |
| Property testing | `proptest` | Codec roundtrips, pruning soundness, merge dedup invariants |
| Fuzzing | `arbitrary` + `cargo-fuzz` (`libfuzzer-sys`) | SQL parser fuzz, data fuzz, bloom/meta decode fuzz |
| Parametrized tests | `rstest` + `test-case` | `#[case(nlist, metric, recall)]` for recall thresholds, codec/type matrices (LanceDB/DataFusion pattern) |
| Snapshot testing | `insta` | `meta.json`, `EXPLAIN` plans, MCP JSON responses |
| Differential oracle | `duckdb` (duckdb-rs) | Embed DuckDB for result diffing |
| Temp files/dirs | `tempfile` | All engine tests |
| Serializing global state | `serial_test` | Tests touching the singleton runtime/config |
| CLI testing | `assert_cmd` + `predicates` + `assert_fs` | `lessdb` CLI smoke/e2e |
| HTTP mocking | `httpmock` / `mockito` | `less-server` route/auth tests (if not using a real dir) |
| LDAP | `glauth` (Go binary) in a container + `ldap3` client | Real bind/rebind flows |
| S3 | MinIO container + `object_store` | Also `object_store`'s `memory://`/`file://` for fast tests |
| Containers | `testcontainers` / `testcontainers-modules` | LDAP + MinIO lifecycle |

### 9.3 What to build ourselves

1. **`less-sqllogictest` runner** — a thin binary that builds a
   `SessionContext`, registers `LessTableProvider` + the `vector_search`
   table function, and hands control to the `sqllogictest` crate. ~150 lines.
   The `sqllogictest` crate API is minimal: implement the `AsyncDB` trait
   (`async fn run(&mut self, sql: &str) -> Result<DBOutput>` returning
   `DBOutput::Rows`, `DBOutput::StatementComplete(_)`, or
   `DBOutput::StatementError(_)`), then drive it with
   `sqllogictest::Runner::new(|| async { test_ctx }).run_file(path).await`.
   Use its **record mode** to generate `.slt` files from a running engine and
   **validate mode** in CI; expected-error assertions map to
   `statement error` lines in the `.slt` file. DataFusion ships this exact
   pattern as its `datafusion-sqllogictest` crate — copy that, but backed by
   LessDB's own session setup.
2. **Differential harness** (`tests/differential/`) — a binary that loads the
   same parquet into LessDB and DuckDB, runs a shared SQL corpus, and diffs
   sorted results with a divergence allowlist.
3. **Fault-injection helpers** — a process-level test harness that can
   `kill -9` the child mid-write, corrupt part bytes, and assert recovery
   (Postgres TAP model, but in Rust).
4. **Recall harness** (`tests/recall/`) — brute-force `flat` ground truth +
   threshold assertions + a tiny curated seed dataset committed as a fixture.
5. **Part/manifest golden helpers** — write a part, assert `meta.json` schema
   + stats exactly (via `insta`), reload and diff.

### 9.4 Phased rollout

**Phase 1 — foundation (now, runs on GitHub Actions, fast)**
- `#[cfg(test)]` unit tests in every crate: `less-storage` (codecs, bloom,
  typed `StatValue` comparisons), `less-catalog` (manifest serde roundtrip),
  `less-graph` (traversal/cascade invariants), `less-memory` (dedup keep-last).
- `sqllogictest` harness + `select/aggregate/join/window/explain.slt`.
- CLI smoke tests (`assert_cmd`); MCP JSON-RPC conformance + error handling;
  auth file-authenticator + fail-closed unit tests.
- **Pruning soundness property test** — highest-value single test; catches
  silent wrong-answer bugs (the ADR D5 class).

**Phase 2 — engine correctness (mostly fast, some slow)**
- Merge correctness + dedup-keep-last differential vs in-memory reference.
- WAL crash-recovery tests (TAP-style kill/restart) once WAL lands.
- Vector recall@k + index rebuild + persistence roundtrip.
- Part atomicity + corrupt-file fault injection.
- `proptest` for codec and part roundtrips.

**Phase 3 — differential + fuzz (self-hosted runner)**
- LessDB vs DuckDB differential corpus.
- `cargo-fuzz` SQL parser + data fuzz + bloom/meta decode fuzz.
- Concurrency: multi-writer CAS publication, parallel queries.
- Upgrade/migration fixtures.

**Phase 4 — cloud, multi-node, chaos, perf (self-hosted runner only)**
- MinIO S3 integration + multi-node FireflyCloud (needs `cloud` feature).
- LDAP flows via testcontainer.
- Chaos: kill nodes mid-merge, corrupt object-store parts.
- TPC-H-inspired workloads correctness+perf regression.

### 9.5 Which tests need real S3/MinIO, and where they run

- **`memory://` / `file://` object store** covers the whole engine path in
  unit/integration tests with zero external deps — run everywhere.
- **MinIO (testcontainers)** for `s3://`-specific behaviour: multipart/atomic
  object put, listing semantics, credential handling. Self-hosted runner (or
  GH Actions with docker — but self-hosted keeps it fast/reliable).
- **Real S3** optional, only for a nightly "cloud smoke" job.
- **LDAP** needs a directory — glauth in a container on the self-hosted runner
  (or GH Actions docker); file-authenticator covers CI without docker.

---

## 10. Citations / consulted URLs


- DuckDB: https://github.com/duckdb/duckdb — `test/sql`, `test/sqlite`
  (`test_sqllogictest.cpp`), `test/unittest.cpp`, `test/fuzzer`, `test/ossfuzz`,
  `test/api`, `src/**/test`.
- DataFusion: https://github.com/apache/datafusion — `datafusion/sqllogictest`,
  `datafusion/core/tests/fuzz_cases`, `datafusion/benchmarks`;
  sqllogictest crate https://github.com/risinglightdb/sqllogictest-rs (v0.29.x);
  contributor testing guide https://datafusion.apache.org/contributor-guide/testing.html.
- SQLite: https://sqlite.org/testing.html, https://github.com/sqlite/sqlite
  (`test/*.test`, `test/tester.tcl`); TH3 + dbsqlfuzz described at
  https://sqlite.org/testing.html.
- PostgreSQL: https://github.com/postgres/postgres — `src/test/regress`,
  `src/test/isolation`, `src/test/perl`, `src/test/recovery/t`;
  SQLsmith https://github.com/anse1/sqlsmith; buildfarm https://buildfarm.postgresql.org.
- LanceDB: https://github.com/lancedb/lance (now `lance-format/lance`),
  https://github.com/lancedb/lancedb — `rust/lance/src/index/vector/ivf/v2.rs`,
  `rust/lance-index/`, `rust/lance-encoding/src/encodings/fuzz_tests.rs`,
  `rust/lance/src/utils/test/failing_store.rs`.
- Qdrant: https://github.com/qdrant/qdrant — `lib/segment/tests/integration`.
- Milvus: https://github.com/milvus-io/milvus — `internal/core/unittest`,
  `tests/python_client/testcases/indexes`.
- cargo-fuzz / libFuzzer: https://github.com/rust-fuzz/cargo-fuzz.
- Rust crates: `sqllogictest` (crates.io), `proptest`, `arbitrary`, `rstest`,
  `insta`, `duckdb`, `testcontainers`, `assert_cmd`, `httpmock`, `ldap3`.
