//! LessDB benchmark suite (sandboxed, small-scale).
//!
//! Phases:
//! 1. TPC-H-inspired load (SF-controlled) + storage footprint
//! 2. TPC-H-inspired queries Q1/Q3/Q5/Q6/Q10/Q14 (warm + measured)
//! 3. Resource utilization (RSS, CPU), compression ratios
//! 4. Metadata resiliency: restart, two-node shared storage, atomic
//!    writes, corrupt-metadata error handling
//! 5. Vector search: IVF-PQ build, recall, QPS
//! 6. Graph: traversal + shortest-path latency
//! 7. Interactive/dashboard queries: latency percentiles
//! 8. MCP agent session: per-tool latencies over stdio
//!
//! Safety: tiny default scale factor (SF 0.01 ≈ 60k lineitems), an RSS
//! guard, and everything runs against throwaway temp directories.
#![allow(clippy::type_complexity)]

mod report;
mod tpchgen;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use arrow::array::{Float64Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use serde::Serialize;

use less_catalog::{EngineKind, FieldSpec, SchemaSpec, TableDef, TypeSpec};
use less_common::{EngineConfig, LessError, Result};
use less_engine::LessEngine;
use less_query::LessSession;

use report::Report;
use tpchgen::TpchData;

fn rss_bytes() -> u64 {
    use sysinfo::{Pid, ProcessesToUpdate, System};
    let mut system = System::new();
    let pid = Pid::from_u32(std::process::id());
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).map(|p| p.memory()).unwrap_or(0)
}

fn cpu_usage_pct() -> f32 {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let mut system = System::new();
    let pid = Pid::from_u32(std::process::id());
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_cpu(),
    );
    system.process(pid).map(|p| p.cpu_usage()).unwrap_or(0.0)
}

fn elapsed_ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx]
}

fn table_def(name: &str, fields: &[(&str, TypeSpec)], sort: &[&str], unique: &[&str]) -> TableDef {
    table_def_with(name, fields, sort, unique, EngineKind::Firefly)
}

fn table_def_with(
    name: &str,
    fields: &[(&str, TypeSpec)],
    sort: &[&str],
    unique: &[&str],
    engine: EngineKind,
) -> TableDef {
    let mut def = TableDef::new(
        name,
        SchemaSpec {
            fields: fields
                .iter()
                .map(|(n, t)| FieldSpec::new((*n).to_string(), t.clone()))
                .collect(),
        },
        engine,
    );
    def.sort_key = sort.iter().map(|s| s.to_string()).collect();
    def.unique = unique.iter().map(|s| s.to_string()).collect();
    def
}

const SCHEMAS: &[(&str, &[(&str, TypeSpec)], &[&str], &[&str])] = &[
    (
        "nation",
        &[
            ("n_nationkey", TypeSpec::Int64),
            ("n_name", TypeSpec::Utf8),
            ("n_regionkey", TypeSpec::Int64),
        ],
        &["n_nationkey"],
        &["n_nationkey"],
    ),
    (
        "region",
        &[("r_regionkey", TypeSpec::Int64), ("r_name", TypeSpec::Utf8)],
        &["r_regionkey"],
        &["r_regionkey"],
    ),
    (
        "part",
        &[
            ("p_partkey", TypeSpec::Int64),
            ("p_name", TypeSpec::Utf8),
            ("p_type", TypeSpec::Utf8),
            ("p_size", TypeSpec::Int64),
            ("p_retailprice", TypeSpec::Float64),
        ],
        &["p_partkey"],
        &["p_partkey"],
    ),
    (
        "supplier",
        &[
            ("s_suppkey", TypeSpec::Int64),
            ("s_name", TypeSpec::Utf8),
            ("s_nationkey", TypeSpec::Int64),
            ("s_acctbal", TypeSpec::Float64),
        ],
        &["s_suppkey"],
        &["s_suppkey"],
    ),
    (
        "partsupp",
        &[
            ("ps_partkey", TypeSpec::Int64),
            ("ps_suppkey", TypeSpec::Int64),
            ("ps_availqty", TypeSpec::Int64),
            ("ps_supplycost", TypeSpec::Float64),
        ],
        &["ps_partkey", "ps_suppkey"],
        &[],
    ),
    (
        "customer",
        &[
            ("c_custkey", TypeSpec::Int64),
            ("c_name", TypeSpec::Utf8),
            ("c_nationkey", TypeSpec::Int64),
            ("c_acctbal", TypeSpec::Float64),
            ("c_mktsegment", TypeSpec::Utf8),
        ],
        &["c_mktsegment", "c_custkey"],
        &[],
    ),
    (
        "orders",
        &[
            ("o_orderkey", TypeSpec::Int64),
            ("o_custkey", TypeSpec::Int64),
            ("o_orderdate", TypeSpec::Utf8),
            ("o_orderpriority", TypeSpec::Utf8),
            ("o_shippriority", TypeSpec::Int64),
            ("o_totalprice", TypeSpec::Float64),
        ],
        &["o_orderkey"],
        &["o_orderkey"],
    ),
    (
        "lineitem",
        &[
            ("l_orderkey", TypeSpec::Int64),
            ("l_partkey", TypeSpec::Int64),
            ("l_suppkey", TypeSpec::Int64),
            ("l_linenumber", TypeSpec::Int64),
            ("l_quantity", TypeSpec::Float64),
            ("l_extendedprice", TypeSpec::Float64),
            ("l_discount", TypeSpec::Float64),
            ("l_tax", TypeSpec::Float64),
            ("l_shipdate", TypeSpec::Utf8),
            ("l_returnflag", TypeSpec::Utf8),
            ("l_linestatus", TypeSpec::Utf8),
            ("l_shipmode", TypeSpec::Utf8),
        ],
        &["l_shipdate", "l_orderkey"],
        &[],
    ),
];

