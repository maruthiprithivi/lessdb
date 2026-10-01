//! LessDB MCP (Model Context Protocol) server — native access for AI agents.
//!
//! Runs over stdio (newline-delimited JSON-RPC 2.0), implementing the MCP
//! `initialize` / `tools/list` / `tools/call` flow with a hand-rolled,
//! dependency-free transport so the server stays stable across MCP SDK
//! churn.
//!
//! Tool families:
//! * **database** — `less_query`, `less_explain`, `less_tables`,
//!   `less_schema`, `less_stats`, `less_optimize` (Firefly /
//!   FireflyCloud SQL database);
//! * **context** — `context_put/get/find/link/unlink/neighbors/path/delete`
//!   (in-memory graph of notes — the Obsidian/Neo4j replacement);
//! * **memory** — `memory_create/insert/get/sql/compact/tables`
//!   (RAM-resident SQL tables with primary-key point lookups).
//!
//! Run it with `less mcp`, or add it to a client like Claude Desktop:
//! ```json
//! { "mcpServers": { "lessdb": { "command": "less", "args": ["mcp", "--dir", "/path/to/db"] } } }
//! ```

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use arrow::util::pretty::pretty_format_batches;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use less_common::{LessError, ROLE_ADMIN, ROLE_READ, ROLE_WRITE, Result};
use less_graph::{ContextStore, Direction};
use less_memory::MemoryStore;
use less_query::LessSession;

/// Default tenant for the MCP server when no `--tenant` is given.
pub const DEFAULT_TENANT: &str = "default";

/// An authenticated caller on the MCP door (resolved at `initialize`).
#[derive(Debug, Clone)]
struct CallerInfo {
    name: String,
    role: String,
}

/// Shared state behind every tool call.
pub struct McpState {
    pub session: Arc<LessSession>,
    /// Tenant namespace for the agent-memory tier (context/memory/vectors).
    /// SQL (`less_*`) tools stay on the shared engine, not tenant-scoped.
    pub tenant: String,
    pub context: Mutex<ContextStore>,
    pub memory: Arc<MemoryStore>,
    /// Vector registry backing the MCP `vector_*` tools, tenant-scoped. The
    /// session's own `vector_search` SQL table function keeps the shared
    /// engine registry at `<data_dir>/vectors/`.
    pub vectors: Arc<RwLock<less_vector::VectorRegistry>>,
    /// Agent token store (optional; `--require-auth` populates it).
    pub tokens: Option<Arc<less_auth::TokenStore>>,
    /// Fail-closed: when true, every `tools/call` needs a verified token.
    pub auth_required: bool,
    /// Append-only audit trail for this door (attributable by default).
    pub audit: Option<Arc<less_telemetry::AuditLog>>,
    /// Caller resolved from the `initialize` token (None until then).
    caller: RwLock<Option<CallerInfo>>,
}

impl McpState {
    /// Open the database session plus the in-memory context/memory/vector
    /// tier under `<data_dir>/tenants/default/`.
    pub fn new(session: Arc<LessSession>) -> Result<Self> {
        Self::new_with_tenant(session, DEFAULT_TENANT)
    }

    /// Open the database session plus a tenant-namespaced agent-memory tier
    /// at `<data_dir>/tenants/<tenant>/memory/` (contexts + memory tables)
    /// and `<data_dir>/tenants/<tenant>/vectors/`. The engine itself is
    /// shared: `less_*` SQL tools are not tenant-scoped.
    ///
    /// An audit log is opened under `<data_dir>/audit/` by default, so every
    /// tool call is attributable even before `--require-auth` is turned on.
    pub fn new_with_tenant(session: Arc<LessSession>, tenant: &str) -> Result<Self> {
        validate_tenant(tenant)?;
        let root = tenant_root_for(&session.engine().config.data_dir, tenant);
        let memory_dir = root.join("memory");
        let vectors = Arc::new(RwLock::new(less_vector::VectorRegistry::open(Some(
            &root.join("vectors"),
        ))?));
        let audit = less_telemetry::AuditLog::open(
            &session
                .engine()
                .config
                .data_dir
                .join(less_telemetry::audit::AUDIT_DIR),
        )
        .ok()
        .map(Arc::new);
        Ok(Self {
            session,
            tenant: tenant.to_string(),
            context: Mutex::new(ContextStore::open(Some(&memory_dir))?),
            memory: Arc::new(MemoryStore::open(Some(&memory_dir))?),
            vectors,
            tokens: None,
            auth_required: false,
            audit,
            caller: RwLock::new(None),
        })
    }

    /// Enable the fail-closed control plane on this door: agent tokens are
    /// required and every tool call is checked against the caller's role.
    pub fn with_auth(mut self, tokens: less_auth::TokenStore, require: bool) -> Self {
        self.tokens = Some(Arc::new(tokens));
        self.auth_required = require;
        self
    }

    fn current_caller(&self) -> Option<CallerInfo> {
        self.caller.read().unwrap().clone()
    }
}

/// The permission class a tool needs. Fail-safe default is `read` — an
/// unknown tool can never widen a caller's powers.
fn tool_permission(name: &str) -> &'static str {
    match name {
        // Schema and space lifecycle: admins only.
        "vector_create" | "vector_drop" | "memory_create" => "admin",
        // Anything that mutates durable state: write role or above.
        "lessdb_optimize" | "context_put" | "context_link" | "context_unlink"
        | "context_delete" | "memory_insert" | "memory_compact" | "vector_put" => "write",
        _ => "read",
    }
}

/// Role check for the permission ladder: admin > write > read.
fn role_allows(role: Option<&str>, permission: &str) -> bool {
    match role {
        Some(ROLE_ADMIN) => true,
        Some(ROLE_WRITE) => permission != "admin",
        Some(ROLE_READ) => permission == "read",
        _ => false,
    }
}

/// A tenant name becomes a path component under `<data_dir>/tenants/`, so
/// reject anything that could escape that root (empty, `..`, separators).
fn validate_tenant(tenant: &str) -> Result<()> {
    if tenant.is_empty()
        || !tenant
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    {
        return Err(LessError::Config(format!(
            "invalid tenant name '{tenant}' (expected [A-Za-z0-9_.-]+)"
        )));
    }
    Ok(())
}

