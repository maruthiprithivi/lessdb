# LessDB Benchmark Suite

A sandboxed, small-scale performance/resource/resiliency harness
(`bench/`, binary `less-bench`) that establishes baselines today and is
designed to scale to real hardware later.

## Design goals

1. **Sandbox-safe**: tiny default scale factor (SF 0.01 ≈ 60k lineitems),
   deterministic seeded data, throwaway temp directories, an RSS budget
   guard mindset (report memory every phase; scale via `--sf`, not by
   accident).
2. **Full-stack**: analytics (TPC-H-inspired), storage footprint,
   metadata resiliency, vector search, graph, interactive/dashboard
   latency, and a simulated MCP **agent session**.
3. **Reproducible baselines**: `results.json` (machine-readable) +
   `latest.md` (human-readable), regenerated per run; deterministic data
   so runs are comparable across machines.
4. **Scalable**: every phase is parameterized (`--sf`, vector counts,
   graph sizes) so the same harness runs the large-scale campaign on the
   proper server unchanged.

## Run it

```bash
cargo build --release                          # CLI (for the MCP phase)
cargo build --release --manifest-path bench/Cargo.toml
./bench/target/release/less-bench --sf 0.01          # small smoke scale
./bench/target/release/less-bench --sf 0.05 --keep   # bigger; keep data dir
LESS_BIN=/path/to/lessdb ./bench/target/release/less-bench   # pin the CLI binary
```

Reports land in `bench/results/{latest.md,results.json}`.

## Baseline (MacBook-class dev machine, SF 0.01, Rust release)

First full run — and the bugs it caught (all fixed):

| Area | Result |
|---|---|
| Load | 86,630 rows generated + flushed in ~55 ms |
| TPC-H-ish queries | Q1–Q14 subset: 1.8–7.9 ms warm, joins included |
| Storage | lineitem **8.5×**, partsupp 5.6×, part 5.5× (zstd) |
| Memory | 42 MB RSS after load, 80 MB after queries |
| Interactive | point lookups p99 1.07 ms; range aggregates p99 1.08 ms |
| Vector (50k×64, IVF-PQ) | recall@10 **0.87**, ~8.7k QPS sequential, 0.11 ms/search |
| Graph (10k nodes/20k edges) | neighbors sub-ms; shortest path p50 1.2 ms |
| MCP agent session | tool calls 0.1–9.5 ms after process start |

Bugs found & fixed by this suite:

1. **two-node shared-storage check failed** — the harness created the test
   table as local `Firefly` instead of `FireflyCloud` (harness bug);
2. **graph bulk-load was O(n²)** (265 s for 10k nodes) — context store
   wrote a full JSON snapshot after *every* mutation; now dirty-flagged,
   flushed on demand/Drop (25 s, dominated by snapshot serialization);
3. **IVF-PQ recall@10 = 0.43** — ADC ordering is approximate, but only
   the ADC top-k was exact-re-ranked; now `k × refine_factor` candidates
   are re-ranked (LanceDB-style), recall 0.87 at default settings.

Open findings:

* MCP `initialize` ≈ 2.3 s — engine + DataFusion session construction
  dominates agent startup (paid once per process; lazy-init on roadmap);
* graph snapshot serialization is JSON-pretty — switch to compact/binary
  for large graphs.

## Phases

| # | Phase | What it measures |
|---|-------|------------------|
| 1 | Data generation | TPC-H-inspired generator (dbgen-lite): cardinalities 6M·SF lineitems etc., deterministic xorshift, ISO date strings |
| 2 | Load + storage | insert throughput, flush, per-table stored bytes vs raw estimate → **compression ratio**, part counts |
| 3 | TPC-H-ish queries | Q1, Q3, Q5, Q6, Q10, Q14 (scan, join, aggregate mix), cold + warm (best-of-2) latency, rows returned |
| 4 | Resource | RSS after load/queries/final, CPU usage |
| 5 | Metadata resiliency | restart (rows intact), two-node shared storage (compute/storage separation), atomic writes (no `.tmp` leftovers), corrupt `meta.json` → clean error, no crash |
| 6 | Vector search | IVF-PQ build/train time, recall@10 vs exact flat, sequential QPS |
| 7 | Graph | 10k nodes/20k edges: BFS neighbors (p50/p99), shortest path (p50/p99) |
| 8 | Interactive | dashboard-style: 300 pk point lookups (bloom-pruned) + 150 range aggregates, latency percentiles |
| 9 | MCP agent | spawns real `less mcp` over stdio; times 14 tool calls (initialize, SQL, context, vector, memory) |

## Reading the results

* **Storage**: `ratio` = raw bytes ÷ stored bytes per table (zstd).
* **Queries**: compare `warm_ms` across runs; cold = plan + first I/O.
* **Resiliency**: all four checks must be ✅ before any scale-up.
* **Vector**: `recall10` should be ≥ 0.85 for the default IVF-PQ config;
  `qps` is sequential single-thread throughput (floor, not ceiling).
* **MCP**: per-call latencies include stdio round-trip — what an agent
  actually experiences.

## Scaling up (proper server)

1. Bump `--sf` (0.1 → 1 → 10) and watch load/queries/resiliency.
2. Add concurrent workloads (parallel query streams, concurrent agents)
   — the harness is sequential today by design (deterministic baselines
   first, contention second).
3. Add the full 22-query TPC-H set with the official dbgen data + compare
   against DuckDB on the same box.
4. Re-run the vector phase with 10M+ vectors; add concurrent search QPS
   and build-time scaling curves.
5. Track results.json in git per hardware profile; diff `latest.md`.