const QUERIES: &[(&str, &str)] = &[
    (
        "Q1 pricing summary",
        "SELECT l_returnflag, l_linestatus, sum(l_quantity) AS sum_qty, \
         sum(l_extendedprice) AS sum_base_price, \
         sum(l_extendedprice * (1 - l_discount)) AS sum_disc_price, \
         sum(l_extendedprice * (1 - l_discount) * (1 + l_tax)) AS sum_charge, \
         avg(l_quantity) AS avg_qty, count(*) AS count_order \
         FROM lineitem WHERE l_shipdate <= '1998-09-02' \
         GROUP BY l_returnflag, l_linestatus ORDER BY l_returnflag, l_linestatus",
    ),
    (
        "Q3 shipping priority",
        "SELECT l_orderkey, sum(l_extendedprice * (1 - l_discount)) AS revenue, \
         o_orderdate, o_shippriority \
         FROM customer JOIN orders ON c_custkey = o_custkey \
         JOIN lineitem ON l_orderkey = o_orderkey \
         WHERE c_mktsegment = 'BUILDING' AND o_orderdate < '1995-03-15' \
         AND l_shipdate > '1995-03-15' \
         GROUP BY l_orderkey, o_orderdate, o_shippriority \
         ORDER BY revenue DESC LIMIT 10",
    ),
    (
        "Q5 local supplier volume",
        "SELECT n_name, sum(l_extendedprice * (1 - l_discount)) AS revenue \
         FROM customer JOIN orders ON c_custkey = o_custkey \
         JOIN lineitem ON l_orderkey = o_orderkey \
         JOIN supplier ON l_suppkey = s_suppkey AND c_nationkey = s_nationkey \
         JOIN nation ON s_nationkey = n_nationkey \
         JOIN region ON n_regionkey = r_regionkey \
         WHERE r_name = 'ASIA' AND o_orderdate >= '1994-01-01' AND o_orderdate < '1995-01-01' \
         GROUP BY n_name ORDER BY revenue DESC",
    ),
    (
        "Q6 forecast revenue",
        "SELECT sum(l_extendedprice * l_discount) AS revenue FROM lineitem \
         WHERE l_shipdate >= '1994-01-01' AND l_shipdate < '1995-01-01' \
         AND l_discount BETWEEN 0.05 AND 0.07 AND l_quantity < 24",
    ),
    (
        "Q10 returned items",
        "SELECT c_name, sum(l_extendedprice * (1 - l_discount)) AS revenue, \
         c_acctbal, n_name \
         FROM customer JOIN orders ON c_custkey = o_custkey \
         JOIN lineitem ON l_orderkey = o_orderkey \
         JOIN nation ON c_nationkey = n_nationkey \
         WHERE o_orderdate >= '1993-10-01' AND o_orderdate < '1994-01-01' \
         AND l_returnflag = 'R' \
         GROUP BY c_name, c_acctbal, n_name ORDER BY revenue DESC LIMIT 20",
    ),
    (
        "Q14 promotion effect",
        "SELECT 100.00 * sum(CASE WHEN p_type LIKE 'PROMO%' THEN l_extendedprice * (1 - l_discount) ELSE 0 END) \
         / sum(l_extendedprice * (1 - l_discount)) AS promo_revenue \
         FROM lineitem JOIN part ON l_partkey = p_partkey \
         WHERE l_shipdate >= '1995-09-01' AND l_shipdate < '1995-10-01'",
    ),
];

