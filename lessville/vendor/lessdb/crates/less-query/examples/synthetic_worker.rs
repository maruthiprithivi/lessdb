//! Local trusted benchmark adapter, not a server or public SQL endpoint.
use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use arrow::array::Array;
use less_common::EngineConfig;
use less_engine::LessEngine;
use less_query::LessSession;
use serde_json::{Value, json};

fn emit(output: &Mutex<()>, value: Value) {
    let _guard = output.lock().unwrap();
    println!("{value}");
    io::stdout().flush().unwrap();
}

async fn query(session: &LessSession, sql: &str) -> Result<Vec<Vec<Option<String>>>, String> {
    let batches = session.sql_batches(sql).await.map_err(|e| e.to_string())?;
    let mut rows = vec![];
    for batch in batches {
        for row in 0..batch.num_rows() {
            let mut values = vec![];
            for column in batch.columns() {
                values.push(if column.is_null(row) {
                    None
                } else {
                    Some(
                        arrow::util::display::array_value_to_string(column, row)
                            .map_err(|e| e.to_string())?,
                    )
                });
            }
            rows.push(values);
        }
    }
    Ok(rows)
}

struct Job {
    id: u64,
    op: String,
    sql: String,
    table: String,
    started: Instant,
    _permit: tokio::sync::OwnedSemaphorePermit,
    test_gate: Option<String>,
}

// Narrow trusted benchmark protocol, not an SQL security parser. Keeping
// explicit transactions out prevents an acknowledgement inside an uncommitted
// DuckDB transaction being mistaken for the fixture's autocommit contract.
fn validate_request_sql(op: &str, sql: &str) -> Result<(), String> {
    let keyword = sql
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    let valid = match op {
        "query" => matches!(keyword.as_str(), "SELECT" | "EXPLAIN"),
        "insert" => keyword == "INSERT",
        "ddl" => keyword == "CREATE",
        _ => false,
    };
    if sql.contains(';') || !valid {
        return Err("benchmark protocol requires one SELECT/EXPLAIN, INSERT or CREATE; no semicolons/transaction controls".into());
    }
    Ok(())
}

