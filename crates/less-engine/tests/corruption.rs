//! Corruption & crash-safety tests for the LessDB engine.
//!
//! These tests simulate the failures a Firefly storage engine must survive
//! without panicking or silently corrupting data:
//!
//! * a part's `data.parquet` corrupted (garbage bytes),
//! * a part's `meta.json` corrupted (invalid JSON),
//! * a torn part write (only one of the two part files present),
//! * a torn/garbage WAL tail record after a good record,
//! * a kill -9 style crash with buffered rows (multiple cycles),
//! * stray files dropped into the parts directory.
//!
//! Every failure must surface as a clean `Result` error (or a graceful skip),
//! never a panic, and previously durable data must survive.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::array::{Float64Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow::record_batch::RecordBatch;

use less_catalog::{EngineKind, FieldSpec, SchemaSpec, TableDef, TypeSpec};
use less_common::EngineConfig;
use less_engine::LessEngine;
use less_query::LessSession;
use less_storage::{DATA_FILE, META_FILE};

fn temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("less-corruption-{tag}-{}", uuid::Uuid::new_v4()))
}

fn schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("city", DataType::Utf8, false),
        Field::new("amount", DataType::Float64, false),
    ]))
}

fn batch(ids: Vec<i64>, cities: Vec<&str>, amounts: Vec<f64>) -> RecordBatch {
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

/// A Firefly table definition. `unique = false` disables the uniqueness
/// collapse so row counts are exact and predictable.
fn def(name: &str, unique: bool) -> TableDef {
    let mut d = TableDef::new(
        name,
        SchemaSpec {
            fields: vec![
                FieldSpec::new("id", TypeSpec::Int64),
                FieldSpec::new("city", TypeSpec::Utf8),
                FieldSpec::new("amount", TypeSpec::Float64),
            ],
        },
        EngineKind::Firefly,
    );
    if unique {
        d.sort_key = vec!["city".into(), "id".into()];
        d.unique = vec!["city".into()];
    }
    d
}

fn open_engine(dir: &Path) -> Arc<LessEngine> {
    LessEngine::open(EngineConfig::with_data_dir(dir)).unwrap()
}

/// The single part directory under `<data_dir>/parts/<table>/`.
fn single_part_dir(dir: &Path, table: &str) -> PathBuf {
    let parts_dir = dir.join("parts").join(table);
    let entries: Vec<PathBuf> = std::fs::read_dir(&parts_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    assert_eq!(entries.len(), 1, "expected exactly one part: {entries:?}");
    entries[0].clone()
}

/// Encode a record batch as an Arrow IPC stream (the WAL payload format).
fn ipc_bytes(batch: &RecordBatch) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut writer =
            arrow::ipc::writer::StreamWriter::try_new(&mut buf, &batch.schema()).unwrap();
        writer.write(batch).unwrap();
        writer.finish().unwrap();
    }
    buf
}

/// Append a raw WAL record `[len u64 LE][lsn u64 LE][payload]`.
fn append_wal_record(path: &Path, lsn: u64, payload: &[u8]) {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    f.write_all(&(payload.len() as u64).to_le_bytes()).unwrap();
    f.write_all(&lsn.to_le_bytes()).unwrap();
    f.write_all(payload).unwrap();
}

// ---- 1. corrupt part data -------------------------------------------------

#[tokio::test]
async fn corrupt_part_data_surfaces_clean_query_error() {
    let dir = temp_dir("data");
    let engine = open_engine(&dir);
    engine.create_table(def("t", false)).unwrap();
    engine
        .insert("t", batch(vec![1, 2], vec!["a", "b"], vec![1.0, 2.0]))
        .unwrap();
    engine.flush("t").unwrap();

    // Corrupt data.parquet in place, keeping its byte length so the reader's
    // footer probe fails on a bad magic/footer rather than a range error.
    let part = single_part_dir(&dir, "t");
    let data_file = part.join(DATA_FILE);
    let len = std::fs::metadata(&data_file).unwrap().len();
    std::fs::write(&data_file, vec![0xABu8; len as usize]).unwrap();

    let session = LessSession::new_async(engine.clone()).await.unwrap();
    let res = session.sql_batches("SELECT * FROM t").await;
    assert!(
        res.is_err(),
        "corrupt data.parquet must surface a clean query error, got: {res:?}"
    );

    // The process survived: the engine and its metadata listing still work.
    assert_eq!(engine.parts("t").unwrap().len(), 1);
    std::fs::remove_dir_all(&dir).ok();
}

