//! The engine-native statement layer: SHOW / DESCRIBE / USE / PRAGMA /
//! VACUUM served by the session, not the SQL planner.

use std::sync::Arc;

use less_catalog::{EngineKind, FieldSpec, SchemaSpec, TableDef, TypeSpec};
use less_common::EngineConfig;
use less_engine::LessEngine;
use less_query::LessSession;

static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

async fn setup() -> (Arc<LessEngine>, LessSession) {
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("less-native-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let engine = LessEngine::open(EngineConfig::with_data_dir(&dir)).unwrap();
    let mut def = TableDef::new(
        "events",
        SchemaSpec {
            fields: vec![
                FieldSpec::new("id", TypeSpec::Int64),
                FieldSpec::new("name", TypeSpec::Utf8),
                FieldSpec::new("ts", TypeSpec::TimestampMs),
            ],
        },
        EngineKind::Firefly,
    );
    def.sort_key = vec!["id".into(), "ts".into()];
    def.unique = vec!["id".into()];
    engine.create_table(def).unwrap();
    let session = LessSession::new_async(engine.clone()).await.unwrap();
    (engine, session)
}

async fn first_col(session: &LessSession, sql: &str) -> Vec<String> {
    session
        .sql_batches(sql)
        .await
        .unwrap()
        .iter()
        .flat_map(|b| {
            (0..b.num_rows())
                .map(|r| arrow::util::display::array_value_to_string(b.column(0), r).unwrap())
        })
        .collect()
}

#[tokio::test]
async fn show_databases_tables_and_filters() {
    let (_e, s) = setup().await;
    assert_eq!(first_col(&s, "SHOW DATABASES").await, vec!["default"]);
    assert_eq!(first_col(&s, "SHOW TABLES").await, vec!["events"]);
    assert_eq!(
        first_col(&s, "SHOW TABLES FROM default").await,
        vec!["events"]
    );
    assert_eq!(
        first_col(&s, "SHOW TABLES LIKE 'eve%'").await,
        vec!["events"]
    );
    assert!(s.sql_batches("SHOW TABLES FROM bogus").await.is_err());
}

#[tokio::test]
async fn show_create_table_renders_full_ddl() {
    let (_e, s) = setup().await;
    let ddl = first_col(&s, "SHOW CREATE TABLE events").await.join("\n");
    assert_eq!(
        ddl,
        "CREATE TABLE events (\n    id Int64,\n    name String,\n    ts DateTime\n) \
         ENGINE=Firefly ORDER BY (id, ts) UNIQUE (id)"
    );
}

#[tokio::test]
async fn show_columns_and_describe_forms() {
    let (_e, s) = setup().await;
    for sql in [
        "SHOW COLUMNS FROM events",
        "DESCRIBE events",
        "DESCRIBE TABLE events",
    ] {
        let batches = s.sql_batches(sql).await.unwrap();
        let names: Vec<String> = (0..batches[0].num_rows())
            .map(|r| arrow::util::display::array_value_to_string(batches[0].column(0), r).unwrap())
            .collect();
        assert_eq!(names, vec!["id", "name", "ts"]);
    }
}

#[tokio::test]
async fn use_and_pragma() {
    let (_e, s) = setup().await;
    assert!(s.sql_batches("USE default").await.is_ok());
    assert!(s.sql_batches("USE bogus").await.is_err());
    assert_eq!(
        first_col(&s, "PRAGMA version").await,
        vec![less_common::VERSION]
    );
}

#[tokio::test]
async fn vacuum_optimizes_all_tables() {
    let (_e, s) = setup().await;
    let out = first_col(&s, "VACUUM").await;
    assert_eq!(out, vec!["events"]);
}

#[tokio::test]
async fn alter_table_add_and_drop_column() {
    let (_e, s) = setup().await;

    // ADD COLUMN with a plain type
    let out = first_col(&s, "ALTER TABLE events ADD COLUMN score Float64").await;
    assert_eq!(out, vec!["added column 'score' to 'events' (4 columns)"]);
    let cols = s.sql_batches("SHOW COLUMNS FROM events").await.unwrap();
    assert_eq!(cols[0].num_rows(), 4);
    assert_eq!(
        arrow::util::display::array_value_to_string(cols[0].column(1), 3).unwrap(),
        "Float64"
    );

    // ADD COLUMN with a trailing DEFAULT modifier is accepted (modifier
    // not yet enforced, type parsed)
    let out = first_col(&s, "ALTER TABLE events ADD COLUMN n Int64 DEFAULT 0").await;
    assert_eq!(out, vec!["added column 'n' to 'events' (5 columns)"]);

    // duplicate column → clear error
    let err = s
        .sql_batches("ALTER TABLE events ADD COLUMN score Int32")
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("already exists"), "{err}");

    // dropping a sort-key column is refused
    let err = s
        .sql_batches("ALTER TABLE events DROP COLUMN id")
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("sort key"), "{err}");

    // dropping a plain column works
    let out = first_col(&s, "ALTER TABLE events DROP COLUMN score").await;
    assert_eq!(
        out,
        vec!["dropped column 'score' from 'events' (4 columns)"]
    );
    let err = s
        .sql_batches("ALTER TABLE events DROP COLUMN score")
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("does not exist"), "{err}");

    // unknown ALTER forms get a friendly pointer, not a DF stack
    let err = s
        .sql_batches("ALTER TABLE events RENAME COLUMN n TO m")
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("ADD COLUMN and DROP COLUMN"), "{err}");

    // bad type is reported with the column context
    let err = s
        .sql_batches("ALTER TABLE events ADD COLUMN q WhatIsThis")
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("ALTER TABLE events ADD COLUMN q"), "{err}");
}