/// Tenant-namespaced root directory for the MCP agent-memory tier.
pub fn tenant_root_for(data_dir: &Path, tenant: &str) -> PathBuf {
    data_dir.join("tenants").join(tenant)
}

#[derive(Debug, Deserialize)]
struct Request {
    #[allow(dead_code)]
    jsonrpc: String,
    /// JSON-RPC notifications (e.g. `notifications/initialized`) carry no id.
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Serialize)]
struct Response {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

impl Response {
    fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }
    fn err(id: Value, code: i64, message: String) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(json!({ "code": code, "message": message })),
        }
    }
}

fn tool(name: &str, description: &str, properties: Value, required: Vec<&str>) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
        }
    })
}

fn tools() -> Value {
    json!([
        tool(
            "lessdb_query",
            "Run a SQL query against the LessDB database. Returns rows as an aligned text table \
             (or compact JSON with format='json').",
            json!({
                "sql": { "type": "string", "description": "SQL statement" },
                "format": { "type": "string", "enum": ["table", "json"], "description": "output format (default table)" }
            }),
            vec!["sql"]
        ),
        tool(
            "lessdb_explain",
            "Show the physical execution plan for a SQL query, including data-part pruning.",
            json!({ "sql": { "type": "string" } }),
            vec!["sql"]
        ),
        tool(
            "lessdb_tables",
            "List all tables in the database.",
            json!({}),
            vec![]
        ),
        tool(
            "lessdb_schema",
            "Describe a database table: columns, types, sort key, uniqueness constraints.",
            json!({ "table": { "type": "string" } }),
            vec!["table"]
        ),
        tool(
            "lessdb_stats",
            "Database table statistics: row count, part count, on-disk size.",
            json!({ "table": { "type": "string" } }),
            vec!["table"]
        ),
        tool(
            "lessdb_optimize",
            "Merge all parts of a table and enforce UNIQUE constraints (OPTIMIZE TABLE).",
            json!({ "table": { "type": "string" } }),
            vec!["table"]
        ),
        tool(
            "context_put",
            "Store (create or update) a context note in the in-memory graph. Use this to remember \
             durable facts, decisions, project state and references for later sessions.",
            json!({
                "key": { "type": "string", "description": "stable key, e.g. proj/lessdb or task/123" },
                "title": { "type": "string" },
                "text": { "type": "string", "description": "body text of the note" },
                "tags": { "type": "array", "items": { "type": "string" }, "description": "optional tags" },
                "kind": { "type": "string", "description": "note | project | task | decision | person | doc (default note)" }
            }),
            vec!["key", "title", "text"]
        ),
        tool(
            "context_get",
            "Fetch one context note by key.",
            json!({ "key": { "type": "string" } }),
            vec!["key"]
        ),
        tool(
            "context_find",
            "Ranked search over context keys, titles, tags and text.",
            json!({
                "query": { "type": "string" },
                "limit": { "type": "integer", "description": "max results (default 10)" }
            }),
            vec!["query"]
        ),
        tool(
            "context_link",
            "Link two nodes with a typed edge (e.g. depends_on, mentions, part_of). \
             Missing endpoints are auto-created.",
            json!({
                "from": { "type": "string" },
                "to": { "type": "string" },
                "kind": { "type": "string" },
                "directed": { "type": "boolean", "description": "default false" }
            }),
            vec!["from", "to", "kind"]
        ),
        tool(
            "context_unlink",
            "Remove link(s) between two nodes (all kinds when kind omitted).",
            json!({
                "from": { "type": "string" },
                "to": { "type": "string" },
                "kind": { "type": "string" }
            }),
            vec!["from", "to"]
        ),
        tool(
            "context_neighbors",
            "BFS neighborhood around a node within a depth.",
            json!({
                "key": { "type": "string" },
                "depth": { "type": "integer", "description": "default 1" },
                "direction": { "type": "string", "enum": ["out", "in", "both"], "description": "default out" }
            }),
            vec!["key"]
        ),
        tool(
            "context_path",
            "Shortest path between two nodes (unweighted BFS).",
            json!({ "from": { "type": "string" }, "to": { "type": "string" } }),
            vec!["from", "to"]
        ),
        tool(
            "context_delete",
            "Delete a context node and all its edges.",
            json!({ "key": { "type": "string" } }),
            vec!["key"]
        ),
        tool(
            "lessdb_cypher",
            "Query the context graph with openCypher: MATCH (n:Label {prop: v})-[r:TYPE*1..3]->(m) \
             [WHERE ...] RETURN n.prop, count(*) [ORDER BY ...] [SKIP/LIMIT]; CREATE/DELETE/SET \
             also supported. id(n) returns the node key.",
            json!({ "query": { "type": "string", "description": "openCypher query" } }),
            vec!["query"]
        ),
        tool(
            "context_stats",
            "Counts of context nodes and edges.",
            json!({}),
            vec![]
        ),
        tool(
            "vector_create",
            "Create a vector space (a named collection of same-dimension vectors with a metric \
             and an index). metric: l2 | cosine | dot; index: flat (exact) or ivf_pq (approximate).",
            json!({
                "space": { "type": "string" },
                "dim": { "type": "integer" },
                "metric": { "type": "string", "enum": ["l2", "cosine", "dot"], "description": "default l2" },
                "index": { "type": "string", "enum": ["flat", "ivf_pq"], "description": "default flat" },
                "nlist": { "type": "integer", "description": "IVF-PQ inverted lists (optional)" },
                "m": { "type": "integer", "description": "IVF-PQ PQ subspaces (optional)" }
            }),
            vec!["space", "dim"]
        ),
        tool(
            "vector_put",
            "Add vectors (optionally with JSON payloads) to a vector space. Use together with \
             vector_search for retrieval; payloads come back with the hits.",
            json!({
                "space": { "type": "string" },
                "vectors": { "type": "array", "items": { "type": "array", "items": { "type": "number" } } },
                "payloads": { "type": "array", "items": { "type": "object" }, "description": "one per vector (optional)" }
            }),
            vec!["space", "vectors"]
        ),
        tool(
            "vector_search",
            "k-nearest-neighbor search in a vector space. Returns id, score (smaller = closer) \
             and payload.",
            json!({
                "space": { "type": "string" },
                "query": { "type": "array", "items": { "type": "number" } },
                "k": { "type": "integer", "description": "default 5" },
                "nprobe": { "type": "integer", "description": "IVF-PQ probe count (optional)" }
            }),
            vec!["space", "query"]
        ),
        tool(
            "vector_list",
            "List vector spaces (name, dim, metric, index, count).",
            json!({}),
            vec![]
        ),
        tool(
            "vector_embed",
            "Embed text with a registered embedding function (built-in: 'trigram'). Returns the vector.",
            json!({
                "embedder": { "type": "string" },
                "text": { "type": "string" }
            }),
            vec!["embedder", "text"]
        ),
        tool(
            "vector_drop",
            "Delete a vector space.",
            json!({ "space": { "type": "string" } }),
            vec!["space"]
        ),
        tool(
            "memory_create",
            "Create a RAM-resident table: fields are [{\"name\":..., \"type\":\"Int64|Utf8|Float64|Bool|...\"}], \
             optional pk column enables point lookups.",
            json!({
                "table": { "type": "string" },
                "fields": { "type": "array", "items": { "type": "object" } },
                "pk": { "type": "string" }
            }),
            vec!["table", "fields"]
        ),
        tool(
            "memory_insert",
            "Insert rows (JSON array of objects) into a memory table.",
            json!({
                "table": { "type": "string" },
                "rows": { "type": "array", "items": { "type": "object" } }
            }),
            vec!["table", "rows"]
        ),
        tool(
            "memory_get",
            "Point lookup by primary key — returns the latest row.",
            json!({ "table": { "type": "string" }, "key": { "type": "string" } }),
            vec!["table", "key"]
        ),
        tool(
            "memory_sql",
            "Run SQL over the in-memory tables (joins, aggregation, everything DataFusion supports).",
            json!({ "sql": { "type": "string" } }),
            vec!["sql"]
        ),
        tool(
            "memory_compact",
            "Deduplicate a memory table by primary key (keep last).",
            json!({ "table": { "type": "string" } }),
            vec!["table"]
        ),
        tool("memory_tables", "List in-memory tables.", json!({}), vec![]),
    ])
}