#[derive(Serialize, Clone)]
struct QueryResult {
    query: String,
    rows: usize,
    cold_ms: f64,
    warm_ms: f64,
}

#[derive(Serialize, Clone)]
struct LatencyStats {
    name: String,
    mean_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    iters: usize,
}

fn main() -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .thread_name("less-bench")
        .enable_all()
        .build()
        .map_err(|e| LessError::Engine(e.to_string()))?;
    rt.block_on(run())
}

async fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let sf: f64 = args
        .iter()
        .position(|a| a == "--sf")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.01);
    let keep_dir = args.iter().any(|a| a == "--keep");
    let skip_mcp = args.iter().any(|a| a == "--skip-mcp");
    println!("=== LessDB benchmark (SF {sf}) ===");
    let root = std::env::temp_dir().join(format!("less-bench-{}", std::process::id()));
    let dir = root.join("db");
    std::fs::create_dir_all(&dir)?;

    let mut report = Report::new(sf);

    // ------------------------------------------------------------- phase 1-2
    let load_start = Instant::now();
    let tpch: TpchData = tpchgen::generate(sf);
    report.gen_ms = elapsed_ms(load_start);
    report.gen_rows = tpch.tables.iter().map(|t| t.batches[0].num_rows()).sum();
    println!(
        "generated {} rows in {:.1} ms",
        report.gen_rows, report.gen_ms
    );

    let load_result: Result<(Arc<LessEngine>, Vec<(String, u64, u64, usize, usize)>)> = {
        let dir = dir.clone();
        let tpch_tables: Vec<(String, Arc<Schema>, Vec<RecordBatch>, u64)> = tpch
            .tables
            .iter()
            .map(|t| {
                (
                    t.name.clone(),
                    t.schema.clone(),
                    t.batches.clone(),
                    t.raw_bytes,
                )
            })
            .collect();
        tokio::task::spawn_blocking(move || load_phase(&dir, tpch_tables))
            .await
            .map_err(|e| LessError::Engine(e.to_string()))?
    };
    let (engine, footprint) = load_result?;
    report.load_ms = elapsed_ms(load_start);
    report.rss_after_load_mb = rss_bytes() as f64 / 1e6;
    report.footprint = footprint
        .iter()
        .map(|(name, stored, raw, rows, parts)| report::Footprint {
            table: name.clone(),
            rows: *rows,
            parts: *parts,
            stored_bytes: *stored,
            raw_bytes: *raw,
            ratio: if *stored > 0 {
                *raw as f64 / *stored as f64
            } else {
                0.0
            },
        })
        .collect();
    println!(
        "load + flush: {:.0} ms, rss {:.0} MB",
        report.load_ms, report.rss_after_load_mb
    );
    for f in &report.footprint {
        println!(
            "  {}: {} rows, {:.2} MB stored ({:.1}x vs raw), {} parts",
            f.table,
            f.rows,
            f.stored_bytes as f64 / 1e6,
            f.ratio,
            f.parts
        );
    }

    // ------------------------------------------------------------- phase 3
    let session = LessSession::new_async(engine.clone()).await?;
    let cpu_before = cpu_usage_pct();
    for (name, sql) in QUERIES {
        let rows = session
            .sql_batches(sql)
            .await?
            .iter()
            .map(|b| b.num_rows())
            .sum();
        let cold = {
            let t = Instant::now();
            let _ = session.sql_batches(sql).await?;
            elapsed_ms(t)
        };
        // warm: best of two
        let mut warm = f64::MAX;
        for _ in 0..2 {
            let t = Instant::now();
            let _ = session.sql_batches(sql).await?;
            warm = warm.min(elapsed_ms(t));
        }
        println!("  {name}: rows={rows} cold={cold:.1}ms warm={warm:.1}ms");
        report.queries.push(QueryResult {
            query: name.to_string(),
            rows,
            cold_ms: cold,
            warm_ms: warm,
        });
    }
    report.rss_after_queries_mb = rss_bytes() as f64 / 1e6;
    report.cpu_pct = cpu_usage_pct().max(cpu_before);

    // ------------------------------------------------------------- phase 4
    println!("--- metadata resiliency ---");
    let lineitem_rows = session
        .sql_batches("SELECT count(*) FROM lineitem")
        .await?
        .first()
        .and_then(|b| {
            b.column(0)
                .as_any()
                .downcast_ref::<Int64Array>()
                .map(|a| a.value(0))
        })
        .unwrap_or(-1);

    // restart: drop session+engine, reopen, count again.
    drop(session);
    drop(engine);
    let restart = {
        let dir = dir.clone();
        tokio::task::spawn_blocking(move || -> Result<(usize, u64)> {
            let engine = LessEngine::open(EngineConfig::with_data_dir(&dir))?;
            let parts = engine.parts("lineitem")?.len();
            let stats = engine.stats("lineitem")?;
            Ok((parts, stats.rows))
        })
        .await
        .map_err(|e| LessError::Engine(e.to_string()))?
    };
    report.resiliency.restart_ok = restart
        .as_ref()
        .map(|r| r.1 as i64 == lineitem_rows)
        .unwrap_or(false);
    report.resiliency.restart_parts = restart.as_ref().map(|r| r.0).unwrap_or(0);
    println!(
        "  restart: {} ({} parts, rows match: {})",
        if report.resiliency.restart_ok {
            "OK"
        } else {
            "FAIL"
        },
        report.resiliency.restart_parts,
        report.resiliency.restart_ok
    );

    // two-node shared storage
    let shared_root = root.join("shared");
    let node_a = root.join("node-a");
    let node_b = root.join("node-b");
    let shared_url = format!("file://{}", shared_root.display());
    let two_node = tokio::task::spawn_blocking(move || -> Result<usize> {
        let a = LessEngine::open(EngineConfig::with_shared_url(&node_a, &shared_url))?;
        a.create_table(table_def_with(
            "shared_t",
            &[("id", TypeSpec::Int64), ("v", TypeSpec::Float64)],
            &["id"],
            &["id"],
            EngineKind::FireflyCloud,
        ))?;
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("v", DataType::Float64, false),
        ]));
        for chunk in (0..5000i64).collect::<Vec<_>>().chunks(1000) {
            let batch = RecordBatch::try_new(
                schema.clone(),
                vec![
                    Arc::new(Int64Array::from(chunk.to_vec())),
                    Arc::new(arrow::array::Float64Array::from(
                        chunk.iter().map(|i| *i as f64 * 1.5).collect::<Vec<_>>(),
                    )),
                ],
            )?;
            a.insert("shared_t", batch)?;
        }
        a.flush("shared_t")?;
        let b = LessEngine::open(EngineConfig::with_shared_url(&node_b, &shared_url))?;
        let n = b.stats("shared_t")?.rows as usize;
        b.drop_table("shared_t")?;
        Ok(n)
    })
    .await
    .map_err(|e| LessError::Engine(e.to_string()))?;
    report.resiliency.two_node_ok = matches!(&two_node, Ok(5000));
    println!(
        "  two-node shared storage: {} (node B saw {} rows)",
        if report.resiliency.two_node_ok {
            "OK"
        } else {
            "FAIL"
        },
        two_node
            .as_ref()
            .map(|n| n.to_string())
            .unwrap_or_else(|e| e.to_string())
    );
    if let Err(e) = &two_node {
        eprintln!("    two-node error: {e}");
    }

    // atomic writes: no *.tmp leftovers anywhere in the data dir
    let tmp_leftovers = {
        let dir = dir.clone();
        tokio::task::spawn_blocking(move || -> Result<usize> {
            let mut n = 0;
            for entry in walkdir(&dir)? {
                if entry.ends_with(".tmp") {
                    n += 1;
                }
            }
            Ok(n)
        })
        .await
        .map_err(|e| LessError::Engine(e.to_string()))??
    };
    report.resiliency.atomic_writes_ok = tmp_leftovers == 0;
    println!(
        "  atomic writes: {} ({tmp_leftovers} .tmp leftovers)",
        if report.resiliency.atomic_writes_ok {
            "OK"
        } else {
            "FAIL"
        }
    );

    // corrupt metadata: engine must return an error, not crash
    let corrupt = {
        let dir = dir.clone();
        tokio::task::spawn_blocking(move || -> Result<String> {
            let engine = LessEngine::open(EngineConfig::with_data_dir(&dir))?;
            let part_dirs = engine.parts("nation")?;
            let Some(part) = part_dirs.first() else {
                return Ok("no parts".to_string());
            };
            if let less_engine::PartLocation::Local(d) = &part.location {
                std::fs::write(d.join("meta.json"), b"{corrupt")?;
            }
            match engine.parts("nation") {
                Err(e) => Ok(e.to_string()),
                Ok(_) => Ok("no error surfaced".to_string()),
            }
        })
        .await
        .map_err(|e| LessError::Engine(e.to_string()))??
    };
    report.resiliency.corrupt_meta_ok = corrupt.contains("json");
    println!(
        "  corrupt meta.json: {} ({})",
        if report.resiliency.corrupt_meta_ok {
            "OK"
        } else {
            "FAIL"
        },
        corrupt.chars().take(60).collect::<String>()
    );
    // remove the corrupted part dir (nation has one part) so later phases
    // don't trip on it; restore by dropping nation table entirely.
    {
        let dir = dir.clone();
        let _ = tokio::task::spawn_blocking(move || -> Result<()> {
            let engine = LessEngine::open(EngineConfig::with_data_dir(&dir))?;
            engine.drop_table("nation")?;
            Ok(())
        })
        .await;
    }

    // ------------------------------------------------------------- phases 5-7
    let (vector, graph) = {
        let dir = dir.clone();
        tokio::task::spawn_blocking(
            move || -> Result<(Vec<report::VectorStat>, Vec<report::GraphStat>)> {
                let _engine = LessEngine::open(EngineConfig::with_data_dir(&dir))?;
                // vectors
                let mut stats = vec![];
                let nvec = 50_000;
                let dim = 64;
                let mut rng = tpchgen::Rng::new(0xBEEF);
                let mut data: Vec<Vec<f32>> = Vec::with_capacity(nvec);
                for c in 0..16usize {
                    for _ in 0..(nvec / 16) {
                        let mut v = vec![0.0f32; dim];
                        let dir = (c * 4) % dim;
                        v[dir] = 10.0 * (c as f32 + 1.0);
                        for x in v.iter_mut() {
                            *x += (rng.f() as f32 - 0.5) * 2.0;
                        }
                        data.push(v);
                    }
                }
                let mut reg = less_vector::VectorRegistry::open(Some(&dir.join("vectors")))?;
                let params = less_vector::IvfPqParams {
                    nlist: 64,
                    m: 8,
                    nbits: 8,
                    niter: 10,
                    refine_factor: 8,
                };
                reg.create_space(
                    "bench_vecs",
                    dim,
                    less_vector::Metric::L2,
                    less_vector::IndexKind::IvfPq(params),
                )?;
                let t = Instant::now();
                reg.add("bench_vecs", data.clone(), vec![])?;
                let add_ms = elapsed_ms(t);
                let t = Instant::now();
                reg.train("bench_vecs")?;
                let train_ms = elapsed_ms(t);

                // flat reference for recall
                let mut flat = less_vector::FlatIndex::new(dim, less_vector::Metric::L2);
                let mut flat_data = vec![];
                for v in &data {
                    flat_data.extend_from_slice(v);
                }
                flat.add(&flat_data);

                let mut recall_hits = 0usize;
                let mut qps_data = vec![];
                let _t = Instant::now();
                let mut rng = tpchgen::Rng::new(0xFEED);
                let mut queries = vec![];
                for _ in 0..100 {
                    let base = (rng.next_u64() as usize % nvec) * dim;
                    let mut q = flat_data[base..base + dim].to_vec();
                    for x in q.iter_mut() {
                        *x += (rng.f() as f32 - 0.5) * 2.0;
                    }
                    queries.push(q);
                }
                for q in &queries {
                    let exact: std::collections::HashSet<u32> =
                        flat.search(q, 10).into_iter().map(|(id, _)| id).collect();
                    let approx: std::collections::HashSet<u32> = reg
                        .search("bench_vecs", q.clone(), 10, 8)?
                        .into_iter()
                        .map(|h| h.id)
                        .collect();
                    recall_hits += exact.intersection(&approx).count();
                }
                let recall = recall_hits as f64 / (100.0 * 10.0);
                // QPS: 200 sequential searches
                let qps_t = Instant::now();
                for q in queries.iter().cycle().take(200) {
                    let _ = reg.search("bench_vecs", q.clone(), 10, 8)?;
                }
                let qps_ms = elapsed_ms(qps_t);
                qps_data.push(200.0 / (qps_ms / 1000.0));
                stats.push(report::VectorStat {
                    vectors: nvec,
                    dim,
                    add_ms,
                    train_ms,
                    recall10: recall,
                    qps: qps_data[0],
                    search_ms_avg: qps_ms / 200.0,
                });
                reg.drop_space("bench_vecs")?;

                // graph
                let mut graph_stats = vec![];
                let mut ctx = less_graph::ContextStore::open(Some(&dir.join("memory")))?;
                let t = Instant::now();
                let nnodes = 10_000usize;
                for i in 0..nnodes {
                    ctx.put(
                        &format!("n{i}"),
                        &format!("Node {i}"),
                        "body text",
                        vec![],
                        "note",
                        Default::default(),
                    )?;
                }
                for i in 0..(nnodes - 1) {
                    ctx.link(
                        &format!("n{i}"),
                        &format!("n{}", i + 1),
                        "next",
                        true,
                        Default::default(),
                    )?;
                }
                let mut rng = tpchgen::Rng::new(0xC0FFEE);
                for _ in 0..nnodes {
                    let a = rng.next_u64() as usize % nnodes;
                    let b = rng.next_u64() as usize % nnodes;
                    if a != b {
                        ctx.link(
                            &format!("n{a}"),
                            &format!("n{b}"),
                            "ref",
                            false,
                            Default::default(),
                        )?;
                    }
                }
                let build_ms = elapsed_ms(t);
                // traversal
                let mut times = vec![];
                let mut rng = tpchgen::Rng::new(0xF00D);
                for _ in 0..100 {
                    let start = format!("n{}", rng.next_u64() as usize % nnodes);
                    let t = Instant::now();
                    let _ = ctx.neighbors(&start, less_graph::Direction::Out, 2)?;
                    times.push(elapsed_ms(t));
                }
                let mut sorted = times.clone();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
                graph_stats.push(report::GraphStat {
                    nodes: nnodes,
                    edges: 2 * nnodes - 1,
                    build_ms,
                    neighbors_p50_ms: percentile(&sorted, 50.0),
                    neighbors_p99_ms: percentile(&sorted, 99.0),
                    path_p50_ms: 0.0,
                    path_p99_ms: 0.0,
                });
                // shortest path
                let mut times = vec![];
                let mut rng = tpchgen::Rng::new(0x5A11);
                for _ in 0..50 {
                    let a = format!("n{}", rng.next_u64() as usize % nnodes);
                    let b = format!("n{}", rng.next_u64() as usize % nnodes);
                    let t = Instant::now();
                    let _ = ctx.path(&a, &b);
                    times.push(elapsed_ms(t));
                }
                let mut sorted = times.clone();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
                graph_stats[0].path_p50_ms = percentile(&sorted, 50.0);
                graph_stats[0].path_p99_ms = percentile(&sorted, 99.0);

                Ok((stats, graph_stats))
            },
        )
        .await
        .map_err(|e| LessError::Engine(e.to_string()))??
    };
    report.vectors = vector;
    report.graph = graph;

    // interactive queries (async, via a fresh session)
    let engine2 = {
        let dir = dir.clone();
        tokio::task::spawn_blocking(move || LessEngine::open(EngineConfig::with_data_dir(&dir)))
            .await
            .map_err(|e| LessError::Engine(e.to_string()))??
    };
    let session2 = LessSession::new_async(engine2.clone()).await?;
    let orders_max: i64 = session2
        .sql_batches("SELECT max(o_orderkey) FROM orders")
        .await?
        .first()
        .and_then(|b| {
            b.column(0)
                .as_any()
                .downcast_ref::<Int64Array>()
                .map(|a| a.value(0))
        })
        .unwrap_or(1500);
    let mut rng = tpchgen::Rng::new(0xDA5A);
    let mut point_times = vec![];
    for _ in 0..300 {
        let key = (rng.next_u64() % orders_max as u64) as i64 + 1;
        let t = Instant::now();
        let _ = session2
            .sql_batches(&format!(
                "SELECT o_orderdate, o_totalprice FROM orders WHERE o_orderkey = {key}"
            ))
            .await;
        point_times.push(elapsed_ms(t));
    }
    let mut sorted = point_times.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    report.dashboard.push(LatencyStats {
        name: "point lookup (orders pk, bloom)".into(),
        mean_ms: sorted.iter().sum::<f64>() / sorted.len() as f64,
        p50_ms: percentile(&sorted, 50.0),
        p95_ms: percentile(&sorted, 95.0),
        p99_ms: percentile(&sorted, 99.0),
        iters: sorted.len(),
    });
    let mut agg_times = vec![];
    for _ in 0..150 {
        let t = Instant::now();
        let _ = session2
            .sql_batches(
                "SELECT count(*), sum(l_quantity) FROM lineitem WHERE l_shipdate >= '1997-01-01'",
            )
            .await;
        agg_times.push(elapsed_ms(t));
    }
    let mut sorted = agg_times.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    report.dashboard.push(LatencyStats {
        name: "dashboard aggregate (range on shipdate)".into(),
        mean_ms: sorted.iter().sum::<f64>() / sorted.len() as f64,
        p50_ms: percentile(&sorted, 50.0),
        p95_ms: percentile(&sorted, 95.0),
        p99_ms: percentile(&sorted, 99.0),
        iters: sorted.len(),
    });

    // ------------------------------------------------------------- phase 8
    if !skip_mcp {
        println!("--- MCP agent session ---");
        let bin = std::env::var("LESS_BIN").unwrap_or_else(|_| {
            let repo = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            repo.join("target/release/less").display().to_string()
        });
        if !Path::new(&bin).exists() {
            println!("  skip: binary '{bin}' not found (build release first, or set LESS_BIN)");
        } else {
            let mcp_dir = root.join("mcp-db");
            std::fs::create_dir_all(&mcp_dir)?;
            // seed a table + context for the agent
            {
                let mcp_dir = mcp_dir.clone();
                tokio::task::spawn_blocking(move || -> Result<()> {
                    let engine = LessEngine::open(EngineConfig::with_data_dir(&mcp_dir))?;
                    engine.create_table(table_def(
                        "mcp_events",
                        &[
                            ("id", TypeSpec::Int64),
                            ("kind", TypeSpec::Utf8),
                            ("amount", TypeSpec::Float64),
                        ],
                        &["kind", "id"],
                        &["kind"],
                    ))?;
                    let schema = Arc::new(Schema::new(vec![
                        Field::new("id", DataType::Int64, false),
                        Field::new("kind", DataType::Utf8, false),
                        Field::new("amount", DataType::Float64, false),
                    ]));
                    for chunk in (0..10_000i64).collect::<Vec<_>>().chunks(2000) {
                        let batch = RecordBatch::try_new(
                            schema.clone(),
                            vec![
                                Arc::new(Int64Array::from(chunk.to_vec())),
                                Arc::new(StringArray::from(
                                    chunk
                                        .iter()
                                        .map(|i| if i % 2 == 0 { "click" } else { "view" })
                                        .collect::<Vec<_>>(),
                                )),
                                Arc::new(Float64Array::from(
                                    chunk.iter().map(|i| *i as f64 * 0.5).collect::<Vec<_>>(),
                                )),
                            ],
                        )?;
                        engine.insert("mcp_events", batch)?;
                    }
                    engine.flush("mcp_events")?;
                    Ok(())
                })
                .await
                .map_err(|e| LessError::Engine(e.to_string()))??;
            }
            let mcp = tokio::process::Command::new(&bin)
                .args(["mcp", "--dir", mcp_dir.to_str().unwrap()])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| LessError::Engine(format!("failed to spawn {bin}: {e}")))?;
            let mut child = mcp;
            let stdin = child.stdin.take().unwrap();
            let stdout = child.stdout.take().unwrap();
            let (tx, rx) = tokio::sync::mpsc::channel::<String>(64);
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if tx.send(line).await.is_err() {
                        break;
                    }
                }
            });
            let stdin = Arc::new(tokio::sync::Mutex::new(tokio::io::BufWriter::new(stdin)));
            struct McpClient {
                stdin: Arc<tokio::sync::Mutex<tokio::io::BufWriter<tokio::process::ChildStdin>>>,
                rx: tokio::sync::mpsc::Receiver<String>,
            }
            impl McpClient {
                async fn call(
                    &mut self,
                    id: u64,
                    method: &str,
                    params: serde_json::Value,
                ) -> Result<(f64, String)> {
                    use tokio::io::AsyncWriteExt;
                    let line = serde_json::json!({
                        "jsonrpc": "2.0", "id": id, "method": method, "params": params
                    })
                    .to_string();
                    let t = Instant::now();
                    {
                        let mut w = self.stdin.lock().await;
                        w.write_all(line.as_bytes()).await?;
                        w.write_all(b"\n").await?;
                        w.flush().await?;
                    }
                    let resp = self
                        .rx
                        .recv()
                        .await
                        .ok_or_else(|| LessError::Engine("mcp closed".into()))?;
                    Ok((elapsed_ms(t), resp))
                }
            }
            let mut mcp_client = McpClient { stdin, rx };
            let tool = |name: &str, args: serde_json::Value| serde_json::json!({"name": name, "arguments": args});
            let mcp_tools: Vec<(&str, serde_json::Value)> = vec![
                (
                    "initialize",
                    serde_json::json!({"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "bench", "version": "0"}}),
                ),
                (
                    "tools/call less_tables",
                    tool("less_tables", serde_json::json!({})),
                ),
                (
                    "tools/call less_query agg",
                    tool(
                        "less_query",
                        serde_json::json!({"sql": "SELECT kind, count(*), sum(amount) FROM mcp_events GROUP BY kind"}),
                    ),
                ),
                (
                    "tools/call less_query point",
                    tool(
                        "less_query",
                        serde_json::json!({"sql": "SELECT * FROM mcp_events WHERE kind = 'click' LIMIT 3"}),
                    ),
                ),
                (
                    "tools/call context_put",
                    tool(
                        "context_put",
                        serde_json::json!({"key": "bench/memo", "title": "memo", "text": "benchmark note"}),
                    ),
                ),
                (
                    "tools/call context_find",
                    tool("context_find", serde_json::json!({"query": "benchmark"})),
                ),
                (
                    "tools/call context_link",
                    tool(
                        "context_link",
                        serde_json::json!({"from": "bench/memo", "to": "bench/other", "kind": "ref"}),
                    ),
                ),
                (
                    "tools/call context_neighbors",
                    tool(
                        "context_neighbors",
                        serde_json::json!({"key": "bench/memo"}),
                    ),
                ),
                (
                    "tools/call vector_create",
                    tool(
                        "vector_create",
                        serde_json::json!({"space": "mcp_vecs", "dim": 4}),
                    ),
                ),
                (
                    "tools/call vector_put",
                    tool(
                        "vector_put",
                        serde_json::json!({"space": "mcp_vecs", "vectors": [[1.0,0,0,0],[0,1.0,0,0]]}),
                    ),
                ),
                (
                    "tools/call vector_search",
                    tool(
                        "vector_search",
                        serde_json::json!({"space": "mcp_vecs", "query": [1.0,0,0,0], "k": 2}),
                    ),
                ),
                (
                    "tools/call memory_create",
                    tool(
                        "memory_create",
                        serde_json::json!({"table": "kv", "fields": [{"name": "k", "type": "Utf8"}], "pk": "k"}),
                    ),
                ),
                (
                    "tools/call memory_insert",
                    tool(
                        "memory_insert",
                        serde_json::json!({"table": "kv", "rows": [{"k": "a"}, {"k": "b"}]}),
                    ),
                ),
                (
                    "tools/call memory_sql",
                    tool(
                        "memory_sql",
                        serde_json::json!({"sql": "SELECT count(*) FROM kv"}),
                    ),
                ),
            ];
            for (i, (name, params)) in mcp_tools.iter().enumerate() {
                let (ms, resp) = mcp_client
                    .call(i as u64 + 1, method_of(params), params.clone())
                    .await?;
                let ok = !resp.contains("\"error\"");
                report.mcp.push(report::McpStat {
                    tool: name.to_string(),
                    ms,
                    ok,
                });
                println!("  {name}: {ms:.1} ms {}", if ok { "ok" } else { "FAILED" });
            }
            let _ = child.kill().await;
        }
    }

    // final resource snapshot
    report.rss_final_mb = rss_bytes() as f64 / 1e6;
    report.cpu_pct = cpu_usage_pct();

    println!("--- summary ---");
    println!("{}", report.render_markdown());
    let results_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("results");
    std::fs::create_dir_all(&results_dir)?;
    std::fs::write(
        results_dir.join("results.json"),
        serde_json::to_string_pretty(&report)?,
    )?;
    std::fs::write(results_dir.join("latest.md"), report.render_markdown())?;
    println!(
        "report written to {}/{{latest.md,results.json}}",
        results_dir.display()
    );

    if !keep_dir {
        let _ = std::fs::remove_dir_all(&root);
    } else {
        println!("data kept at {}", dir.display());
    }
    Ok(())
}

