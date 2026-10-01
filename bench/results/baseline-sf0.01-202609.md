# LessDB Benchmark Report

- version: `0.1.0`
- scale factor (TPC-H-ish): **0.01**
- generated rows: 86630 (8 ms)
- load + flush: 55 ms
- RSS: 42 MB after load, 80 MB after queries, 170 MB final

## Storage footprint (compression)

| table | rows | parts | stored | raw est. | ratio |
|---|---|---|---|---|---|
| nation | 25 | 1 | 0.00 MB | 0.00 MB | 0.4x |
| region | 5 | 1 | 0.00 MB | 0.00 MB | 0.1x |
| part | 2000 | 1 | 0.02 MB | 0.11 MB | 5.5x |
| supplier | 100 | 1 | 0.00 MB | 0.00 MB | 1.6x |
| partsupp | 8000 | 1 | 0.05 MB | 0.26 MB | 5.6x |
| customer | 1500 | 1 | 0.02 MB | 0.08 MB | 3.8x |
| orders | 15000 | 1 | 0.20 MB | 0.76 MB | 3.8x |
| lineitem | 60000 | 1 | 0.57 MB | 4.82 MB | 8.5x |

## TPC-H-inspired queries (warm = best of 2)

| query | rows | cold ms | warm ms |
|---|---|---|---|
| Q1 pricing summary | 6 | 3.2 | 2.9 |
| Q3 shipping priority | 10 | 6.1 | 5.9 |
| Q5 local supplier volume | 5 | 8.6 | 7.9 |
| Q6 forecast revenue | 1 | 1.8 | 1.8 |
| Q10 returned items | 20 | 6.5 | 6.1 |
| Q14 promotion effect | 1 | 2.2 | 2.1 |

## Metadata resiliency

- restart (reopen, rows intact, 1 parts): ✅ OK
- two-node shared storage (node B sees node A's data): ✅ OK
- atomic writes (no `.tmp` leftovers): ✅ OK
- corrupt part metadata surfaces an error, no crash: ✅ OK

## Vector search (IVF-PQ)

50000 vectors, dim 64

| add ms | train ms | recall@10 | search avg | QPS |
|---|---|---|---|---|
| 13 | 25963 | 0.87 | 0.11 ms | 8727 |

## Graph (context) queries

| nodes | edges | build ms | neighbors p50 | neighbors p99 | path p50 | path p99 |
|---|---|---|---|---|---|---|
| 10000 | 19999 | 25153 | 0.00 ms | 0.00 ms | 1.22 ms | 3.97 ms |

## Interactive / dashboard queries

| workload | iters | mean | p50 | p95 | p99 |
|---|---|---|---|---|---|
| point lookup (orders pk, bloom) | 300 | 0.82 ms | 0.81 ms | 0.93 ms | 1.07 ms |
| dashboard aggregate (range on shipdate) | 150 | 0.78 ms | 0.76 ms | 0.87 ms | 1.08 ms |

## MCP agent session (stdio JSON-RPC, per call)

| tool | ms | status |
|---|---|---|
| initialize | 2277.7 | ✅ OK |
| tools/call less_tables | 0.1 | ✅ OK |
| tools/call less_query agg | 9.5 | ✅ OK |
| tools/call less_query point | 1.9 | ✅ OK |
| tools/call context_put | 0.3 | ✅ OK |
| tools/call context_find | 0.0 | ✅ OK |
| tools/call context_link | 0.2 | ✅ OK |
| tools/call context_neighbors | 0.0 | ✅ OK |
| tools/call vector_create | 0.4 | ✅ OK |
| tools/call vector_put | 0.4 | ✅ OK |
| tools/call vector_search | 0.0 | ✅ OK |
| tools/call memory_create | 0.4 | ✅ OK |
| tools/call memory_insert | 0.6 | ✅ OK |
| tools/call memory_sql | 0.6 | ✅ OK |