/// Serve MCP over stdio until stdin closes.
pub async fn run_stdio(state: Arc<McpState>) -> Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let mut lines = BufReader::new(stdin).lines();
    let mut out = stdout;

    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match handle_request_text(&state, line).await {
            Ok(resp) => {
                if resp.is_empty() {
                    continue; // notification: nothing to send back
                }
                out.write_all(resp.as_bytes()).await?;
                out.write_all(b"\n").await?;
                out.flush().await?;
            }
            Err(e) => {
                let resp = Response::err(Value::Null, -32000, e.to_string());
                let line = serde_json::to_string(&resp)?;
                out.write_all(line.as_bytes()).await?;
                out.write_all(b"\n").await?;
                out.flush().await?;
            }
        }
    }
    Ok(())
}

/// Handle one JSON-RPC request body over ANY transport (stdio, HTTP,
/// SSE, …) and return the response JSON text. Notifications (no `id`)
/// execute for effect and return an empty string. The caller is resolved
/// per request from the message itself (initialize token), stdio-style.
pub async fn handle_request_text(state: &Arc<McpState>, body: &str) -> Result<String> {
    handle_request(state, body, None).await
}

/// HTTP transport variant: `bearer` (from the HTTP `Authorization`
/// header) is verified and resolved **per request** — a hosted door
/// serves many agents, so the caller must never be process-global.
pub async fn handle_request_http(
    state: &Arc<McpState>,
    body: &str,
    bearer: Option<&str>,
) -> Result<String> {
    handle_request(state, body, bearer).await
}

async fn handle_request(state: &Arc<McpState>, body: &str, bearer: Option<&str>) -> Result<String> {
    let req: Request = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => {
            return Ok(serde_json::to_string(&Response::err(
                Value::Null,
                -32700,
                format!("parse error: {e}"),
            ))?);
        }
    };
    // Resolve the HTTP bearer against the token store (tenant must
    // match). Unknown/missing tokens fall through to the door's own
    // rules — fail-closed when auth is required.
    let caller = bearer
        .and_then(|t| state.tokens.as_ref().and_then(|s| s.verify(t)))
        .and_then(|rec| {
            if rec.tenant.is_empty() || rec.tenant == state.tenant {
                Some(CallerInfo {
                    name: rec.name.clone(),
                    role: rec.role.clone(),
                })
            } else {
                None
            }
        });
    let Some(id) = req.id.clone() else {
        let _ = handle(state, &req, caller).await;
        return Ok(String::new());
    };
    let resp = match handle(state, &req, caller).await {
        Ok(result) => Response::ok(id, result),
        Err(e) => Response::err(id, -32000, e.to_string()),
    };
    Ok(serde_json::to_string(&resp)?)
}

