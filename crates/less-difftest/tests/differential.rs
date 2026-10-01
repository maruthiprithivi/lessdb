//! Differential testing: the same data and queries run on LessDB
//! (DataFusion) and DuckDB must agree — the SQLite-vs-Postgres style
//! oracle that catches semantic drift.

use std::sync::Arc;

use arrow::array::{Array, Float64Array, Int32Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use duckdb::types::Value as DuckValue;
use less_catalog::{EngineKind, FieldSpec, SchemaSpec, TableDef, TypeSpec};
use less_common::EngineConfig;
use less_engine::LessEngine;
use less_query::LessSession;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

/// A single result cell, normalized for cross-engine comparison.
#[derive(Debug, Clone, PartialEq)]
enum Cell {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
}

impl PartialOrd for Cell {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Cell {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use Cell::*;
        match (self, other) {
            (Null, Null) => std::cmp::Ordering::Equal,
            (Null, _) => std::cmp::Ordering::Less,
            (_, Null) => std::cmp::Ordering::Greater,
            (Int(a), Int(b)) => a.cmp(b),
            (Real(a), Real(b)) => a.total_cmp(b),
            (Text(a), Text(b)) => a.cmp(b),
            (Int(a), Real(b)) => (*a as f64).total_cmp(b),
            (Real(a), Int(b)) => a.total_cmp(&(*b as f64)),
            (a, b) => format!("{a:?}").cmp(&format!("{b:?}")),
        }
    }
}

impl Eq for Cell {}

/// Floats compare with a relative epsilon (engines may round differently).
fn cells_equal(a: &[Cell], b: &[Cell]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).all(|(x, y)| match (x, y) {
        (Cell::Real(x), Cell::Real(y)) => (x - y).abs() <= 1e-9 * (1.0 + x.abs() + y.abs()),
        _ => x == y,
    })
}

/// Sorted rows, so engines may return rows in any order.
fn normalize(mut rows: Vec<Vec<Cell>>) -> Vec<Vec<Cell>> {
    rows.sort();
    rows
}

fn batch(rng: &mut StdRng, rows: usize) -> arrow::record_batch::RecordBatch {
    let groups = ["alpha", "beta", "gamma", "delta", "epsilon"];
    let mut id = Vec::with_capacity(rows);
    let mut g = Vec::with_capacity(rows);
    let mut v = Vec::with_capacity(rows);
    let mut s = Vec::with_capacity(rows);
    let mut n = Vec::with_capacity(rows);
    for i in 0..rows {
        id.push(i as i64);
        g.push(rng.gen_range(0..6i32));
        v.push(rng.gen_range(-100.0..100.0));
        s.push(groups[rng.gen_range(0..groups.len())].to_string());
        // ~10% nulls in the nullable column.
        n.push(if rng.gen_bool(0.1) {
            None
        } else {
            Some(rng.gen_range(0.0..10.0))
        });
    }
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("g", DataType::Int32, false),
        Field::new("v", DataType::Float64, false),
        Field::new("s", DataType::Utf8, false),
        Field::new("n", DataType::Float64, true),
    ]));
    arrow::record_batch::RecordBatch::try_new(
        schema,
        vec![
            Arc::new(Int64Array::from(id)),
            Arc::new(Int32Array::from(g)),
            Arc::new(Float64Array::from(v)),
            Arc::new(StringArray::from(s)),
            Arc::new(Float64Array::from(n)),
        ],
    )
    .unwrap()
}

/// LessDB result rows → normalized cells.
fn less_rows(batches: &[arrow::record_batch::RecordBatch]) -> Vec<Vec<Cell>> {
    let mut out = Vec::new();
    for b in batches {
        for r in 0..b.num_rows() {
            let mut row = Vec::with_capacity(b.num_columns());
            for c in 0..b.num_columns() {
                let col = b.column(c);
                let cell = if col.is_null(r) {
                    Cell::Null
                } else {
                    match b.schema().field(c).data_type() {
                        DataType::Int64 | DataType::Int32 | DataType::Int16 | DataType::Int8 => {
                            if let Some(a) = col.as_any().downcast_ref::<Int64Array>() {
                                Cell::Int(a.value(r))
                            } else if let Some(a) = col.as_any().downcast_ref::<Int32Array>() {
                                Cell::Int(a.value(r) as i64)
                            } else if let Some(a) =
                                col.as_any().downcast_ref::<arrow::array::Int16Array>()
                            {
                                Cell::Int(a.value(r) as i64)
                            } else {
                                let a = col
                                    .as_any()
                                    .downcast_ref::<arrow::array::Int8Array>()
                                    .unwrap();
                                Cell::Int(a.value(r) as i64)
                            }
                        }
                        DataType::Float64 | DataType::Float32 => {
                            let v = col.as_any().downcast_ref::<Float64Array>();
                            match v {
                                Some(a) => Cell::Real(a.value(r)),
                                None => {
                                    let v = col
                                        .as_any()
                                        .downcast_ref::<arrow::array::Float32Array>()
                                        .unwrap();
                                    Cell::Real(v.value(r) as f64)
                                }
                            }
                        }
                        DataType::Utf8 => {
                            let v = col.as_any().downcast_ref::<StringArray>().unwrap();
                            Cell::Text(v.value(r).to_string())
                        }
                        other => Cell::Text(format!(
                            "{other:?}:{}",
                            arrow::util::display::array_value_to_string(col, r).unwrap()
                        )),
                    }
                };
                row.push(cell);
            }
            out.push(row);
        }
    }
    out
}

