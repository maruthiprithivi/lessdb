//! Built-in benchmark: generate rows, measure insert throughput, then run
//! scan / filter / group-by queries and report rows-per-second and query
//! timings.

use std::sync::Arc;
use std::time::Instant;

use arrow::array::{Float64Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use less_catalog::{EngineKind, FieldSpec, SchemaSpec, TableDef, TypeSpec};
use less_common::Result;
use less_engine::LessEngine;
use less_query::LessSession;

/// Tiny xorshift RNG — fast, dependency-free, reproducible.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn f64(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Run the benchmark against `dir` (created if needed).
pub async fn bench(dir: &std::path::Path, rows: usize) -> Result<()> {
    // Disable auto-merge during the benchmark so insert throughput isn't
    // polluted by background merges.
    let mut config = less_common::EngineConfig::with_data_dir(dir);
    config.auto_merge_parts = usize::MAX;
    let engine = LessEngine::open(config)?;
    let table = "bench";

    let def = {
        let mut d = TableDef::new(
            table,
            SchemaSpec {
                fields: vec![
                    FieldSpec::new("id", TypeSpec::Int64),
                    FieldSpec::new("group", TypeSpec::Int64),
                    FieldSpec::new("value", TypeSpec::Float64),
                    FieldSpec::new("payload", TypeSpec::Utf8),
                ],
            },
            EngineKind::Firefly,
        );
        d.sort_key = vec!["group".into(), "id".into()];
        d
    };
    let _ = engine.drop_table(table);
    engine.create_table(def)?;

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("group", DataType::Int64, false),
        Field::new("value", DataType::Float64, false),
        Field::new("payload", DataType::Utf8, false),
    ]));

    let batch_rows = 262_144;
    let mut rng = Rng::new(0x5EED_CAFE);
    let t0 = Instant::now();
    let mut inserted = 0usize;
    while inserted < rows {
        let n = batch_rows.min(rows - inserted);
        let mut ids = Vec::with_capacity(n);
        let mut groups = Vec::with_capacity(n);
        let mut values = Vec::with_capacity(n);
        let mut payloads = Vec::with_capacity(n);
        for i in 0..n {
            let id = (inserted + i) as i64;
            ids.push(id);
            groups.push((rng.next() % 10_000) as i64);
            values.push(rng.f64() * 1000.0);
            payloads.push(format!("payload-{id}-{}", rng.next() % 100));
        }
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(Int64Array::from(ids)),
                Arc::new(Int64Array::from(groups)),
                Arc::new(Float64Array::from(values)),
                Arc::new(StringArray::from(payloads)),
            ],
        )?;
        engine.insert(table, batch)?;
        inserted += n;
    }
    engine.flush(table)?;
    let insert_secs = t0.elapsed().as_secs_f64();
    let stats = engine.stats(table)?;
    println!(
        "insert:  {rows} rows in {insert_secs:.2}s  ({:.0} rows/s)",
        rows as f64 / insert_secs
    );
    println!(
        "storage: {} parts, {:.2} MB on disk ({:.0} bytes/row)",
        stats.part_count,
        stats.disk_bytes as f64 / 1e6,
        stats.disk_bytes as f64 / rows as f64
    );

    let session = LessSession::new_async(engine.clone()).await?;

    let queries = [
        ("count", format!("SELECT count(*) FROM {table}")),
        (
            "scan+filter",
            format!("SELECT count(*) FROM {table} WHERE value > 500"),
        ),
        (
            "group-by",
            format!(
                "SELECT \"group\", count(*), sum(value), avg(value) FROM {table} GROUP BY \"group\" ORDER BY \"group\""
            ),
        ),
        (
            "filter+group-by",
            format!(
                "SELECT \"group\", count(*) FROM {table} WHERE id > {} AND value > 500 GROUP BY \"group\" ORDER BY \"group\"",
                rows as i64 / 2
            ),
        ),
    ];

    for (name, sql) in queries {
        let t = Instant::now();
        let batches = session.sql_batches(&sql).await?;
        let elapsed = t.elapsed().as_secs_f64();
        let result_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
        println!(
            "query:   {name:<16} {elapsed:>8.3}s  ({} result rows)",
            result_rows
        );
    }

    Ok(())
}
