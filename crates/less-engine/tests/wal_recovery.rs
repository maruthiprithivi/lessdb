//! Malformed WAL recovery regression.
//! Process-exit tests leave the OS alive and are not physical power-loss proof.
use arrow::array::Int64Array;
use arrow::record_batch::RecordBatch;
use less_catalog::{EngineKind, FieldSpec, SchemaSpec, TableDef, TypeSpec};
use less_common::EngineConfig;
use less_engine::LessEngine;
use std::sync::Arc;

fn table() -> TableDef {
    TableDef::new(
        "events",
        SchemaSpec {
            fields: vec![FieldSpec::new("id", TypeSpec::Int64)],
        },
        EngineKind::Firefly,
    )
}
fn open(path: &std::path::Path) -> Arc<LessEngine> {
    let mut config = EngineConfig::with_data_dir(path);
    config.auto_merge_parts = usize::MAX;
    LessEngine::open(config).unwrap()
}
fn ids(engine: &LessEngine) -> Vec<i64> {
    let mut result = vec![];
    for part in engine.parts("events").unwrap() {
        for b in engine.read_data_part(&part).unwrap() {
            result.extend(
                b.column(0)
                    .as_any()
                    .downcast_ref::<Int64Array>()
                    .unwrap()
                    .values(),
            );
        }
    }
    result.sort();
    result
}

#[test]
fn malformed_wal_tail_cannot_hide_a_later_acknowledgement() {
    use std::io::Write;
    for keep_covered_record in [false, true] {
        let dir = std::env::temp_dir().join(format!("less-wal-reopen-{}", uuid::Uuid::new_v4()));
        let engine = open(&dir);
        assert!(engine.config.wal_fsync);
        engine.create_table(table()).unwrap();
        let batch = |id| {
            RecordBatch::try_new(
                table().arrow_schema(),
                vec![Arc::new(Int64Array::from(vec![id]))],
            )
            .unwrap()
        };
        engine.insert("events", batch(1)).unwrap();
        let wal_path = dir.join("wal/events.wal");
        let covered = std::fs::read(&wal_path).unwrap();
        engine.flush("events").unwrap();
        drop(engine);
        // Model a partial final append after the previous record is already
        // covered by a flushed part. The OS remains alive: this is not a power cut.
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&wal_path)
            .unwrap();
        if keep_covered_record {
            file.write_all(&covered).unwrap();
        }
        file.write_all(&[1]).unwrap();
        file.sync_all().unwrap();
        drop(file);
        let before = std::fs::read(&wal_path).unwrap();
        match LessEngine::open(EngineConfig::with_data_dir(&dir)) {
            Err(error) => {
                assert!(error.to_string().contains("invalid WAL"));
                assert_eq!(std::fs::read(&wal_path).unwrap(), before);
            }
            Ok(reopened) => {
                // This branch reproduces the old defect: a writable reopen
                // must not acknowledge a record hidden behind that tail.
                assert_eq!(reopened.insert("events", batch(2)).unwrap(), 1);
                drop(reopened);
                let recovered = open(&dir);
                assert_eq!(
                    ids(&recovered),
                    vec![1, 2],
                    "acknowledged row lost after reopen"
                );
                drop(recovered);
                panic!("malformed WAL unexpectedly allowed writable startup");
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
