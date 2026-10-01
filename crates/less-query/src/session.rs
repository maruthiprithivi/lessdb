//! [`LessSession`]: one engine + one DataFusion session = an embeddable,
//! DuckDB-style SQL handle.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

use arrow::record_batch::RecordBatch;
use datafusion::common::Result as DfResult;
use datafusion::dataframe::DataFrame;
use datafusion::execution::context::{SessionConfig, SessionContext};
use datafusion::execution::memory_pool::{GreedyMemoryPool, MemoryPool, MemoryReservation};
use datafusion::execution::runtime_env::RuntimeEnvBuilder;
use object_store::local::LocalFileSystem;

use less_catalog::{EngineKind, FieldSpec, SchemaSpec, TableDef};
use less_common::{LessError, Result};
use less_engine::LessEngine;
use less_vector::VectorRegistry;

use crate::gpu_udaf::{dot_udaf, filtered_sum_udaf, init_gpu};
use crate::provider::{LOCAL_STORE_URL, LessTableProvider};
use crate::vector_udtf::VectorSearchTableFunction;

/// An embedded SQL session over a LessDB engine.
///
/// ```no_run
/// # use std::sync::{Arc, RwLock};
/// # use less_engine::LessEngine;
/// # use less_query::LessSession;
/// # #[tokio::main]
/// # async fn main() -> less_common::Result<()> {
/// let engine = LessEngine::open_local("mydb")?;
/// let session = LessSession::new(engine)?;
/// let batches = session.sql_batches("SELECT 1 + 1 AS two").await?;
/// # Ok(())
/// # }
/// ```
pub struct LessSession {
    ctx: SessionContext,
    engine: Arc<LessEngine>,
    /// The vector registry (spaces, indexes, embedders) with SQL access
    /// via the `vector_search` table function.
    vectors: Arc<RwLock<VectorRegistry>>,
    /// Counter for `read_parquet()`-style temporary table registrations.
    tmp_counter: AtomicUsize,
}

/// A [`GreedyMemoryPool`] that mirrors its reserved-bytes total into the
/// process telemetry registry after every reservation change.
struct TrackingMemoryPool {
    inner: GreedyMemoryPool,
    limit: usize,
}

impl std::fmt::Debug for TrackingMemoryPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrackingMemoryPool")
            .field("limit", &self.limit)
            .finish()
    }
}

impl std::fmt::Display for TrackingMemoryPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "lessdb-pool({} bytes)", self.limit)
    }
}

impl TrackingMemoryPool {
    fn track(&self) {
        less_telemetry::global()
            .memory_pool_bytes
            .set(self.inner.reserved() as i64);
    }
}

impl MemoryPool for TrackingMemoryPool {
    fn name(&self) -> &str {
        "lessdb"
    }

    fn grow(&self, reservation: &MemoryReservation, additional: usize) {
        self.inner.grow(reservation, additional);
        self.track();
    }

    fn shrink(&self, reservation: &MemoryReservation, shrink: usize) {
        self.inner.shrink(reservation, shrink);
        self.track();
    }

    fn try_grow(&self, reservation: &MemoryReservation, additional: usize) -> DfResult<()> {
        let out = self.inner.try_grow(reservation, additional);
        self.track();
        out
    }

    fn reserved(&self) -> usize {
        self.inner.reserved()
    }
}

impl LessSession {
    /// Create a session over an engine and register all existing tables
    /// (async; use this constructor inside async runtimes — it discovers
    /// shared tables without blocking).
    pub async fn new_async(engine: Arc<LessEngine>) -> Result<Self> {
        // Parallelism: DataFusion's repartition options default to OFF for
        // aggregates/sorts/windows (single-threaded GROUP BY — the exact
        // ~1-core profile wide analytical workloads exposed). Enable them so
        // multi-core boxes are actually used; scans already fan out one task per part.
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(8);
        let config = SessionConfig::new()
            .with_information_schema(true)
            .with_target_partitions(cores)
            .with_repartition_joins(true)
            .with_repartition_aggregations(true)
            .with_repartition_sorts(true)
            .with_repartition_windows(true)
            .with_repartition_file_scans(true)
            .with_repartition_file_min_size(1)
            // Case-preserving identifiers: unquoted names keep their exact
            // case (DataFusion's default lowercases them, which breaks
            // SQL against CamelCase columns).
            .set_bool("datafusion.sql_parser.enable_ident_normalization", false)
            // Page-level skipping toggle (see EngineConfig::parquet_page_index):
            // off = row-group-statistics skipping only, the workaround for the
            // Mask-selection sparse-column-chunk bug (apache/datafusion#8092).
            .set_bool(
                "datafusion.execution.parquet.enable_page_index",
                engine.config.parquet_page_index,
            );
        // COUNT(*)/MIN/MAX over a bare table should answer from part
        // metadata (no scan). DataFusion has no such rule, so prepend ours.
        let mut optimizer_rules = datafusion::optimizer::Optimizer::new().rules;
        optimizer_rules.insert(
            0,
            Arc::new(crate::stats_optimizer::CountMinMaxFromStats::new()),
        );

        // Memory limit: bound the DataFusion memory pool so runaway queries
        // fail with ResourcesExhausted instead of OOMing the box. The pool
        // is wrapped to keep a Prometheus gauge of reserved bytes current.
        let build_state = |rt: Option<Arc<datafusion::execution::runtime_env::RuntimeEnv>>| {
            use datafusion::execution::session_state::SessionStateBuilder;
            let mut b = SessionStateBuilder::new()
                .with_default_features()
                .with_config(config.clone())
                .with_optimizer_rules(optimizer_rules.clone());
            if let Some(rt) = rt {
                b = b.with_runtime_env(rt);
            }
            b.build()
        };
        let ctx = if engine.config.memory_limit > 0 {
            let pool: Arc<dyn MemoryPool> = Arc::new(TrackingMemoryPool {
                inner: GreedyMemoryPool::new(engine.config.memory_limit),
                limit: engine.config.memory_limit,
            });
            let rt = RuntimeEnvBuilder::new()
                .with_memory_pool(pool)
                .build()
                .map_err(|e| LessError::Query(format!("runtime env: {e}")))?;
            SessionContext::new_with_state(build_state(Some(Arc::new(rt))))
        } else {
            SessionContext::new_with_state(build_state(None))
        };

        // Register the object stores the table provider relies on:
        //  * local parts -> LocalFileSystem rooted at the data directory
        //  * shared parts -> the engine's shared store at its base URL
        let data_dir = std::fs::canonicalize(&engine.config.data_dir)?;
        let local: Arc<dyn object_store::ObjectStore> = Arc::new(
            LocalFileSystem::new_with_prefix(&data_dir)
                .map_err(|e| LessError::ObjectStore(e.to_string()))?,
        );
        // Cache small parquet metadata reads (footers) in-session: parts are
        // immutable, and re-reading 71 footers per query is pure overhead.
        let local: Arc<dyn object_store::ObjectStore> =
            Arc::new(crate::scan_cache::ScanCache::new(local, 64 << 20));
        let local_url = url::Url::parse(LOCAL_STORE_URL)
            .map_err(|e| LessError::Config(format!("bad local store url: {e}")))?;
        ctx.register_object_store(&local_url, local);

        if let Some(shared) = engine.shared() {
            // Register a prefix-rooted view of the shared store: the
            // provider plans scans with store-relative keys
            // (`tables/.../data.parquet`), and the shared store's key
            // prefix (`s3://bucket/<prefix>`) must be applied.
            let for_df: Arc<dyn object_store::ObjectStore> = Arc::new(
                object_store::prefix::PrefixStore::new(shared.store(), shared.key_prefix()),
            );
            ctx.register_object_store(&shared.base_url(), for_df);
        }

        // Native vector search: registry persisted under the data dir +
        // the `vector_search` table function (LanceDB-style).
        let vectors = Arc::new(RwLock::new(VectorRegistry::open(Some(
            &engine.config.data_dir.join("vectors"),
        ))?));
        ctx.register_udtf(
            "vector_search",
            Arc::new(VectorSearchTableFunction {
                vectors: vectors.clone(),
            }),
        );
        // Fan-out hook: `lessdb_shard(i, n)` restricts scans to part-name
        // shard i of n (interpreted by the table provider).
        ctx.register_udf(crate::shard::shard_udf());

        // GPU aggregates: always registered, GPU only when enabled *and* a
        // device initialized. See `crate::gpu_udaf` for the fallback policy.
        let gpu = init_gpu(engine.config.gpu_enabled).await;
        ctx.register_udaf(filtered_sum_udaf(gpu.clone()));
        ctx.register_udaf(dot_udaf(gpu.clone()));

        let session = Self {
            ctx,
            engine,
            vectors,
            tmp_counter: AtomicUsize::new(0),
        };
        session.refresh_async().await?;
        Ok(session)
    }

