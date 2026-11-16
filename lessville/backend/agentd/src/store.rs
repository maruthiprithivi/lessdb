//! Thin, observable wrapper over an embedded LessDB session.
//!
//! Every statement the world runs goes through [`Store::run`], which records
//! the exact SQL text, row count, latency and outcome in a bounded ring
//! buffer so the game can show what LessDB actually did.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use less_engine::LessEngine;
use less_query::LessSession;
use serde::Serialize;
use serde_json::{Map, Value};
use tokio::sync::Mutex;

pub type Row = Map<String, Value>;

#[derive(Clone, Serialize)]
pub struct SqlLog {
    pub id: u64,
    pub sim_minute: i64,
    pub actor: String,
    pub purpose: String,
    pub sql: String,
    pub rows: usize,
    pub micros: u128,
    pub ok: bool,
    pub error: Option<String>,
}

pub struct Store {
    pub session: LessSession,
    pub engine: Arc<LessEngine>,
    log: Mutex<VecDeque<SqlLog>>,
    seq: AtomicU64,
}

const LOG_CAP: usize = 200;
/// Longest SQL text kept in the observer log; LessDB always receives the full statement.
const LOG_SQL_CHARS: usize = 480;

fn log_text(sql: &str) -> String {
    let total = sql.chars().count();
    if total <= LOG_SQL_CHARS {
        return sql.to_string();
    }
    let head: String = sql.chars().take(LOG_SQL_CHARS).collect();
    format!("{head} ... [{total} chars total; shortened for display]")
}

#[cfg(test)]
mod log_text_tests {
    #[test]
    fn long_sql_is_shortened_for_display_only() {
        let long = format!(
            "INSERT INTO lv_kv VALUES ('world', '{}', 1)",
            "x".repeat(5000)
        );
        let shown = super::log_text(&long);
        assert!(shown.len() < 600);
        assert!(shown.ends_with("chars total; shortened for display]"));
        assert_eq!(super::log_text("SELECT 1"), "SELECT 1");
    }
}

/// Quote a string literal for SQL (standard '' escaping, control chars dropped).
pub fn q(s: &str) -> String {
    let clean: String = s.chars().filter(|c| !c.is_control()).collect();
    format!("'{}'", clean.replace('\'', "''"))
}

impl Store {
    pub fn new(session: LessSession, engine: Arc<LessEngine>) -> Self {
        Store {
            session,
            engine,
            log: Mutex::new(VecDeque::new()),
            seq: AtomicU64::new(0),
        }
    }

    /// Run one statement; record it; return rows as JSON objects.
    pub async fn run(
        &self,
        sim_minute: i64,
        actor: &str,
        purpose: &str,
        sql: &str,
    ) -> Result<Vec<Row>, String> {
        let start = Instant::now();
        let rows = match self.session.sql_batches(sql).await {
            Ok(batches) => batches_to_rows(&batches),
            Err(e) => Err(e.to_string()),
        };
        let micros = start.elapsed().as_micros();
        let entry = SqlLog {
            id: self.seq.fetch_add(1, Ordering::Relaxed) + 1,
            sim_minute,
            actor: actor.to_string(),
            purpose: purpose.to_string(),
            sql: log_text(sql),
            rows: rows.as_ref().map(|r| r.len()).unwrap_or(0),
            micros,
            ok: rows.is_ok(),
            error: rows.as_ref().err().cloned(),
        };
        if let Some(err) = &entry.error {
            eprintln!("[lessdb] {actor}/{purpose} failed: {err}\n  sql: {sql}");
        }
        let mut log = self.log.lock().await;
        if log.len() >= LOG_CAP {
            log.pop_front();
        }
        log.push_back(entry);
        rows
    }

    pub async fn recent_log(&self, n: usize) -> Vec<SqlLog> {
        let log = self.log.lock().await;
        log.iter().rev().take(n).cloned().collect()
    }

    pub fn statements(&self) -> u64 {
        self.seq.load(Ordering::Relaxed)
    }
}

fn batches_to_rows(batches: &[arrow::record_batch::RecordBatch]) -> Result<Vec<Row>, String> {
    if batches.iter().all(|b| b.num_rows() == 0) {
        return Ok(Vec::new());
    }
    let mut writer = arrow_json::ArrayWriter::new(Vec::new());
    let refs: Vec<&arrow::record_batch::RecordBatch> = batches.iter().collect();
    writer.write_batches(&refs).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    let buf = writer.into_inner();
    if buf.is_empty() {
        return Ok(Vec::new());
    }
    let v: Value = serde_json::from_slice(&buf).map_err(|e| e.to_string())?;
    Ok(match v {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|i| match i {
                Value::Object(m) => Some(m),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    })
}

/// Helpers for reading typed fields out of JSON rows.
pub fn s(row: &Row, k: &str) -> String {
    match row.get(k) {
        Some(Value::String(v)) => v.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

pub fn i(row: &Row, k: &str) -> i64 {
    match row.get(k) {
        Some(Value::Number(n)) => n
            .as_i64()
            .unwrap_or_else(|| n.as_f64().unwrap_or(0.0) as i64),
        Some(Value::String(v)) => v.parse().unwrap_or(0),
        _ => 0,
    }
}
