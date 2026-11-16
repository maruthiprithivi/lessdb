//! lessville-agentd — Lessville's agent world, persisted in embedded LessDB.

mod agent_session;
mod checkpoint;
mod context;
mod llm;
mod physical;
mod presence;
mod retention;
#[cfg(test)]
mod scenarios;
mod security;
mod sim;
mod store;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use less_common::config::EngineConfig;
use less_engine::LessEngine;
use less_query::LessSession;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{Mutex, RwLock, mpsc};
use tower_http::cors::CorsLayer;

use context::{RecallIntent, recall};
use security::SecurityWorld;
use sim::{DAY, Sim, TABLES, World, clock};
use store::{Store, i, q, s};

#[derive(Clone)]
struct App {
    sim: Arc<Mutex<Sim>>,
    security: Arc<Mutex<SecurityWorld>>,
    store: Arc<Store>,
    snap: Arc<RwLock<Value>>,
    data_dir: Arc<PathBuf>,
    public: bool,
    llm: Arc<Mutex<llm::Status>>,
    ckpt: Arc<Mutex<checkpoint::Status>>,
}

const SOURCE: &str = "LessDB private-archive candidate 84ae452 (WAL fail-closed, durable live parts, ownership) + reconstructed less-agent contracts (embedded, single local owner)";

#[tokio::main]
async fn main() {
    let data_dir =
        PathBuf::from(std::env::var("LESSVILLE_DATA").unwrap_or_else(|_| "/data".into()));
    let port: u16 = std::env::var("LESSVILLE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8790);
    std::fs::create_dir_all(&data_dir).expect("create data dir");
    let public = std::env::var("LESSVILLE_PUBLIC")
        .map(|v| v == "1")
        .unwrap_or(false);
    let r2 = checkpoint::from_env();
    let mut ckpt_status = checkpoint::Status {
        enabled: r2.is_some(),
        interval_secs: std::env::var("LESSVILLE_CHECKPOINT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(120),
        ..Default::default()
    };
    if let Some(r2) = &r2 {
        match r2.restore(&data_dir) {
            Ok(Some(note)) => {
                eprintln!("{note}");
                ckpt_status.restored = Some(note);
            }
            Ok(None) => {
                eprintln!("No R2 checkpoint restored (none yet, or data dir already populated)")
            }
            Err(e) => {
                eprintln!("FATAL: {e}");
                std::process::exit(2);
            }
        }
    }
    let dir = data_dir.clone();
    let engine: Arc<LessEngine> =
        tokio::task::spawn_blocking(move || LessEngine::open(EngineConfig::with_data_dir(dir)))
            .await
            .expect("join")
            .unwrap_or_else(|e| panic!("open LessDB at {}: {e}", data_dir.display()));
    let session = LessSession::new_async(engine.clone())
        .await
        .expect("LessDB session");
    let store = Arc::new(Store::new(session, engine.clone()));

    let existing = engine
        .tables_async()
        .await
        .expect("list stored tables; refusing fresh initialization on read failure");
    for (name, ddl) in TABLES {
        if !existing.iter().any(|t| t == name) {
            store
                .run(0, "world", &format!("create {name}"), ddl)
                .await
                .expect("create table");
        }
    }

    let snapshot = store
        .run(
            0,
            "world",
            "load world snapshot",
            "SELECT v, sim_minute FROM lv_kv WHERE k = 'world' OR k LIKE 'world:%' ORDER BY sim_minute DESC, k DESC LIMIT 1",
        )
        .await
        .expect("load stored world; refusing fresh initialization on read failure");
    // Read legacy revisions before compaction can discard a newer replaced key.
    for t in LATEST_STATE_TABLES {
        if let Err(e) = collapse(&engine, t).await {
            eprintln!("FATAL: could not collapse {t} before resume: {e}");
            std::process::exit(2);
        }
    }
    let gateway = llm::from_env();
    let (llm_tx, llm_rx) = mpsc::unbounded_channel();
    let mut sim = Sim {
        store: store.clone(),
        w: World::fresh(1),
        llm: gateway.as_ref().map(|_| llm_tx),
    };
    match snapshot.first().map(|r| {
        serde_json::from_str::<World>(&s(r, "v"))
            .expect("stored world is invalid; refusing to replace it with a fresh world")
    }) {
        Some(mut w) => {
            w.hydrate();
            let counts = count_rows(&store, w.minute).await;
            let mem = counts
                .get("lv_memories")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let msg = counts
                .get("lv_messages")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let note = format!(
                "Resumed Day {} {} from LessDB ({mem} memories, {msg} messages on disk)",
                sim::day_of(w.minute),
                clock(w.minute)
            );
            w.resumed = Some(note.clone());
            // Never reuse an id: counters must be above every stored row, even if the
            // snapshot is older than the newest writes.
            for (name, ddl) in TABLES {
                if !ddl.contains("(id Int64") {
                    continue;
                }
                let rows = store
                    .run(
                        w.minute,
                        "world",
                        "reconcile id counter",
                        &format!("SELECT max(id) AS m FROM {name}"),
                    )
                    .await
                    .unwrap_or_default();
                let max = rows.first().map(|r| i(r, "m")).unwrap_or(0);
                let e = w.next_id.entry(name.to_string()).or_insert(0);
                if *e < max {
                    eprintln!(
                        "id counter for {name} raised from {} to {max} (snapshot older than data)",
                        *e
                    );
                    *e = max;
                }
            }
            sim.w = w;
            sim.feed(
                "world",
                "world",
                "Service restarted — world resumed from LessDB",
                &note,
                vec![],
                "",
            );
            eprintln!("{note}");
        }
        None => {
            sim.seed().await;
            eprintln!("Seeded a fresh Lessville world");
        }
    }
    sim.ensure_identity_threads().await;

    if public {
        sim.w.paused = false;
        sim.w.speed = 1;
    }
    let security_snapshot = store
        .run(
            0,
            "security",
            "load security simulation",
            "SELECT v FROM lv_kv WHERE k = 'security.world' ORDER BY sim_minute DESC LIMIT 1",
        )
        .await
        .unwrap_or_default();
    let security_world = security_snapshot
        .first()
        .and_then(|row| serde_json::from_str::<SecurityWorld>(&s(row, "v")).ok())
        .unwrap_or_else(SecurityWorld::fresh);
    let app = App {
        sim: Arc::new(Mutex::new(sim)),
        security: Arc::new(Mutex::new(security_world)),
        store: store.clone(),
        snap: Arc::new(RwLock::new(Value::Null)),
        data_dir: Arc::new(data_dir.clone()),
        public,
        llm: Arc::new(Mutex::new(llm::Status {
            model: llm::MODEL.into(),
            ..Default::default()
        })),
        ckpt: Arc::new(Mutex::new(ckpt_status)),
    };
    if let Some(gw) = gateway {
        llm::spawn(gw, llm_rx, app.sim.clone(), store.clone(), app.llm.clone());
        eprintln!(
            "Model voice enabled ({}) via gateway; decisions stay rule-based",
            llm::MODEL
        );
    } else {
        eprintln!(
            "Model voice disabled (LESSVILLE_LLM_URL / LESSVILLE_INTERNAL_KEY not set): rule-based lines only"
        );
    }
    refresh(&app).await;
    spawn_live_weather(&app);

    if let Some(r2) = r2 {
        let ck = app.clone();
        tokio::spawn(async move {
            let secs = ck.ckpt.lock().await.interval_secs.max(30);
            let mut iv = tokio::time::interval(Duration::from_secs(secs));
            iv.tick().await;
            loop {
                iv.tick().await;
                let res = run_checkpoint(&ck, &r2).await;
                let mut st = ck.ckpt.lock().await;
                match res {
                    Ok(bytes) => {
                        st.uploads += 1;
                        st.last_bytes = bytes;
                        st.last_error = None;
                        st.last_ok_unix = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .ok()
                            .map(|d| d.as_secs());
                    }
                    Err(e) => {
                        st.failures += 1;
                        eprintln!("checkpoint failed: {e}");
                        st.last_error = Some(e);
                    }
                }
            }
        });
    }

    let ticker = app.clone();
    tokio::spawn(async move {
        let mut acc = 0.0f64;
        let mut iv = tokio::time::interval(Duration::from_millis(100));
        loop {
            iv.tick().await;
            let mut changed = false;
            {
                let mut sim = ticker.sim.lock().await;
                if !sim.w.paused {
                    acc += 0.2 * sim.w.speed as f64; // x1 = 2 sim-minutes per second
                    let mut steps = 0;
                    while acc >= 1.0 && steps < 8 && !sim.w.paused {
                        acc -= 1.0;
                        steps += 1;
                        sim.advance().await;
                        if sim.w.stop_at.is_some_and(|end| sim.w.minute >= end) {
                            sim.w.paused = true;
                            sim.w.stop_at = None;
                            sim.persist().await;
                        }
                        changed = true;
                    }
                } else {
                    acc = 0.0;
                }
            }
            if changed {
                refresh(&ticker).await;
            }
        }
    });

    let router = Router::new()
        .route("/api/health", get(health))
        .route("/api/state", get(state))
        .route("/api/control", post(control))
        .route("/api/nudge", post(nudge))
        .route("/api/reset", post(reset))
        .route("/api/agent/{id}", get(agent_detail))
        .route("/api/agent/{id}/context", get(agent_context))
        .route("/api/agent/session/catalog", get(agent_session_catalog))
        .route(
            "/api/agent/session/explain/{id}",
            get(agent_session_explain),
        )
        .route("/api/agent/session/compose", post(agent_session_compose))
        .route("/api/agent/session/propose", post(agent_session_propose))
        .route("/api/agent/session/simulate", post(agent_session_simulate))
        .route("/api/physical/replay", post(physical_replay))
        .route("/api/physical/reconcile", post(physical_reconcile))
        .route("/api/physical/summary", get(physical_summary))
        .route("/api/physical/analytics", get(physical_analytics))
        .route("/api/security/state", get(security_state))
        .route("/api/security/step", post(security_step))
        .route("/api/security/reset", post(security_reset))
        .route("/api/memory/{id}", get(memory_detail))
        .route("/api/tables", get(tables))
        .route("/api/boundaries", get(boundaries))
        .route("/api/llm", get(llm_calls))
        .layer(CorsLayer::permissive())
        .with_state(app.clone());

    let bind = std::env::var("LESSVILLE_BIND").unwrap_or_else(|_| "0.0.0.0".into());
    let listener = tokio::net::TcpListener::bind((bind.as_str(), port))
        .await
        .expect("bind");
    eprintln!(
        "lessville-agentd listening on {bind}:{port}, LessDB data dir {}",
        data_dir.display()
    );
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            #[cfg(unix)]
            {
                let mut term =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("SIGTERM");
                tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
            }
            #[cfg(not(unix))]
            let _ = tokio::signal::ctrl_c().await;
            let mut sim = app.sim.lock().await;
            sim.persist().await;
            sim.w.paused = true;
            for (table, _) in TABLES {
                let engine = app.store.engine.clone();
                let table = table.to_string();
                tokio::task::spawn_blocking(move || engine.flush(&table))
                    .await
                    .expect("flush task")
                    .expect("flush on shutdown");
            }
            eprintln!("World saved and flushed on shutdown");
        })
        .await
        .expect("serve");
}

