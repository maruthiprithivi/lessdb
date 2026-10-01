//! LessDB HTTP server: SQL API (JSON/Arrow IPC), Prometheus metrics, and
//! LDAP/AD-backed authentication.
//!
//! Endpoints:
//! * `GET  /health` — liveness (unauthenticated)
//! * `GET  /metrics` — Prometheus text exposition (unauthenticated; bind
//!   to a private interface or front with a proxy if that matters)
//! * `POST /v1/sql` — `{"sql": "...", "format": "json" | "arrow"}` (auth)
//! * `GET  /v1/tables`, `GET /v1/describe/{table}` (auth)
//! * `POST /v1/admin/optimize` — `{"table": "..."}`; requires role
//!   `admin` or `write` (auth)
//!
//! Authentication: HTTP Basic against the engine's `auth` config
//! (LDAP/Active Directory via `less-auth`, or a dev users file). No auth
//! configured = open server (local dev default).

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use serde::Deserialize;

use arrow::datatypes::Schema;
use arrow::ipc::writer::StreamWriter;
use less_auth::{Authenticator, User};
use less_common::{LessError, Result};
use less_query::LessSession;

/// PEM files for serving HTTPS.
#[derive(Debug, Clone)]
pub struct TlsPaths {
    /// PEM certificate chain (leaf first).
    pub cert: PathBuf,
    /// PEM private key.
    pub key: PathBuf,
}

#[derive(Clone)]
pub struct AppState {
    pub session: Arc<LessSession>,
    pub auth: Option<Arc<dyn Authenticator>>,
    /// The MCP door (same state/tool set as `lessdb mcp`), served at
    /// POST /mcp with agent-token auth. `None` disables the endpoint.
    pub mcp: Option<Arc<less_mcp::McpState>>,
}

fn internal(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn unauthorized(msg: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Basic realm=\"lessdb\"")],
        msg.to_string(),
    )
        .into_response()
}

/// Build the HTTP application. Split out for tests.
pub fn router(state: AppState) -> Router {
    let authed = Router::new()
        .route("/v1/sql", post(sql))
        .route("/v1/tables", get(tables))
        .route("/v1/describe/{table}", get(describe))
        .route(
            "/v1/admin/optimize",
            post(admin_optimize).route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                require_writer,
            )),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_auth,
        ));
    Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/mcp", post(mcp_endpoint))
        .route("/mcp", axum::routing::options(mcp_preflight))
        .merge(authed)
        .with_state(state)
}

/// Start the HTTP server on `addr` (e.g. `127.0.0.1:7080`).
pub async fn serve(session: Arc<LessSession>, addr: &str) -> Result<()> {
    serve_with_tls(session, addr, None, None).await
}

/// Start the server on `addr`; with `tls` set, serve HTTPS (rustls) using
/// the given PEM certificate chain + private key. `mcp` enables the
/// authenticated MCP-over-HTTP door at POST /mcp.
pub async fn serve_with_tls(
    session: Arc<LessSession>,
    addr: &str,
    tls: Option<&TlsPaths>,
    mcp: Option<Arc<less_mcp::McpState>>,
) -> Result<()> {
    let auth = match &session.engine().config.auth {
        Some(cfg) => less_auth::build(cfg)?,
        None => None,
    };
    if auth.is_some() {
        tracing::info!("authentication enabled");
    }
    if mcp.is_some() {
        tracing::info!("MCP endpoint enabled at POST /mcp (agent tokens required)");
    }
    let app = router(AppState { session, auth, mcp }).layer(axum::middleware::from_fn(access_log));
    if let Some(tls) = tls {
        // rustls 0.23 needs a process-level crypto provider installed
        // before building configs (idempotent: later installs are no-ops).
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let config = axum_server::tls_rustls::RustlsConfig::from_pem_file(&tls.cert, &tls.key)
            .await
            .map_err(|e| LessError::Config(format!("TLS config: {e}")))?;
        let addr: std::net::SocketAddr = addr
            .parse()
            .map_err(|e| LessError::Config(format!("invalid listen address '{addr}': {e}")))?;
        tracing::info!(%addr, "https server listening");
        axum_server::bind_rustls(addr, config)
            .serve(app.into_make_service())
            .await
            .map_err(|e| LessError::Server(format!("https server: {e}")))?;
    } else {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        tracing::info!(%addr, "http server listening");
        axum::serve(listener, app).await?;
    }
    Ok(())
}