/// DuckDB result rows → normalized cells (streaming row iteration).
fn duck_rows(stmt: &mut duckdb::Statement) -> Vec<Vec<Cell>> {
    let mut rows = stmt.query([]).unwrap();
    let mut out = Vec::new();
    while let Some(row) = rows.next().unwrap() {
        let mut r = Vec::new();
        for i in 0..row.as_ref().column_count() {
            let v = row.get::<_, DuckValue>(i).unwrap();
            r.push(match v {
                DuckValue::Null => Cell::Null,
                DuckValue::TinyInt(v) => Cell::Int(v as i64),
                DuckValue::SmallInt(v) => Cell::Int(v as i64),
                DuckValue::Int(v) => Cell::Int(v as i64),
                DuckValue::BigInt(v) => Cell::Int(v),
                DuckValue::HugeInt(v) => Cell::Int(v as i64),
                DuckValue::UInt(v) => Cell::Int(v as i64),
                DuckValue::Float(v) => Cell::Real(v as f64),
                DuckValue::Double(v) => Cell::Real(v),
                DuckValue::Text(v) => Cell::Text(v),
                other => Cell::Text(format!("{other:?}")),
            });
        }
        out.push(r);
    }
    out
}

#[tokio::test]
async fn differential_queries_match_duckdb() {
    let mut rng = StdRng::seed_from_u64(42);
    let data = batch(&mut rng, 500);

    // LessDB side.
    let dir = std::env::temp_dir().join(format!("less-diff-{}", uuid::Uuid::new_v4()));
    let engine = LessEngine::open(EngineConfig::with_data_dir(&dir)).unwrap();
    let def = TableDef::new(
        "t",
        SchemaSpec {
            fields: vec![
                FieldSpec::new("id", TypeSpec::Int64),
                FieldSpec::new("g", TypeSpec::Int32),
                FieldSpec::new("v", TypeSpec::Float64),
                FieldSpec::new("s", TypeSpec::Utf8),
                FieldSpec::new("n", TypeSpec::Float64),
            ],
        },
        EngineKind::Firefly,
    );
    engine.create_table(def).unwrap();
    engine.insert("t", data.clone()).unwrap();
    engine.flush("t").unwrap();
    let session = LessSession::new_async(engine).await.unwrap();

    // DuckDB side: same rows via parameterized inserts.
    let mut duck = duckdb::Connection::open_in_memory().unwrap();
    duck.execute_batch("CREATE TABLE t (id BIGINT, g INTEGER, v DOUBLE, s VARCHAR, n DOUBLE)")
        .unwrap();
    {
        let ids = data
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        let gs = data
            .column(1)
            .as_any()
            .downcast_ref::<Int32Array>()
            .unwrap();
        let vs = data
            .column(2)
            .as_any()
            .downcast_ref::<Float64Array>()
            .unwrap();
        let ss = data
            .column(3)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let ns = data
            .column(4)
            .as_any()
            .downcast_ref::<Float64Array>()
            .unwrap();
        let tx = duck.transaction().unwrap();
        for r in 0..data.num_rows() {
            let n: Option<f64> = if ns.is_null(r) {
                None
            } else {
                Some(ns.value(r))
            };
            tx.execute(
                "INSERT INTO t VALUES (?, ?, ?, ?, ?)",
                duckdb::params![ids.value(r), gs.value(r), vs.value(r), ss.value(r), n],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }

    let queries = [
        "SELECT count(*) FROM t",
        "SELECT sum(v), min(v), max(v), avg(v) FROM t",
        "SELECT g, count(*), sum(v), avg(v), min(n), max(n) FROM t GROUP BY g ORDER BY g",
        "SELECT s, count(*), sum(v) FROM t GROUP BY s ORDER BY s",
        "SELECT count(DISTINCT s), count(DISTINCT g) FROM t",
        "SELECT count(*) FROM t WHERE v > 0",
        "SELECT sum(v), avg(v) FROM t WHERE v <= -50 OR v >= 50",
        "SELECT g, count(*) FROM t WHERE n IS NOT NULL GROUP BY g ORDER BY g",
        "SELECT count(*), sum(n) FROM t WHERE n IS NULL",
        "SELECT s, count(*) FROM t WHERE id BETWEEN 100 AND 200 GROUP BY s ORDER BY s",
        "SELECT count(*), avg(v) FROM t WHERE g = 3 AND s <> 'alpha'",
        "SELECT g, count(*), sum(v) FROM t GROUP BY g HAVING count(*) > 70 ORDER BY g",
        "SELECT count(*), sum(v), avg(v) FROM t WHERE v > 0 AND n IS NOT NULL AND id < 400",
    ];

    for q in queries {
        let less = normalize(less_rows(&session.sql_batches(q).await.unwrap()));
        let mut duck_stmt = duck.prepare(q).unwrap();
        let duck = normalize(duck_rows(&mut duck_stmt));
        assert_eq!(
            less.len(),
            duck.len(),
            "row count mismatch for {q}\nless={less:?}\nduck={duck:?}"
        );
        for (i, (l, d)) in less.iter().zip(duck.iter()).enumerate() {
            assert!(
                cells_equal(l, d),
                "row {i} mismatch for {q}\nless={l:?}\nduck={d:?}"
            );
        }
    }

    std::fs::remove_dir_all(&dir).ok();
}