#[tokio::test]
async fn truncate_table_empties_and_keeps_schema() {
    let (_e, s) = setup().await;
    s.sql_batches("INSERT INTO events (id, name) VALUES (1, 'a')")
        .await
        .unwrap();
    assert_eq!(
        first_col(&s, "SELECT count(*) FROM events").await,
        vec!["1"]
    );

    let out = first_col(&s, "TRUNCATE TABLE events").await;
    assert_eq!(out, vec!["truncated 'events'"]);
    assert_eq!(
        first_col(&s, "SELECT count(*) FROM events").await,
        vec!["0"]
    );
    // schema survives
    let cols = s.sql_batches("SHOW COLUMNS FROM events").await.unwrap();
    assert_eq!(cols[0].num_rows(), 3);
    // the table still takes writes
    s.sql_batches("INSERT INTO events (id, name) VALUES (2, 'b')")
        .await
        .unwrap();
    assert_eq!(
        first_col(&s, "SELECT count(*) FROM events").await,
        vec!["1"]
    );
    // unknown table
    let err = s
        .sql_batches("TRUNCATE bogus")
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("bogus"), "{err}");
}

#[tokio::test]
async fn show_processlist_reports_the_session() {
    let (_e, s) = setup().await;
    let batches = s.sql_batches("SHOW PROCESSLIST").await.unwrap();
    assert_eq!(batches[0].num_rows(), 1);
    assert_eq!(batches[0].schema().field(0).name(), "pid");
    assert_eq!(batches[0].schema().field(1).name(), "state");
    assert_eq!(batches[0].schema().field(2).name(), "query");
    assert_eq!(
        arrow::util::display::array_value_to_string(batches[0].column(1), 0).unwrap(),
        "running"
    );
}

#[tokio::test]
async fn transactions_attach_and_views_fail_with_friendly_errors() {
    let (_e, s) = setup().await;
    for sql in [
        "BEGIN",
        "START TRANSACTION",
        "COMMIT",
        "ROLLBACK",
        "END",
        "ATTACH 'other.db'",
        "DETACH other",
        "CREATE VIEW vw AS SELECT * FROM events",
        "CREATE OR REPLACE MATERIALIZED VIEW mv AS SELECT * FROM events",
    ] {
        let err = s.sql_batches(sql).await.unwrap_err().to_string();
        assert!(
            err.contains("atomic")
                || err.contains("not supported")
                || err.contains("on the roadmap"),
            "{sql} → {err}"
        );
    }
    // …but CREATE TABLE AS SELECT still works (not swallowed by the view check)
    s.sql_batches("INSERT INTO events (id, name) VALUES (1, 'a')")
        .await
        .unwrap();
    s.sql_batches("CREATE TABLE copy AS SELECT * FROM events")
        .await
        .unwrap();
    assert_eq!(first_col(&s, "SHOW TABLES").await, vec!["copy", "events"]);
}

