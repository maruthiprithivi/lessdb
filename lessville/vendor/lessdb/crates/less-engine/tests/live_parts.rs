//! Deterministic live-generation regressions, including actual process exit.
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
fn seed(engine: &LessEngine) {
    engine.create_table(table()).unwrap();
    for id in [1, 2] {
        let batch = RecordBatch::try_new(
            table().arrow_schema(),
            vec![Arc::new(Int64Array::from(vec![id]))],
        )
        .unwrap();
        engine.insert("events", batch).unwrap();
        engine.flush("events").unwrap();
    }
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
fn merge_publication_never_exposes_both_generations() {
    let dir = std::env::temp_dir().join(format!("less-live-red-{}", uuid::Uuid::new_v4()));
    let engine = open(&dir);
    seed(&engine);
    let reader = engine.clone();
    let observed = Arc::new(std::sync::Mutex::new(None));
    let saved = observed.clone();
    // The replacement's meta.json exists, but the writer has not completed
    // its final parent barrier. Observe through the normal reader API there.
    let fault = less_common::durability::test_faults::fail_dir_sync_with(
        dir.join("parts/events"),
        3,
        std::io::ErrorKind::Other,
        move || *saved.lock().unwrap() = Some(ids(&reader)),
    );
    assert!(engine.optimize("events").is_err());
    drop(fault);
    assert_eq!(
        *observed.lock().unwrap(),
        Some(vec![1, 2]),
        "replacement and inputs must never both be live"
    );
    drop(engine);
    let reopened = open(&dir);
    assert_eq!(ids(&reopened), vec![1, 2]);
    drop(reopened);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn pinned_inputs_survive_merge_and_are_reclaimed_after_last_reader() {
    let dir = std::env::temp_dir().join(format!("less-live-pin-{}", uuid::Uuid::new_v4()));
    let engine = open(&dir);
    seed(&engine);
    let pinned = engine.parts("events").unwrap();
    let paths: Vec<_> = pinned
        .iter()
        .map(|p| match &p.location {
            less_engine::PartLocation::Local(path) => path.clone(),
            _ => unreachable!(),
        })
        .collect();
    engine.optimize("events").unwrap();
    assert_eq!(ids(&engine), vec![1, 2]);
    for part in &pinned {
        assert!(!engine.read_data_part(part).unwrap().is_empty());
    }
    assert!(paths.iter().all(|p| p.exists()));
    drop(engine);
    assert!(
        LessEngine::open_local(&dir).is_err(),
        "reader leases must retain directory ownership"
    );
    drop(pinned);
    assert!(paths.iter().all(|p| !p.exists()));
    let reopened = open(&dir);
    assert_eq!(ids(&reopened), vec![1, 2]);
    drop(reopened);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn failed_manifest_barrier_keeps_wal_for_either_recovery_image() {
    // Model either outcome of a rename whose parent sync failed. The OS stays
    // alive; this is not a physical power-loss experiment.
    for old_survives in [true, false] {
        let dir =
            std::env::temp_dir().join(format!("less-live-ambiguous-{}", uuid::Uuid::new_v4()));
        let engine = open(&dir);
        seed(&engine);
        let manifest = dir.join("parts/events/ACTIVE.json");
        let old = std::fs::read(&manifest).unwrap();
        let b = RecordBatch::try_new(
            table().arrow_schema(),
            vec![Arc::new(Int64Array::from(vec![3]))],
        )
        .unwrap();
        engine.insert("events", b).unwrap();
        let fault = less_common::durability::test_faults::fail_dir_sync_with(
            dir.join("parts/events"),
            3,
            std::io::ErrorKind::Other,
            || {},
        );
        assert!(engine.flush("events").is_err());
        drop(fault);
        assert_ne!(
            std::fs::read(&manifest).unwrap(),
            old,
            "failure must occur after manifest rename"
        );
        assert!(
            std::fs::metadata(dir.join("wal/events.wal")).unwrap().len() > 0,
            "ambiguous publication must not reclaim acknowledged WAL"
        );
        assert!(
            engine.parts("events").is_err(),
            "failed barrier must fail closed"
        );
        drop(engine);
        if old_survives {
            std::fs::write(&manifest, old).unwrap();
        }
        let reopened = open(&dir);
        assert_eq!(ids(&reopened), vec![1, 2, 3]);
        drop(reopened);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[tokio::test]
async fn delayed_scan_and_stream_keep_files_and_table_snapshot() {
    let dir = std::env::temp_dir().join(format!("less-live-plan-{}", uuid::Uuid::new_v4()));
    let engine = open(&dir);
    seed(&engine);
    let session = less_query::LessSession::new(engine.clone()).unwrap();
    let count_frame = session
        .sql("SELECT COUNT(*) AS n, MIN(id) AS lo, MAX(id) AS hi FROM events")
        .await
        .unwrap();
    let subquery_frame = session
        .sql("SELECT (SELECT COUNT(*) FROM events) AS a, (SELECT COUNT(*) FROM events) AS b")
        .await
        .unwrap();
    let frame = session
        .sql("SELECT id FROM events WHERE id >= 1 ORDER BY id")
        .await
        .unwrap();
    let plan = frame.create_physical_plan().await.unwrap();
    drop(frame);
    engine.optimize("events").unwrap();
    let third = RecordBatch::try_new(
        table().arrow_schema(),
        vec![Arc::new(Int64Array::from(vec![3]))],
    )
    .unwrap();
    engine.insert("events", third).unwrap();
    engine.flush("events").unwrap();
    let later = session
        .sql_batches("SELECT id FROM events ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        later.iter().map(|b| b.num_rows()).sum::<usize>(),
        3,
        "new statement must observe new committed parts"
    );
    let count = count_frame.collect().await.unwrap();
    assert_eq!(
        count[0]
            .column(0)
            .as_any()
            .downcast_ref::<arrow::array::Int64Array>()
            .unwrap()
            .value(0),
        2,
        "metadata aggregate must use the planned generation"
    );
    let subquery = subquery_frame.collect().await.unwrap();
    for column in [0, 1] {
        assert_eq!(
            subquery[0]
                .column(column)
                .as_any()
                .downcast_ref::<arrow::array::Int64Array>()
                .unwrap()
                .value(0),
            2,
            "expression subqueries must freeze the same generation"
        );
    }
    let stream = plan.execute(0, session.ctx().task_ctx()).unwrap();
    drop(plan);
    drop(session);
    drop(engine);
    assert!(
        LessEngine::open_local(&dir).is_err(),
        "stream alone must retain ownership"
    );
    use futures::TryStreamExt;
    let batches: Vec<RecordBatch> = stream.try_collect().await.unwrap();
    let values: Vec<i64> = batches
        .iter()
        .flat_map(|b| {
            b.column(0)
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap()
                .values()
                .iter()
                .copied()
        })
        .collect();
    assert_eq!(
        values,
        vec![1, 2],
        "old plan must keep its generation and files through execution"
    );
    let reopened = open(&dir);
    assert_eq!(ids(&reopened), vec![1, 2, 3]);
    drop(reopened);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ambiguous_legacy_and_corrupt_manifest_fail_without_deleting_files() {
    let dir = std::env::temp_dir().join(format!("less-live-legacy-{}", uuid::Uuid::new_v4()));
    let engine = open(&dir);
    seed(&engine);
    let parts = engine.parts("events").unwrap();
    let paths: Vec<_> = parts
        .iter()
        .map(|p| match &p.location {
            less_engine::PartLocation::Local(path) => path.clone(),
            _ => unreachable!(),
        })
        .collect();
    drop(parts);
    drop(engine);
    let manifest = dir.join("parts/events/ACTIVE.json");
    let saved = std::fs::read(&manifest).unwrap();
    std::fs::remove_file(&manifest).unwrap();
    assert!(LessEngine::open_local(&dir).is_err());
    assert!(paths.iter().all(|p| p.exists()));
    std::fs::write(&manifest, b"{ corrupt").unwrap();
    assert!(LessEngine::open_local(&dir).is_err());
    assert!(paths.iter().all(|p| p.exists()));
    std::fs::write(manifest, saved).unwrap();
    let reopened = open(&dir);
    assert_eq!(ids(&reopened), vec![1, 2]);
    drop(reopened);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn empty_generation_preserves_wal_cutoff_and_next_lsn() {
    let dir = std::env::temp_dir().join(format!("less-live-water-{}", uuid::Uuid::new_v4()));
    let engine = open(&dir);
    seed(&engine);
    engine.truncate("events").unwrap();
    drop(engine);
    let engine = open(&dir);
    let b = RecordBatch::try_new(
        table().arrow_schema(),
        vec![Arc::new(Int64Array::from(vec![3]))],
    )
    .unwrap();
    engine.insert("events", b).unwrap(); // WAL-only acknowledgment; recover it.
    drop(engine);
    let engine = open(&dir);
    assert_eq!(ids(&engine), vec![3]);
    drop(engine);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn process_crash_helper() {
    let Ok(path) = std::env::var("LESS_LIVE_CRASH_ROOT") else {
        return;
    };
    let occurrence: usize = std::env::var("LESS_LIVE_CRASH_SYNC")
        .unwrap()
        .parse()
        .unwrap();
    let engine = open(std::path::Path::new(&path));
    let _fault = less_common::durability::test_faults::fail_dir_sync_with(
        std::path::PathBuf::from(&path).join("parts/events"),
        occurrence,
        std::io::ErrorKind::Other,
        || std::process::exit(91),
    );
    engine.optimize("events").unwrap();
    panic!("crash point did not fire");
}

#[test]
fn process_restart_selects_one_generation_at_publication_commit_and_cleanup() {
    // Parent-directory barriers: 3 = replacement publication, 4 = ACTIVE
    // renamed (before directory sync), 5 = retired input cleanup. OS alive.
    for occurrence in [3, 4, 5] {
        let dir = std::env::temp_dir().join(format!(
            "less-live-exit-{occurrence}-{}",
            uuid::Uuid::new_v4()
        ));
        let engine = open(&dir);
        seed(&engine);
        drop(engine);
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "process_crash_helper", "--nocapture"])
            .env("LESS_LIVE_CRASH_ROOT", &dir)
            .env("LESS_LIVE_CRASH_SYNC", occurrence.to_string())
            .status()
            .unwrap();
        assert_eq!(
            status.code(),
            Some(91),
            "must reach the intended crash stage"
        );
        let engine = open(&dir);
        assert_eq!(ids(&engine), vec![1, 2]);
        drop(engine);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn concurrent_flush_compactions_and_readers_preserve_row_identity() {
    let dir = std::env::temp_dir().join(format!("less-live-concurrent-{}", uuid::Uuid::new_v4()));
    let engine = open(&dir);
    seed(&engine);
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    std::thread::scope(|scope| {
        let writer = engine.clone();
        let finished = done.clone();
        scope.spawn(move || {
            struct Finish(Arc<std::sync::atomic::AtomicBool>);
            impl Drop for Finish {
                fn drop(&mut self) {
                    self.0.store(true, std::sync::atomic::Ordering::Release);
                }
            }
            let _finish = Finish(finished);
            for id in 3..=66 {
                let b = RecordBatch::try_new(
                    table().arrow_schema(),
                    vec![Arc::new(Int64Array::from(vec![id]))],
                )
                .unwrap();
                writer.insert("events", b).unwrap();
                writer.flush("events").unwrap();
            }
        });
        for _ in 0..2 {
            let merge = engine.clone();
            let finished = done.clone();
            scope.spawn(move || {
                while !finished.load(std::sync::atomic::Ordering::Acquire) {
                    merge.optimize("events").unwrap();
                    std::thread::yield_now();
                }
            });
        }
        let reader = engine.clone();
        let finished = done.clone();
        scope.spawn(move || {
            while !finished.load(std::sync::atomic::Ordering::Acquire) {
                let visible = ids(&reader);
                assert!(!visible.is_empty());
                assert_eq!(
                    visible,
                    (1..=visible.len() as i64).collect::<Vec<_>>(),
                    "one active append prefix, never duplicate or missing rows"
                );
            }
        });
    });
    assert_eq!(ids(&engine), (1..=66).collect::<Vec<_>>());
    drop(engine);
    let engine = open(&dir);
    assert_eq!(ids(&engine), (1..=66).collect::<Vec<_>>());
    drop(engine);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn symlinked_table_container_cannot_redirect_startup_cleanup() {
    let root = std::env::temp_dir().join(format!("less-live-symlink-{}", uuid::Uuid::new_v4()));
    let engine = open(&root);
    engine.create_table(table()).unwrap();
    drop(engine);
    let external =
        std::env::temp_dir().join(format!("less-live-external-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(external.join("unrelated")).unwrap();
    std::fs::write(
        external.join("unrelated/keep"),
        b"owned by another database",
    )
    .unwrap();
    std::fs::remove_dir_all(root.join("parts/events")).unwrap();
    std::os::unix::fs::symlink(&external, root.join("parts/events")).unwrap();
    assert!(LessEngine::open_local(&root).is_err());
    assert_eq!(
        std::fs::read(external.join("unrelated/keep")).unwrap(),
        b"owned by another database"
    );
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(external).unwrap();
}