    /// Synchronous convenience wrapper for [`Self::new_async`], for
    /// embedded (non-async) callers like the Python/Node SDKs.
    pub fn new(engine: Arc<LessEngine>) -> Result<Self> {
        engine.block_on_owned(Self::new_async(engine.clone()))
    }

    pub fn engine(&self) -> &Arc<LessEngine> {
        &self.engine
    }

    /// The vector registry backing `vector_search`.
    pub fn vectors(&self) -> &Arc<RwLock<VectorRegistry>> {
        &self.vectors
    }

    pub fn ctx(&self) -> &SessionContext {
        &self.ctx
    }

    /// (Re-)register every table in the catalog with DataFusion (async).
    /// Call this after creating or dropping tables.
    pub async fn refresh_async(&self) -> Result<()> {
        for table in self.engine.tables_async().await? {
            let def = self.engine.table_async(&table).await?;
            let schema = def.arrow_schema();
            let provider = Arc::new(LessTableProvider::new(
                self.engine.clone(),
                table.clone(),
                def,
                schema,
            ));
            let _ = self.ctx.deregister_table(&table);
            self.ctx.register_table(&table, provider).map_err(|e| {
                LessError::Query(format!("failed to register table '{table}': {e}"))
            })?;
        }
        Ok(())
    }

    /// Synchronous convenience wrapper for [`Self::refresh_async`]
    /// (embedded callers outside async runtimes).
    pub fn refresh(&self) -> Result<()> {
        self.engine.runtime().block_on(self.refresh_async())
    }

    /// Plan SQL (deferred execution).
    pub async fn sql(&self, sql: &str) -> Result<DataFrame> {
        // Native statements the SQL planner does not implement (SHOW …,
        // DESCRIBE, USE, PRAGMA version, VACUUM) are served directly by
        // the engine so every interface (CLI, SDKs, MCP, HTTP) sees them.
        if let Some(r) = self.native_statements(sql).await {
            return r;
        }
        self.ctx
            .sql(sql)
            .await
            .map_err(|e| LessError::Query(format!("{sql}\n{e}")))
    }

    /// Serve engine-native statements. Returns `None` when the statement
    /// is not one of ours (the SQL planner takes it from there).
    async fn native_statements(&self, sql: &str) -> Option<Result<DataFrame>> {
        let t = sql.trim();
        let upper = t.to_ascii_uppercase();
        // Keyword scanning helpers over the uppercased text.
        let starts = |kw: &str| {
            upper.starts_with(kw) && {
                let raw = &t[kw.len()..];
                raw.is_empty() || raw.starts_with(char::is_whitespace)
            }
        };
        let ident_after = |kw: &str| -> Option<String> {
            if !starts(kw) {
                return None;
            }
            let rest = t[kw.len()..].trim();
            let name: String = rest
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != ';' && *c != '(')
                .collect();
            if name.is_empty() || name.starts_with(['"', '`', '\'']) {
                // strip backticks/quotes if present
                let inner: String = rest
                    .chars()
                    .skip_while(|c| !c.is_alphanumeric() && *c != '_')
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
                    .collect();
                if inner.is_empty() {
                    return None;
                }
                return Some(inner);
            }
            Some(name.trim_end_matches(';').to_string())
        };

        // DuckDB-style read_*() table functions: register each file as a
        // temporary table and rewrite the SQL before handing it to the
        // planner. `read_parquet('a.parquet')` becomes `_lessdb_file_0`, and
        // multi-path calls become a UNION ALL subquery.
        if let Some(rewritten) = self.rewrite_read_functions(t).await {
            return match rewritten {
                Ok(sql2) => {
                    // A rewritten CREATE TABLE … AS must still persist.
                    if let Some(r) = self.handle_create_table(&sql2).await {
                        return Some(r);
                    }
                    Some(
                        self.ctx
                            .sql(&sql2)
                            .await
                            .map_err(|e| LessError::Query(format!("{t}\n{e}"))),
                    )
                }
                Err(e) => Some(Err(e)),
            };
        }