#[tokio::test]
async fn create_and_drop_table_persist_through_the_catalog() {
    let (_e, s) = setup().await;

    // Schema-DDL form with LessDB flavor
    let out = first_col(
        &s,
        "CREATE TABLE pageviews (page String, hits Int64) ENGINE=Firefly ORDER BY (page)",
    )
    .await;
    assert_eq!(out, vec!["created table 'pageviews'"]);
    assert_eq!(
        first_col(&s, "SHOW TABLES").await,
        vec!["events", "pageviews"]
    );

    // DDL persists: a fresh session over the same engine still sees it
    let ddl = first_col(&s, "SHOW CREATE TABLE pageviews")
        .await
        .join("\n");
    assert!(ddl.contains("pageviews"), "{ddl}");

    // Duplicate CREATE errors
    let err = s
        .sql_batches("CREATE TABLE pageviews (x Int64)")
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("already exists"), "{err}");

    // IF NOT EXISTS is a friendly no-op
    let out = first_col(&s, "CREATE TABLE IF NOT EXISTS pageviews (x Int64)").await;
    assert!(out[0].contains("already exists"), "{out:?}");

    // CTAS persists through the catalog (not a volatile memory table)
    s.sql_batches("INSERT INTO events (id, name) VALUES (1, 'a'), (2, 'b')")
        .await
        .unwrap();
    let out = first_col(
        &s,
        "CREATE TABLE agg AS SELECT name, count(*) AS n FROM events GROUP BY name ORDER BY name",
    )
    .await;
    assert_eq!(out, vec!["created table 'agg' from SELECT (2 rows)"]);
    assert_eq!(
        first_col(&s, "SELECT n FROM agg ORDER BY name").await,
        vec!["1", "1"]
    );

    // DROP TABLE removes the table and its data
    let out = first_col(&s, "DROP TABLE agg").await;
    assert_eq!(out, vec!["dropped table 'agg'"]);
    assert!(s.sql_batches("SELECT * FROM agg").await.is_err());

    // DROP TABLE IF EXISTS is a friendly no-op for missing tables
    let out = first_col(&s, "DROP TABLE IF EXISTS nope").await;
    assert!(out[0].contains("does not exist"), "{out:?}");
    // …and a real error without IF EXISTS
    let err = s
        .sql_batches("DROP TABLE nope")
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("nope"), "{err}");
}

#[tokio::test]
async fn read_table_functions_register_files() {
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("less-readfn-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (_e, s) = setup().await;

    // read_csv + join with a real table
    let csv = dir.join("a.csv");
    std::fs::write(&csv, "id,name\n1,alpha\n2,beta\n").unwrap();
    let batches = s
        .sql_batches(&format!(
            "SELECT name FROM read_csv('{}') ORDER BY id",
            csv.display()
        ))
        .await
        .unwrap();
    assert_eq!(
        arrow::util::display::array_value_to_string(batches[0].column(0), 0).unwrap(),
        "alpha"
    );

    // multi-path call → UNION ALL
    let csv2 = dir.join("b.csv");
    std::fs::write(&csv2, "id,name\n3,gamma\n").unwrap();
    let batches = s
        .sql_batches(&format!(
            "SELECT count(*) FROM read_csv('{}', '{}')",
            csv.display(),
            csv2.display()
        ))
        .await
        .unwrap();
    assert_eq!(
        arrow::util::display::array_value_to_string(batches[0].column(0), 0).unwrap(),
        "3"
    );

    // read_parquet after exporting with COPY TO
    let pq = dir.join("t.parquet");
    s.sql_batches(&format!(
        "COPY (SELECT 1 AS a, 'x' AS b) TO '{}'",
        pq.display()
    ))
    .await
    .unwrap();
    let batches = s
        .sql_batches(&format!("SELECT a FROM read_parquet('{}')", pq.display()))
        .await
        .unwrap();
    assert_eq!(
        arrow::util::display::array_value_to_string(batches[0].column(0), 0).unwrap(),
        "1"
    );

    // missing file → clear error naming the function
    let err = s
        .sql_batches(&format!(
            "SELECT * FROM read_parquet('{}')",
            dir.join("nope.parquet").display()
        ))
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("read_parquet"), "{err}");

    // options are refused explicitly, not mistaken for paths
    let err = s
        .sql_batches(&format!(
            "SELECT * FROM read_csv('{}', header = false)",
            csv.display()
        ))
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("options are not supported"), "{err}");

    let _ = std::fs::remove_dir_all(&dir);
}
