//! LessDB sqllogictest harness.
//!
//! Runs `.slt` files (the SQLLogicTest golden-SQL format)
//! against a fresh in-process LessDB engine + DataFusion session:
//!
//! ```text
//! statement ok
//! CREATE TABLE t (id Int64, v Float64) ENGINE=Firefly ORDER BY id
//!
//! statement ok
//! INSERT INTO t VALUES (1, 1.5), (2, 2.5)
//!
//! query II
//! SELECT id, v FROM t ORDER BY id
//! ----
//! 1 1.5
//! 2 2.5
//! ```
//!
//! DDL (CREATE/DROP/ALTER/TRUNCATE and SHOW) is served by the session's
//! native statement layer; OPTIMIZE/FLUSH are routed to the engine.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::datatypes::DataType;
use less_common::{EngineConfig, LessError, Result};
use less_engine::LessEngine;
use less_query::LessSession;
use sqllogictest::{AsyncDB, DBOutput, DefaultColumnType, Runner};

struct LessDb {
    engine: Arc<LessEngine>,
    session: LessSession,
}

impl LessDb {
    async fn open(dir: &Path) -> Result<Self> {
        let engine = LessEngine::open(EngineConfig::with_data_dir(dir))?;
        let session = LessSession::new_async(engine.clone()).await?;
        Ok(Self { engine, session })
    }
}

fn arrow_to_slt(t: &DataType) -> DefaultColumnType {
    match t {
        DataType::Int8
        | DataType::Int16
        | DataType::Int32
        | DataType::Int64
        | DataType::UInt8
        | DataType::UInt16
        | DataType::UInt32
        | DataType::UInt64 => DefaultColumnType::Integer,
        DataType::Float32 | DataType::Float64 => DefaultColumnType::FloatingPoint,
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => DefaultColumnType::Text,
        _ => DefaultColumnType::Any,
    }
}

impl LessDb {
    async fn run(&mut self, sql: &str) -> Result<DBOutput<DefaultColumnType>> {
        let sql = sql.trim();
        let upper = sql.to_ascii_uppercase();
        // CREATE/DROP TABLE (both DDL and CREATE TABLE … AS forms) are
        // served by the session's native statement layer; only engine-only
        // maintenance statements are routed around it.
        if upper.starts_with("OPTIMIZE TABLE") || upper.starts_with("FLUSH TABLE") {
            let keyword = if upper.starts_with("OPTIMIZE TABLE") {
                "OPTIMIZE TABLE"
            } else {
                "FLUSH TABLE"
            };
            // Slice the ORIGINAL statement (table names are case-sensitive
            // on Linux).
            let name = sql[keyword.len()..].trim().trim_end_matches(';').trim();
            if keyword == "OPTIMIZE TABLE" {
                self.engine.optimize(name)?;
            } else {
                self.engine.flush(name)?;
            }
            self.session.refresh_async().await?;
            return Ok(DBOutput::StatementComplete(0));
        }
        let batches = self.session.sql_batches(sql).await?;
        if batches.is_empty() {
            return Ok(DBOutput::Rows {
                types: vec![],
                rows: vec![],
            });
        }
        let types: Vec<DefaultColumnType> = batches[0]
            .schema()
            .fields()
            .iter()
            .map(|f| arrow_to_slt(f.data_type()))
            .collect();
        let mut rows = Vec::new();
        for batch in &batches {
            for r in 0..batch.num_rows() {
                let mut row = Vec::with_capacity(batch.num_columns());
                for c in 0..batch.num_columns() {
                    let col = batch.column(c);
                    if col.is_null(r) {
                        row.push("NULL".to_string());
                    } else {
                        row.push(
                            arrow::util::display::array_value_to_string(col, r)
                                .map_err(LessError::Arrow)?,
                        );
                    }
                }
                rows.push(row);
            }
        }
        Ok(DBOutput::Rows { types, rows })
    }
}

/// All "connections" share one engine + session (serialized) — the runner
/// may open several, and two engines on one local data dir would conflict.
#[derive(Clone)]
struct SharedDb(Arc<tokio::sync::Mutex<LessDb>>);

#[async_trait::async_trait]
impl AsyncDB for SharedDb {
    type Error = LessError;
    type ColumnType = DefaultColumnType;

    async fn run(
        &mut self,
        sql: &str,
    ) -> std::result::Result<DBOutput<DefaultColumnType>, Self::Error> {
        self.0.lock().await.run(sql).await
    }

    async fn shutdown(&mut self) {}

    fn engine_name(&self) -> &str {
        "lessdb"
    }

    async fn sleep(dur: std::time::Duration) {
        tokio::time::sleep(dur).await;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let update = args.iter().any(|a| a == "--update");
    let paths: Vec<PathBuf> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .collect();
    let files = discover(&paths)?;
    if files.is_empty() {
        eprintln!("no .slt files found under {paths:?}");
        std::process::exit(2);
    }

    let mut failures = 0usize;
    for file in &files {
        // One fresh engine per test file (hermetic state).
        let dir = std::env::temp_dir().join(format!(
            "less-slt-{}-{}",
            std::process::id(),
            file.file_stem()
                .map(|s| s.to_string_lossy())
                .unwrap_or_default()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        let db = SharedDb(Arc::new(tokio::sync::Mutex::new(LessDb::open(&dir).await?)));
        let mut runner = Runner::new(move || std::future::ready(Ok(db.clone())));
        let result = if update {
            runner
                .update_test_file(
                    file,
                    " ",
                    sqllogictest::default_validator,
                    sqllogictest::default_normalizer,
                    sqllogictest::default_column_validator,
                )
                .await
                .map_err(|e| LessError::Query(format!("{file:?}: {e}")))
        } else {
            runner
                .run_file_async(file)
                .await
                .map_err(|e| LessError::Query(format!("{file:?}: {e}")))
        };
        match result {
            Ok(()) => println!("ok  {file:?}"),
            Err(e) => {
                failures += 1;
                eprintln!("FAIL {file:?}: {e}");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
    if failures > 0 {
        std::process::exit(1);
    }
    println!("sqllogictest: {} files passed", files.len());
    Ok(())
}

fn discover(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for p in paths {
        if p.is_file() {
            out.push(p.clone());
        } else if p.is_dir() {
            for entry in walk(p)? {
                if entry.extension().is_some_and(|e| e == "slt") {
                    out.push(entry);
                }
            }
        }
    }
    out.sort();
    Ok(out)
}

fn walk(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path)?);
        } else {
            out.push(path);
        }
    }
    Ok(out)
}
