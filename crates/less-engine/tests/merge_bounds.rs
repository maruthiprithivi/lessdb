//! Bounded-merge regression tests.
//!
//! A large analytical load flushes hundreds of same-sized parts; a naive
//! optimize used to read *all* of them into memory as sorted runs and was
//! OOM-killed at 100M rows. [`less_engine::merge::merge_all`] now merges in
//! repeated passes capped at `EngineConfig::max_merge_rows` input rows and
//! refuses merges whose two smallest candidates exceed the budget. These
//! tests pin that behavior at a small scale.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::array::{Float64Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow::record_batch::RecordBatch;

use less_catalog::{EngineKind, FieldSpec, SchemaSpec, TableDef, TypeSpec};
use less_common::EngineConfig;
use less_engine::LessEngine;

fn temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("less-merge-bounds-{tag}-{}", uuid::Uuid::new_v4()))
}

fn schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("city", DataType::Utf8, false),
        Field::new("amount", DataType::Float64, false),
    ]))
}

fn def() -> TableDef {
    let mut d = TableDef::new(
        "t",
        SchemaSpec {
            fields: vec![
                FieldSpec::new("id", TypeSpec::Int64),
                FieldSpec::new("city", TypeSpec::Utf8),
                FieldSpec::new("amount", TypeSpec::Float64),
            ],
        },
        EngineKind::Firefly,
    );
    d.sort_key = vec!["id".into()];
    d
}

fn batch(n: i64, rows: usize) -> RecordBatch {
    let ids: Vec<i64> = (0..rows as i64).map(|i| n * 100_000 + i).collect();
    let cities: Vec<&str> = (0..rows)
        .map(|i| if i % 2 == 0 { "nyc" } else { "sf" })
        .collect();
    let amounts: Vec<f64> = (0..rows).map(|i| i as f64 * 0.5).collect();
    RecordBatch::try_new(
        schema(),
        vec![
            Arc::new(Int64Array::from(ids)),
            Arc::new(StringArray::from(cities)),
            Arc::new(Float64Array::from(amounts)),
        ],
    )
    .unwrap()
}

fn open_engine(dir: &Path) -> Arc<LessEngine> {
    let config = EngineConfig {
        data_dir: dir.to_path_buf(),
        flush_rows: 256,
        max_buffer_rows: 4096,
        max_merge_rows: 1000,
        auto_merge_parts: usize::MAX, // no inline merges during insert
        ..EngineConfig::default()
    };
    LessEngine::open(config).unwrap()
}

fn part_sizes(engine: &LessEngine, table: &str) -> Vec<u64> {
    let mut sizes: Vec<u64> = engine
        .parts(table)
        .unwrap()
        .iter()
        .map(|p| p.meta.row_count)
        .collect();
    sizes.sort();
    sizes
}

/// 40 flushes of 256 rows (one flush per insert) with a 1000-row merge
/// budget: passes merge 3 parts at a time (768 rows), then refuse to merge
/// two 768-row parts (1536 > budget). The table converges to
/// 13 × 768 + 1 × 256 = 14 parts with every row preserved, and no pass ever
/// materializes more than the budget.
#[test]
fn optimize_converges_in_bounded_passes() {
    let dir = temp_dir("converge");
    let engine = open_engine(&dir);
    engine.create_table(def()).unwrap();
    for n in 0..40 {
        engine.insert("t", batch(n, 256)).unwrap(); // triggers a flush per insert
    }
    engine.flush("t").unwrap();
    assert_eq!(part_sizes(&engine, "t").len(), 40);

    let merged = engine.optimize("t").unwrap();
    assert!(merged.is_some(), "optimize should merge something");

    let sizes = part_sizes(&engine, "t");
    assert_eq!(
        sizes,
        vec![
            256, 768, 768, 768, 768, 768, 768, 768, 768, 768, 768, 768, 768, 768
        ]
    );
    let total: u64 = sizes.iter().sum();
    assert_eq!(total, 40 * 256);
    assert_eq!(engine.stats("t").unwrap().rows, 40 * 256);

    // Idempotent: a second optimize has nothing eligible left.
    assert!(engine.optimize("t").unwrap().is_none());
}

/// Two parts that individually exceed the merge budget must not be merged
/// (a non-streaming merge of both would blow the budget); optimize leaves
/// them alone instead of OOM-ing.
#[test]
fn oversized_parts_are_left_unmerged() {
    let dir = temp_dir("oversized");
    let engine = open_engine(&dir);
    engine.create_table(def()).unwrap();
    engine.insert("t", batch(0, 2000)).unwrap();
    engine.flush("t").unwrap();
    engine.insert("t", batch(1, 2000)).unwrap();
    engine.flush("t").unwrap();
    assert_eq!(part_sizes(&engine, "t"), vec![2000, 2000]);

    assert!(engine.optimize("t").unwrap().is_none());
    assert_eq!(part_sizes(&engine, "t"), vec![2000, 2000]);
    assert_eq!(engine.stats("t").unwrap().rows, 4000);
}

/// With a budget that fits, the two-smallest fallback path still merges
/// (parts across *different* tiers: no tier holds two parts).
#[test]
fn fallback_merges_two_smallest_within_budget() {
    let dir = temp_dir("fallback");
    let engine = open_engine(&dir);
    engine.create_table(def()).unwrap();
    // 100-row part (tier 6) + 500-row part (tier 8): no tier has two parts.
    engine.insert("t", batch(0, 100)).unwrap();
    engine.flush("t").unwrap();
    engine.insert("t", batch(1, 500)).unwrap(); // 500 >= flush_rows → auto-flush
    assert_eq!(part_sizes(&engine, "t"), vec![100, 500]);

    // 100 + 500 = 600 <= 1000 budget → merged into one 600-row part.
    assert!(engine.optimize("t").unwrap().is_some());
    assert_eq!(part_sizes(&engine, "t"), vec![600]);
    assert_eq!(engine.stats("t").unwrap().rows, 600);
}