/// Access-log middleware: one structured line per request (method, path,
/// status, latency). Rides on `tracing` so it obeys the same level filter
/// and lands in the same rotated log file.
async fn access_log(req: axum::extract::Request, next: Next) -> axum::response::Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let start = std::time::Instant::now();
    let resp = next.run(req).await;
    let status = resp.status().as_u16();
    let latency_ms = ((start.elapsed().as_secs_f64() * 1000.0) * 100.0).round() / 100.0;
    tracing::info!(
        method = %method, path = %path, status = status,
        latency_ms, "request"
    );
    resp
}

/// MCP-over-HTTP (streamable-HTTP flavoured): one JSON-RPC request per
/// POST. Agent tokens ride the HTTP `Authorization: Bearer …` header and
/// are merged into `initialize` params, exactly like stdio clients pass
/// them. Responses are plain JSON, or a single SSE `message` event when
/// the client asks for `text/event-stream`.
async fn mcp_endpoint(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Response {
    let Some(mcp) = &st.mcp else {
        return (StatusCode::NOT_FOUND, "mcp endpoint disabled").into_response();
    };
    // HTTP bearer token, resolved per request (a hosted door serves many
    // agents, so the caller is never process-global).
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());

    let accept = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    match less_mcp::handle_request_http(mcp, &body, bearer.as_deref()).await {
        Ok(resp) => {
            if resp.is_empty() {
                return StatusCode::ACCEPTED.into_response();
            }
            less_telemetry::global()
                .queries
                .inc(&[("route", "/mcp"), ("status", "ok")]);
            if accept.contains("text/event-stream") {
                let mut out = String::with_capacity(resp.len() + 40);
                out.push_str("event: message\ndata: ");
                out.push_str(&resp.replace('\n', " "));
                out.push_str("\n\n");
                mcp_response(out, "text/event-stream")
            } else {
                mcp_response(resp, "application/json")
            }
        }
        Err(e) => {
            less_telemetry::global()
                .queries
                .inc(&[("route", "/mcp"), ("status", "error")]);
            internal(e).into_response()
        }
    }
}

/// CORS preflight for browser-based MCP clients.
async fn mcp_preflight() -> Response {
    mcp_response(String::new(), "text/plain")
}

fn mcp_response(body: String, content_type: &str) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".to_string()),
            (
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                "content-type, authorization, mcp-session-id, mcp-protocol-version".to_string(),
            ),
        ],
        body,
    )
        .into_response()
}

async fn health() -> &'static str {
    "ok"
}

/// Prometheus text exposition.
async fn metrics(State(st): State<AppState>) -> String {
    let metrics = less_telemetry::global();
    // Live gauges from engine state (async-safe table discovery).
    if let Ok(tables) = st.session.engine().tables_async().await {
        metrics.tables.set(tables.len() as i64);
    }
    let buffered: usize = st.session.engine().buffered_rows().unwrap_or(0);
    metrics.buffered_rows.set(buffered as i64);
    metrics.render()
}

/// Basic-auth middleware. Requests carry the resolved user in extensions
/// when authentication succeeds.
async fn require_auth(State(st): State<AppState>, mut req: Request, next: Next) -> Response {
    let Some(auth) = &st.auth else {
        return next.run(req).await; // auth disabled: open server
    };
    let header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let ok = header
        .strip_prefix("Basic ")
        .and_then(|encoded| {
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .ok()
        })
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|pair| {
            let (user, pass) = pair.split_once(':')?;
            Some((user.to_string(), pass.to_string()))
        });
    match ok {
        Some((user, pass)) => match auth.authenticate(&user, &pass).await {
            Ok(user) => {
                req.extensions_mut().insert(user);
                next.run(req).await
            }
            Err(_) => {
                less_telemetry::global().auth_failures.inc();
                unauthorized("invalid credentials")
            }
        },
        None => {
            less_telemetry::global().auth_failures.inc();
            unauthorized("missing credentials")
        }
    }
}

/// Role check for admin endpoints (runs after `require_auth`).
async fn require_writer(State(_st): State<AppState>, req: Request, next: Next) -> Response {
    match req.extensions().get::<User>() {
        Some(user) if user.can_write() => next.run(req).await,
        Some(user) => (
            StatusCode::FORBIDDEN,
            format!("role '{}' cannot run admin operations", user.role),
        )
            .into_response(),
        None => unauthorized("authentication required"),
    }
}

#[derive(Deserialize)]
struct SqlReq {
    sql: String,
    #[serde(default = "default_format")]
    format: String,
}

fn default_format() -> String {
    "json".to_string()
}