/// UNIQUE (replacing) tables holding "latest state per key".
const LATEST_STATE_TABLES: [&str; 2] = ["lv_kv", "lv_agent_state"];

/// Flush then OPTIMIZE (engine API; the embedded SQL session has no OPTIMIZE statement),
/// so a replaced key has exactly one row: the last one written.
async fn collapse(engine: &Arc<LessEngine>, table: &str) -> Result<(), String> {
    let e = engine.clone();
    let t = table.to_string();
    tokio::task::spawn_blocking(move || {
        e.flush(&t).map_err(|x| x.to_string())?;
        e.optimize(&t).map(|_| ()).map_err(|x| x.to_string())
    })
    .await
    .map_err(|x| x.to_string())?
}

/// Flush every table with the world paused, archive, then upload outside the lock.
async fn run_checkpoint(app: &App, r2: &checkpoint::R2) -> Result<u64, String> {
    let archive = {
        let mut sim = app.sim.lock().await;
        sim.persist().await;
        for (name, _) in TABLES {
            let engine = app.store.engine.clone();
            let t = name.to_string();
            tokio::task::spawn_blocking(move || {
                engine.flush(&t).map(|_| ()).map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| e.to_string())??;
        }
        for t in LATEST_STATE_TABLES {
            collapse(&app.store.engine, t).await?;
        }
        let dir = (*app.data_dir).clone();
        tokio::task::spawn_blocking(move || checkpoint::R2::archive(&dir))
            .await
            .map_err(|e| e.to_string())??
    };
    let bytes = std::fs::metadata(&archive).map(|m| m.len()).unwrap_or(0);
    let hour = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 3600)
        .unwrap_or(0);
    let r2 = r2.clone();
    tokio::task::spawn_blocking(move || {
        r2.upload(&archive, "latest.tar.gz")?;
        r2.upload(&archive, &format!("hourly/{hour}.tar.gz"))
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(bytes)
}

async fn refresh(app: &App) {
    let log = app.store.recent_log(40).await;
    let snap = {
        let sim = app.sim.lock().await;
        sim.snapshot(log, app.store.statements())
    };
    *app.snap.write().await = snap;
}

#[derive(Deserialize)]
struct OpenMeteoCurrent {
    temperature_2m: f64,
    wind_speed_10m: f64,
    weather_code: i32,
}

#[derive(Deserialize)]
struct OpenMeteoResponse {
    current: Option<OpenMeteoCurrent>,
}

fn fetch_live_weather() -> Option<(i32, i32, i32)> {
    let latitude = std::env::var("LESSVILLE_WEATHER_LAT").unwrap_or_else(|_| "1.3521".into());
    let longitude = std::env::var("LESSVILLE_WEATHER_LON").unwrap_or_else(|_| "103.8198".into());
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={latitude}&longitude={longitude}&current=temperature_2m,wind_speed_10m,weather_code&timezone=Asia%2FSingapore"
    );
    let output = std::process::Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--max-time",
            "8",
            "--user-agent",
            "LessVille research simulation weather poller",
            &url,
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let body = serde_json::from_slice::<OpenMeteoResponse>(&output.stdout).ok()?;
    let current = body.current?;
    Some((
        current.weather_code,
        current.temperature_2m.round() as i32,
        current.wind_speed_10m.round() as i32,
    ))
}

fn spawn_live_weather(app: &App) {
    if std::env::var("LESSVILLE_LIVE_WEATHER").ok().as_deref() != Some("1") {
        return;
    }
    let weather_app = app.clone();
    tokio::spawn(async move {
        let refresh_secs = std::env::var("LESSVILLE_WEATHER_REFRESH_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(3600)
            .max(900);
        let mut interval = tokio::time::interval(Duration::from_secs(refresh_secs));
        loop {
            interval.tick().await;
            let weather = tokio::task::spawn_blocking(fetch_live_weather)
                .await
                .ok()
                .flatten();
            if let Some((code, temperature, wind)) = weather {
                let observed_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                let mut sim = weather_app.sim.lock().await;
                sim.apply_external_weather(
                    code,
                    temperature,
                    wind,
                    observed_at,
                    "open-meteo-free",
                    refresh_secs,
                )
                .await;
                sim.persist().await;
                drop(sim);
                refresh(&weather_app).await;
            } else {
                eprintln!("live weather poll failed; retaining the last weather state");
            }
        }
    });
}

fn err(code: StatusCode, msg: impl Into<String>) -> Response {
    (code, Json(json!({"error": msg.into()}))).into_response()
}

async fn health(State(app): State<App>) -> Json<Value> {
    Json(json!({
        "ok": true,
        "service": "lessville-agentd",
        "engine": "LessDB (embedded less-engine + less-query)",
        "source": SOURCE,
        "data_dir": app.data_dir.display().to_string(),
        "statements": app.store.statements(),
        "public": app.public,
        "llm": app.llm.lock().await.clone(),
        "checkpoint": app.ckpt.lock().await.clone(),
    }))
}

async fn state(State(app): State<App>) -> Json<Value> {
    let mut v = app.snap.read().await.clone();
    if let Some(o) = v.as_object_mut() {
        o.insert("public".into(), json!(app.public));
        let l = app.llm.lock().await.clone();
        o.insert("llm".into(), json!({"enabled": l.enabled, "model": l.model, "ok": l.calls_ok, "rejected": l.calls_rejected, "failed": l.calls_failed, "skipped": l.skipped, "budget_used": l.budget_used, "budget_cap": l.budget_cap, "budget_exhausted": l.budget_exhausted}));
    }
    Json(v)
}

const OBSERVER_ONLY: &str = "This public world is observer-only: the clock and events run on their own so every visitor sees the same town.";

#[derive(Deserialize)]
struct Control {
    action: String,
    speed: Option<u32>,
}

async fn control(State(app): State<App>, Json(c): Json<Control>) -> Response {
    if app.public {
        return err(StatusCode::FORBIDDEN, OBSERVER_ONLY);
    }
    {
        let mut sim = app.sim.lock().await;
        match c.action.as_str() {
            "pause" => sim.w.paused = true,
            "resume" => sim.w.paused = false,
            "run_day" => {
                sim.w.stop_at = Some(sim.w.minute + DAY);
                sim.w.speed = 8;
                sim.w.paused = false;
            }
            "step" => {
                for _ in 0..5 {
                    sim.advance().await;
                }
            }
            "speed" => {
                let sp = c.speed.unwrap_or(1);
                if ![1, 2, 4, 8].contains(&sp) {
                    return err(StatusCode::BAD_REQUEST, "speed must be 1, 2, 4 or 8");
                }
                sim.w.speed = sp;
            }
            _ => return err(StatusCode::BAD_REQUEST, "unknown action"),
        }
        let m = sim.w.minute;
        let sql = format!(
            "INSERT INTO lv_kv VALUES ('clock.control', {}, {m})",
            q(&format!(
                "{} speed={} paused={}",
                c.action, sim.w.speed, sim.w.paused
            ))
        );
        let _ = app
            .store
            .run(m, "observer", "persist clock control", &sql)
            .await;
        sim.persist().await;
    }
    refresh(&app).await;
    Json(json!({"ok": true})).into_response()
}

#[derive(Deserialize)]
struct Nudge {
    kind: String,
}

async fn nudge(State(app): State<App>, Json(n): Json<Nudge>) -> Response {
    if app.public {
        return err(StatusCode::FORBIDDEN, OBSERVER_ONLY);
    }
    let res = {
        let mut sim = app.sim.lock().await;
        sim.nudge(&n.kind).await
    };
    refresh(&app).await;
    match res {
        Ok(m) => Json(json!({"ok": true, "result": m})).into_response(),
        Err(e) => err(StatusCode::BAD_REQUEST, e),
    }
}

async fn reset(State(app): State<App>) -> Response {
    if app.public {
        return err(StatusCode::FORBIDDEN, OBSERVER_ONLY);
    }
    {
        let mut sim = app.sim.lock().await;
        let generation = sim.w.generation + 1;
        for (name, _) in TABLES {
            if let Err(e) = app
                .store
                .run(
                    sim.w.minute,
                    "observer",
                    "reset world",
                    &format!("TRUNCATE TABLE {name}"),
                )
                .await
            {
                return err(StatusCode::INTERNAL_SERVER_ERROR, e);
            }
        }
        sim.w = World::fresh(generation);
        sim.seed().await;
    }
    refresh(&app).await;
    Json(json!({"ok": true})).into_response()
}

async fn now_of(app: &App) -> i64 {
    app.snap
        .read()
        .await
        .pointer("/clock/minute")
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
}

const AGENTS: &[&str] = &["maya", "sam", "lin", "ama", "bo"];
/// Read-only view of the bounded context packet an agent would receive for an
/// "observe" task right now. No model call, no writes. Scope filtering is an
/// application-level predicate over LessDB SQL, not native row-level ACL.
async fn agent_context(State(app): State<App>, Path(id): Path<String>) -> Response {
    if !AGENTS.contains(&id.as_str()) {
        return err(StatusCode::NOT_FOUND, "unknown agent");
    }
    let (world, minute) = {
        let sim = app.sim.lock().await;
        (sim.w.clone(), sim.w.minute)
    };
    let intent = RecallIntent {
        task: "observe".into(),
        purpose: "inspector view of what this agent can recall now".into(),
        predicate: "1 = 1".into(),
        max_items: 6,
    };
    match recall(&app.store, &world, &id, minute, intent).await {
        Ok(packet) => Json(json!({
            "agent": id,
            "packet": packet,
            "boundary": "Application-layer scope filter over LessDB SQL; not native row-level ACL.",
        }))
        .into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn agent_session_catalog(State(app): State<App>) -> Json<Value> {
    Json(agent_session::catalog(app.public))
}

async fn agent_session_explain(State(app): State<App>, Path(id): Path<String>) -> Response {
    let minute = now_of(&app).await;
    match agent_session::explain(&app.store, &id, minute).await {
        Ok(Some(record)) => Json(json!({
            "ok": true,
            "id": id,
            "record": record,
            "boundary": "Read-only provenance explanation; candidate state is not activation."
        }))
        .into_response(),
        Ok(None) => err(StatusCode::NOT_FOUND, "agent-session record not found"),
        Err(error) => err(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

async fn agent_session_compose(
    State(app): State<App>,
    Json(request): Json<agent_session::ComposeRequest>,
) -> Response {
    if app.public {
        return err(StatusCode::FORBIDDEN, OBSERVER_ONLY);
    }
    let (world, minute) = {
        let sim = app.sim.lock().await;
        (sim.w.clone(), sim.w.minute)
    };
    match agent_session::compose(&app.store, &world, request, minute).await {
        Ok(result) => Json(json!({"ok": true, "result": result})).into_response(),
        Err(error) => err(StatusCode::BAD_REQUEST, error),
    }
}

async fn agent_session_propose(
    State(app): State<App>,
    Json(proposal): Json<less_agent::EvolutionProposal>,
) -> Response {
    if app.public {
        return err(StatusCode::FORBIDDEN, OBSERVER_ONLY);
    }
    let minute = now_of(&app).await;
    match agent_session::propose(&app.store, proposal, minute).await {
        Ok(result) => {
            refresh(&app).await;
            Json(result).into_response()
        }
        Err(error) => err(StatusCode::BAD_REQUEST, error),
    }
}

async fn agent_session_simulate(
    State(app): State<App>,
    Json(request): Json<agent_session::SimulateRequest>,
) -> Response {
    if app.public {
        return err(StatusCode::FORBIDDEN, OBSERVER_ONLY);
    }
    let minute = now_of(&app).await;
    match agent_session::simulate(&app.store, request, minute).await {
        Ok(result) => Json(result).into_response(),
        Err(error) => err(StatusCode::BAD_REQUEST, error),
    }
}

async fn security_state(State(app): State<App>) -> Json<Value> {
    let world = app.security.lock().await.clone();
    Json(json!({
        "ok": true,
        "world": world,
        "boundary": "Application-owned security simulation. Events and decisions are persisted in LessDB tables, but this is not a security certification or native policy engine."
    }))
}

async fn security_step(State(app): State<App>) -> Response {
    if app.public {
        return err(StatusCode::FORBIDDEN, OBSERVER_ONLY);
    }
    let (step, world) = {
        let mut security = app.security.lock().await;
        let step = security.step();
        (step, security.clone())
    };
    let (minute, event_id, decision_id) = {
        let mut sim = app.sim.lock().await;
        let minute = sim.w.minute;
        let event_id = sim
            .event(
                "security-proof",
                &format!("security.{}", step.event.kind),
                "security_lab",
                &format!("{} · verdict={}", step.event.detail, step.event.verdict),
            )
            .await;
        let decision_id = sim.next_id("lv_decisions");
        let why = format!(
            "quadrant={} evidence={} action={}",
            step.event.quadrant,
            step.event.evidence.join(","),
            step.action
        );
        let decision_sql = format!(
            "INSERT INTO lv_decisions VALUES ({decision_id}, {minute}, {}, {}, {}, {}, {})",
            q("security-proof"),
            q("security-guardrails-v1"),
            q(&format!(",event:{event_id},")),
            q(&step.action),
            q(&why)
        );
        let _ = app
            .store
            .run(
                minute,
                "security-proof",
                "persist security verdict",
                &decision_sql,
            )
            .await;
        let _ = sim.persist().await;
        (minute, event_id, decision_id)
    };
    let state_sql = format!(
        "INSERT INTO lv_kv VALUES ('security.world', {}, {minute})",
        q(&serde_json::to_string(&world).unwrap_or_else(|_| "{}".into()))
    );
    if let Err(error) = app
        .store
        .run(minute, "security", "persist security world", &state_sql)
        .await
    {
        return err(StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    Json(json!({
        "ok": true,
        "tick": world.tick,
        "event": step.event,
        "action": step.action,
        "event_id": event_id,
        "decision_id": decision_id,
        "world": world,
        "boundary": "Local development mutation only; public observer mode rejects this route."
    }))
    .into_response()
}

async fn security_reset(State(app): State<App>) -> Response {
    if app.public {
        return err(StatusCode::FORBIDDEN, OBSERVER_ONLY);
    }
    let world = {
        let mut security = app.security.lock().await;
        *security = SecurityWorld::fresh();
        security.clone()
    };
    let minute = now_of(&app).await;
    let sql = format!(
        "INSERT INTO lv_kv VALUES ('security.world', {}, {minute})",
        q(&serde_json::to_string(&world).unwrap_or_else(|_| "{}".into()))
    );
    if let Err(error) = app
        .store
        .run(minute, "security", "reset security world", &sql)
        .await
    {
        return err(StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    Json(json!({"ok": true, "world": world})).into_response()
}

/// Local development replay only. Public LessVille remains observer-only and
/// receives 403 here; simulator physics never depends on this request path.
async fn physical_replay(State(app): State<App>, Json(envelope): Json<Value>) -> Response {
    if app.public {
        return err(StatusCode::FORBIDDEN, OBSERVER_ONLY);
    }
    let sim_minute = envelope
        .pointer("/situation/logical_time")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let actor = envelope
        .get("profile")
        .and_then(Value::as_str)
        .map(|profile| format!("simulator:{profile}"))
        .unwrap_or_else(|| "simulator:unknown".into());
    match physical::ingest_envelope(&app.store, &envelope, sim_minute, &actor).await {
        Ok(summary) => Json(json!({
            "ok": true,
            "episode_created": summary.episode_created,
            "observation_inserted": summary.observation_inserted,
            "proposal_inserted": summary.proposal_inserted,
            "boundary": "Local development replay; simulator control remains outside LessDB.",
        }))
        .into_response(),
        Err(e) => err(StatusCode::BAD_REQUEST, e),
    }
}

/// Local development reconciliation only; public mode remains observer-only.
async fn physical_reconcile(State(app): State<App>, Json(reconciliation): Json<Value>) -> Response {
    if app.public {
        return err(StatusCode::FORBIDDEN, OBSERVER_ONLY);
    }
    let sim_minute = reconciliation
        .get("sim_minute")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    match physical::ingest_reconciliation(&app.store, &reconciliation, sim_minute, "simulator")
        .await
    {
        Ok(inserted) => Json(json!({
            "ok": true,
            "inserted": inserted,
            "boundary": "Local development reconciliation; simulator control remains outside LessDB.",
        }))
        .into_response(),
        Err(e) => err(StatusCode::BAD_REQUEST, e),
    }
}

/// Read-only physical replay counts for local visual-twin observability.
/// This reports embedded LessDB rows; it does not expose control or mutation.
async fn physical_summary(State(app): State<App>) -> Response {
    let queries = [
        ("runs", "SELECT count(*) AS n FROM lv_physical_runs"),
        (
            "observations",
            "SELECT count(*) AS n FROM lv_physical_observations",
        ),
        (
            "proposals",
            "SELECT count(*) AS n FROM lv_physical_proposals",
        ),
        (
            "reconciliations",
            "SELECT count(*) AS n FROM lv_physical_reconciliations",
        ),
    ];
    let mut persisted = serde_json::Map::new();
    for (name, sql) in queries {
        let rows = match app
            .store
            .run(0, "observer", "physical replay summary", sql)
            .await
        {
            Ok(rows) => rows,
            Err(error) => return err(StatusCode::INTERNAL_SERVER_ERROR, error),
        };
        let count = rows
            .first()
            .and_then(|row| row.get("n"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        persisted.insert(name.to_owned(), json!(count));
    }
    Json(json!({
        "ok": true,
        "persisted": persisted,
        "boundary": "Read-only counts of application-owned physical replay tables in embedded LessDB; not native robot safety or controller state."
    }))
    .into_response()
}

/// Read-only analytics over the application-owned physical replay rows. The
/// detail envelope is parsed after the bounded row query; this is telemetry for
/// operator understanding, not a native spatial index or native row ACL.
async fn physical_analytics(State(app): State<App>) -> Response {
    let rows = match app
        .store
        .run(
            0,
            "observer",
            "physical spatial and scope analytics",
            "SELECT sequence, sensor_id, kind, observed_at, detail FROM lv_physical_observations ORDER BY episode_id DESC, sequence DESC LIMIT 500",
        )
        .await
    {
        Ok(rows) => rows,
        Err(error) => return err(StatusCode::INTERNAL_SERVER_ERROR, error),
    };

    let mut frames: BTreeMap<String, (i64, i64, String)> = BTreeMap::new();
    let mut scopes: BTreeMap<String, (i64, String, bool)> = BTreeMap::new();
    let mut simulators: BTreeMap<String, i64> = BTreeMap::new();
    let mut latest_sequence = 0_i64;
    let mut oldest_sequence = i64::MAX;

    for row in &rows {
        let sequence = i(row, "sequence");
        latest_sequence = latest_sequence.max(sequence);
        oldest_sequence = oldest_sequence.min(sequence);
        let detail = s(row, "detail");
        let Ok(envelope) = serde_json::from_str::<Value>(&detail) else {
            continue;
        };
        let simulator = envelope
            .get("simulator")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        *simulators.entry(simulator).or_default() += 1;

        let frame = envelope
            .pointer("/observation/pose/frame_id")
            .and_then(Value::as_str)
            .or_else(|| {
                envelope
                    .pointer("/observation/frame_id")
                    .and_then(Value::as_str)
            })
            .unwrap_or("frame:unknown")
            .to_owned();
        let frame_entry = frames.entry(frame).or_insert_with(|| (0, 0, String::new()));
        frame_entry.0 += 1;
        frame_entry.1 = frame_entry.1.max(sequence);
        frame_entry.2 = envelope
            .pointer("/observation/observed_at")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();

        let shared = envelope.pointer("/situation/shared_context");
        let scope = shared
            .and_then(|v| v.get("scope_id"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| {
                let profile = envelope
                    .get("profile")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                format!("inferred:local:{profile}")
            });
        let grant = shared
            .and_then(|v| v.get("grant_id"))
            .and_then(Value::as_str)
            .unwrap_or("none")
            .to_owned();
        let scope_entry = scopes
            .entry(scope)
            .or_insert_with(|| (0, grant, shared.is_some()));
        scope_entry.0 += 1;
    }

    let count_queries = [
        (
            "observations",
            "SELECT count(*) AS n FROM lv_physical_observations",
        ),
        (
            "proposals",
            "SELECT count(*) AS n FROM lv_physical_proposals",
        ),
        (
            "reconciliations",
            "SELECT count(*) AS n FROM lv_physical_reconciliations",
        ),
    ];
    let mut persisted = serde_json::Map::new();
    for (name, sql) in count_queries {
        let rows = match app
            .store
            .run(0, "observer", "physical analytics count", sql)
            .await
        {
            Ok(rows) => rows,
            Err(error) => return err(StatusCode::INTERNAL_SERVER_ERROR, error),
        };
        persisted.insert(
            name.to_owned(),
            json!(rows.first().map(|row| i(row, "n")).unwrap_or(0)),
        );
    }

    let frame_values = frames
        .into_iter()
        .map(|(frame_id, (samples, latest, observed_at))| {
            json!({
                "frame_id": frame_id,
                "samples": samples,
                "latest_sequence": latest,
                "latest_observed_at": observed_at,
            })
        })
        .collect::<Vec<_>>();
    let scope_values = scopes
        .into_iter()
        .map(|(scope_id, (samples, grant_id, explicit))| {
            json!({
                "scope_id": scope_id,
                "samples": samples,
                "grant_id": grant_id,
                "explicit_grant_metadata": explicit,
            })
        })
        .collect::<Vec<_>>();

    Json(json!({
        "ok": true,
        "source": "embedded LessDB physical observation rows",
        "world_frame": "lessville:embodied-district/v1",
        "sample_window": rows.len(),
        "sequence": {
            "latest": latest_sequence,
            "oldest_in_window": if oldest_sequence == i64::MAX { 0 } else { oldest_sequence },
            "span": if oldest_sequence == i64::MAX { 0 } else { latest_sequence.saturating_sub(oldest_sequence) },
        },
        "frames": frame_values,
        "scopes": scope_values,
        "simulators": simulators,
        "persisted": persisted,
        "contract_versions": {
            "frame": "lessdb-spatial-frame/v1",
            "transform": "lessdb-frame-transform/v1",
            "observation": "lessdb-spatial-observation/v1",
            "shared_context": "application-owned-grant/v1"
        },
        "validity_policy": "Application recall omits observations after their bounded logical validity window while retaining evidence rows for explanation and replay.",
        "boundary": "Application-owned telemetry projection. Frame identity and shared-context grants are metadata carried by envelopes; LessDB does not provide native spatial indexing, row ACL, controller safety or cross-system atomicity here."
    }))
    .into_response()
}

async fn agent_detail(State(app): State<App>, Path(id): Path<String>) -> Response {
    if !AGENTS.contains(&id.as_str()) {
        return err(StatusCode::NOT_FOUND, "unknown agent");
    }
    let now = now_of(&app).await;
    let a = q(&id);
    let acl =
        format!("(owner = {a} OR scope IN (SELECT scope FROM lv_scope_members WHERE agent = {a}))");
    let visible_sql = format!(
        "SELECT id, sim_minute, owner, kind, scope, location, tag, context_id, expires_at, source_kind, source_id, content FROM lv_memories WHERE {acl} AND (expires_at < 0 OR expires_at > {now}) ORDER BY id DESC LIMIT 40"
    );
    let expired_sql = format!(
        "SELECT id, sim_minute, owner, kind, scope, tag, expires_at, content FROM lv_memories WHERE {acl} AND expires_at >= 0 AND expires_at <= {now} ORDER BY id DESC LIMIT 10"
    );
    let hidden_sql = format!("SELECT count(*) AS n FROM lv_memories WHERE NOT {acl}");
    let inbox_sql = format!(
        "SELECT id, sim_minute, sender, channel, recipient, topic, body FROM lv_messages WHERE sender = {a} OR recipient = {a} OR channel IN (SELECT scope FROM lv_scope_members WHERE agent = {a}) ORDER BY id DESC LIMIT 25"
    );
    let decisions_sql = format!(
        "SELECT id, sim_minute, policy, recalled, action, explanation FROM lv_decisions WHERE agent = {a} ORDER BY id DESC LIMIT 12"
    );
    let lessons_sql = format!(
        "SELECT lesson_id, sim_minute, topic, trigger, statement, state, confidence, source_refs, policy_version, applied_at, auto_apply FROM lv_lessons WHERE agent = {a} ORDER BY sim_minute DESC LIMIT 20"
    );
    let relationships_sql = format!(
        "SELECT edge_id, subject_agent, object_agent, relation, consent, reciprocity, trust_context, evidence_refs, state, confidence, policy_version, updated_at, auto_apply FROM lv_relationships WHERE subject_agent = {a} OR object_agent = {a} ORDER BY updated_at DESC LIMIT 20"
    );
    let challenges_sql = format!(
        "SELECT challenge_id, lesson_id, agent, challenger, kind, claim, evidence_refs, resolution, sim_minute, auto_apply FROM lv_lesson_challenges WHERE agent = {a} OR challenger = {a} ORDER BY sim_minute DESC LIMIT 20"
    );
    let teaching_sql = format!(
        "SELECT grant_id, lesson_id, teacher_agent, recipient_agent, scope_id, purpose, valid_from, valid_until, state, evidence_refs, policy_version, sim_minute, auto_apply FROM lv_teaching_grants WHERE teacher_agent = {a} OR recipient_agent = {a} ORDER BY sim_minute DESC LIMIT 20"
    );
    let counterfactuals_sql = format!(
        "SELECT replay_id, agent, sim_minute, topic, baseline_policy, baseline_action, learned_policy, learned_action, difference, source_refs, not_applied FROM lv_counterfactuals WHERE agent = {a} ORDER BY sim_minute DESC LIMIT 20"
    );
    let competencies_sql = format!(
        "SELECT competency_id, agent, domain, skill, level, evidence_count, status, source_refs, policy_version, updated_at, auto_apply FROM lv_competencies WHERE agent = {a} ORDER BY updated_at DESC LIMIT 20"
    );
    let interactions_sql = format!(
        "SELECT interaction_id, sim_minute, initiator, recipient, kind, context_id, initiator_action, recipient_response, reciprocity, evidence_refs, outcome, auto_apply FROM lv_interactions WHERE initiator = {a} OR recipient = {a} ORDER BY sim_minute DESC LIMIT 20"
    );
    let identity_threads_sql = format!(
        "SELECT record_id, thread_id, agent, parent_thread, operation, reason, continuity, evidence_refs, policy_version, sim_minute, auto_apply FROM lv_identity_threads WHERE agent = {a} ORDER BY sim_minute DESC LIMIT 20"
    );
    let competency_events_sql = format!(
        "SELECT event_id, competency_id, agent, domain, skill, kind, prior_level, next_level, evidence_refs, reason, policy_version, sim_minute, auto_apply FROM lv_competency_events WHERE agent = {a} ORDER BY sim_minute DESC LIMIT 30"
    );
    let scopes_sql =
        format!("SELECT scope, granted_by FROM lv_scope_members WHERE agent = {a} ORDER BY scope");
    let st = &app.store;
    let visible = st
        .run(
            now,
            "inspector",
            &format!("{id}: visible memories"),
            &visible_sql,
        )
        .await
        .unwrap_or_default();
    let expired = st
        .run(
            now,
            "inspector",
            &format!("{id}: expired temporary memories"),
            &expired_sql,
        )
        .await
        .unwrap_or_default();
    let hidden = st
        .run(
            now,
            "inspector",
            &format!("{id}: rows hidden by scope rule"),
            &hidden_sql,
        )
        .await
        .unwrap_or_default();
    let inbox = st
        .run(now, "inspector", &format!("{id}: conversation"), &inbox_sql)
        .await
        .unwrap_or_default();
    let decisions = st
        .run(
            now,
            "inspector",
            &format!("{id}: decisions"),
            &decisions_sql,
        )
        .await
        .unwrap_or_default();
    let lessons = st
        .run(
            now,
            "inspector",
            &format!("{id}: learning lessons"),
            &lessons_sql,
        )
        .await
        .unwrap_or_default();
    let relationships = st
        .run(
            now,
            "inspector",
            &format!("{id}: relationship edges"),
            &relationships_sql,
        )
        .await
        .unwrap_or_default();
    let challenges = st
        .run(
            now,
            "inspector",
            &format!("{id}: lesson challenges"),
            &challenges_sql,
        )
        .await
        .unwrap_or_default();
    let teaching = st
        .run(
            now,
            "inspector",
            &format!("{id}: teaching grants"),
            &teaching_sql,
        )
        .await
        .unwrap_or_default();
    let counterfactuals = st
        .run(
            now,
            "inspector",
            &format!("{id}: counterfactual replays"),
            &counterfactuals_sql,
        )
        .await
        .unwrap_or_default();
    let competencies = st
        .run(
            now,
            "inspector",
            &format!("{id}: competency evidence"),
            &competencies_sql,
        )
        .await
        .unwrap_or_default();
    let interactions = st
        .run(
            now,
            "inspector",
            &format!("{id}: reciprocal interactions"),
            &interactions_sql,
        )
        .await
        .unwrap_or_default();
    let identity_threads = st
        .run(
            now,
            "inspector",
            &format!("{id}: identity thread history"),
            &identity_threads_sql,
        )
        .await
        .unwrap_or_default();
    let competency_events = st
        .run(
            now,
            "inspector",
            &format!("{id}: competency state events"),
            &competency_events_sql,
        )
        .await
        .unwrap_or_default();
    let scopes = st
        .run(
            now,
            "inspector",
            &format!("{id}: scope memberships"),
            &scopes_sql,
        )
        .await
        .unwrap_or_default();
    let candidate_count = lessons
        .iter()
        .filter(|row| s(row, "state") == "candidate")
        .count();
    let accepted_count = lessons
        .iter()
        .filter(|row| s(row, "state") == "accepted")
        .count();
    let growth_stage = if accepted_count >= 2 {
        "integrating"
    } else if accepted_count == 1 {
        "adapting"
    } else if candidate_count > 0 {
        "forming"
    } else {
        "observing"
    };
    let competency_count = competencies.len();
    let interaction_count = interactions.len();
    let contradiction_count = competency_events
        .iter()
        .filter(|row| s(row, "kind") == "contradiction")
        .count();
    let decay_count = competency_events
        .iter()
        .filter(|row| s(row, "kind") == "decay")
        .count();
    Json(json!({
        "agent": id,
        "now": now,
        "visible": visible, "visible_sql": visible_sql,
        "expired": expired, "expired_sql": expired_sql,
        "hidden": hidden.first().map(|r| i(r, "n")).unwrap_or(0), "hidden_sql": hidden_sql,
        "conversation": inbox, "conversation_sql": inbox_sql,
        "decisions": decisions, "decisions_sql": decisions_sql,
        "lessons": lessons, "lessons_sql": lessons_sql,
        "relationships": relationships, "relationships_sql": relationships_sql,
        "lesson_challenges": challenges, "lesson_challenges_sql": challenges_sql,
        "teaching_grants": teaching, "teaching_grants_sql": teaching_sql,
        "counterfactuals": counterfactuals, "counterfactuals_sql": counterfactuals_sql,
        "competencies": competencies, "competencies_sql": competencies_sql,
        "interactions": interactions, "interactions_sql": interactions_sql,
        "identity_threads": identity_threads, "identity_threads_sql": identity_threads_sql,
        "competency_events": competency_events, "competency_events_sql": competency_events_sql,
        "growth": {
            "stage": growth_stage,
            "candidate_count": candidate_count,
            "accepted_count": accepted_count,
            "competency_count": competency_count,
            "interaction_count": interaction_count,
            "identity_thread_count": identity_threads.len(),
            "competency_event_count": competency_events.len(),
            "contradiction_count": contradiction_count,
            "decay_count": decay_count,
            "boundary": "Application-owned progression summary. Lessons are evidence for deterministic rules, not consciousness, automatic policy changes or physical authorization."
        },
        "scopes": scopes,
    }))
    .into_response()
}

async fn memory_detail(State(app): State<App>, Path(id): Path<i64>) -> Response {
    let now = now_of(&app).await;
    let st = &app.store;
    let msql = format!("SELECT * FROM lv_memories WHERE id = {id}");
    let rows = st
        .run(now, "inspector", "memory provenance", &msql)
        .await
        .unwrap_or_default();
    let Some(mem) = rows.first().cloned() else {
        return err(
            StatusCode::NOT_FOUND,
            "memory not found (it may have been deleted)",
        );
    };
    let sid = i(&mem, "source_id");
    let (source_sql, source) = match s(&mem, "source_kind").as_str() {
        "message" => {
            let q = format!("SELECT * FROM lv_messages WHERE id = {sid}");
            let r = st
                .run(now, "inspector", "memory source message", &q)
                .await
                .unwrap_or_default();
            (q, r)
        }
        "event" if sid > 0 => {
            let q = format!("SELECT * FROM lv_events WHERE id = {sid}");
            let r = st
                .run(now, "inspector", "memory source event", &q)
                .await
                .unwrap_or_default();
            (q, r)
        }
        "llm" => {
            let q = format!("SELECT * FROM lv_llm_calls WHERE id = {sid}");
            let r = st
                .run(now, "inspector", "memory source model call", &q)
                .await
                .unwrap_or_default();
            (q, r)
        }
        "decision" => {
            let q = format!("SELECT * FROM lv_decisions WHERE id = {sid}");
            let r = st
                .run(now, "inspector", "memory source decision", &q)
                .await
                .unwrap_or_default();
            (q, r)
        }
        _ => (String::new(), Vec::new()),
    };
    let used_sql = format!(
        "SELECT id, sim_minute, agent, policy, action, explanation FROM lv_decisions WHERE recalled LIKE '%,mem:{id},%' ORDER BY id"
    );
    let used = st
        .run(
            now,
            "inspector",
            "decisions influenced by memory",
            &used_sql,
        )
        .await
        .unwrap_or_default();
    let readers_sql = format!(
        "SELECT agent FROM lv_scope_members WHERE scope = {} ORDER BY agent",
        q(&s(&mem, "scope"))
    );
    let readers = st
        .run(now, "inspector", "who may read this memory", &readers_sql)
        .await
        .unwrap_or_default();
    Json(json!({
        "memory": mem, "memory_sql": msql,
        "source": source, "source_sql": source_sql,
        "used_by": used, "used_sql": used_sql,
        "readers": readers, "readers_sql": readers_sql,
    }))
    .into_response()
}

async fn count_rows(store: &Store, now: i64) -> serde_json::Map<String, Value> {
    let mut out = serde_json::Map::new();
    for (name, _) in TABLES {
        let rows = store
            .run(
                now,
                "inspector",
                "count rows",
                &format!("SELECT count(*) AS n FROM {name}"),
            )
            .await
            .unwrap_or_default();
        out.insert(
            (*name).into(),
            json!(rows.first().map(|r| i(r, "n")).unwrap_or(0)),
        );
    }
    out
}

async fn tables(State(app): State<App>) -> Json<Value> {
    let now = now_of(&app).await;
    let counts = count_rows(&app.store, now).await;
    Json(json!({
        "tables": counts,
        "ddl": TABLES.iter().map(|(n, d)| json!({"name": n, "ddl": d})).collect::<Vec<_>>(),
        "data_dir": app.data_dir.display().to_string(),
        "source": SOURCE,
        "day_minutes": DAY,
    }))
}

async fn boundaries() -> Json<Value> {
    Json(json!({
        "native": [
            ["Persistent rows", "Every memory, message, decision, observation and world checkpoint is a SQL INSERT into embedded LessDB tables on local disk."],
            ["WAL-backed acknowledgement", "Inserts are acknowledged after LessDB's write-ahead log append; malformed WAL recovery fails closed. This reconstructed candidate is freshly re-tested, not the earlier qualified build, and not a power-loss certification."],
            ["Latest-state tables", "lv_agent_state and lv_kv use LessDB UNIQUE (replacing) tables. LessDB uniqueness is eventually exact: rows collapse to the last one per key at flush and merge, but older rows can stay visible across parts until OPTIMIZE. The app runs OPTIMIZE before resuming and before every checkpoint, reads the newest row explicitly, and keeps id counters above every stored id."],
            ["Delete on request", "Revoking Lin's consent runs a LessDB DELETE; the rows are removed, not merely hidden."],
            ["Restart recovery", "Stop and restart the service: the world resumes from the last LessDB checkpoint and all stored memories."],
            ["Single directory owner", "This service holds LessDB's exclusive directory lease; a second engine on the same data directory is refused."],
        ],
        "application": [
            ["Scopes & permissions", "Who can read what is a SQL predicate written by this app (owner OR scope IN memberships). LessDB has no native per-agent ACL here."],
            ["Temporary memory expiry", "Temporary memories expire by the simulated clock via an expires_at filter. LessDB's native TTL uses wall-clock time and is not used."],
            ["Bounded hot tables", "Each simulated night the app DELETEs log rows older than a 3–7 day window and caps permanent memories per owner (lessdb-retention-policy/v1). Older evidence survives only in R2 checkpoint history. This is application compaction, not native TTL or tiered storage."],
            ["Memory lifecycle", "Application-owned lv_memory_lifecycle rows distinguish active, expired, archived, revoked and deleted. Expiry is a visibility decision; deletion removes the payload. This is not native LessDB semantic forgetting."],
            ["Messaging", "Agents 'receive' messages by polling SQL queries over lv_messages. There is no push delivery or native pub/sub."],
            ["Agent behaviour", "Every decision follows deterministic, versioned rules (see the policy on each decision). A small model (Workers AI, Llama 3.1 8B) may only re-voice a line the rules already chose, or write one nightly reflection from at most 6 of the agent's own memories. Each call has one task, a bounded LessDB context query, output checks and a provenance row in lv_llm_calls; it never changes what an agent does."],
            ["Model budget", "Model calls stop for the day once the hard Workers AI budget is reached; the world continues on rules alone."],
            ["Durability in the cloud", "LessDB runs on the container's local disk. Every couple of minutes the tables are flushed and the directory is checkpointed to Cloudflare R2; after a container restart the world resumes from the latest checkpoint, so up to one interval of activity can be lost."],
            ["Multi-row writes", "A decision, its memory and its message are separate inserts, not one atomic transaction."],
        ],
        "open": [
            "Native tenant isolation and per-agent authentication for memory rows",
            "Relationship graph between agents (beyond scope membership)",
            "Retention and forgetting policies beyond explicit DELETE and time-window filters",
            "Schema evolution for memory records",
            "Multi-node / distributed agent swarms on shared storage",
            "Atomic multi-statement transactions",
        ],
    }))
}

async fn llm_calls(State(app): State<App>) -> Json<Value> {
    let now = now_of(&app).await;
    let sql = "SELECT id, sim_minute, agent, task, context_ids, input_tokens, output_tokens, neurons, status, output, prompt FROM lv_llm_calls ORDER BY id DESC LIMIT 30";
    let rows = app
        .store
        .run(now, "inspector", "recent model calls", sql)
        .await
        .unwrap_or_default();
    Json(json!({"calls": rows, "sql": sql, "status": app.llm.lock().await.clone()}))
}