fn method_of(params: &serde_json::Value) -> &'static str {
    if params.get("arguments").is_some() {
        "tools/call"
    } else {
        "initialize"
    }
}

fn load_phase(
    dir: &Path,
    tpch_tables: Vec<(String, Arc<Schema>, Vec<RecordBatch>, u64)>,
) -> Result<(Arc<LessEngine>, Vec<(String, u64, u64, usize, usize)>)> {
    let engine = LessEngine::open(EngineConfig::with_data_dir(dir))?;
    for (name, fields, sort, unique) in SCHEMAS {
        engine.create_table(table_def(name, fields, sort, unique))?;
    }
    let mut footprint = vec![];
    for (name, _schema, batches, raw) in tpch_tables {
        for batch in batches {
            engine.insert(&name, batch)?;
        }
        engine.flush(&name)?;
        let stats = engine.stats(&name)?;
        footprint.push((
            name,
            stats.disk_bytes,
            raw,
            stats.rows as usize,
            stats.part_count,
        ));
    }
    Ok((engine, footprint))
}

/// Recursively list files (small helper for leftover checks).
fn walkdir(dir: &Path) -> Result<Vec<String>> {
    let mut out = vec![];
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            out.extend(walkdir(&path)?);
        } else {
            out.push(path.display().to_string());
        }
    }
    Ok(out)
}