async fn handle(
    state: &Arc<McpState>,
    req: &Request,
    http_caller: Option<CallerInfo>,
) -> std::result::Result<Value, LessError> {
    match req.method.as_str() {
        "initialize" => {
            // Resolve the caller from an optional bearer token (Authorization
            // header or `_meta.lessdbToken`) — the agent credential on this
            // door. Fail closed when auth is required; otherwise the door
            // stays open (backward-compatible local dev). An HTTP bearer
            // (already verified per request) wins over message tokens.
            let token = extract_token(req.params.as_ref());
            let verified = token.and_then(|t| state.tokens.as_ref().and_then(|s| s.verify(t)));
            let caller = match http_caller {
                Some(c) => c,
                None => match verified {
                    Some(rec) if rec.tenant.is_empty() || rec.tenant == state.tenant => {
                        CallerInfo {
                            name: rec.name.clone(),
                            role: rec.role.clone(),
                        }
                    }
                    _ => CallerInfo {
                        name: "anonymous".to_string(),
                        role: if state.auth_required {
                            String::new()
                        } else {
                            ROLE_ADMIN.to_string()
                        },
                    },
                },
            };
            *state.caller.write().unwrap() = Some(caller);
            Ok(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "lessdb", "version": less_common::VERSION },
                "tenant": state.tenant,
                "instructions": format!(
                    "LessDB: SQL analytics (lessdb_* tools, shared engine) + tenant-namespaced \
                     in-memory context graph (context_*), RAM tables (memory_*) and vector \
                     spaces (vector_*) for agent '{}'. Store durable facts with context_put, \
                     link them with context_link, and query analytics with lessdb_query.",
                    state.tenant
                )
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => {
            let params = req
                .params
                .as_ref()
                .ok_or_else(|| LessError::Query("tools/call missing params".into()))?;
            let name = params["name"]
                .as_str()
                .ok_or_else(|| LessError::Query("tools/call missing tool name".into()))?;
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

            // ---- control plane: authenticate, authorize, audit -------------
            // Per-request HTTP bearer wins; stdio falls back to the caller
            // resolved at initialize.
            let caller = http_caller
                .or_else(|| state.current_caller())
                .unwrap_or(CallerInfo {
                    name: "anonymous".to_string(),
                    role: String::new(),
                });
            let (denied, role): (Option<String>, String) =
                if state.auth_required && !role_allows(Some(&caller.role), "read") {
                    (
                        Some(
                            "authentication required: present a valid agent token at initialize \
                         (Authorization: Bearer … or _meta.lessdbToken)"
                                .to_string(),
                        ),
                        caller.role,
                    )
                } else if state.auth_required {
                    let permission = tool_permission(name);
                    if role_allows(Some(&caller.role), permission) {
                        (None, caller.role)
                    } else {
                        (
                            Some(format!(
                                "access denied: tool '{name}' requires role '{permission}' \
                             (caller role: '{}')",
                                if caller.role.is_empty() {
                                    "none"
                                } else {
                                    &caller.role
                                }
                            )),
                            caller.role,
                        )
                    }
                } else {
                    (None, ROLE_ADMIN.to_string())
                };

            if let Some(reason) = denied {
                if let Some(audit) = &state.audit {
                    let _ = audit.record(
                        &less_telemetry::AuditEntry::new(
                            "agent",
                            &caller.name,
                            &state.tenant,
                            "mcp",
                            "tools/call",
                        )
                        .tool(name)
                        .role(&role)
                        .outcome("denied")
                        .detail(&reason),
                    );
                }
                return Ok(json!({
                    "content": [{ "type": "text", "text": reason }],
                    "isError": true,
                }));
            }

            let start = std::time::Instant::now();
            let result = call_tool(state, name, &arguments).await;
            let (text, is_error) = match result {
                Ok(text) => (text, false),
                Err(e) => (format!("error: {e}"), true),
            };
            if let Some(audit) = &state.audit {
                let mut entry = less_telemetry::AuditEntry::new(
                    "agent",
                    &caller.name,
                    &state.tenant,
                    "mcp",
                    "tools/call",
                )
                .tool(name)
                .role(&role)
                .outcome(if is_error { "error" } else { "ok" })
                .dur_ms(start.elapsed().as_secs_f64() * 1000.0);
                if let Some(sql) = arguments.get("sql").and_then(|v| v.as_str()) {
                    entry = entry.detail(sql);
                }
                let _ = audit.record(&entry);
            }
            Ok(json!({
                "content": [{ "type": "text", "text": text }],
                "isError": is_error,
            }))
        }
        "resources/list" => Ok(json!({ "resources": [] })),
        "prompts/list" => Ok(json!({ "prompts": [] })),
        // Notifications: no response body needed; return null result.
        m if m.starts_with("notifications/") => Ok(Value::Null),
        other => Err(LessError::NotImplemented(format!(
            "unknown MCP method '{other}'"
        ))),
    }
}

/// Extract a bearer token from `initialize` params: `headers.Authorization`
/// (`Bearer <token>`) or `_meta.lessdbToken`.
fn extract_token(params: Option<&Value>) -> Option<&str> {
    let p = params?;
    p.get("headers")
        .and_then(|h| h.get("Authorization"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.strip_prefix("Bearer "))
        .or_else(|| {
            p.get("_meta")
                .and_then(|m| m.get("lessdbToken"))
                .and_then(|v| v.as_str())
        })
}

fn arg_str<'a>(args: &'a Value, name: &str) -> std::result::Result<&'a str, LessError> {
    args.get(name)
        .and_then(|v| v.as_str())
        .ok_or_else(|| LessError::Query(format!("missing string argument '{name}'")))
}