async fn sql(
    State(st): State<AppState>,
    Json(body): Json<SqlReq>,
) -> std::result::Result<Response, (StatusCode, String)> {
    let start = std::time::Instant::now();
    let batches = st.session.sql_batches(&body.sql).await.map_err(internal)?;
    less_telemetry::global()
        .http_requests
        .inc(&[("route", "/v1/sql"), ("status", "ok")]);
    less_telemetry::global()
        .query_duration
        .observe(start.elapsed().as_secs_f64());

    match body.format.as_str() {
        "arrow" => {
            let schema = batches
                .first()
                .map(|b| b.schema())
                .unwrap_or_else(|| Arc::new(Schema::empty()));
            let mut buf = Vec::new();
            let mut writer = StreamWriter::try_new(&mut buf, &schema).map_err(internal)?;
            for b in &batches {
                writer.write(b).map_err(internal)?;
            }
            writer.finish().map_err(internal)?;
            Ok((
                [(header::CONTENT_TYPE, "application/vnd.apache.arrow.stream")],
                buf,
            )
                .into_response())
        }
        _ => {
            let mut buf = Vec::new();
            {
                let mut writer =
                    arrow_json::writer::Writer::<_, arrow_json::writer::JsonArray>::new(&mut buf);
                for b in &batches {
                    writer.write(b).map_err(internal)?;
                }
                writer.finish().map_err(internal)?;
            }
            Ok(([(header::CONTENT_TYPE, "application/json")], buf).into_response())
        }
    }
}

async fn tables(
    State(st): State<AppState>,
) -> std::result::Result<Json<Vec<String>>, (StatusCode, String)> {
    let names = st.session.engine().tables_async().await.map_err(internal)?;
    less_telemetry::global()
        .http_requests
        .inc(&[("route", "/v1/tables"), ("status", "ok")]);
    Ok(Json(names))
}

async fn describe(
    State(st): State<AppState>,
    Path(table): Path<String>,
) -> std::result::Result<Json<less_catalog::TableDef>, (StatusCode, String)> {
    let def = st.session.engine().table(&table).map_err(internal)?;
    less_telemetry::global()
        .http_requests
        .inc(&[("route", "/v1/describe"), ("status", "ok")]);
    Ok(Json(def))
}

#[derive(Deserialize)]
struct OptimizeReq {
    table: String,
}

