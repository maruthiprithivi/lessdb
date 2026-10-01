//! LessDB telemetry: a tiny, dependency-light metrics registry with a
//! Prometheus text-exposition renderer.
//!
//! Hand-rolled instead of pulling the `prometheus` crate: the exposition
//! format is simple, and owning it keeps the dependency tree small (one
//! registry, fixed metric set, no pulling/collector machinery). Everything
//! is atomic or mutex-guarded, so instrumentation is cheap and safe from
//! async contexts.
//!
//! Metrics:
//! * `lessdb_uptime_seconds` / `lessdb_process_resident_memory_bytes` — process
//! * `lessdb_queries_total{status}` / `lessdb_query_duration_seconds` (histogram)
//! * `lessdb_rows_returned_total`, `lessdb_rows_inserted_total`
//! * `lessdb_parts_written_total`, `lessdb_parts_merged_total`
//! * `lessdb_parts_scanned_total`, `lessdb_parts_pruned_total`
//! * `lessdb_http_requests_total{route,status}` / `lessdb_auth_failures_total`
//! * `lessdb_tables` / `lessdb_buffered_rows` (gauges, set by callers)
//! * `lessdb_build_info{version}`

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

pub mod audit;
pub mod logging;
pub use audit::{AuditEntry, AuditLog, parse_since, query};

/// Monotonic counter.
#[derive(Debug, Default)]
pub struct Counter {
    value: AtomicU64,
}

impl Counter {
    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }
    pub fn add(&self, n: u64) {
        self.value.fetch_add(n, Ordering::Relaxed);
    }
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}

/// Counter with a small label set (e.g. `status="ok"`).
#[derive(Debug, Default)]
pub struct LabeledCounter {
    values: Mutex<HashMap<String, AtomicU64>>,
}

impl LabeledCounter {
    pub fn inc(&self, labels: &[(&str, &str)]) {
        let key = label_key(labels);
        let mut values = self.values.lock().unwrap();
        values
            .entry(key)
            .or_default()
            .fetch_add(1, Ordering::Relaxed);
    }
    pub fn add(&self, labels: &[(&str, &str)], n: u64) {
        let key = label_key(labels);
        let mut values = self.values.lock().unwrap();
        values
            .entry(key)
            .or_default()
            .fetch_add(n, Ordering::Relaxed);
    }
    pub fn snapshot(&self) -> Vec<(String, u64)> {
        let values = self.values.lock().unwrap();
        let mut out: Vec<(String, u64)> = values
            .iter()
            .map(|(k, v)| (k.clone(), v.load(Ordering::Relaxed)))
            .collect();
        out.sort();
        out
    }
}