        // SHOW DATABASES
        if starts("SHOW DATABASES") {
            return Some(self.native_show_databases().await);
        }
        // SHOW [FULL] PROCESSLIST — the session's own activity
        if starts("SHOW PROCESSLIST") || starts("SHOW FULL PROCESSLIST") {
            use arrow::array::{StringArray, UInt64Array};
            use arrow::datatypes::{DataType, Field, Schema};
            use std::sync::Arc as A;
            let batch = match RecordBatch::try_new(
                A::new(Schema::new(vec![
                    Field::new("pid", DataType::UInt64, false),
                    Field::new("state", DataType::Utf8, false),
                    Field::new("query", DataType::Utf8, false),
                ])),
                vec![
                    A::new(UInt64Array::from(vec![std::process::id() as u64])),
                    A::new(StringArray::from(vec!["running"])),
                    A::new(StringArray::from(vec![t])),
                ],
            ) {
                Ok(b) => b,
                Err(e) => return Some(Err(LessError::Arrow(e))),
            };
            return Some(
                self.ctx
                    .read_batch(batch)
                    .map_err(|e| LessError::Query(e.to_string())),
            );
        }
        // SHOW TABLES [FROM|IN <db>] [LIKE 'pattern']
        if starts("SHOW TABLES") || starts("SHOW FULL TABLES") {
            let rest = t[if upper.starts_with("SHOW FULL TABLES") {
                "SHOW FULL TABLES".len()
            } else {
                "SHOW TABLES".len()
            }..]
                .trim();
            let mut db: Option<String> = None;
            let mut like: Option<String> = None;
            let mut r = rest;
            if let Some(after_from) = r.strip_prefix("FROM").or_else(|| r.strip_prefix("IN")) {
                let seg: String = after_from
                    .trim_start()
                    .chars()
                    .take_while(|c| !c.is_whitespace())
                    .collect();
                if !seg.is_empty() {
                    db = Some(seg.trim_matches(['"', '`', '\'']).to_string());
                }
                r = &after_from[after_from
                    .find(&seg)
                    .map(|i| i + seg.len())
                    .unwrap_or(after_from.len())..];
            }
            if let Some(li) = r.find("LIKE") {
                let pat = r[li + 4..].trim().trim_matches(['\'', '"', '`']);
                like = Some(pat.to_string());
            }
            return Some(
                self.native_show_tables(db.as_deref(), like.as_deref())
                    .await,
            );
        }
        // SHOW CREATE TABLE <t>
        if upper.starts_with("SHOW CREATE TABLE") {
            let name = ident_after("SHOW CREATE TABLE")?;
            return Some(self.native_show_create(&name).await);
        }
        // SHOW COLUMNS FROM <t>  (also SHOW FIELDS)
        for kw in ["SHOW COLUMNS FROM", "SHOW FIELDS FROM"] {
            if upper.starts_with(kw) {
                let name = ident_after(kw)?;
                return Some(self.native_show_columns(&name).await);
            }
        }
        // DESCRIBE [TABLE] <t> / DESC <t>
        for kw in ["DESCRIBE TABLE", "DESCRIBE", "DESC TABLE", "DESC"] {
            if starts(kw)
                && let Some(name) = ident_after(kw)
            {
                return Some(self.native_show_columns(&name).await);
            }
        }
        // USE <db>
        if starts("USE")
            && let Some(db) = ident_after("USE")
        {
            return Some(self.native_use(&db).await);
        }
        // PRAGMA version
        if upper.starts_with("PRAGMA VERSION") {
            return Some(self.native_pragma_version().await);
        }
        // VACUUM — merges all tables (parts are immutable; optimize is the
        // equivalent of vacuuming/compacting)
        if starts("VACUUM") {
            return Some(self.native_vacuum().await);
        }
        // ALTER TABLE <t> ADD|DROP COLUMN <name> [<type>]
        if starts("ALTER TABLE") {
            let rest = t["ALTER TABLE".len()..].trim();
            let name: String = rest
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != ';')
                .collect();
            let name = name.trim_matches(['"', '`']).to_string();
            if name.is_empty() {
                return Some(Err(LessError::Query(
                    "ALTER TABLE syntax: ALTER TABLE <table> ADD|DROP COLUMN …".into(),
                )));
            }
            let after = rest[name.len()..].trim();
            let after_upper = after.to_ascii_uppercase();
            let add_len = if after_upper.starts_with("ADD COLUMN ") || after_upper == "ADD COLUMN" {
                Some("ADD COLUMN".len())
            } else if after_upper.starts_with("ADD ") {
                Some("ADD".len())
            } else {
                None
            };
            if let Some(len) = add_len {
                let s = after[len..].trim();
                let col: String = s
                    .chars()
                    .take_while(|c| !c.is_whitespace() && *c != ';')
                    .collect();
                let col = col.trim_matches(['"', '`']).to_string();
                let ty_str = s[col.len()..].trim().trim_end_matches(';').trim();
                if col.is_empty() || ty_str.is_empty() {
                    return Some(Err(LessError::Query(
                        "ALTER TABLE ADD COLUMN syntax: ALTER TABLE <table> ADD COLUMN <name> <type>"
                            .into(),
                    )));
                }
                // Trailing modifiers (DEFAULT …, NOT NULL, CODEC …) are
                // accepted but not yet enforced — parse the leading type.
                let ty = match less_catalog::TypeSpec::parse(ty_str) {
                    Ok(t) => t,
                    Err(_) => {
                        let first = ty_str.split_whitespace().next().unwrap_or_default();
                        match less_catalog::TypeSpec::parse(first) {
                            Ok(t) => t,
                            Err(e) => {
                                return Some(Err(LessError::Query(format!(
                                    "ALTER TABLE {name} ADD COLUMN {col}: {e}"
                                ))));
                            }
                        }
                    }
                };
                match self.engine.alter_add_column(&name, &col, ty) {
                    Ok(n) => {
                        if let Err(e) = self.refresh_async().await {
                            return Some(Err(e));
                        }
                        return Some(
                            self.native_ok(format!(
                                "added column '{col}' to '{name}' ({n} columns)"
                            ))
                            .await,
                        );
                    }
                    Err(e) => {
                        return Some(Err(LessError::Query(format!(
                            "ALTER TABLE {name} ADD COLUMN {col}:\n{e}"
                        ))));
                    }
                }
            }
            if after_upper.starts_with("DROP COLUMN ") || after_upper == "DROP COLUMN" {
                let s = after["DROP COLUMN".len()..].trim();
                let col: String = s
                    .chars()
                    .take_while(|c| !c.is_whitespace() && *c != ';')
                    .collect();
                let col = col.trim_matches(['"', '`']).to_string();
                if col.is_empty() {
                    return Some(Err(LessError::Query(
                        "ALTER TABLE DROP COLUMN syntax: ALTER TABLE <table> DROP COLUMN <name>"
                            .into(),
                    )));
                }
                match self.engine.alter_drop_column(&name, &col) {
                    Ok(n) => {
                        if let Err(e) = self.refresh_async().await {
                            return Some(Err(e));
                        }
                        return Some(
                            self.native_ok(format!(
                                "dropped column '{col}' from '{name}' ({n} columns)"
                            ))
                            .await,
                        );
                    }
                    Err(e) => {
                        return Some(Err(LessError::Query(format!(
                            "ALTER TABLE {name} DROP COLUMN {col}:\n{e}"
                        ))));
                    }
                }
            }
            return Some(Err(LessError::Query(
                "ALTER TABLE supports ADD COLUMN and DROP COLUMN (RENAME coming soon)".into(),
            )));
        }
        // TRUNCATE [TABLE] <t> — empty the table, keep the schema
        if starts("TRUNCATE") {
            let name = if upper.starts_with("TRUNCATE TABLE") {
                ident_after("TRUNCATE TABLE")
            } else {
                ident_after("TRUNCATE")
            };
            let Some(name) = name else {
                return Some(Err(LessError::Query(
                    "TRUNCATE syntax: TRUNCATE TABLE <table>".into(),
                )));
            };
            match self.engine.truncate(&name) {
                Ok(()) => {
                    if let Err(e) = self.refresh_async().await {
                        return Some(Err(e));
                    }
                    return Some(self.native_ok(format!("truncated '{name}'")).await);
                }
                Err(e) => return Some(Err(LessError::Query(format!("TRUNCATE {name}:\n{e}")))),
            }
        }
        // Explicit transactions: every LessDB statement is atomic and
        // commits immediately, so BEGIN/COMMIT/ROLLBACK get an honest answer.
        if starts("BEGIN") || starts("START TRANSACTION") {
            return Some(Err(LessError::Query(
                "BEGIN is not supported: LessDB statements are atomic — every statement \
                 commits immediately, so explicit transactions are never needed"
                    .into(),
            )));
        }
        if starts("COMMIT") || starts("ROLLBACK") || starts("END") {
            return Some(Err(LessError::Query(
                "nothing to commit or roll back: LessDB statements are atomic and \
                 there is no open transaction"
                    .into(),
            )));
        }
        // ATTACH / DETACH: LessDB has one database; pointing at external
        // files is done with read_parquet()/read_csv()/read_json() instead.
        if starts("ATTACH") || starts("DETACH") {
            return Some(Err(LessError::Query(
                "ATTACH/DETACH are not supported: LessDB has one database ('default'). \
                 To query external data, use read_parquet(), read_csv(), read_json(), \
                 or the .import REPL command"
                    .into(),
            )));
        }
        // CREATE TABLE — both the schema-DDL form and CREATE TABLE … AS
        // SELECT persist through the catalog (the planner's CTAS would only
        // create a volatile in-memory table that vanishes with the session).
        if let Some(r) = self.handle_create_table(t).await {
            return Some(r);
        }
        // DROP TABLE — remove the table *and its parts* from the catalog.
        if let Some(r) = self.handle_drop_table(t).await {
            return Some(r);
        }
        // CREATE VIEW is not real in LessDB yet — refuse loudly instead of
        // pretending it worked.
        if is_create_view(&upper) {
            return Some(Err(LessError::Query(
                "CREATE VIEW is not supported yet. Use a CTE (WITH … AS) for readable \
                 query composition, or INSERT INTO for materialized results — views \
                 are on the roadmap"
                    .into(),
            )));
        }
        None
    }

    async fn native_show_databases(&self) -> Result<DataFrame> {
        use arrow::array::StringArray;
        use arrow::datatypes::{DataType, Field, Schema};
        use std::sync::Arc as A;
        let batch = RecordBatch::try_new(
            A::new(Schema::new(vec![Field::new("name", DataType::Utf8, false)])),
            vec![A::new(StringArray::from(vec!["default"]))],
        )?;
        self.ctx
            .read_batch(batch)
            .map_err(|e| LessError::Query(e.to_string()))
    }

    async fn native_show_tables(&self, db: Option<&str>, like: Option<&str>) -> Result<DataFrame> {
        if let Some(db) = db
            && db != "default"
        {
            return Err(LessError::Query(format!(
                "unknown database '{db}' (LessDB has one database: 'default')"
            )));
        }
        let mut tables = self.engine.tables()?;
        tables.sort();
        if let Some(pat) = like {
            let rx = like_to_regex(pat);
            tables.retain(|t| rx.is_match(t));
        }
        use arrow::array::StringArray;
        use arrow::datatypes::{DataType, Field, Schema};
        use std::sync::Arc as A;
        let batch = RecordBatch::try_new(
            A::new(Schema::new(vec![Field::new("name", DataType::Utf8, false)])),
            vec![A::new(StringArray::from(tables))],
        )?;
        self.ctx
            .read_batch(batch)
            .map_err(|e| LessError::Query(e.to_string()))
    }

    async fn native_show_create(&self, name: &str) -> Result<DataFrame> {
        let def = self.engine.table(name)?;
        use arrow::array::StringArray;
        use arrow::datatypes::{DataType, Field, Schema};
        use std::sync::Arc as A;
        let batch = RecordBatch::try_new(
            A::new(Schema::new(vec![Field::new(
                "create_table",
                DataType::Utf8,
                false,
            )])),
            vec![A::new(StringArray::from(vec![def.to_ddl()]))],
        )?;
        self.ctx
            .read_batch(batch)
            .map_err(|e| LessError::Query(e.to_string()))
    }

    async fn native_show_columns(&self, name: &str) -> Result<DataFrame> {
        let def = self.engine.table(name)?;
        use arrow::array::{BooleanArray, StringArray};
        use arrow::datatypes::{DataType, Field, Schema};
        use std::sync::Arc as A;
        let names: Vec<&str> = def.schema.fields.iter().map(|f| f.name.as_str()).collect();
        let types: Vec<String> = def.schema.fields.iter().map(|f| f.ty.name()).collect();
        let keys: Vec<bool> = def
            .schema
            .fields
            .iter()
            .map(|f| def.sort_key.contains(&f.name))
            .collect();
        let nulls: Vec<bool> = vec![false; names.len()];
        let batch = RecordBatch::try_new(
            A::new(Schema::new(vec![
                Field::new("column_name", DataType::Utf8, false),
                Field::new("column_type", DataType::Utf8, false),
                Field::new("is_nullable", DataType::Boolean, false),
                Field::new("is_in_sort_key", DataType::Boolean, false),
            ])),
            vec![
                A::new(StringArray::from(names)),
                A::new(StringArray::from(types)),
                A::new(BooleanArray::from(nulls)),
                A::new(BooleanArray::from(keys)),
            ],
        )?;
        self.ctx
            .read_batch(batch)
            .map_err(|e| LessError::Query(e.to_string()))
    }

    async fn native_use(&self, db: &str) -> Result<DataFrame> {
        if db != "default" {
            return Err(LessError::Query(format!(
                "unknown database '{db}' (LessDB has one database: 'default')"
            )));
        }
        use arrow::datatypes::{Field, Schema};
        use std::sync::Arc as A;
        self.ctx
            .read_batch(RecordBatch::new_empty(A::new(Schema::new(vec![
                Field::new("ok", arrow::datatypes::DataType::Boolean, true),
            ]))))
            .map_err(|e| LessError::Query(e.to_string()))
    }

    async fn native_pragma_version(&self) -> Result<DataFrame> {
        use arrow::array::StringArray;
        use arrow::datatypes::{DataType, Field, Schema};
        use std::sync::Arc as A;
        let batch = RecordBatch::try_new(
            A::new(Schema::new(vec![Field::new(
                "version",
                DataType::Utf8,
                false,
            )])),
            vec![A::new(StringArray::from(vec![less_common::VERSION]))],
        )?;
        self.ctx
            .read_batch(batch)
            .map_err(|e| LessError::Query(e.to_string()))
    }

    async fn native_vacuum(&self) -> Result<DataFrame> {
        // Immutable parts make VACUUM an alias for OPTIMIZE across tables.
        let mut tables = self.engine.tables()?;
        tables.sort();
        for t in &tables {
            self.engine.optimize(t)?;
        }
        use arrow::array::StringArray;
        use arrow::datatypes::{DataType, Field, Schema};
        use std::sync::Arc as A;
        let batch = RecordBatch::try_new(
            A::new(Schema::new(vec![Field::new(
                "optimized",
                DataType::Utf8,
                false,
            )])),
            vec![A::new(StringArray::from(tables))],
        )?;
        self.ctx
            .read_batch(batch)
            .map_err(|e| LessError::Query(e.to_string()))
    }

    /// One-row "ok" result for statements whose effect is a message.
    async fn native_ok(&self, msg: String) -> Result<DataFrame> {
        use arrow::array::StringArray;
        use arrow::datatypes::{DataType, Field, Schema};
        use std::sync::Arc as A;
        let batch = RecordBatch::try_new(
            A::new(Schema::new(vec![Field::new("ok", DataType::Utf8, false)])),
            vec![A::new(StringArray::from(vec![msg]))],
        )?;
        self.ctx
            .read_batch(batch)
            .map_err(|e| LessError::Query(e.to_string()))
    }

    /// `CREATE TABLE` — both the schema-DDL form (through the catalog's
    /// DDL parser) and `CREATE TABLE … AS SELECT` (executed, then persisted
    /// into the catalog so it survives the session).
    async fn handle_create_table(&self, t: &str) -> Option<Result<DataFrame>> {
        let upper = t.to_ascii_uppercase();
        let (if_ne, rest) = if upper.starts_with("CREATE TABLE IF NOT EXISTS ")
            || upper == "CREATE TABLE IF NOT EXISTS"
        {
            (
                true,
                t["CREATE TABLE IF NOT EXISTS".len()..].trim().to_string(),
            )
        } else if upper.starts_with("CREATE TABLE ") {
            (false, t["CREATE TABLE".len()..].trim().to_string())
        } else {
            return None;
        };
        let name: String = rest
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != ';' && *c != '(')
            .collect();
        let name = name.trim_matches(['"', '`']).to_string();
        if name.is_empty() {
            return Some(Err(LessError::Query(
                "CREATE TABLE syntax: CREATE TABLE [IF NOT EXISTS] <name> (…) \
                 or CREATE TABLE <name> AS SELECT …"
                    .into(),
            )));
        }
        let after = rest[name.len()..].trim();
        let is_ctas = after.len() >= 2
            && after[..2].eq_ignore_ascii_case("AS")
            && (after.len() == 2 || after.as_bytes()[2].is_ascii_whitespace());

        if is_ctas {
            return Some(self.create_table_as(&name, if_ne, after[2..].trim()).await);
        }

        // Schema-DDL form: the catalog parser understands IF NOT EXISTS,
        // engines, ORDER BY, UNIQUE, COMPRESSION and TTL.
        let parsed = match less_catalog::ddl::parse_create(t) {
            Ok(p) => p,
            Err(e) => return Some(Err(LessError::Query(format!("CREATE TABLE:\n{e}")))),
        };
        if if_ne
            && self
                .engine
                .tables()
                .map(|ts| ts.contains(&parsed.table))
                .unwrap_or(false)
        {
            return Some(
                self.native_ok(format!(
                    "table '{}' already exists (IF NOT EXISTS)",
                    parsed.table
                ))
                .await,
            );
        }
        if let Err(e) = self.engine.create_table(parsed.to_def()) {
            return Some(Err(LessError::Query(format!(
                "CREATE TABLE {}:\n{e}",
                parsed.table
            ))));
        }
        if let Err(e) = self.refresh_async().await {
            return Some(Err(e));
        }
        Some(
            self.native_ok(format!("created table '{}'", parsed.table))
                .await,
        )
    }

    /// The `CREATE TABLE <name> AS SELECT …` half of [`Self::handle_create_table`].
    async fn create_table_as(
        &self,
        name: &str,
        if_ne: bool,
        select_sql: &str,
    ) -> Result<DataFrame> {
        let exists = self.engine.tables()?.iter().any(|x| x == name);
        if if_ne && exists {
            return self
                .native_ok(format!("table '{name}' already exists (IF NOT EXISTS)"))
                .await;
        }
        if exists {
            return Err(LessError::Query(format!("table '{name}' already exists")));
        }
        let df = self
            .ctx
            .sql(select_sql)
            .await
            .map_err(|e| LessError::Query(format!("CREATE TABLE {name} AS SELECT:\n{e}")))?;
        let fields: Vec<FieldSpec> = df
            .schema()
            .fields()
            .iter()
            .map(|f| {
                less_catalog::TypeSpec::from_arrow(f.data_type())
                    .map(|ty| FieldSpec::new(f.name(), ty))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut def = TableDef::new(name.to_string(), SchemaSpec { fields }, EngineKind::Firefly);
        if let Some(first) = def.schema.fields.first() {
            def.sort_key = vec![first.name.clone()];
        }
        if let Err(e) = self.engine.create_table(def) {
            return Err(LessError::Query(format!("CREATE TABLE {name}:\n{e}")));
        }
        let batches = match df.collect().await {
            Ok(b) => b,
            Err(e) => {
                // Roll back the empty table so a failed CTAS leaves nothing behind.
                let _ = self.engine.drop_table(name);
                return Err(LessError::Query(format!(
                    "CREATE TABLE {name} AS SELECT:\n{e}"
                )));
            }
        };
        let rows: usize = batches.iter().map(|b| b.num_rows()).sum();
        for b in &batches {
            if let Err(e) = self.engine.insert(name, b.clone()) {
                let _ = self.engine.drop_table(name);
                return Err(LessError::Query(format!(
                    "CREATE TABLE {name} AS SELECT:\n{e}"
                )));
            }
        }
        // Make the rows visible to scans immediately (the SQL INSERT path
        // flushes at completion; we must too).
        if let Err(e) = self.engine.flush(name) {
            let _ = self.engine.drop_table(name);
            return Err(LessError::Query(format!(
                "CREATE TABLE {name} AS SELECT:\n{e}"
            )));
        }
        self.refresh_async().await?;
        self.native_ok(format!("created table '{name}' from SELECT ({rows} rows)"))
            .await
    }

    /// `DROP TABLE [IF EXISTS] <name>` through the catalog so the table's
    /// parts are actually removed (the planner's DROP only deregisters a
    /// volatile registration).
    async fn handle_drop_table(&self, t: &str) -> Option<Result<DataFrame>> {
        let upper = t.to_ascii_uppercase();
        let (if_exists, name) =
            if upper.starts_with("DROP TABLE IF EXISTS ") || upper == "DROP TABLE IF EXISTS" {
                (true, t["DROP TABLE IF EXISTS".len()..].trim().to_string())
            } else if upper.starts_with("DROP TABLE ") {
                (false, t["DROP TABLE".len()..].trim().to_string())
            } else {
                return None;
            };
        let name = name.trim_end_matches(';').trim().trim_matches(['"', '`']);
        if name.is_empty() || name.chars().any(char::is_whitespace) {
            return Some(Err(LessError::Query(
                "DROP TABLE syntax: DROP TABLE [IF EXISTS] <name>".into(),
            )));
        }
        let tables = match self.engine.tables() {
            Ok(ts) => ts,
            Err(e) => return Some(Err(e)),
        };
        let exists = tables.iter().any(|x| x == name);
        if if_exists && !exists {
            return Some(
                self.native_ok(format!("table '{name}' does not exist (IF EXISTS)"))
                    .await,
            );
        }
        if let Err(e) = self.engine.drop_table(name) {
            return Some(Err(LessError::Query(format!("DROP TABLE {name}:\n{e}"))));
        }
        if let Err(e) = self.refresh_async().await {
            return Some(Err(e));
        }
        Some(self.native_ok(format!("dropped table '{name}'")).await)
    }

    /// Find `read_parquet(` / `read_csv(` / `read_json(` / `read_ndjson(`
    /// calls in `sql`, register every referenced file as a temporary table,
    /// and return the rewritten statement (each call replaced by a table
    /// name, multi-path calls by a UNION ALL subquery). Returns `None` when
    /// the statement has no such calls.
    async fn rewrite_read_functions(&self, sql: &str) -> Option<Result<String>> {
        const READ_FNS: [&str; 4] = ["read_parquet", "read_csv", "read_json", "read_ndjson"];
        let lower = sql.to_ascii_lowercase();
        // (start byte, fn name, argument text, close-paren byte)
        let mut matches: Vec<(usize, &'static str, String, usize)> = vec![];
        for f in READ_FNS {
            let needle = format!("{f}(");
            let mut from = 0usize;
            while let Some(pos) = lower[from..].find(&needle) {
                let abs = from + pos;
                let before_ok = abs == 0 || {
                    let c = sql.as_bytes()[abs - 1] as char;
                    !(c.is_alphanumeric() || c == '_' || c == '\'' || c == '"' || c == '`')
                };
                if before_ok
                    && let Some((inner, close)) = find_paren_end(sql, abs + needle.len() - 1)
                {
                    matches.push((abs, f, inner, close));
                }
                from = abs + needle.len();
            }
        }
        if matches.is_empty() {
            return None;
        }
        matches.sort_by_key(|m| m.0);
        let mut out = String::with_capacity(sql.len() + 64);
        let mut last = 0usize;
        for (abs, f, inner, close) in matches {
            if abs < last {
                continue; // nested inside an already-rewritten call: skip
            }
            let mut paths = vec![];
            for arg in split_top_level(&inner) {
                let arg = arg.trim();
                if arg.is_empty() {
                    continue;
                }
                if !(arg.starts_with('\'') || arg.starts_with('"')) {
                    return Some(Err(LessError::Query(format!(
                        "{f}: options are not supported yet, pass file paths only (got '{arg}')"
                    ))));
                }
                paths.push(arg.trim_matches(['\'', '"']).to_string());
            }
            if paths.is_empty() {
                return Some(Err(LessError::Query(format!(
                    "{f}() needs at least one file path"
                ))));
            }
            let mut names = vec![];
            for p in &paths {
                let idx = self.tmp_counter.fetch_add(1, Ordering::Relaxed);
                let name = format!("_lessdb_file_{idx}");
                if let Err(e) = self.register_file(&name, p).await {
                    return Some(Err(LessError::Query(format!("{f}('{p}'): {e}"))));
                }
                names.push(name);
            }
            let repl = if names.len() == 1 {
                names[0].clone()
            } else {
                format!(
                    "({})",
                    names
                        .iter()
                        .map(|n| format!("SELECT * FROM {n}"))
                        .collect::<Vec<_>>()
                        .join(" UNION ALL ")
                )
            };
            out.push_str(&sql[last..abs]);
            out.push_str(&repl);
            last = close + 1;
        }
        out.push_str(&sql[last..]);
        Some(Ok(out))
    }

    /// Register an external file (CSV / Parquet / JSONL / Arrow IPC) as a
    /// queryable table by path — the SQL "read a file"
    /// pattern. The table can then feed `INSERT INTO`:
    ///
    /// ```sql
    /// INSERT INTO events SELECT * FROM staging;
    /// ```
    pub async fn register_file(&self, name: &str, path: &str) -> Result<()> {
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "csv" => self
                .ctx
                .register_csv(
                    name,
                    path,
                    datafusion::datasource::file_format::options::CsvReadOptions::new(),
                )
                .await
                .map_err(|e| LessError::Query(e.to_string())),
            "parquet" => self
                .ctx
                .register_parquet(
                    name,
                    path,
                    datafusion::datasource::file_format::options::ParquetReadOptions::default(),
                )
                .await
                .map_err(|e| LessError::Query(e.to_string())),
            "json" | "jsonl" | "ndjson" => self
                .ctx
                .register_json(
                    name,
                    path,
                    datafusion::datasource::file_format::options::JsonReadOptions::default(),
                )
                .await
                .map_err(|e| LessError::Query(e.to_string())),
            "arrow" | "ipc" => {
                self.ctx
                    .register_arrow(
                        name,
                        path,
                        datafusion::datasource::file_format::options::ArrowReadOptions::default(),
                    )
                    .await
                    .map_err(|e| LessError::Query(e.to_string()))?;
                Ok(())
            }
            other => Err(LessError::Config(format!(
                "unsupported file extension '{other}' (supported: csv, parquet, json/jsonl, arrow)"
            ))),
        }
    }

    /// Run SQL and collect all result batches.
    pub async fn sql_batches(&self, sql: &str) -> Result<Vec<RecordBatch>> {
        let start = std::time::Instant::now();
        match self.sql(sql).await {
            Ok(df) => match df.collect().await {
                Ok(batches) => {
                    let rows: usize = batches.iter().map(|b| b.num_rows()).sum();
                    let metrics = less_telemetry::global();
                    metrics.queries.inc(&[("status", "ok")]);
                    metrics
                        .query_duration
                        .observe(start.elapsed().as_secs_f64());
                    metrics.rows_returned.add(rows as u64);
                    Ok(batches)
                }
                Err(e) => {
                    less_telemetry::global().queries.inc(&[("status", "error")]);
                    Err(LessError::Query(e.to_string()))
                }
            },
            Err(e) => {
                less_telemetry::global().queries.inc(&[("status", "error")]);
                Err(e)
            }
        }
    }

    /// Run `EXPLAIN <sql>` and collect the plan.
    pub async fn explain_batches(&self, sql: &str) -> Result<Vec<RecordBatch>> {
        self.sql_batches(&format!("EXPLAIN {sql}")).await
    }
}

fn like_to_regex(pat: &str) -> regex::Regex {
    let mut rx = String::from("^");
    for c in pat.chars() {
        match c {
            '%' => rx.push_str(".*"),
            '_' => rx.push('.'),
            _ => rx.push_str(&regex::escape(&c.to_string())),
        }
    }
    rx.push('$');
    regex::Regex::new(&rx).expect("LIKE pattern is a valid regex")
}

/// `CREATE [OR REPLACE] [MATERIALIZED] VIEW` (and only that — never
/// `CREATE TABLE … AS`).
fn is_create_view(t: &str) -> bool {
    let words: Vec<&str> = t.split_whitespace().collect();
    let mut i = 0;
    if !words
        .first()
        .is_some_and(|w| w.eq_ignore_ascii_case("CREATE"))
    {
        return false;
    }
    i += 1;
    if words.get(i).is_some_and(|w| w.eq_ignore_ascii_case("OR"))
        && words
            .get(i + 1)
            .is_some_and(|w| w.eq_ignore_ascii_case("REPLACE"))
    {
        i += 2;
    }
    if words
        .get(i)
        .is_some_and(|w| w.eq_ignore_ascii_case("MATERIALIZED"))
    {
        i += 1;
    }
    words.get(i).is_some_and(|w| w.eq_ignore_ascii_case("VIEW"))
}

/// Index of the `)` matching the `(` at `open`, returning the text between
/// the parens. Quote-aware (strings may contain parens).
fn find_paren_end(s: &str, open: usize) -> Option<(String, usize)> {
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == q && bytes.get(i.wrapping_sub(1)) != Some(&b'\\') {
                    quote = None;
                }
            }
            None => match b {
                b'\'' | b'"' => quote = Some(b),
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some((s[open + 1..i].to_string(), i));
                    }
                }
                _ => {}
            },
        }
        i += 1;
    }
    None
}

