// Diagnostic: are duplicate context rows stored duplicates or an IN-subquery artefact?
use less_common::config::EngineConfig;
use less_engine::LessEngine;
use less_query::LessSession;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let dir = std::env::args().nth(1).expect("data dir");
    let engine: Arc<LessEngine> =
        tokio::task::spawn_blocking(move || LessEngine::open(EngineConfig::with_data_dir(dir)))
            .await
            .unwrap()
            .unwrap();
    let s = LessSession::new_async(engine.clone()).await.unwrap();
    for sql in [
        "SELECT k, sim_minute, length(v) AS len FROM lv_kv",
        "SELECT id, sim_minute, owner, source_kind, source_id, tag FROM lv_memories WHERE id >= 12 ORDER BY id",
        "SELECT count(*) AS n FROM lv_messages",
    ] {
        println!("== {sql}");
        match s.sql_batches(sql).await {
            Ok(batches) => println!(
                "{}",
                arrow::util::pretty::pretty_format_batches(&batches).unwrap()
            ),
            Err(e) => println!("ERR {e}"),
        }
    }
}