fn label_key(labels: &[(&str, &str)]) -> String {
    let mut parts: Vec<String> = labels
        .iter()
        .map(|(k, v)| format!("{k}=\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    parts.sort();
    parts.join(",")
}

/// Gauge (settable integer).
#[derive(Debug, Default)]
pub struct Gauge {
    value: AtomicI64,
}

impl Gauge {
    pub fn set(&self, v: i64) {
        self.value.store(v, Ordering::Relaxed);
    }
    pub fn get(&self) -> i64 {
        self.value.load(Ordering::Relaxed)
    }
}

/// Histogram with fixed, pre-declared buckets (cumulative in output).
#[derive(Debug)]
pub struct Histogram {
    buckets: Vec<f64>,
    state: Mutex<HistogramState>,
}

#[derive(Debug, Default)]
struct HistogramState {
    per_bucket: Vec<u64>,
    sum: f64,
    count: u64,
}

impl Histogram {
    pub fn new(buckets: &[f64]) -> Self {
        assert!(!buckets.is_empty() && buckets.windows(2).all(|w| w[0] < w[1]));
        Self {
            buckets: buckets.to_vec(),
            state: Mutex::new(HistogramState {
                per_bucket: vec![0; buckets.len()],
                sum: 0.0,
                count: 0,
            }),
        }
    }

    pub fn observe(&self, value: f64) {
        let idx = self.buckets.partition_point(|b| *b < value);
        let mut state = self.state.lock().unwrap();
        if idx < state.per_bucket.len() {
            state.per_bucket[idx] += 1;
        }
        state.sum += value;
        state.count += 1;
    }

    fn render(&self, name: &str, help: &str, out: &mut String) {
        let state = self.state.lock().unwrap();
        let _ = writeln!(out, "# HELP {name} {help}");
        let _ = writeln!(out, "# TYPE {name} histogram");
        let mut cumulative = 0u64;
        for (b, c) in self.buckets.iter().zip(state.per_bucket.iter()) {
            cumulative += c;
            let _ = writeln!(out, "{name}_bucket{{le=\"{b}\"}} {cumulative}");
        }
        let _ = writeln!(out, "{name}_bucket{{le=\"+Inf\"}} {}", state.count);
        let _ = writeln!(out, "{name}_sum {}", state.sum);
        let _ = writeln!(out, "{name}_count {}", state.count);
    }
}

/// The LessDB metric set.
#[derive(Debug)]
pub struct Metrics {
    start: Instant,
    pub queries: LabeledCounter,
    pub query_duration: Histogram,
    pub rows_returned: Counter,
    pub rows_inserted: Counter,
    pub parts_written: Counter,
    pub parts_merged: Counter,
    pub parts_scanned: Counter,
    pub parts_pruned: Counter,
    pub http_requests: LabeledCounter,
    pub auth_failures: Counter,
    pub tables: Gauge,
    pub buffered_rows: Gauge,
    pub memory_pool_bytes: Gauge,
}

impl Metrics {
    fn new() -> Self {
        Self {
            start: Instant::now(),
            queries: LabeledCounter::default(),
            query_duration: Histogram::new(&[
                0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0,
                60.0,
            ]),
            rows_returned: Counter::default(),
            rows_inserted: Counter::default(),
            parts_written: Counter::default(),
            parts_merged: Counter::default(),
            parts_scanned: Counter::default(),
            parts_pruned: Counter::default(),
            http_requests: LabeledCounter::default(),
            auth_failures: Counter::default(),
            tables: Gauge::default(),
            buffered_rows: Gauge::default(),
            memory_pool_bytes: Gauge::default(),
        }
    }

    /// Render the full Prometheus text exposition.
    pub fn render(&self) -> String {
        let mut out = String::with_capacity(2048);

        // Process metrics.
        let uptime = self.start.elapsed().as_secs_f64();
        let _ = writeln!(
            out,
            "# HELP lessdb_uptime_seconds Process uptime in seconds."
        );
        let _ = writeln!(out, "# TYPE lessdb_uptime_seconds gauge");
        let _ = writeln!(out, "lessdb_uptime_seconds {uptime:.3}");
        if let Some(rss) = process_rss_bytes() {
            let _ = writeln!(
                out,
                "# HELP lessdb_process_resident_memory_bytes Resident memory of this process."
            );
            let _ = writeln!(out, "# TYPE lessdb_process_resident_memory_bytes gauge");
            let _ = writeln!(out, "lessdb_process_resident_memory_bytes {rss}");
        }

        // Build info.
        let _ = writeln!(out, "# HELP lessdb_build_info LessDB build information.");
        let _ = writeln!(out, "# TYPE lessdb_build_info gauge");
        let _ = writeln!(
            out,
            "lessdb_build_info{{version=\"{}\"}} 1",
            less_common::VERSION
        );

        // Queries.
        let _ = writeln!(out, "# HELP lessdb_queries_total SQL queries executed.");
        let _ = writeln!(out, "# TYPE lessdb_queries_total counter");
        for (labels, v) in self.queries.snapshot() {
            let _ = writeln!(out, "lessdb_queries_total{{{labels}}} {v}");
        }
        self.query_duration.render(
            "lessdb_query_duration_seconds",
            "SQL query execution time in seconds.",
            &mut out,
        );

        // Rows & parts.
        let _ = writeln!(
            out,
            "# HELP lessdb_rows_returned_total Rows returned to clients."
        );
        let _ = writeln!(out, "# TYPE lessdb_rows_returned_total counter");
        let _ = writeln!(
            out,
            "lessdb_rows_returned_total {}",
            self.rows_returned.get()
        );
        let _ = writeln!(
            out,
            "# HELP lessdb_rows_inserted_total Rows accepted by inserts."
        );
        let _ = writeln!(out, "# TYPE lessdb_rows_inserted_total counter");
        let _ = writeln!(
            out,
            "lessdb_rows_inserted_total {}",
            self.rows_inserted.get()
        );
        let _ = writeln!(
            out,
            "# HELP lessdb_parts_written_total Data parts flushed to storage."
        );
        let _ = writeln!(out, "# TYPE lessdb_parts_written_total counter");
        let _ = writeln!(
            out,
            "lessdb_parts_written_total {}",
            self.parts_written.get()
        );
        let _ = writeln!(
            out,
            "# HELP lessdb_parts_merged_total Data parts merged (OPTIMIZE)."
        );
        let _ = writeln!(out, "# TYPE lessdb_parts_merged_total counter");
        let _ = writeln!(out, "lessdb_parts_merged_total {}", self.parts_merged.get());
        let _ = writeln!(
            out,
            "# HELP lessdb_parts_scanned_total Data parts read by scans."
        );
        let _ = writeln!(out, "# TYPE lessdb_parts_scanned_total counter");
        let _ = writeln!(
            out,
            "lessdb_parts_scanned_total {}",
            self.parts_scanned.get()
        );
        let _ = writeln!(
            out,
            "# HELP lessdb_parts_pruned_total Data parts skipped by pruning."
        );
        let _ = writeln!(out, "# TYPE lessdb_parts_pruned_total counter");
        let _ = writeln!(out, "lessdb_parts_pruned_total {}", self.parts_pruned.get());

        // HTTP & auth.
        let _ = writeln!(out, "# HELP lessdb_http_requests_total HTTP API requests.");
        let _ = writeln!(out, "# TYPE lessdb_http_requests_total counter");
        for (labels, v) in self.http_requests.snapshot() {
            let _ = writeln!(out, "lessdb_http_requests_total{{{labels}}} {v}");
        }
        let _ = writeln!(
            out,
            "# HELP lessdb_auth_failures_total Failed authentication attempts."
        );
        let _ = writeln!(out, "# TYPE lessdb_auth_failures_total counter");
        let _ = writeln!(
            out,
            "lessdb_auth_failures_total {}",
            self.auth_failures.get()
        );

        // Gauges.
        let _ = writeln!(out, "# HELP lessdb_tables Tables known to this node.");
        let _ = writeln!(out, "# TYPE lessdb_tables gauge");
        let _ = writeln!(out, "lessdb_tables {}", self.tables.get());
        let _ = writeln!(
            out,
            "# HELP lessdb_buffered_rows Rows buffered in memory, not yet flushed."
        );
        let _ = writeln!(out, "# TYPE lessdb_buffered_rows gauge");
        let _ = writeln!(out, "lessdb_buffered_rows {}", self.buffered_rows.get());
        let _ = writeln!(
            out,
            "# HELP lessdb_memory_pool_bytes Bytes reserved in the query memory pool."
        );
        let _ = writeln!(out, "# TYPE lessdb_memory_pool_bytes gauge");
        let _ = writeln!(
            out,
            "lessdb_memory_pool_bytes {}",
            self.memory_pool_bytes.get()
        );

        out
    }
}

/// The process-wide metric registry.
static GLOBAL: OnceLock<Metrics> = OnceLock::new();

pub fn global() -> &'static Metrics {
    GLOBAL.get_or_init(Metrics::new)
}

/// Resident set size of the current process, when measurable.
fn process_rss_bytes() -> Option<u64> {
    use sysinfo::{Pid, ProcessesToUpdate, System};
    let pid = Pid::from_u32(std::process::id());
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).map(|p| p.memory())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_and_labels() {
        let m = Metrics::new();
        m.queries.inc(&[("status", "ok")]);
        m.queries.inc(&[("status", "ok")]);
        m.queries.inc(&[("status", "error")]);
        let snap = m.queries.snapshot();
        assert_eq!(snap.len(), 2);
        assert!(m.render().contains("lessdb_queries_total{status=\"ok\"} 2"));
        assert!(
            m.render()
                .contains("lessdb_queries_total{status=\"error\"} 1")
        );
    }

    #[test]
    fn histogram_is_cumulative_and_formatted() {
        let m = Metrics::new();
        m.query_duration.observe(0.004); // bucket le=0.005
        m.query_duration.observe(0.25); // bucket le=0.25
        m.query_duration.observe(120.0); // +Inf only
        let rendered = m.render();
        assert!(rendered.contains("lessdb_query_duration_seconds_bucket{le=\"0.005\"} 1"));
        assert!(rendered.contains("lessdb_query_duration_seconds_bucket{le=\"0.25\"} 2"));
        assert!(rendered.contains("lessdb_query_duration_seconds_bucket{le=\"+Inf\"} 3"));
        assert!(rendered.contains("lessdb_query_duration_seconds_count 3"));
    }

    #[test]
    fn render_has_expected_sections() {
        let rendered = Metrics::new().render();
        for needle in [
            "lessdb_uptime_seconds",
            "lessdb_build_info{version=\"",
            "lessdb_rows_inserted_total",
            "lessdb_parts_pruned_total",
            "lessdb_auth_failures_total",
            "lessdb_tables",
        ] {
            assert!(rendered.contains(needle), "missing {needle}");
        }
    }

    #[test]
    fn label_escaping() {
        assert_eq!(label_key(&[("k", "a\"b")]), "k=\"a\\\"b\"");
    }
}