/// Split on commas at paren depth 0 (string- and nested-arg-aware).
fn split_top_level(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut depth = 0i32;
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in s.chars() {
        match quote {
            Some(q) => {
                cur.push(c);
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    cur.push(c);
                }
                '(' => {
                    depth += 1;
                    cur.push(c);
                }
                ')' => {
                    depth -= 1;
                    cur.push(c);
                }
                ',' if depth == 0 => {
                    out.push(cur.trim().to_string());
                    cur.clear();
                }
                _ => cur.push(c),
            },
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Array, Float32Array, Float64Array, Int64Array, StringArray, UInt32Array};
    use arrow::datatypes::{DataType, Field, Schema};
    use less_catalog::{EngineKind, FieldSpec, SchemaSpec, TableDef, TypeSpec};
    use less_common::EngineConfig;

    #[tokio::test]
    async fn sql_select_count_group_by_join() {
        let dir = std::env::temp_dir().join(format!("less-query-{}", uuid::Uuid::new_v4()));
        let engine = LessEngine::open_local(&dir).unwrap();

        let events_schema = SchemaSpec {
            fields: vec![
                FieldSpec::new("id", TypeSpec::Int64),
                FieldSpec::new("kind", TypeSpec::Utf8),
                FieldSpec::new("amount", TypeSpec::Float64),
            ],
        };
        let mut events = TableDef::new("events", events_schema, EngineKind::Firefly);
        events.sort_key = vec!["kind".into(), "id".into()];
        engine.create_table(events).unwrap();

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("kind", DataType::Utf8, false),
            Field::new("amount", DataType::Float64, false),
        ]));
        let mk = |ids: Vec<i64>, kinds: Vec<&str>, amounts: Vec<f64>| {
            RecordBatch::try_new(
                schema.clone(),
                vec![
                    Arc::new(Int64Array::from(ids)),
                    Arc::new(StringArray::from(kinds)),
                    Arc::new(Float64Array::from(amounts)),
                ],
            )
            .unwrap()
        };
        engine
            .insert(
                "events",
                mk(
                    vec![1, 2, 3, 4],
                    vec!["click", "view", "click", "view"],
                    vec![10.0, 1.0, 20.0, 2.0],
                ),
            )
            .unwrap();
        engine.flush("events").unwrap();

        let session = LessSession::new_async(engine.clone()).await.unwrap();

        let batches = session
            .sql_batches(
                "SELECT kind, count(*) AS c, sum(amount) AS total \
                 FROM events GROUP BY kind ORDER BY kind",
            )
            .await
            .unwrap();
        let b = &batches[0];
        assert_eq!(b.num_rows(), 2);
        let kinds = b.column(0).as_any().downcast_ref::<StringArray>().unwrap();
        assert_eq!((kinds.value(0), kinds.value(1)), ("click", "view"));
        let counts = b.column(1).as_any().downcast_ref::<Int64Array>().unwrap();
        assert_eq!((counts.value(0), counts.value(1)), (2, 2));

        // Filtered query: only click rows.
        let batches = session
            .sql_batches("SELECT id FROM events WHERE kind = 'click' ORDER BY id")
            .await
            .unwrap();
        let ids = batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!(ids.len(), 2);

        // EXPLAIN works.
        let plan = session
            .explain_batches("SELECT count(*) FROM events")
            .await
            .unwrap();
        assert!(!plan.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn shared_table_queried_from_second_compute_node() {
        // End-to-end compute/storage separation: node A writes to shared
        // storage, node B — a completely independent process with its own
        // local directory — queries the same data through DataFusion.
        let base = std::env::temp_dir().join(format!("less-cloudq-{}", uuid::Uuid::new_v4()));
        let shared_root = base.join("shared-root");
        let shared_url = format!("file://{}", shared_root.display());

        // Ingestion on node A uses the synchronous write APIs; from an
        // async runtime that means a blocking thread (exactly how a server
        // would run sync ingestion).
        // Keep node A alive for the duration of the test (its engine
        // instance proves the two nodes run independently).
        let _engine_a = {
            let shared_url = shared_url.clone();
            let node_a = base.join("node-a");
            tokio::task::spawn_blocking(move || -> less_common::Result<Arc<LessEngine>> {
                let engine_a =
                    LessEngine::open(EngineConfig::with_shared_url(node_a, &shared_url))?;
                let mut def = TableDef::new(
                    "cloud_events",
                    SchemaSpec {
                        fields: vec![
                            FieldSpec::new("id", TypeSpec::Int64),
                            FieldSpec::new("kind", TypeSpec::Utf8),
                            FieldSpec::new("amount", TypeSpec::Float64),
                        ],
                    },
                    EngineKind::FireflyCloud,
                );
                def.sort_key = vec!["kind".into(), "id".into()];
                engine_a.create_table(def)?;

                let schema = Arc::new(Schema::new(vec![
                    Field::new("id", DataType::Int64, false),
                    Field::new("kind", DataType::Utf8, false),
                    Field::new("amount", DataType::Float64, false),
                ]));
                let batch = RecordBatch::try_new(
                    schema,
                    vec![
                        Arc::new(Int64Array::from(vec![1, 2, 3])),
                        Arc::new(StringArray::from(vec!["click", "view", "click"])),
                        Arc::new(Float64Array::from(vec![1.0, 2.0, 3.0])),
                    ],
                )?;
                engine_a.insert("cloud_events", batch)?;
                engine_a.flush("cloud_events")?;
                Ok(engine_a)
            })
            .await
            .map_err(|e| less_common::LessError::Engine(e.to_string()))
            .unwrap()
            .unwrap()
        };

        // Node B: nothing local, same shared storage.
        let engine_b = LessEngine::open(EngineConfig::with_shared_url(
            base.join("node-b"),
            &shared_url,
        ))
        .unwrap();
        let session_b = LessSession::new_async(engine_b).await.unwrap();
        let batches = session_b
            .sql_batches(
                "SELECT kind, count(*) AS c, sum(amount) AS total \
                          FROM cloud_events GROUP BY kind ORDER BY kind",
            )
            .await
            .unwrap();
        let b = &batches[0];
        assert_eq!(b.num_rows(), 2);
        let counts = b.column(1).as_any().downcast_ref::<Int64Array>().unwrap();
        assert_eq!((counts.value(0), counts.value(1)), (2, 1)); // click x2, view x1

        std::fs::remove_dir_all(&base).ok();
    }

    #[tokio::test]
    async fn vector_search_sql_table_function() {
        let dir = std::env::temp_dir().join(format!("less-vecsql-{}", uuid::Uuid::new_v4()));
        let engine = LessEngine::open_local(&dir).unwrap();
        let session = LessSession::new_async(engine.clone()).await.unwrap();

        {
            let mut vectors = session.vectors().write().unwrap();
            vectors
                .create_space(
                    "docs",
                    3,
                    less_vector::Metric::L2,
                    less_vector::IndexKind::Flat,
                )
                .unwrap();
            vectors
                .add(
                    "docs",
                    vec![
                        vec![1.0, 0.0, 0.0],
                        vec![0.0, 1.0, 0.0],
                        vec![0.9, 0.1, 0.0],
                    ],
                    vec![
                        serde_json::json!({"title": "a"}),
                        serde_json::json!({"title": "b"}),
                        serde_json::json!({"title": "c"}),
                    ],
                )
                .unwrap();
        }

        // SQL-first vector search.
        let batches = session
            .sql_batches("SELECT * FROM vector_search('docs', [1.0, 0.0, 0.0], 2)")
            .await
            .unwrap();
        let b = &batches[0];
        assert_eq!(b.num_rows(), 2);
        assert_eq!(b.schema().field(0).name(), "id");
        assert_eq!(b.schema().field(1).name(), "score");
        let ids = b.column(0).as_any().downcast_ref::<UInt32Array>().unwrap();
        assert_eq!(ids.value(0), 0); // exact match first
        assert_eq!(ids.value(1), 2); // then the near one
        let scores = b.column(1).as_any().downcast_ref::<Float32Array>().unwrap();
        assert!(scores.value(0) < scores.value(1));
        let payloads = b.column(2).as_any().downcast_ref::<StringArray>().unwrap();
        assert_eq!(payloads.value(0), r#"{"title":"a"}"#);

        // Join vector hits with payload contents via SQL.
        let batches = session
            .sql_batches(
                "SELECT id, payload FROM vector_search('docs', [0.0, 1.0, 0.0], 1) \
                 WHERE payload LIKE '%b%'",
            )
            .await
            .unwrap();
        assert_eq!(batches[0].num_rows(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn sql_insert_into_and_copy_from() {
        let dir = std::env::temp_dir().join(format!("less-insert-{}", uuid::Uuid::new_v4()));
        let engine = LessEngine::open_local(&dir).unwrap();
        let mut def = TableDef::new(
            "target",
            SchemaSpec {
                fields: vec![
                    FieldSpec::new("x", TypeSpec::Int64),
                    FieldSpec::new("s", TypeSpec::Utf8),
                ],
            },
            EngineKind::Firefly,
        );
        def.sort_key = vec!["x".into()];
        def.unique = vec!["x".into()];
        engine.create_table(def).unwrap();
        let session = LessSession::new_async(engine.clone()).await.unwrap();

        // INSERT INTO ... VALUES
        session
            .sql_batches("INSERT INTO target VALUES (1, 'a'), (2, 'b')")
            .await
            .unwrap();

        // INSERT INTO ... SELECT (from a source table)
        let mut src = TableDef::new(
            "source",
            SchemaSpec {
                fields: vec![
                    FieldSpec::new("x", TypeSpec::Int64),
                    FieldSpec::new("s", TypeSpec::Utf8),
                ],
            },
            EngineKind::Firefly,
        );
        src.sort_key = vec!["x".into()];
        engine.create_table(src).unwrap();
        session.refresh_async().await.unwrap();
        session
            .sql_batches("INSERT INTO source VALUES (3, 'c'), (4, 'd')")
            .await
            .unwrap();
        session
            .sql_batches("INSERT INTO target SELECT * FROM source")
            .await
            .unwrap();

        // File ingestion: register the file, then INSERT INTO ... SELECT.
        let csv_path = dir.join("more.csv");
        std::fs::write(&csv_path, "x,s\n5,e\n6,f\n").unwrap();
        session
            .register_file("staging", csv_path.to_str().unwrap())
            .await
            .unwrap();
        session
            .sql_batches("INSERT INTO target SELECT * FROM staging")
            .await
            .unwrap();

        let batches = session
            .sql_batches("SELECT count(*) AS n, sum(x) AS total FROM target")
            .await
            .unwrap();
        let b = &batches[0];
        assert_eq!(b.num_rows(), 1);
        let n = b.column(0).as_any().downcast_ref::<Int64Array>().unwrap();
        let total = b.column(1).as_any().downcast_ref::<Int64Array>().unwrap();
        // 1..=6 (unique on x: all distinct) => n=6, sum=21
        assert_eq!((n.value(0), total.value(0)), (6, 21));

        // COPY TO works (export).
        let out = dir.join("exported.parquet");
        session
            .sql_batches(&format!(
                "COPY (SELECT * FROM target) TO '{}'",
                out.display()
            ))
            .await
            .unwrap();
        assert!(out.exists() && std::fs::metadata(&out).unwrap().len() > 0);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn refresh_picks_up_new_tables() {
        let dir = std::env::temp_dir().join(format!("less-refresh-{}", uuid::Uuid::new_v4()));
        let engine = LessEngine::open_local(&dir).unwrap();
        let session = LessSession::new_async(engine.clone()).await.unwrap();

        let def = TableDef::new(
            "fresh",
            SchemaSpec {
                fields: vec![FieldSpec::new("x", TypeSpec::Int64)],
            },
            EngineKind::Firefly,
        );
        engine.create_table(def).unwrap();
        assert!(session.sql_batches("SELECT * FROM fresh").await.is_err());

        session.refresh_async().await.unwrap();
        let batches = session
            .sql_batches("SELECT count(*) FROM fresh")
            .await
            .unwrap();
        assert_eq!(batches[0].num_rows(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn memory_pool_enforces_limit_and_tracks_gauge() {
        use datafusion::execution::memory_pool::MemoryConsumer;
        let pool: Arc<dyn MemoryPool> = Arc::new(TrackingMemoryPool {
            inner: GreedyMemoryPool::new(1024),
            limit: 1024,
        });
        let consumer = MemoryConsumer::new("test");
        let reservation = consumer.register(&pool);
        reservation.try_resize(512).unwrap();
        assert_eq!(pool.reserved(), 512);
        assert_eq!(less_telemetry::global().memory_pool_bytes.get(), 512);
        // Beyond the 1 KiB pool → rejected, and the gauge still reflects
        // the old reservation.
        assert!(reservation.try_resize(2048).is_err());
        assert_eq!(pool.reserved(), 512);
        reservation.free();
        assert_eq!(pool.reserved(), 0);
        assert_eq!(less_telemetry::global().memory_pool_bytes.get(), 0);
    }

    #[tokio::test]
    async fn session_with_memory_limit_runs_queries() {
        let dir = std::env::temp_dir().join(format!("less-memlimit-{}", uuid::Uuid::new_v4()));
        let mut config = EngineConfig::with_data_dir(&dir);
        config.memory_limit = 64 << 20; // 64 MiB pool
        let engine = LessEngine::open(config).unwrap();
        let def = TableDef::new(
            "ml",
            SchemaSpec {
                fields: vec![
                    FieldSpec::new("id", TypeSpec::Int64),
                    FieldSpec::new("v", TypeSpec::Float64),
                ],
            },
            EngineKind::Firefly,
        );
        engine.create_table(def).unwrap();
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("v", DataType::Float64, false),
        ]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(Int64Array::from(vec![1, 2, 3, 4])),
                Arc::new(Float64Array::from(vec![1.5, 2.5, 3.5, 4.5])),
            ],
        )
        .unwrap();
        engine.insert("ml", batch).unwrap();
        engine.flush("ml").unwrap();

        let session = LessSession::new_async(engine).await.unwrap();
        let batches = session
            .sql_batches("SELECT sum(v) AS total FROM ml")
            .await
            .unwrap();
        let b = &batches[0];
        let total = b.column(0).as_any().downcast_ref::<Float64Array>().unwrap();
        assert_eq!(total.value(0), 12.0);
        // The tracking pool reported its reservations at least once.
        assert!(less_telemetry::global().memory_pool_bytes.get() >= 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn shard_predicate_splits_parts_across_nodes() {
        let dir = std::env::temp_dir().join(format!("less-shard-{}", uuid::Uuid::new_v4()));
        let engine = LessEngine::open_local(&dir).unwrap();
        let def = TableDef::new(
            "sh",
            SchemaSpec {
                fields: vec![FieldSpec::new("id", TypeSpec::Int64)],
            },
            EngineKind::Firefly,
        );
        engine.create_table(def).unwrap();
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        // Four flushable parts so sharding has something to split.
        for round in 0..4 {
            let ids: Vec<i64> = (0..10).map(|k| round * 10 + k).collect();
            let batch = RecordBatch::try_new(schema.clone(), vec![Arc::new(Int64Array::from(ids))])
                .unwrap();
            engine.insert("sh", batch).unwrap();
            engine.flush("sh").unwrap();
        }
        let session = LessSession::new_async(engine).await.unwrap();

        let total = session
            .sql_batches("SELECT count(*) FROM sh")
            .await
            .unwrap();
        let all = total[0]
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .value(0);
        assert_eq!(all, 40);

        // The union of every shard is the whole table; each shard sees a
        // (possibly empty) subset of the parts.
        let mut seen = 0i64;
        let mut per_shard = vec![];
        for i in 0..4 {
            let b = session
                .sql_batches(&format!(
                    "SELECT count(*) FROM sh WHERE lessdb_shard(id, {i}, 4)"
                ))
                .await
                .unwrap();
            let n = b[0]
                .column(0)
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap()
                .value(0);
            per_shard.push(n);
            seen += n;
        }
        assert_eq!(seen, 40, "shards must partition the parts: {per_shard:?}");
        // n=1 keeps everything.
        let one = session
            .sql_batches("SELECT count(*) FROM sh WHERE lessdb_shard(id, 0, 1)")
            .await
            .unwrap();
        assert_eq!(
            one[0]
                .column(0)
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap()
                .value(0),
            40
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn gpu_filtered_sum_sql_matches_sum() {
        let dir = std::env::temp_dir().join(format!("less-gpusum-{}", uuid::Uuid::new_v4()));
        let engine = LessEngine::open_local(&dir).unwrap();
        let def = TableDef::new(
            "t",
            SchemaSpec {
                fields: vec![FieldSpec::new("v", TypeSpec::Float64)],
            },
            EngineKind::Firefly,
        );
        engine.create_table(def).unwrap();
        let schema = Arc::new(Schema::new(vec![Field::new("v", DataType::Float64, true)]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![Arc::new(Float64Array::from(vec![
                Some(1.5),
                None,
                Some(-2.0),
                Some(3.25),
                Some(0.0),
            ]))],
        )
        .unwrap();
        engine.insert("t", batch).unwrap();
        engine.flush("t").unwrap();

        let session = LessSession::new_async(engine.clone()).await.unwrap();
        let gpu = session
            .sql_batches("SELECT gpu_filtered_sum(v) AS s FROM t")
            .await
            .unwrap();
        let cpu = session
            .sql_batches("SELECT sum(v) AS s FROM t")
            .await
            .unwrap();
        let g = gpu[0]
            .column(0)
            .as_any()
            .downcast_ref::<Float64Array>()
            .unwrap()
            .value(0);
        let c = cpu[0]
            .column(0)
            .as_any()
            .downcast_ref::<Float64Array>()
            .unwrap()
            .value(0);
        // Small batch: always the CPU path, so exact equality (and it must
        // match SQL `sum`, which ignores nulls).
        assert_eq!(g, c);
        assert_eq!(g, 1.5 - 2.0 + 3.25 + 0.0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn gpu_dot_sql_matches_sum_xy() {
        let dir = std::env::temp_dir().join(format!("less-gpudot-{}", uuid::Uuid::new_v4()));
        let engine = LessEngine::open_local(&dir).unwrap();
        let def = TableDef::new(
            "t",
            SchemaSpec {
                fields: vec![
                    FieldSpec::new("x", TypeSpec::Float64),
                    FieldSpec::new("y", TypeSpec::Float64),
                ],
            },
            EngineKind::Firefly,
        );
        engine.create_table(def).unwrap();
        let schema = Arc::new(Schema::new(vec![
            Field::new("x", DataType::Float64, true),
            Field::new("y", DataType::Float64, true),
        ]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(Float64Array::from(vec![
                    Some(2.0),
                    None,
                    Some(3.0),
                    Some(-1.0),
                ])),
                Arc::new(Float64Array::from(vec![
                    Some(1.5),
                    Some(10.0),
                    None,
                    Some(4.0),
                ])),
            ],
        )
        .unwrap();
        engine.insert("t", batch).unwrap();
        engine.flush("t").unwrap();

        let session = LessSession::new_async(engine.clone()).await.unwrap();
        let gpu = session
            .sql_batches("SELECT gpu_dot(x, y) AS d FROM t")
            .await
            .unwrap();
        let cpu = session
            .sql_batches("SELECT sum(x * y) AS d FROM t")
            .await
            .unwrap();
        let g = gpu[0]
            .column(0)
            .as_any()
            .downcast_ref::<Float64Array>()
            .unwrap()
            .value(0);
        let c = cpu[0]
            .column(0)
            .as_any()
            .downcast_ref::<Float64Array>()
            .unwrap()
            .value(0);
        assert_eq!(g, c);
        assert_eq!(g, -1.0); // 2.0*1.5 + (-1.0)*4.0
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn gpu_udaf_float32_null_and_empty_fallback() {
        let dir = std::env::temp_dir().join(format!("less-gpuf32-{}", uuid::Uuid::new_v4()));
        let engine = LessEngine::open_local(&dir).unwrap();
        let schema = Arc::new(Schema::new(vec![Field::new("v", DataType::Float32, true)]));

        // Float32 column: the GPU kernel is f64-only, so this must fall back.
        let def = TableDef::new(
            "f32t",
            SchemaSpec {
                fields: vec![FieldSpec::new("v", TypeSpec::Float32)],
            },
            EngineKind::Firefly,
        );
        engine.create_table(def).unwrap();
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![Arc::new(Float32Array::from(vec![
                Some(1.5f32),
                None,
                Some(2.5f32),
            ]))],
        )
        .unwrap();
        engine.insert("f32t", batch).unwrap();
        engine.flush("f32t").unwrap();

        // All-null Float64 column.
        let def_null = TableDef::new(
            "allnull",
            SchemaSpec {
                fields: vec![FieldSpec::new("v", TypeSpec::Float64)],
            },
            EngineKind::Firefly,
        );
        engine.create_table(def_null).unwrap();
        let null_schema = Arc::new(Schema::new(vec![Field::new("v", DataType::Float64, true)]));
        let null_batch = RecordBatch::try_new(
            null_schema.clone(),
            vec![Arc::new(Float64Array::from(vec![None::<f64>, None::<f64>]))],
        )
        .unwrap();
        engine.insert("allnull", null_batch).unwrap();
        engine.flush("allnull").unwrap();

        // Truly empty table.
        let def_empty = TableDef::new(
            "empty",
            SchemaSpec {
                fields: vec![FieldSpec::new("v", TypeSpec::Float64)],
            },
            EngineKind::Firefly,
        );
        engine.create_table(def_empty).unwrap();

        let session = LessSession::new_async(engine.clone()).await.unwrap();
        session.refresh_async().await.unwrap();

        let f32 = session
            .sql_batches("SELECT gpu_filtered_sum(v) FROM f32t")
            .await
            .unwrap();
        let v = f32[0]
            .column(0)
            .as_any()
            .downcast_ref::<Float64Array>()
            .unwrap();
        assert_eq!(v.value(0), 4.0);

        let allnull = session
            .sql_batches("SELECT gpu_filtered_sum(v) FROM allnull")
            .await
            .unwrap();
        assert!(
            allnull[0]
                .column(0)
                .as_any()
                .downcast_ref::<Float64Array>()
                .unwrap()
                .is_null(0)
        );

        let empty = session
            .sql_batches("SELECT gpu_filtered_sum(v) FROM empty")
            .await
            .unwrap();
        assert!(
            empty[0]
                .column(0)
                .as_any()
                .downcast_ref::<Float64Array>()
                .unwrap()
                .is_null(0)
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn gpu_enabled_matches_sum() {
        // `gpu_enabled = true` arms the GPU path; when no adapter exists the
        // GPU crate's availability check (guarded by an init timeout) must
        // fall back to CPU, so the result still matches `sum`. On a machine
        // with a GPU the large batch exercises the wgpu kernel; the f32
        // rounding means an approximate (not exact) comparison.
        let dir = std::env::temp_dir().join(format!("less-gpuon-{}", uuid::Uuid::new_v4()));
        let mut config = EngineConfig::with_data_dir(&dir);
        config.gpu_enabled = true;
        let engine = LessEngine::open(config).unwrap();
        let def = TableDef::new(
            "big",
            SchemaSpec {
                fields: vec![FieldSpec::new("v", TypeSpec::Float64)],
            },
            EngineKind::Firefly,
        );
        engine.create_table(def).unwrap();
        let schema = Arc::new(Schema::new(vec![Field::new("v", DataType::Float64, false)]));
        let n = 8192usize;
        let values: Vec<f64> = (0..n).map(|i| i as f64 * 0.5 - 1.0).collect();
        let batch =
            RecordBatch::try_new(schema.clone(), vec![Arc::new(Float64Array::from(values))])
                .unwrap();
        engine.insert("big", batch).unwrap();
        engine.flush("big").unwrap();

        let session = LessSession::new_async(engine).await.unwrap();
        let gpu = session
            .sql_batches("SELECT gpu_filtered_sum(v) FROM big")
            .await
            .unwrap();
        let cpu = session.sql_batches("SELECT sum(v) FROM big").await.unwrap();
        let g = gpu[0]
            .column(0)
            .as_any()
            .downcast_ref::<Float64Array>()
            .unwrap()
            .value(0);
        let c = cpu[0]
            .column(0)
            .as_any()
            .downcast_ref::<Float64Array>()
            .unwrap()
            .value(0);
        let diff = (g - c).abs();
        assert!(
            diff <= c.abs() * 1e-5 + 1e-3,
            "gpu_filtered_sum={g} vs sum={c} (diff {diff})"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
