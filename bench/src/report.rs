//! Benchmark report structures + markdown rendering.

use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct Footprint {
    pub table: String,
    pub rows: usize,
    pub parts: usize,
    pub stored_bytes: u64,
    pub raw_bytes: u64,
    pub ratio: f64,
}

#[derive(Serialize, Clone, Default)]
pub struct Resiliency {
    pub restart_ok: bool,
    pub restart_parts: usize,
    pub two_node_ok: bool,
    pub atomic_writes_ok: bool,
    pub corrupt_meta_ok: bool,
}

#[derive(Serialize, Clone)]
pub struct VectorStat {
    pub vectors: usize,
    pub dim: usize,
    pub add_ms: f64,
    pub train_ms: f64,
    pub recall10: f64,
    pub qps: f64,
    pub search_ms_avg: f64,
}

#[derive(Serialize, Clone)]
pub struct GraphStat {
    pub nodes: usize,
    pub edges: usize,
    pub build_ms: f64,
    pub neighbors_p50_ms: f64,
    pub neighbors_p99_ms: f64,
    pub path_p50_ms: f64,
    pub path_p99_ms: f64,
}

#[derive(Serialize, Clone)]
pub struct McpStat {
    pub tool: String,
    pub ms: f64,
    pub ok: bool,
}

use super::LatencyStats;
use super::QueryResult;

#[derive(Serialize, Clone, Default)]
pub struct Report {
    pub version: String,
    pub sf: f64,
    pub gen_ms: f64,
    pub gen_rows: usize,
    pub load_ms: f64,
    pub rss_after_load_mb: f64,
    pub rss_after_queries_mb: f64,
    pub rss_final_mb: f64,
    pub cpu_pct: f32,
    pub footprint: Vec<Footprint>,
    pub queries: Vec<QueryResult>,
    pub resiliency: Resiliency,
    pub vectors: Vec<VectorStat>,
    pub graph: Vec<GraphStat>,
    pub dashboard: Vec<LatencyStats>,
    pub mcp: Vec<McpStat>,
}

impl Report {
    pub fn new(sf: f64) -> Self {
        Self {
            version: less_common::VERSION.to_string(),
            sf,
            resiliency: Resiliency {
                restart_ok: false,
                restart_parts: 0,
                two_node_ok: false,
                atomic_writes_ok: false,
                corrupt_meta_ok: false,
            },
            ..Default::default()
        }
    }

    pub fn render_markdown(&self) -> String {
        let mut out = String::new();
        use std::fmt::Write as _;
        let ok = |b: bool| if b { "✅ OK" } else { "❌ FAIL" };
        let _ = writeln!(out, "# LessDB Benchmark Report");
        let _ = writeln!(out);
        let _ = writeln!(out, "- version: `{}`", self.version);
        let _ = writeln!(out, "- scale factor (TPC-H-ish): **{}**", self.sf);
        let _ = writeln!(
            out,
            "- generated rows: {} ({:.0} ms)",
            self.gen_rows, self.gen_ms
        );
        let _ = writeln!(out, "- load + flush: {:.0} ms", self.load_ms);
        let _ = writeln!(
            out,
            "- RSS: {:.0} MB after load, {:.0} MB after queries, {:.0} MB final",
            self.rss_after_load_mb, self.rss_after_queries_mb, self.rss_final_mb
        );
        let _ = writeln!(out);
        let _ = writeln!(out, "## Storage footprint (compression)");
        let _ = writeln!(out);
        let _ = writeln!(out, "| table | rows | parts | stored | raw est. | ratio |");
        let _ = writeln!(out, "|---|---|---|---|---|---|");
        for f in &self.footprint {
            let _ = writeln!(
                out,
                "| {} | {} | {} | {:.2} MB | {:.2} MB | {:.1}x |",
                f.table,
                f.rows,
                f.parts,
                f.stored_bytes as f64 / 1e6,
                f.raw_bytes as f64 / 1e6,
                f.ratio
            );
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "## TPC-H-inspired queries (warm = best of 2)");
        let _ = writeln!(out);
        let _ = writeln!(out, "| query | rows | cold ms | warm ms |");
        let _ = writeln!(out, "|---|---|---|---|");
        for q in &self.queries {
            let _ = writeln!(
                out,
                "| {} | {} | {:.1} | {:.1} |",
                q.query, q.rows, q.cold_ms, q.warm_ms
            );
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "## Metadata resiliency");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "- restart (reopen, rows intact, {} parts): {}",
            self.resiliency.restart_parts,
            ok(self.resiliency.restart_ok)
        );
        let _ = writeln!(
            out,
            "- two-node shared storage (node B sees node A's data): {}",
            ok(self.resiliency.two_node_ok)
        );
        let _ = writeln!(
            out,
            "- atomic writes (no `.tmp` leftovers): {}",
            ok(self.resiliency.atomic_writes_ok)
        );
        let _ = writeln!(
            out,
            "- corrupt part metadata surfaces an error, no crash: {}",
            ok(self.resiliency.corrupt_meta_ok)
        );
        let _ = writeln!(out);
        let _ = writeln!(out, "## Vector search (IVF-PQ)");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "{} vectors, dim {}",
            self.vectors.first().map(|v| v.vectors).unwrap_or(0),
            self.vectors.first().map(|v| v.dim).unwrap_or(0)
        );
        let _ = writeln!(out);
        let _ = writeln!(out, "| add ms | train ms | recall@10 | search avg | QPS |");
        let _ = writeln!(out, "|---|---|---|---|---|");
        for v in &self.vectors {
            let _ = writeln!(
                out,
                "| {:.0} | {:.0} | {:.2} | {:.2} ms | {:.0} |",
                v.add_ms, v.train_ms, v.recall10, v.search_ms_avg, v.qps
            );
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "## Graph (context) queries");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "| nodes | edges | build ms | neighbors p50 | neighbors p99 | path p50 | path p99 |"
        );
        let _ = writeln!(out, "|---|---|---|---|---|---|---|");
        for g in &self.graph {
            let _ = writeln!(
                out,
                "| {} | {} | {:.0} | {:.2} ms | {:.2} ms | {:.2} ms | {:.2} ms |",
                g.nodes,
                g.edges,
                g.build_ms,
                g.neighbors_p50_ms,
                g.neighbors_p99_ms,
                g.path_p50_ms,
                g.path_p99_ms
            );
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "## Interactive / dashboard queries");
        let _ = writeln!(out);
        let _ = writeln!(out, "| workload | iters | mean | p50 | p95 | p99 |");
        let _ = writeln!(out, "|---|---|---|---|---|---|");
        for d in &self.dashboard {
            let _ = writeln!(
                out,
                "| {} | {} | {:.2} ms | {:.2} ms | {:.2} ms | {:.2} ms |",
                d.name, d.iters, d.mean_ms, d.p50_ms, d.p95_ms, d.p99_ms
            );
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "## MCP agent session (stdio JSON-RPC, per call)");
        let _ = writeln!(out);
        let _ = writeln!(out, "| tool | ms | status |");
        let _ = writeln!(out, "|---|---|---|");
        for m in &self.mcp {
            let _ = writeln!(out, "| {} | {:.1} | {} |", m.tool, m.ms, ok(m.ok));
        }
        out
    }
}