fn execute(
    job: Job,
    session: Arc<LessSession>,
    engine: Arc<LessEngine>,
    output: Arc<Mutex<()>>,
    runtime: tokio::runtime::Handle,
    gate_dir: Option<std::path::PathBuf>,
) {
    let result = runtime.block_on(async {
        validate_request_sql(&job.op, &job.sql)?;
        if let Some(gate) = &job.test_gate {
            let directory = gate_dir.ok_or("test gate disabled")?;
            if gate != "writer" || job.op != "insert" {
                return Err("invalid test gate".to_string());
            }
            std::fs::write(directory.join("writer.entered"), b"").map_err(|e| e.to_string())?;
            let deadline = Instant::now() + std::time::Duration::from_secs(30);
            while !directory.join("writer.release").exists() {
                if Instant::now() >= deadline {
                    return Err("test gate release timeout".to_string());
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        }
        let rows = query(&session, &job.sql).await?;
        if job.op == "insert" {
            engine.flush(&job.table).map_err(|e| e.to_string())?;
        }
        if job.op == "ddl" {
            session.refresh_async().await.map_err(|e| e.to_string())?;
        }
        Ok::<_, String>(rows)
    });
    emit(
        &output,
        match result {
            Ok(rows) => {
                json!({"id":job.id,"rows":rows,"engine_ms":job.started.elapsed().as_secs_f64()*1000.0})
            }
            Err(error) => {
                json!({"id":job.id,"error":error,"engine_ms":job.started.elapsed().as_secs_f64()*1000.0})
            }
        },
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: synthetic_worker DATABASE_DIR THREADS MEMORY_BYTES".into());
    }
    let threads: usize = args[2].parse()?;
    let mut config = EngineConfig::with_data_dir(&args[1]);
    config.memory_limit = args[3].parse()?;
    // Retain engine defaults for WAL, fsync, buffering, compression and merging.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(threads.max(1))
        .enable_all()
        .build()?;
    let engine = LessEngine::open(config)?;
    let session = Arc::new(runtime.block_on(LessSession::new_async_with_partitions(
        engine.clone(),
        threads,
    ))?);
    let spill_bytes: u64 = std::env::var("LESSDB_BENCH_SPILL_BYTES")
        .unwrap_or_else(|_| "536870912".into())
        .parse()?;
    session
        .ctx()
        .runtime_env()
        .disk_manager
        .set_max_temp_directory_size(spill_bytes)?;
    let output = Arc::new(Mutex::new(()));
    let capacity: usize = std::env::var("LESSDB_BENCH_ADMISSION")
        .unwrap_or_else(|_| "64".into())
        .parse()?;
    if !(2..=64).contains(&capacity) {
        return Err("admission capacity must be between 2 and 64".into());
    }
    let gate_dir = std::env::var_os("LESSDB_BENCH_TEST_GATE_DIR").map(std::path::PathBuf::from);
    emit(
        &output,
        json!({"ready":true,"engine":"lessdb","version":env!("CARGO_PKG_VERSION"),
        "source_commit":option_env!("LESSDB_BENCH_SOURCE_COMMIT").unwrap_or("unrecorded"),
        "debug_assertions":cfg!(debug_assertions),"threads":threads,
        "memory_bytes":engine.config.memory_limit,"wal":engine.config.wal_enabled,
        "wal_fsync":engine.config.wal_fsync,"flush_rows":engine.config.flush_rows,
        "max_buffer_rows":engine.config.max_buffer_rows,
        "auto_merge_parts":engine.config.auto_merge_parts,
        "compression":engine.config.compression,"block_cache_bytes":engine.config.block_cache_bytes,
        "io_threads":2,"spill_limit_bytes":session.ctx().runtime_env().disk_manager.max_temp_directory_size(),
        "temp_directory":std::env::temp_dir(),"max_adapter_inflight":capacity,
        "admission_model":"bounded-async-query-fifo-writer-v2",
        "acknowledgement":"SQL insert then part flush; default WAL sync_data and local part/ACTIVE publication barriers; filesystem/device sync semantics required; no full transaction guarantee",
        "statement_atomicity":"not guaranteed", "power_loss_equivalence_verified":false,
        "transaction_mode":"single-statement-only; explicit transaction controls and semicolons unsupported"}),
    );
    let admission = Arc::new(tokio::sync::Semaphore::new(capacity));
    let (writers, mut pending_writes) = tokio::sync::mpsc::channel::<Job>(capacity);
    let (write_session, write_engine, write_output, write_runtime) = (
        session.clone(),
        engine.clone(),
        output.clone(),
        runtime.handle().clone(),
    );
    let writer_gate_dir = gate_dir.clone();
    let writer = runtime.spawn(async move {
        while let Some(job) = pending_writes.recv().await {
            let (session, engine, output, runtime, gates) = (
                write_session.clone(),
                write_engine.clone(),
                write_output.clone(),
                write_runtime.clone(),
                writer_gate_dir.clone(),
            );
            // One reused blocking caller lane, so sync flush does not occupy a
            // query-runtime worker. The FIFO drains in request acceptance order.
            tokio::task::spawn_blocking(move || {
                execute(job, session, engine, output, runtime, gates)
            })
            .await?;
        }
        Ok::<_, tokio::task::JoinError>(())
    });
    let mut jobs = vec![];
    for line in io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line?)?;
        let op = request["op"].as_str().ok_or("missing operation")?;
        if op == "close" {
            break;
        }
        let id = request["id"].as_u64().ok_or("missing request id")?;
        if !matches!(op, "query" | "insert" | "ddl") {
            emit(
                &output,
                json!({"id":id,"error":"unsupported operation","engine_ms":0}),
            );
            continue;
        }
        let sql = request["sql"].as_str().unwrap_or("").to_string();
        let table = request["table"].as_str().unwrap_or("").to_string();
        let started = Instant::now();
        let permit = runtime.block_on(admission.clone().acquire_owned())?;
        let job = Job {
            id,
            op: op.to_string(),
            sql,
            table,
            started,
            _permit: permit,
            test_gate: request["test_gate"].as_str().map(str::to_string),
        };
        let (session, output) = (session.clone(), output.clone());
        if op == "query" {
            jobs.retain(|job: &tokio::task::JoinHandle<()>| !job.is_finished());
            jobs.push(runtime.spawn(async move {
                let result = if let Err(error) = validate_request_sql(&job.op, &job.sql) { Err(error) }
                    else if job.test_gate.is_some() { Err("invalid test gate".to_string()) }
                    else { query(&session, &job.sql).await };
                emit(&output, match result {
                    Ok(rows) => json!({"id":id,"rows":rows,"engine_ms":started.elapsed().as_secs_f64()*1000.0}),
                    Err(error) => json!({"id":id,"error":error,"engine_ms":started.elapsed().as_secs_f64()*1000.0}),
                });
                drop(job); // keep admission held through result delivery
            }));
        } else {
            runtime.block_on(writers.send(job))?;
        }
    }
    drop(writers);
    runtime.block_on(writer)??;
    for job in jobs {
        runtime.block_on(job)?;
    }
    Ok(())
}