async fn call_tool(
    state: &Arc<McpState>,
    name: &str,
    args: &Value,
) -> std::result::Result<String, LessError> {
    match name {
        // ---- database ----------------------------------------------------
        "lessdb_query" => {
            let sql = arg_str(args, "sql")?;
            let batches = state.session.sql_batches(sql).await?;
            if batches.is_empty() || batches.iter().all(|b| b.num_rows() == 0) {
                return Ok("(0 rows)".to_string());
            }
            if args["format"].as_str() == Some("json") {
                let mut buf = Vec::new();
                let mut writer =
                    arrow_json::writer::Writer::<_, arrow_json::writer::JsonArray>::new(&mut buf);
                for b in &batches {
                    writer.write(b)?;
                }
                writer.finish()?;
                Ok(String::from_utf8_lossy(&buf).to_string())
            } else {
                Ok(pretty_format_batches(&batches)?.to_string())
            }
        }
        "lessdb_explain" => {
            let sql = arg_str(args, "sql")?;
            let batches = state.session.explain_batches(sql).await?;
            Ok(pretty_format_batches(&batches)?.to_string())
        }
        "lessdb_tables" => Ok(format!(
            "tables: {}",
            state.session.engine().tables_async().await?.join(", ")
        )),
        "lessdb_schema" => {
            let table = arg_str(args, "table")?;
            let def = state.session.engine().table(table)?;
            let mut out = format!(
                "table {} (engine={})\n",
                def.name,
                match def.engine {
                    less_catalog::EngineKind::Firefly => "Firefly",
                    less_catalog::EngineKind::FireflyCloud => "FireflyCloud",
                }
            );
            if !def.sort_key.is_empty() {
                out.push_str(&format!("sort key: {}\n", def.sort_key.join(", ")));
            }
            if !def.unique.is_empty() {
                out.push_str(&format!("unique:   {}\n", def.unique.join(", ")));
            }
            for f in &def.schema.fields {
                out.push_str(&format!("  {}  {}\n", f.name, f.ty.name()));
            }
            Ok(out)
        }
        "lessdb_stats" => {
            let table = arg_str(args, "table")?.to_string();
            let engine = state.session.engine().clone();
            let stats = tokio::task::spawn_blocking(move || engine.stats(&table))
                .await
                .map_err(|e| LessError::Query(e.to_string()))??;
            Ok(format!(
                "table {}: {} rows, {} parts, {:.2} MB on disk, {} rows buffered",
                stats.table,
                stats.rows,
                stats.part_count,
                stats.disk_bytes as f64 / 1e6,
                stats.buffered_rows
            ))
        }
        "lessdb_optimize" => {
            let table = arg_str(args, "table")?.to_string();
            let engine = state.session.engine().clone();
            let t = table.clone();
            let merged = tokio::task::spawn_blocking(move || engine.optimize(&t))
                .await
                .map_err(|e| LessError::Query(e.to_string()))??;
            match merged {
                Some(meta) => Ok(format!(
                    "merged {table} into part {} ({} rows)",
                    meta.name, meta.row_count
                )),
                None => Ok(format!(
                    "table {table} has fewer than 2 parts; nothing to merge"
                )),
            }
        }

        // ---- context graph ----------------------------------------------
        "context_put" => {
            let key = arg_str(args, "key")?.to_string();
            let title = arg_str(args, "title")?.to_string();
            let text = arg_str(args, "text")?.to_string();
            let tags: Vec<String> = args["tags"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let kind = args["kind"].as_str().unwrap_or("note").to_string();
            let mut store = state.context.lock().unwrap();
            let ctx = store.put(&key, &title, &text, tags, &kind, Default::default())?;
            store.flush()?;
            Ok(format!(
                "stored context {key} (title: {}, kind: {}, {} tags)",
                ctx.title,
                ctx.kind,
                ctx.tags.len()
            ))
        }
        "context_get" => {
            let key = arg_str(args, "key")?;
            let store = state.context.lock().unwrap();
            match store.get(key) {
                Some(ctx) => Ok(serde_json::to_string_pretty(&ctx)?),
                None => Ok(format!("context '{key}' not found")),
            }
        }
        "context_find" => {
            let query = arg_str(args, "query")?;
            let limit = args["limit"].as_u64().unwrap_or(10) as usize;
            let store = state.context.lock().unwrap();
            let hits = store.find(query, limit);
            if hits.is_empty() {
                return Ok("(no matches)".to_string());
            }
            let mut out = String::new();
            for h in hits {
                out.push_str(&format!(
                    "{}  [{}]  kind={}  score={}\n  {} — {}\n",
                    h.key,
                    h.tags.join(","),
                    h.kind,
                    h.score,
                    h.title,
                    h.snippet
                ));
            }
            Ok(out)
        }
        "context_link" => {
            let from = arg_str(args, "from")?.to_string();
            let to = arg_str(args, "to")?.to_string();
            let kind = arg_str(args, "kind")?.to_string();
            let directed = args["directed"].as_bool().unwrap_or(false);
            let mut store = state.context.lock().unwrap();
            let edge = store.link(&from, &to, &kind, directed, Default::default())?;
            store.flush()?;
            Ok(format!(
                "linked {from} -[{}{}]-> {to}",
                edge.kind,
                if edge.directed { "" } else { " (undirected)" }
            ))
        }
        "context_unlink" => {
            let from = arg_str(args, "from")?;
            let to = arg_str(args, "to")?;
            let kind = args["kind"].as_str();
            let mut store = state.context.lock().unwrap();
            let n = store.unlink(from, to, kind)?;
            store.flush()?;
            Ok(format!("removed {n} link(s)"))
        }
        "context_neighbors" => {
            let key = arg_str(args, "key")?;
            let depth = args["depth"].as_u64().unwrap_or(1) as u32;
            let direction = Direction::parse(args["direction"].as_str().unwrap_or("out"))?;
            let store = state.context.lock().unwrap();
            let neighbors = store.neighbors(key, direction, depth)?;
            if neighbors.is_empty() {
                return Ok("(no neighbors)".to_string());
            }
            let mut out = String::new();
            for n in neighbors {
                out.push_str(&format!(
                    "depth {}  {}  via {}\n",
                    n.depth,
                    n.node,
                    n.via.as_deref().unwrap_or("-")
                ));
            }
            Ok(out)
        }
        "context_path" => {
            let from = arg_str(args, "from")?;
            let to = arg_str(args, "to")?;
            let store = state.context.lock().unwrap();
            match store.path(from, to) {
                Some(hops) => {
                    let mut out = format!("{from}\n");
                    for h in &hops {
                        out.push_str(&format!("  -[{}]-> {}\n", h.kind, h.to));
                    }
                    out.push_str(&format!("({} hops)", hops.len()));
                    Ok(out)
                }
                None => Ok("no path".to_string()),
            }
        }
        "context_delete" => {
            let key = arg_str(args, "key")?;
            let mut store = state.context.lock().unwrap();
            let removed = store.delete(key)?;
            store.flush()?;
            Ok(if removed {
                format!("deleted {key}")
            } else {
                format!("{key} not found")
            })
        }
        "lessdb_cypher" => {
            let query = arg_str(args, "query")?;
            let mut store = state.context.lock().unwrap();
            let json = less_cypher::run_json(store.graph_mut(), query)?;
            store.flush()?;
            Ok(json)
        }
        "context_stats" => {
            let store = state.context.lock().unwrap();
            let stats = store.stats();
            Ok(format!("nodes: {}  edges: {}", stats.nodes, stats.edges))
        }

        // ---- vector search ----------------------------------------------
        "vector_create" => {
            let space = arg_str(args, "space")?.to_string();
            let dim = args["dim"]
                .as_u64()
                .ok_or_else(|| LessError::Query("missing integer 'dim'".into()))?
                as usize;
            let metric = less_vector::Metric::parse(args["metric"].as_str().unwrap_or("l2"))?;
            let index = match args["index"].as_str().unwrap_or("flat") {
                "flat" => less_vector::IndexKind::Flat,
                "ivf_pq" | "ivfpq" => {
                    let mut params = less_vector::IvfPqParams::for_dim(dim);
                    if let Some(v) = args["nlist"].as_u64() {
                        params.nlist = v as usize;
                    }
                    if let Some(v) = args["m"].as_u64() {
                        params.m = v as usize;
                    }
                    less_vector::IndexKind::IvfPq(params)
                }
                other => {
                    return Err(LessError::Config(format!(
                        "unknown index '{other}' (expected flat | ivf_pq)"
                    )));
                }
            };
            let mut vectors = state.vectors.write().unwrap();
            vectors.create_space(&space, dim, metric, index)?;
            Ok(format!("created vector space {space} (dim {dim})"))
        }
        "vector_put" => {
            let space = arg_str(args, "space")?.to_string();
            let rows = args["vectors"]
                .as_array()
                .ok_or_else(|| LessError::Query("missing 'vectors' array".into()))?;
            let mut vecs = Vec::with_capacity(rows.len());
            for r in rows {
                let v: Vec<f32> = r
                    .as_array()
                    .ok_or_else(|| LessError::Query("each vector must be an array".into()))?
                    .iter()
                    .map(|x| x.as_f64().map(|f| f as f32).unwrap_or(0.0))
                    .collect();
                vecs.push(v);
            }
            let payloads: Vec<Value> = args["payloads"].as_array().cloned().unwrap_or_default();
            let mut vectors = state.vectors.write().unwrap();
            let ids = vectors.add(&space, vecs, payloads)?;
            Ok(format!(
                "added {} vectors to {space} (ids {}..{})",
                ids.len(),
                ids.first().copied().unwrap_or(0),
                ids.last().copied().unwrap_or(0)
            ))
        }
        "vector_search" => {
            let space = arg_str(args, "space")?.to_string();
            let query: Vec<f32> = args["query"]
                .as_array()
                .ok_or_else(|| LessError::Query("missing 'query' array".into()))?
                .iter()
                .map(|x| x.as_f64().map(|f| f as f32).unwrap_or(0.0))
                .collect();
            let k = args["k"].as_u64().unwrap_or(5) as usize;
            let nprobe = args["nprobe"].as_u64().unwrap_or(8) as usize;
            let vectors = state.vectors.read().unwrap();
            let hits = vectors.search(&space, query, k, nprobe)?;
            if hits.is_empty() {
                return Ok("(no results)".to_string());
            }
            let mut out = String::new();
            for h in hits {
                out.push_str(&format!(
                    "id {}  score {:.6}  payload {}\n",
                    h.id, h.score, h.payload
                ));
            }
            Ok(out)
        }
        "vector_list" => {
            let vectors = state.vectors.read().unwrap();
            let infos = vectors.list_spaces();
            if infos.is_empty() {
                return Ok("(no vector spaces)".to_string());
            }
            let mut out = String::new();
            for i in infos {
                out.push_str(&format!(
                    "{}  dim={}  metric={}  index={}  count={}\n",
                    i.name, i.dim, i.metric, i.index, i.count
                ));
            }
            Ok(out)
        }
        "vector_embed" => {
            let embedder = arg_str(args, "embedder")?;
            let text = arg_str(args, "text")?;
            let vectors = state.vectors.read().unwrap();
            let v = vectors.embed(embedder, text)?;
            Ok(serde_json::to_string(&v)?)
        }
        "vector_drop" => {
            let space = arg_str(args, "space")?;
            let mut vectors = state.vectors.write().unwrap();
            let removed = vectors.drop_space(space)?;
            Ok(if removed {
                format!("dropped vector space {space}")
            } else {
                format!("vector space {space} not found")
            })
        }

        // ---- memory tables ----------------------------------------------
        "memory_create" => {
            let table = arg_str(args, "table")?.to_string();
            let fields = args["fields"]
                .as_array()
                .ok_or_else(|| LessError::Query("missing 'fields' array".into()))?;
            let mut specs = vec![];
            for f in fields {
                let name = f["name"]
                    .as_str()
                    .ok_or_else(|| LessError::Query("field needs 'name'".into()))?;
                let ty = f["type"]
                    .as_str()
                    .ok_or_else(|| LessError::Query("field needs 'type'".into()))?;
                specs.push(less_catalog::FieldSpec::new(
                    name,
                    less_catalog::TypeSpec::parse(ty)?,
                ));
            }
            let pk = args["pk"].as_str().map(|s| s.to_string());
            state.memory.create_table(&table, specs, pk)?;
            Ok(format!("created memory table {table}"))
        }
        "memory_insert" => {
            let table = arg_str(args, "table")?.to_string();
            let rows = args["rows"]
                .as_array()
                .ok_or_else(|| LessError::Query("missing 'rows' array".into()))?;
            let manifest = state.memory.describe(&table)?;
            let schema = less_catalog::SchemaSpec {
                fields: manifest.fields,
            }
            .to_arrow();
            let json = serde_json::to_string(rows)?;
            let batches = less_memory::json_to_batches(schema, &json)?;
            let mut inserted = 0usize;
            for b in batches {
                inserted += state.memory.insert(&table, b)?;
            }
            Ok(format!("inserted {inserted} rows into {table}"))
        }
        "memory_get" => {
            let table = arg_str(args, "table")?;
            let key = arg_str(args, "key")?;
            let value: Value = match key.parse::<i64>() {
                Ok(v) => json!(v),
                Err(_) => json!(key),
            };
            match state.memory.point_get(table, &value)? {
                Some(row) => Ok(serde_json::to_string_pretty(&row)?),
                None => Ok("(not found)".to_string()),
            }
        }
        "memory_sql" => {
            let sql = arg_str(args, "sql")?;
            let batches = state.memory.sql_async(sql).await?;
            if batches.is_empty() || batches.iter().all(|b| b.num_rows() == 0) {
                return Ok("(0 rows)".to_string());
            }
            Ok(pretty_format_batches(&batches)?.to_string())
        }
        "memory_compact" => {
            let table = arg_str(args, "table")?;
            let removed = state.memory.compact(table)?;
            Ok(format!("removed {removed} duplicate row(s) from {table}"))
        }
        "memory_tables" => Ok(format!(
            "memory tables: {}",
            state.memory.table_names().join(", ")
        )),
        other => Err(LessError::NotImplemented(format!("unknown tool '{other}'"))),
    }
}

/// Convenience constructor used by the CLI (default tenant).
pub fn state_from(session: Arc<LessSession>) -> Result<Arc<McpState>> {
    Ok(Arc::new(McpState::new(session)?))
}

/// Convenience constructor for a named tenant (`less mcp --tenant <name>`).
pub fn state_from_tenant(session: Arc<LessSession>, tenant: &str) -> Result<Arc<McpState>> {
    Ok(Arc::new(McpState::new_with_tenant(session, tenant)?))
}

/// Same as [`state_from_tenant`], with the fail-closed control plane enabled:
/// agent tokens are required and tool calls are role-checked.
pub fn state_from_tenant_with_auth(
    session: Arc<LessSession>,
    tenant: &str,
    tokens: less_auth::TokenStore,
    require: bool,
) -> Result<Arc<McpState>> {
    Ok(Arc::new(
        McpState::new_with_tenant(session, tenant)?.with_auth(tokens, require),
    ))
}

/// Directory the non-tenanted context/memory tier persists to (the CLI's
/// `less context` / `less memory` layout), for callers that build their own
/// state. The MCP server instead uses [`tenant_root_for`].
pub fn memory_dir_for(data_dir: &Path) -> PathBuf {
    data_dir.join("memory")
}

#[cfg(test)]
mod tests {
    use super::*;
    use less_catalog::{EngineKind, FieldSpec, SchemaSpec, TableDef, TypeSpec};
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    static DIR_SEQ: AtomicU64 = AtomicU64::new(0);

    /// Unique temp dir per test without pulling in tempfile/uuid.
    fn test_dir(name: &str) -> PathBuf {
        let seq = DIR_SEQ.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!("less-mcp-{name}-{}-{seq}", std::process::id()))
    }

    async fn open_tenants(dir: &Path) -> (Arc<McpState>, Arc<McpState>) {
        let engine = less_engine::LessEngine::open_local(dir).unwrap();
        let session = Arc::new(LessSession::new_async(engine).await.unwrap());
        let a = Arc::new(McpState::new_with_tenant(session.clone(), "tenant-a").unwrap());
        let b = Arc::new(McpState::new_with_tenant(session.clone(), "tenant-b").unwrap());
        (a, b)
    }

    #[tokio::test]
    async fn tenant_namespaces_isolate_context_and_memory() {
        let dir = test_dir("tenants");
        let (a, b) = open_tenants(&dir).await;

        // Put a context in tenant A.
        let out = call_tool(
            &a,
            "context_put",
            &json!({ "key": "proj/lessdb", "title": "LessDB", "text": "analytical db", "tags": ["db"] }),
        )
        .await
        .unwrap();
        assert!(out.contains("stored context proj/lessdb"), "{out}");

        // Tenant A sees it; tenant B does not.
        let get_a = call_tool(&a, "context_get", &json!({ "key": "proj/lessdb" }))
            .await
            .unwrap();
        assert!(get_a.contains("LessDB"), "{get_a}");
        let get_b = call_tool(&b, "context_get", &json!({ "key": "proj/lessdb" }))
            .await
            .unwrap();
        assert!(get_b.contains("not found"), "{get_b}");

        // context_find is isolated too.
        let find_a = call_tool(&a, "context_find", &json!({ "query": "analytical" }))
            .await
            .unwrap();
        assert!(find_a.contains("proj/lessdb"), "{find_a}");
        let find_b = call_tool(&b, "context_find", &json!({ "query": "analytical" }))
            .await
            .unwrap();
        assert!(!find_b.contains("proj/lessdb"), "{find_b}");

        // And vice versa: B's note is invisible to A.
        call_tool(
            &b,
            "context_put",
            &json!({ "key": "task/42", "title": "task42", "text": "only in b" }),
        )
        .await
        .unwrap();
        let get_a = call_tool(&a, "context_get", &json!({ "key": "task/42" }))
            .await
            .unwrap();
        assert!(get_a.contains("not found"), "{get_a}");
        let get_b = call_tool(&b, "context_get", &json!({ "key": "task/42" }))
            .await
            .unwrap();
        assert!(get_b.contains("task42"), "{get_b}");

        // Memory tables are tenant-scoped too.
        call_tool(
            &a,
            "memory_create",
            &json!({ "table": "notes", "fields": [{ "name": "id", "type": "Int64" }], "pk": "id" }),
        )
        .await
        .unwrap();
        let tables_a = call_tool(&a, "memory_tables", &json!({})).await.unwrap();
        assert!(tables_a.contains("notes"), "{tables_a}");
        let tables_b = call_tool(&b, "memory_tables", &json!({})).await.unwrap();
        assert!(!tables_b.contains("notes"), "{tables_b}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn less_sql_tools_are_not_tenant_scoped() {
        let dir = test_dir("shared-engine");
        let engine = less_engine::LessEngine::open_local(&dir).unwrap();
        let schema = SchemaSpec {
            fields: vec![FieldSpec::new("x", TypeSpec::Int64)],
        };
        let mut table = TableDef::new("shared_t", schema, EngineKind::Firefly);
        table.sort_key = vec!["x".into()];
        engine.create_table(table).unwrap();

        let session = Arc::new(LessSession::new_async(engine).await.unwrap());
        let a = Arc::new(McpState::new_with_tenant(session.clone(), "tenant-a").unwrap());
        let b = Arc::new(McpState::new_with_tenant(session.clone(), "tenant-b").unwrap());

        let ta = call_tool(&a, "lessdb_tables", &json!({})).await.unwrap();
        let tb = call_tool(&b, "lessdb_tables", &json!({})).await.unwrap();
        assert!(ta.contains("shared_t"), "{ta}");
        assert!(tb.contains("shared_t"), "{tb}");

        std::fs::remove_dir_all(&dir).ok();
    }

    fn req(method: &str, params: Option<Value>) -> Request {
        Request {
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: method.into(),
            params,
        }
    }

    fn tool_result_text(result: &Value) -> (&str, bool) {
        let text = result["content"][0]["text"].as_str().unwrap_or("");
        (text, result["isError"].as_bool().unwrap_or(false))
    }

    async fn authz_state(dir: &Path) -> (Arc<McpState>, String, String) {
        let engine = less_engine::LessEngine::open_local(dir).unwrap();
        let session = Arc::new(LessSession::new_async(engine).await.unwrap());
        let mut tokens = less_auth::TokenStore::open(dir).unwrap();
        let read_tok = tokens.create("claude", ROLE_READ, "default", None).unwrap();
        let admin_tok = tokens.create("dba", ROLE_ADMIN, "", None).unwrap();
        let state = Arc::new(
            McpState::new_with_tenant(session, DEFAULT_TENANT)
                .unwrap()
                .with_auth(tokens, true),
        );
        (state, read_tok, admin_tok)
    }

    #[tokio::test]
    async fn require_auth_denies_unauthenticated_calls() {
        let dir = test_dir("authz-deny");
        let (state, _read, _admin) = authz_state(&dir).await;

        // No token at initialize → every tool call is denied.
        handle(
            &state,
            &req(
                "initialize",
                Some(json!({ "protocolVersion": "2024-11-05" })),
            ),
            None,
        )
        .await
        .unwrap();
        let out = handle(
            &state,
            &req(
                "tools/call",
                Some(json!({ "name": "lessdb_query", "arguments": { "sql": "SELECT 1" } })),
            ),
            None,
        )
        .await
        .unwrap();
        let (text, is_error) = tool_result_text(&out);
        assert!(
            is_error && text.contains("authentication required"),
            "{text}"
        );

        // The refusal is on the audit trail.
        let lines = less_telemetry::audit::query(&dir.join("audit"), None).unwrap();
        assert!(
            lines
                .iter()
                .any(|l| l.entry.outcome == "denied" && l.entry.caller == "anonymous")
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn read_token_cannot_run_write_tools() {
        let dir = test_dir("authz-read");
        let (state, read_tok, _admin) = authz_state(&dir).await;

        // A read-role agent passes the door…
        handle(
            &state,
            &req(
                "initialize",
                Some(json!({ "headers": { "Authorization": format!("Bearer {read_tok}") } })),
            ),
            None,
        )
        .await
        .unwrap();
        // …and may query.
        let ok = handle(
            &state,
            &req(
                "tools/call",
                Some(json!({ "name": "lessdb_query", "arguments": { "sql": "SELECT 1" } })),
            ),
            None,
        )
        .await
        .unwrap();
        assert!(!tool_result_text(&ok).1, "read tool must pass: {ok}");

        // …but may not optimize (write class).
        let denied = handle(
            &state,
            &req(
                "tools/call",
                Some(json!({ "name": "lessdb_optimize", "arguments": { "table": "events" } })),
            ),
            None,
        )
        .await
        .unwrap();
        let (text, is_error) = tool_result_text(&denied);
        assert!(is_error && text.contains("access denied"), "{text}");

        // Both attempts are audited with the caller's identity.
        let lines = less_telemetry::audit::query(&dir.join("audit"), None).unwrap();
        assert!(
            lines.iter().any(|l| l.entry.outcome == "denied"
                && l.entry.tool.as_deref() == Some("lessdb_optimize"))
        );
        assert!(
            lines
                .iter()
                .any(|l| l.entry.outcome == "ok" && l.entry.tool.as_deref() == Some("lessdb_query"))
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn admin_token_runs_write_tools() {
        let dir = test_dir("authz-admin");
        let (state, _read, admin_tok) = authz_state(&dir).await;

        handle(
            &state,
            &req(
                "initialize",
                Some(json!({ "headers": { "Authorization": format!("Bearer {admin_tok}") } })),
            ),
            None,
        )
        .await
        .unwrap();
        // optimize on a missing table errors on the engine, not on the
        // control plane — proving the write-class call was authorized.
        let out = handle(
            &state,
            &req(
                "tools/call",
                Some(json!({ "name": "lessdb_optimize", "arguments": { "table": "events" } })),
            ),
            None,
        )
        .await
        .unwrap();
        let (text, is_error) = tool_result_text(&out);
        assert!(is_error, "missing table should error");
        assert!(!text.contains("access denied"), "{text}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn without_require_auth_door_stays_open() {
        let dir = test_dir("authz-open");
        let engine = less_engine::LessEngine::open_local(&dir).unwrap();
        let session = Arc::new(LessSession::new_async(engine).await.unwrap());
        let state = Arc::new(McpState::new_with_tenant(session, DEFAULT_TENANT).unwrap());
        handle(&state, &req("initialize", None), None)
            .await
            .unwrap();
        let out = handle(
            &state,
            &req(
                "tools/call",
                Some(json!({ "name": "lessdb_tables", "arguments": {} })),
            ),
            None,
        )
        .await
        .unwrap();
        assert!(
            !tool_result_text(&out).1,
            "backward-compatible open door: {out}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
