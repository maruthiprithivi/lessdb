//! End-to-end fan-out: two real HTTP servers over one shared root.

use std::sync::Arc;

use arrow::array::{Float64Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use less_catalog::{EngineKind, FieldSpec, SchemaSpec, TableDef, TypeSpec};
use less_common::EngineConfig;
use less_engine::LessEngine;
use less_query::LessSession;

use less_fanout::fanout;

fn batch(ids: Vec<i64>, kinds: Vec<&str>, amounts: Vec<f64>) -> arrow::record_batch::RecordBatch {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("kind", DataType::Utf8, false),
        Field::new("v", DataType::Float64, false),
    ]));
    arrow::record_batch::RecordBatch::try_new(
        schema,
        vec![
            Arc::new(Int64Array::from(ids)),
            Arc::new(StringArray::from(kinds)),
            Arc::new(Float64Array::from(amounts)),
        ],
    )
    .unwrap()
}

/// Start a real HTTP server on an ephemeral port; returns its base URL.
async fn spawn_node(engine: Arc<LessEngine>) -> String {
    let session = LessSession::new_async(engine).await.unwrap();
    let app = less_server::router(less_server::AppState {
        session: Arc::new(session),
        auth: None,
        mcp: None,
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn fanout_across_two_nodes_matches_direct() {
    let base = std::env::temp_dir().join(format!("less-fanout-{}", uuid::Uuid::new_v4()));
    let shared_root = base.join("shared");
    let url = format!("file://{}", shared_root.display());

    // Node A creates the table and writes four parts.
    let a = LessEngine::open(EngineConfig::with_shared_url(base.join("node-a"), &url)).unwrap();
    let def = TableDef::new(
        "ft",
        SchemaSpec {
            fields: vec![
                FieldSpec::new("id", TypeSpec::Int64),
                FieldSpec::new("kind", TypeSpec::Utf8),
                FieldSpec::new("v", TypeSpec::Float64),
            ],
        },
        EngineKind::FireflyCloud,
    );
    a.create_table(def).unwrap();
    for round in 0..4 {
        let ids: Vec<i64> = (0..10).map(|k| round * 10 + k).collect();
        let kinds: Vec<&str> = ids
            .iter()
            .map(|i| if i % 2 == 0 { "even" } else { "odd" })
            .collect();
        let vals: Vec<f64> = ids.iter().map(|i| *i as f64).collect();
        a.insert("ft", batch(ids, kinds, vals)).unwrap();
        a.flush("ft").unwrap();
    }
    let a = a;

    // Node B is a fresh compute node on the same bucket.
    let b = LessEngine::open(EngineConfig::with_shared_url(base.join("node-b"), &url)).unwrap();

    let node_a = spawn_node(a).await;
    let node_b = spawn_node(b).await;
    let nodes = vec![node_a, node_b];

    // Direct reference result from a third engine.
    let direct_engine =
        LessEngine::open(EngineConfig::with_shared_url(base.join("node-c"), &url)).unwrap();
    let direct_session = LessSession::new_async(direct_engine).await.unwrap();
    let expected = direct_session
        .sql_batches(
            "SELECT kind, count(*) AS n, sum(v) AS total FROM ft GROUP BY kind ORDER BY kind",
        )
        .await
        .unwrap();

    // Fan out the same query across the two nodes.
    let got = fanout(
        &nodes,
        "SELECT kind, count(*) AS n, sum(v) AS total FROM ft GROUP BY kind ORDER BY kind",
    )
    .await
    .unwrap();

    assert_eq!(got.len(), expected.len());
    assert_eq!(got[0].num_rows(), 2, "even and odd groups");
    assert_eq!(
        arrow::util::pretty::pretty_format_batches(&got)
            .unwrap()
            .to_string(),
        arrow::util::pretty::pretty_format_batches(&expected)
            .unwrap()
            .to_string(),
        "fan-out result must match the direct query"
    );

    // Simple count across both nodes.
    let counts = fanout(&nodes, "SELECT count(*) FROM ft").await.unwrap();
    let n = counts[0]
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .unwrap()
        .value(0);
    assert_eq!(n, 40);

    std::fs::remove_dir_all(&base).ok();
}