// ---- 2. corrupt part metadata --------------------------------------------

#[tokio::test]
async fn corrupt_part_meta_returns_error_not_panic() {
    let dir = temp_dir("meta");
    let engine = open_engine(&dir);
    engine.create_table(def("t", false)).unwrap();
    engine
        .insert("t", batch(vec![1], vec!["a"], vec![1.0]))
        .unwrap();
    engine.flush("t").unwrap();

    let part = single_part_dir(&dir, "t");
    std::fs::write(part.join(META_FILE), b"this is not json{{{").unwrap();

    // Listing the part fails cleanly (the part is unreadable, so the table
    // errors) — never a panic.
    assert!(
        engine.parts("t").is_err(),
        "corrupt meta.json must not panic"
    );

    // The SQL path surfaces the same clean error.
    let session = LessSession::new_async(engine.clone()).await.unwrap();
    let res = session.sql_batches("SELECT * FROM t").await;
    assert!(res.is_err(), "query over corrupt meta must error cleanly");
    std::fs::remove_dir_all(&dir).ok();
}

// ---- 3. partial (torn) part write ----------------------------------------

#[test]
fn torn_part_with_data_only_is_skipped_and_swept() {
    let dir = temp_dir("torn-data");
    let engine = open_engine(&dir);
    engine.create_table(def("t", false)).unwrap();
    engine
        .insert("t", batch(vec![1], vec!["a"], vec![1.0]))
        .unwrap();
    engine.flush("t").unwrap();

    // Simulate a crash mid-write: `write_part` writes data.parquet first and
    // meta.json last, so a torn part is a directory holding only data.parquet
    // (or nothing at all — the crash can land right after mkdir). The
    // meta-less part must never be visible: listing skips it gracefully.
    let torn = dir
        .join("parts")
        .join("t")
        .join("9999999999999999_deadbeef_1_0");
    std::fs::create_dir_all(&torn).unwrap();
    std::fs::write(torn.join(DATA_FILE), b"partial parquet bytes").unwrap();

    let parts = engine.parts("t").unwrap();
    assert_eq!(parts.len(), 1, "torn meta-less part must not be visible");
    assert_eq!(engine.stats("t").unwrap().rows, 1);

    // A reopen sweeps the uncommitted directory away entirely.
    let engine2 = LessEngine::open(EngineConfig::with_data_dir(&dir)).unwrap();
    assert_eq!(engine2.parts("t").unwrap().len(), 1);
    assert!(!torn.exists(), "open must sweep the torn part dir");
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn torn_part_with_meta_only_errors_on_read() {
    let dir = temp_dir("torn-meta");
    let engine = open_engine(&dir);
    engine.create_table(def("t", false)).unwrap();
    engine
        .insert("t", batch(vec![1], vec!["a"], vec![1.0]))
        .unwrap();
    engine.flush("t").unwrap();

    // A part directory holding only meta.json (data file missing). Since the
    // meta is valid it is listed, but reading its rows must fail cleanly.
    let torn = dir
        .join("parts")
        .join("t")
        .join("9999999999999999_c0ffee_1_0");
    std::fs::create_dir_all(&torn).unwrap();
    let meta = less_storage::PartMeta {
        id: "c0ffee".into(),
        name: "9999999999999999_c0ffee_1_0".into(),
        table: "t".into(),
        row_count: 1,
        created_at: "2020-01-01T00:00:00Z".into(),
        compression: "zstd".into(),
        sort_key: vec![],
        unique: vec![],
        columns: vec![],
        wal_lsn_max: None,
    };
    std::fs::write(
        torn.join(META_FILE),
        serde_json::to_string_pretty(&meta).unwrap(),
    )
    .unwrap();

    // The torn part is listed (it has valid meta) but has no data file.
    let parts = engine.parts("t").unwrap();
    assert!(parts.len() >= 2, "expected the torn part to be listed");

    let session = LessSession::new_async(engine.clone()).await.unwrap();
    let res = session.sql_batches("SELECT * FROM t").await;
    assert!(
        res.is_err(),
        "a part with no data.parquet must surface a clean error on read"
    );
    std::fs::remove_dir_all(&dir).ok();
}

// ---- 4. WAL torn/garbage tail record -------------------------------------

#[test]
fn wal_garbage_tail_is_ignored_and_good_records_recovered() {
    let dir = temp_dir("wal-garbage");
    let engine = open_engine(&dir);
    engine.create_table(def("t", false)).unwrap();
    engine
        .insert("t", batch(vec![1, 2], vec!["a", "b"], vec![1.0, 2.0]))
        .unwrap();
    engine.flush("t").unwrap(); // part durable at lsn 1; WAL truncated

    // Append a fresh good record (lsn 2) followed by a garbage tail record
    // (valid length prefix + lsn, but the payload is not an IPC stream).
    let wal_path = dir.join("wal").join("t.wal");
    append_wal_record(
        &wal_path,
        2,
        &ipc_bytes(&batch(vec![3], vec!["c"], vec![3.0])),
    );
    append_wal_record(&wal_path, 3, b"not-an-arrow-ipc-stream");
    drop(engine);

    // Reopen: the good record (lsn 2 > max flushed 1) must be recovered and
    // the garbage tail ignored, without failing the whole open.
    let engine = open_engine(&dir);
    assert_eq!(
        engine.stats("t").unwrap().rows,
        3,
        "flushed 2 + recovered 1"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn wal_bogus_length_prefix_does_not_panic() {
    let dir = temp_dir("wal-len");
    let engine = open_engine(&dir);
    engine.create_table(def("t", false)).unwrap();
    engine
        .insert("t", batch(vec![1, 2], vec!["a", "b"], vec![1.0, 2.0]))
        .unwrap();
    engine.flush("t").unwrap();

    // Good record, then a bogus length prefix (u64::MAX) with no payload.
    let wal_path = dir.join("wal").join("t.wal");
    append_wal_record(
        &wal_path,
        2,
        &ipc_bytes(&batch(vec![3], vec!["c"], vec![3.0])),
    );
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&wal_path)
            .unwrap();
        f.write_all(&u64::MAX.to_le_bytes()).unwrap();
    }
    drop(engine);

    // Without a length guard this would attempt an absurd allocation and
    // panic; the torn tail must instead be ignored.
    let engine = open_engine(&dir);
    assert_eq!(engine.stats("t").unwrap().rows, 3);
    std::fs::remove_dir_all(&dir).ok();
}

// ---- 5. kill -9 simulation (multiple crash cycles) -----------------------

#[test]
fn wal_recovers_across_multiple_crash_cycles() {
    let dir = temp_dir("crash");
    {
        let engine = open_engine(&dir);
        engine.create_table(def("t", false)).unwrap();
        engine
            .insert(
                "t",
                batch(vec![1, 2, 3], vec!["a", "b", "c"], vec![1.0, 2.0, 3.0]),
            )
            .unwrap();
        // No flush: drop the engine out from under the buffered rows.
    }

    let engine = open_engine(&dir);
    assert_eq!(engine.stats("t").unwrap().rows, 3, "cycle 1: WAL replay");

    // Second crash cycle: buffer more rows, crash again.
    engine
        .insert("t", batch(vec![4, 5], vec!["d", "e"], vec![4.0, 5.0]))
        .unwrap();
    drop(engine);

    let engine = open_engine(&dir);
    assert_eq!(
        engine.stats("t").unwrap().rows,
        5,
        "cycle 2: cumulative rows must survive both crashes"
    );
    std::fs::remove_dir_all(&dir).ok();
}

// ---- 6. stray files in the parts directory -------------------------------

#[test]
fn stray_files_in_parts_dir_are_ignored() {
    let dir = temp_dir("junk");
    let engine = open_engine(&dir);
    engine.create_table(def("t", false)).unwrap();
    engine
        .insert("t", batch(vec![1, 2], vec!["a", "b"], vec![1.0, 2.0]))
        .unwrap();
    engine.flush("t").unwrap();

    let parts_dir = dir.join("parts").join("t");
    let before = engine.parts("t").unwrap();
    assert_eq!(before.len(), 1);

    // Stray non-directory entries must not be mistaken for parts.
    std::fs::write(parts_dir.join("tmp-abc"), b"partial tmp file").unwrap();
    std::fs::write(parts_dir.join(".DS_Store"), b"mac junk").unwrap();
    std::fs::write(parts_dir.join("foo.txt"), b"hello").unwrap();

    let after = engine.parts("t").unwrap();
    assert_eq!(after.len(), 1, "stray files must be ignored: {after:?}");
    assert_eq!(after[0].meta.name, before[0].meta.name);
    std::fs::remove_dir_all(&dir).ok();
}