/// OPTIMIZE behind role-based authorization (admin or write; enforced by
/// the `require_writer` middleware).
async fn admin_optimize(
    State(st): State<AppState>,
    Json(body): Json<OptimizeReq>,
) -> std::result::Result<Json<serde_json::Value>, (StatusCode, String)> {
    let merged = st
        .session
        .engine()
        .optimize(&body.table)
        .map_err(internal)?;
    less_telemetry::global()
        .http_requests
        .inc(&[("route", "/v1/admin/optimize"), ("status", "ok")]);
    let merged_part = merged.as_ref().map(|m| m.name.clone());
    let merged_rows = merged.as_ref().map(|m| m.row_count).unwrap_or(0);
    Ok(Json(serde_json::json!({
        "table": body.table,
        "merged_part": merged_part,
        "merged_rows": merged_rows,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Float64Array, Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field};
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use less_catalog::{EngineKind, FieldSpec, SchemaSpec, TableDef, TypeSpec};
    use less_common::EngineConfig;
    use less_engine::LessEngine;
    use tower::ServiceExt;

    async fn test_state(auth_json: Option<&str>) -> (AppState, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("less-server-{}", uuid::Uuid::new_v4()));
        let mut config = EngineConfig::with_data_dir(&dir);
        if let Some(json) = auth_json {
            config.auth = Some(serde_json::from_str(json).unwrap());
        }
        let engine = LessEngine::open(config).unwrap();
        let def = TableDef::new(
            "events",
            SchemaSpec {
                fields: vec![
                    FieldSpec::new("id", TypeSpec::Int64),
                    FieldSpec::new("kind", TypeSpec::Utf8),
                    FieldSpec::new("amount", TypeSpec::Float64),
                ],
            },
            EngineKind::Firefly,
        );
        engine.create_table(def).unwrap();
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("kind", DataType::Utf8, false),
            Field::new("amount", DataType::Float64, false),
        ]));
        let batch = arrow::record_batch::RecordBatch::try_new(
            schema,
            vec![
                Arc::new(Int64Array::from(vec![1, 2])),
                Arc::new(StringArray::from(vec!["click", "view"])),
                Arc::new(Float64Array::from(vec![1.0, 2.0])),
            ],
        )
        .unwrap();
        engine.insert("events", batch).unwrap();
        engine.flush("events").unwrap();
        let session = LessSession::new_async(engine.clone()).await.unwrap();
        let auth = match &engine.config.auth {
            Some(cfg) => less_auth::build(cfg).unwrap(),
            None => None,
        };
        (
            AppState {
                session: Arc::new(session),
                auth,
                mcp: None,
            },
            dir,
        )
    }

    fn json_body(v: serde_json::Value) -> Body {
        Body::from(v.to_string())
    }

    fn basic(user: &str, pass: &str) -> String {
        use base64::Engine as _;
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"))
        )
    }

    #[tokio::test]
    async fn open_server_queries_and_metrics() {
        let (state, dir) = test_state(None).await;
        let app = router(state);

        let res = app
            .clone()
            .oneshot(
                HttpRequest::post("/v1/sql")
                    .header("content-type", "application/json")
                    .body(json_body(serde_json::json!({
                        "sql": "SELECT count(*) AS n FROM events"
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let res = app
            .clone()
            .oneshot(HttpRequest::get("/metrics").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let (_, body) = res.into_parts();
        let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("lessdb_queries_total{status=\"ok\"}"));
        // The metric registry is process-global and tests run in parallel,
        // so assert "at least" on cumulative counters.
        let metric_at_least = |name: &str, min: u64| {
            text.lines()
                .find(|l| l.starts_with(name))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse::<u64>().ok())
                .map(|v| v >= min)
                .unwrap_or(false)
        };
        assert!(metric_at_least("lessdb_parts_written_total", 1));
        assert!(metric_at_least("lessdb_rows_inserted_total", 2));
        assert!(metric_at_least("lessdb_tables", 1));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn basic_auth_enforced_with_roles() {
        let (state, dir) = test_state(Some(
            r#"{"file":{"users":{
                "alice": {"password": "pw1", "role": "admin"},
                "bob":   {"password": "pw2", "role": "read"}
            }}}"#,
        ))
        .await;
        let app = router(state);

        // No credentials -> 401.
        let res = app
            .clone()
            .oneshot(
                HttpRequest::post("/v1/sql")
                    .header("content-type", "application/json")
                    .body(json_body(serde_json::json!({"sql": "SELECT 1"})))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        // Wrong password -> 401.
        let res = app
            .clone()
            .oneshot(
                HttpRequest::post("/v1/sql")
                    .header("content-type", "application/json")
                    .header("authorization", basic("alice", "nope"))
                    .body(json_body(serde_json::json!({"sql": "SELECT 1"})))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        // Valid credentials -> 200.
        let res = app
            .clone()
            .oneshot(
                HttpRequest::post("/v1/sql")
                    .header("content-type", "application/json")
                    .header("authorization", basic("alice", "pw1"))
                    .body(json_body(serde_json::json!({"sql": "SELECT 1 AS one"})))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // Admin can optimize.
        let res = app
            .clone()
            .oneshot(
                HttpRequest::post("/v1/admin/optimize")
                    .header("content-type", "application/json")
                    .header("authorization", basic("alice", "pw1"))
                    .body(json_body(serde_json::json!({"table": "events"})))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // read-only user is forbidden.
        let res = app
            .clone()
            .oneshot(
                HttpRequest::post("/v1/admin/optimize")
                    .header("content-type", "application/json")
                    .header("authorization", basic("bob", "pw2"))
                    .body(json_body(serde_json::json!({"table": "events"})))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn auth_failures_are_counted() {
        let (state, dir) = test_state(Some(
            r#"{"file":{"users":{"alice": {"password": "pw1", "role": "admin"}}}}"#,
        ))
        .await;
        let app = router(state);
        let _ = app
            .clone()
            .oneshot(
                HttpRequest::post("/v1/sql")
                    .header("content-type", "application/json")
                    .header("authorization", basic("alice", "wrong"))
                    .body(json_body(serde_json::json!({"sql": "SELECT 1"})))
                    .unwrap(),
            )
            .await
            .unwrap();
        let res = app
            .clone()
            .oneshot(HttpRequest::get("/metrics").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let (_, body) = res.into_parts();
        let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let failures = text
            .lines()
            .find(|l| l.starts_with("lessdb_auth_failures_total "))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        assert!(
            failures >= 1,
            "expected auth failures counted, got {failures}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// End-to-end HTTPS: self-signed cert on the server, rustls client
    /// trusting that cert, `/health` answered over TLS.
    #[tokio::test]
    async fn https_serves_health_over_tls() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let dir = std::env::temp_dir().join(format!("less-tls-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
        let cert_path = dir.join("cert.pem");
        let key_path = dir.join("key.pem");
        std::fs::write(&cert_path, certified.cert.pem()).unwrap();
        std::fs::write(&key_path, certified.key_pair.serialize_pem()).unwrap();

        // Ephemeral port.
        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = probe.local_addr().unwrap();
        drop(probe);

        let engine = LessEngine::open_local(dir.join("db")).unwrap();
        let session = LessSession::new_async(engine).await.unwrap();
        let tls = TlsPaths {
            cert: cert_path,
            key: key_path,
        };
        tokio::spawn(async move {
            let _ = serve_with_tls(Arc::new(session), &addr.to_string(), Some(&tls), None).await;
        });
        // Give the server a moment to bind.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        // rustls client trusting the self-signed cert.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let mut roots = rustls::RootCertStore::empty();
        roots.add(certified.cert.der().clone()).unwrap();
        let client_config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let server_name = rustls::pki_types::ServerName::try_from("localhost")
            .unwrap()
            .to_owned();
        let mut stream = connector.connect(server_name, tcp).await.unwrap();
        stream
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        let text = String::from_utf8_lossy(&buf);
        assert!(text.contains("200"), "expected 200 response, got: {text}");
        assert!(text.contains("ok"), "expected health body, got: {text}");
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod mcp_tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::Request as HttpRequest;
    use less_common::EngineConfig;
    use less_engine::LessEngine;
    use tower::ServiceExt;

    fn json_body(v: serde_json::Value) -> Body {
        Body::from(v.to_string())
    }

    async fn mcp_state(dir: &std::path::Path) -> (Arc<less_mcp::McpState>, String) {
        let engine = LessEngine::open(EngineConfig::with_data_dir(dir)).unwrap();
        let session = Arc::new(LessSession::new_async(engine).await.unwrap());
        let plain = {
            let mut tokens = less_auth::TokenStore::open(dir).unwrap();
            tokens.create("alice", "read", "default", None).unwrap()
        };
        let tokens = less_auth::TokenStore::open(dir).unwrap();
        let state =
            less_mcp::state_from_tenant_with_auth(session, "default", tokens, true).unwrap();
        (state, plain)
    }

    #[tokio::test]
    async fn mcp_http_endpoint_fail_closed_and_token_flow() {
        let dir = std::env::temp_dir().join(format!("less-mcp-http-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        let (mcp, token) = mcp_state(&dir).await;
        let app = router(AppState {
            session: mcp.session.clone(),
            auth: None,
            mcp: Some(mcp),
        });

        // initialize itself is a handshake and always succeeds…
        let res = app
            .clone()
            .oneshot(
                HttpRequest::post("/mcp")
                    .header("content-type", "application/json")
                    .body(json_body(serde_json::json!({
                        "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let text =
            String::from_utf8(to_bytes(res.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap();
        assert!(text.contains("protocolVersion"), "{text}");
        assert!(text.contains("\"name\":\"lessdb\""), "{text}");

        // …but tools/call without a verified token is denied (fail-closed).
        let res = app
            .clone()
            .oneshot(
                HttpRequest::post("/mcp")
                    .body(json_body(serde_json::json!({
                        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                        "params": {"name": "lessdb_tables", "arguments": {}}
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();
        let text =
            String::from_utf8(to_bytes(res.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap();
        assert!(text.contains("authentication required"), "{text}");

        // A bearer token on the HTTP request is merged into initialize and
        // then authorizes subsequent tools/calls.
        let init = serde_json::json!({
            "jsonrpc": "2.0", "id": 3, "method": "initialize", "params": {}
        });
        let res = app
            .clone()
            .oneshot(
                HttpRequest::post("/mcp")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(json_body(init))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let _ = to_bytes(res.into_body(), 1 << 20).await.unwrap();

        let res = app
            .clone()
            .oneshot(
                HttpRequest::post("/mcp")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(json_body(serde_json::json!({
                        "jsonrpc": "2.0", "id": 4, "method": "tools/call",
                        "params": {"name": "lessdb_tables", "arguments": {}}
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();
        let text =
            String::from_utf8(to_bytes(res.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap();
        assert!(text.contains("\"content\""), "{text}");

        // SSE clients get an event-stream framing.
        let res = app
            .clone()
            .oneshot(
                HttpRequest::post("/mcp")
                    .header("Accept", "text/event-stream")
                    .body(json_body(serde_json::json!({
                        "jsonrpc": "2.0", "id": 5, "method": "ping"
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();
        let text =
            String::from_utf8(to_bytes(res.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap();
        assert!(text.starts_with("event: message\ndata: "), "{text}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
