//! The LessDB table engine.
//!
//! [`LessEngine`] is a synchronous facade (internally backed by a small
//! tokio runtime for object-store I/O) providing:
//!
//! * table lifecycle (create/drop/list),
//! * buffered inserts that auto-flush into immutable parts,
//! * part discovery with metadata for pruning,
//! * `optimize` (merge) with uniqueness deduplication,
//! * local Firefly and shared (object-storage) FireflyCloud backends.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use arrow::array::BooleanArray;
use arrow::datatypes::Schema;
use arrow::record_batch::RecordBatch;
use bytes::Bytes;

use less_catalog::{
    Catalog, EngineKind, FieldSpec, MetaStore, ObjectMetaStore, SharedCatalog, TableDef, TypeSpec,
};
use less_common::{EngineConfig, LessError, Result};
use less_storage::{
    BlockCache, Compression, DATA_FILE, META_FILE, PartMeta, SharedStore, WriteOptions,
    list_part_metas, read_part, read_part_from_bytes, write_part, write_part_sorted,
};

use crate::merge::merge_all;
use crate::part::{DataPart, PartLocation};
use crate::wal::Wal;

/// Is row `r` of a TTL column older than `cutoff` (unix seconds)?
fn row_expired(arr: &dyn arrow::array::Array, r: usize, cutoff: u64, ty: &TypeSpec) -> bool {
    use arrow::array::{Date32Array, TimestampMillisecondArray, TimestampNanosecondArray};
    match ty {
        TypeSpec::TimestampMs => {
            let a = arr
                .as_any()
                .downcast_ref::<TimestampMillisecondArray>()
                .unwrap();
            ((a.value(r) / 1000) as u64) < cutoff
        }
        TypeSpec::TimestampNs => {
            let a = arr
                .as_any()
                .downcast_ref::<TimestampNanosecondArray>()
                .unwrap();
            ((a.value(r) / 1_000_000_000) as u64) < cutoff
        }
        TypeSpec::Date32 => {
            let a = arr.as_any().downcast_ref::<Date32Array>().unwrap();
            (a.value(r) as u64) < cutoff / 86_400
        }
        other => panic!("TTL column of unsupported type {other:?} (validated at CREATE)"),
    }
}

/// Merge level encoded in a part name (`{ts}_{uuid}_{rows}_{level}`).
fn part_level(meta: &PartMeta) -> u32 {
    meta.name
        .rsplit('_')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// In-memory insert buffer for one table.
#[derive(Default)]
struct Buffer {
    batches: Vec<RecordBatch>,
    rows: usize,
}

/// Cumulative statistics for a table.
#[derive(Debug, Clone)]
pub struct TableStats {
    pub table: String,
    pub part_count: usize,
    pub rows: u64,
    pub buffered_rows: usize,
    pub disk_bytes: u64,
}

/// The LessDB engine.
///
/// **Compute/storage separation**: the engine is a compute node. Durable
/// state of local Firefly tables lives under `config.data_dir`
/// (node-affine storage), while *all* durable state of FireflyCloud
/// tables — data parts and table manifests — lives in the shared object
/// store at `config.shared_url`. A node's local disk is otherwise
/// ephemeral scratch (insert buffers, caches).
pub struct LessEngine {
    pub config: EngineConfig,
    catalog: Catalog,
    shared: Option<SharedStore>,
    shared_catalog: SharedCatalog,
    wal: Wal,
    lsn: std::sync::atomic::AtomicU64,
    last_wal_lsn: Mutex<HashMap<String, u64>>,
    /// Tokio runtime driving object-store I/O. `Option` so `Drop` can hand
    /// it to a plain OS thread: tokio panics if a runtime is dropped from
    /// inside an async context.
    runtime: Option<Arc<tokio::runtime::Runtime>>,
    /// Own handle, so sync wrappers can build `'static` futures for
    /// [`Self::block_on_owned`].
    self_arc: std::sync::OnceLock<Arc<LessEngine>>,
    /// This compute node's identity for multi-writer coordination.
    node_id: String,
    /// CAS coordination store (merge claims, part publication) — lives in
    /// shared storage so writers on different boxes coordinate.
    metastore: Arc<dyn MetaStore>,
    buffers: Mutex<HashMap<String, Buffer>>,
}

impl Drop for LessEngine {
    fn drop(&mut self) {
        if let Some(rt) = self.runtime.take() {
            std::thread::spawn(move || drop(rt));
        }
    }
}

impl std::fmt::Debug for LessEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LessEngine")
            .field("config", &self.config)
            .finish()
    }
}

impl LessEngine {
    /// Open (or initialize) a database at `config.data_dir`.
    pub fn open(config: EngineConfig) -> Result<Arc<Self>> {
        let catalog = Catalog::open(&config.data_dir)?;
        // Shared storage: a cloud URL when configured (`s3://...`), else a
        // local object-store root under the data dir (dev mode). The code
        // path is identical — that's the compute/storage separation.
        let shared = Some(
            match &config.shared_url {
                Some(url) => SharedStore::new_with_url(url)?,
                None => SharedStore::new_local(&config.data_dir.join("shared"))?,
            }
            .with_cache(Arc::new(BlockCache::new(
                config.block_cache_bytes,
                config.block_cache_dir.clone(),
            ))),
        );
        let shared_catalog = SharedCatalog::new(shared.clone().expect("just constructed"));
        let metastore: Arc<dyn MetaStore> = Arc::new(ObjectMetaStore::new(
            shared.as_ref().expect("just constructed").store(),
            "metastore/",
        ));
        let node_id = config
            .node_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("less-io")
                .enable_all()
                .build()
                .map_err(|e| LessError::Engine(format!("failed to start io runtime: {e}")))?,
        );
        let wal = Wal::open(
            &config.data_dir.join("wal"),
            config.wal_enabled && config.wal_fsync,
        )?;
        let engine = Arc::new(Self {
            config,
            catalog,
            shared,
            shared_catalog,
            wal,
            lsn: std::sync::atomic::AtomicU64::new(1),
            last_wal_lsn: Mutex::new(HashMap::new()),
            runtime: Some(runtime),
            self_arc: std::sync::OnceLock::new(),
            node_id,
            metastore,
            buffers: Mutex::new(HashMap::new()),
        });
        engine.self_arc.set(engine.clone()).ok();
        // Sweep torn part writes (dirs killed before their meta.json landed)
        // before anything lists parts, then recover: replay WAL records not
        // yet durable in parts and flush them so recovered rows are
        // immediately queryable.
        engine.sweep_incomplete_parts()?;
        if engine.config.wal_enabled {
            engine.recover()?;
        }
        Ok(engine)
    }

    /// Remove part directories that never committed their `meta.json`
    /// (crashed mid-`write_part`): they must never become visible, and
    /// dropping them keeps the parts directory tidy at startup.
    fn sweep_incomplete_parts(&self) -> Result<()> {
        for table in self.catalog.tables()? {
            let Ok(def) = self.table(&table) else {
                continue;
            };
            if def.engine != EngineKind::Firefly {
                continue; // shared parts are object-store objects
            }
            let parts_dir = self.catalog.parts_dir(&table);
            let entries = match std::fs::read_dir(&parts_dir) {
                Ok(e) => e,
                Err(_) => continue, // no local parts yet
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && !path.join(META_FILE).exists() {
                    let _ = std::fs::remove_dir_all(&path);
                }
            }
        }
        Ok(())
    }

    /// Replay WAL records newer than the newest durable part per table,
    /// then flush them (idempotent via per-part `wal_lsn_max`).
    fn recover(&self) -> Result<()> {
        let tables = self.wal.tables_with_wal()?;
        for table in tables {
            let max_flushed = self
                .parts(&table)
                .ok()
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(|p| p.meta.wal_lsn_max)
                        .max()
                        .unwrap_or(0)
                })
                .unwrap_or(0);
            let batches = self.wal.replay(&table, max_flushed)?;
            if batches.is_empty() {
                continue;
            }
            {
                let mut buffers = self.buffers.lock().unwrap();
                let buf = buffers.entry(table.clone()).or_default();
                for batch in batches {
                    buf.rows += batch.num_rows();
                    buf.batches.push(batch);
                }
            }
            self.flush(&table)?;
        }
        Ok(())
    }

    /// Open a database in a directory with default settings.
    pub fn open_local(dir: impl Into<PathBuf>) -> Result<Arc<Self>> {
        Self::open(EngineConfig::with_data_dir(dir.into()))
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    /// The shared catalog (table manifests in shared storage).
    pub fn shared_catalog(&self) -> &SharedCatalog {
        &self.shared_catalog
    }

    pub fn shared(&self) -> Option<&SharedStore> {
        self.shared.as_ref()
    }

    /// Block-cache stats for the shared store (hits/misses/bytes), when
    /// the cache is configured.
    pub fn cache_stats(&self) -> Option<less_storage::block_cache::CacheStats> {
        self.shared
            .as_ref()
            .and_then(|s| s.cache())
            .map(|c| c.stats())
    }

    /// The CAS coordination store backing FireflyCloud publication and
    /// merge claims (lives in shared storage).
    pub fn metastore(&self) -> &Arc<dyn MetaStore> {
        &self.metastore
    }

    /// This compute node's identity for coordination claims.
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// The runtime used for shared/object-store I/O.
    pub fn runtime(&self) -> &Arc<tokio::runtime::Runtime> {
        self.runtime
            .as_ref()
            .expect("io runtime is only absent while the engine is being dropped")
    }

    /// Run a `'static` future to completion and return its output.
    ///
    /// From a plain sync context this drives the future on the engine's own
    /// io-runtime. When the caller is already inside *any* tokio runtime
    /// context (e.g. the CLI/server/MCP multi-thread runtime executing a
    /// sync engine API), tokio forbids nesting `block_on` on that thread —
    /// so the future is spawned onto the engine's io-runtime and the
    /// calling thread waits on a channel instead. This is what makes sync
    /// engine APIs safe to call from async contexts.
    pub fn block_on_owned<F, T>(&self, fut: F) -> T
    where
        F: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        if tokio::runtime::Handle::try_current().is_err() {
            return self.runtime().block_on(fut);
        }
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        self.runtime().spawn(async move {
            let _ = tx.send(fut.await);
        });
        rx.recv()
            .expect("engine io-runtime dropped while a sync call was in flight")
    }

    /// The engine's own `Arc` (for `'static` futures in sync wrappers).
    fn self_arc(&self) -> Result<Arc<Self>> {
        self.self_arc
            .get()
            .cloned()
            .ok_or_else(|| LessError::Engine("engine self-handle unavailable".into()))
    }

    // ---- table lifecycle -------------------------------------------------

    pub fn create_table(&self, def: TableDef) -> Result<()> {
        def.validate()?;
        if def.engine.is_shared() {
            // Manifest lives in shared storage: any compute node can
            // discover this table.
            let this = self.self_arc()?;
            self.block_on_owned(async move { this.shared_catalog.put_table(&def).await })?;
        } else {
            self.catalog.create_table(&def)?;
            let _ = std::fs::create_dir_all(self.catalog.parts_dir(&def.name));
        }
        Ok(())
    }

    pub fn drop_table(&self, name: &str) -> Result<()> {
        let def = self.table(name)?;
        if def.engine.is_shared() {
            let shared = self
                .shared
                .as_ref()
                .ok_or_else(|| {
                    LessError::Engine("shared store unavailable for FireflyCloud table".into())
                })?
                .clone();
            let name = name.to_string();
            let this = self.self_arc()?;
            // Delete part objects first; the manifest last, so the table
            // disappears from every node's view atomically at the end.
            self.block_on_owned(async move {
                shared.delete_prefix(&format!("tables/{name}/")).await?;
                this.shared_catalog.delete_table(&name).await
            })?;
        } else {
            self.catalog.drop_table(name)?;
        }
        self.buffers.lock().unwrap().remove(name);
        if self.config.wal_enabled {
            self.wal.remove_table(name)?;
        }
        Ok(())
    }

    /// Load a table definition, looking in the local catalog first and the
    /// shared catalog second (async; use from async contexts).
    pub async fn table_async(&self, name: &str) -> Result<TableDef> {
        match self.catalog.load_table(name) {
            Ok(def) => Ok(def),
            Err(_) => self.shared_catalog.get_table(name).await,
        }
    }

    /// Synchronous convenience wrapper for [`Self::table_async`]. Local
    /// tables never touch the io runtime (safe inside async contexts);
    /// shared tables block on the shared catalog.
    pub fn table(&self, name: &str) -> Result<TableDef> {
        match self.catalog.load_table(name) {
            Ok(def) => Ok(def),
            Err(_) => {
                let this = self.self_arc()?;
                let name = name.to_string();
                self.block_on_owned(async move { this.shared_catalog.get_table(&name).await })
            }
        }
    }

    /// All tables known to this compute node: local catalog + shared
    /// catalog (async; use from async contexts).
    pub async fn tables_async(&self) -> Result<Vec<String>> {
        let mut names = self.catalog.tables()?;
        names.extend(self.shared_catalog.tables().await?);
        names.sort();
        names.dedup();
        Ok(names)
    }

    /// Synchronous convenience wrapper for [`Self::tables_async`].
    pub fn tables(&self) -> Result<Vec<String>> {
        let this = self.self_arc()?;
        self.block_on_owned(async move { this.tables_async().await })
    }

    // ---- writes ----------------------------------------------------------

    /// Insert one record batch into a table. The batch is aligned to the
    /// table schema (column reordering allowed), appended to the in-memory
    /// buffer and flushed into an immutable part when the buffer fills.
    /// Returns the number of rows accepted.
    pub fn insert(&self, table: &str, batch: RecordBatch) -> Result<usize> {
        let def = self.table(table)?;
        let batch = align_batch(&def.arrow_schema(), batch)?;
        let rows = batch.num_rows();
        if rows == 0 {
            return Ok(0);
        }
        // WAL first: rows are durable before they are accepted.
        if self.config.wal_enabled {
            let lsn = self.lsn.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.wal.append(table, lsn, &batch)?;
            self.last_wal_lsn
                .lock()
                .unwrap()
                .insert(table.to_string(), lsn);
        }
        let should_flush = {
            let mut buffers = self.buffers.lock().unwrap();
            let buf = buffers.entry(table.to_string()).or_default();
            buf.batches.push(batch);
            buf.rows += rows;
            buf.rows >= self.config.flush_rows || buf.rows >= self.config.max_buffer_rows
        };
        if should_flush {
            self.flush(table)?;
        }
        less_telemetry::global().rows_inserted.add(rows as u64);
        less_telemetry::global()
            .buffered_rows
            .set(self.total_buffered_rows() as i64);
        Ok(rows)
    }

    /// Total rows buffered across all tables.
    pub fn buffered_rows(&self) -> Result<usize> {
        Ok(self.buffers.lock().unwrap().values().map(|b| b.rows).sum())
    }

    fn total_buffered_rows(&self) -> usize {
        self.buffers.lock().unwrap().values().map(|b| b.rows).sum()
    }

    /// Flush a table's in-memory buffer into a new immutable part.
    /// Returns the new part's metadata, or `None` if the buffer was empty.
    pub fn flush(&self, table: &str) -> Result<Option<PartMeta>> {
        let def = self.table(table)?;
        let batches = {
            let mut buffers = self.buffers.lock().unwrap();
            let buf = buffers.entry(table.to_string()).or_default();
            let taken = std::mem::take(&mut buf.batches);
            buf.rows = 0;
            taken
        };
        if batches.is_empty() {
            return Ok(None);
        }
        less_telemetry::global()
            .buffered_rows
            .set(self.total_buffered_rows() as i64);
        let schema = def.arrow_schema();
        let mut opts = self.write_options(&def)?;
        opts.wal_lsn_max = self.last_wal_lsn.lock().unwrap().get(table).copied();
        let meta = if def.engine.is_shared() {
            let shared = self
                .shared
                .as_ref()
                .ok_or_else(|| LessError::Engine("shared store unavailable".into()))?
                .clone();
            let name = def.name.clone();
            self.block_on_owned(async move {
                write_part_shared(&shared, &name, &schema, batches, &opts).await
            })?
        } else {
            write_part(
                &self.catalog.parts_dir(table),
                table,
                &schema,
                batches,
                &opts,
            )?
        };

        // Background merge policy (v1): once a table accumulates enough
        // small parts, merge them all into larger parts. (flush is a
        // synchronous API; see docs about calling it outside async runtimes
        // for FireflyCloud tables.)
        less_telemetry::global().parts_written.inc();
        less_telemetry::global()
            .buffered_rows
            .set(self.total_buffered_rows() as i64);

        // The part is durable and carries its WAL lsn; records up to it are
        // now safe to drop (replay skips them via wal_lsn_max).
        if self.config.wal_enabled {
            self.wal.truncate(table)?;
        }

        if self.parts(table)?.len() >= self.config.auto_merge_parts {
            merge_all(self, table)?;
        }
        self.enforce_ttl(table)?;
        Ok(Some(meta))
    }

    pub(crate) fn write_options(&self, def: &TableDef) -> Result<WriteOptions> {
        Ok(WriteOptions {
            compression: Compression::parse(&def.effective_compression(&self.config.compression))?,
            zstd_level: self.config.zstd_level,
            sort_key: def.sort_key.clone(),
            unique: def.unique.clone(),
            bloom_fp_rate: def.bloom_fp_rate,
            level: 0,
            wal_lsn_max: None,
        })
    }

    /// Merge all parts of a table into a single part, enforcing uniqueness
    /// constraints (keeping the last row per unique key). Returns the new
    /// part metadata, or `None` when there was nothing to merge.
    pub fn optimize(&self, table: &str) -> Result<Option<PartMeta>> {
        let merged = merge_all(self, table)?;
        // TTL retention rides along with the maintenance operation.
        self.enforce_ttl(table)?;
        Ok(merged)
    }

    /// Delete every row of `table` for which `matches` returns `true`
    /// (one boolean mask per batch). Buffered rows are flushed first so
    /// the delete covers the whole table; rewritten parts carry the old
    /// part's `wal_lsn_max`, so WAL replay cannot resurrect deleted rows.
    /// Returns the number of rows deleted.
    pub fn delete_where<F>(&self, table: &str, matches: F) -> Result<u64>
    where
        F: Fn(&RecordBatch) -> Result<BooleanArray> + Send + Sync + 'static,
    {
        let def = self.table(table)?;
        // Materialize buffered rows (also truncates the WAL).
        self.flush(table)?;
        if def.engine.is_shared() {
            return self.delete_where_shared(table, matches);
        }
        let mut deleted = 0u64;
        let parts_dir = self.catalog.parts_dir(table);
        for (dir, meta) in list_part_metas(&parts_dir)? {
            let batches = read_part(&dir, None)?;
            let total: usize = batches.iter().map(|b| b.num_rows()).sum();
            let mut kept_batches = Vec::with_capacity(batches.len());
            let mut removed = 0usize;
            for b in &batches {
                let mask = matches(b)?;
                removed += mask.true_count();
                let survivor = arrow::compute::kernels::boolean::not(&mask)?;
                let kept = arrow::compute::filter_record_batch(b, &survivor)?;
                kept_batches.push(kept);
            }
            if removed == 0 {
                continue;
            }
            deleted += removed as u64;
            if removed == total {
                std::fs::remove_dir_all(&dir)?;
                continue;
            }
            let mut opts = self.write_options(&def)?;
            opts.level = part_level(&meta);
            opts.wal_lsn_max = meta.wal_lsn_max;
            write_part(&parts_dir, table, &def.arrow_schema(), kept_batches, &opts)?;
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(deleted)
    }

    /// `ALTER TABLE … ADD COLUMN <name> <type>`: extend the schema and
    /// rewrite every part with the new column filled with a default
    /// (NULL for nullable types, 0/empty otherwise). Buffered rows are
    /// flushed first. Returns the new column count.
    pub fn alter_add_column(&self, table: &str, name: &str, ty: TypeSpec) -> Result<usize> {
        self.alter_schema(table, name, Some(ty))
    }

    /// `ALTER TABLE … DROP COLUMN <name>`: drop a column from the schema
    /// and rewrite every part without it. The sort key and UNIQUE lists
    /// must not reference the column. Returns the new column count.
    pub fn alter_drop_column(&self, table: &str, name: &str) -> Result<usize> {
        self.alter_schema(table, name, None)
    }

    fn alter_schema(&self, table: &str, name: &str, ty: Option<TypeSpec>) -> Result<usize> {
        let mut def = self.table(table)?;
        self.flush(table)?;
        if def.sort_key.iter().any(|c| c == name) || def.unique.iter().any(|c| c == name) {
            return Err(LessError::Engine(format!(
                "cannot alter column '{name}': it is part of the sort key / UNIQUE columns"
            )));
        }
        let old_schema = def.arrow_schema();
        let new_fields: Vec<FieldSpec> = match &ty {
            Some(t) => {
                if def.schema.fields.iter().any(|f| f.name == name) {
                    return Err(LessError::Engine(format!("column '{name}' already exists")));
                }
                let mut fields = def.schema.fields.clone();
                fields.push(FieldSpec::new(name, t.clone()));
                fields
            }
            None => {
                let before = def.schema.fields.len();
                def.schema.fields.retain(|f| f.name != name);
                if def.schema.fields.len() == before {
                    return Err(LessError::Engine(format!("column '{name}' does not exist")));
                }
                def.schema.fields.clone()
            }
        };
        def.schema.fields = new_fields;
        let new_schema = def.arrow_schema();

        let parts_dir = self.catalog.parts_dir(table);
        for (dir, meta) in list_part_metas(&parts_dir)? {
            let batches = read_part(&dir, None)?;
            let rewritten: Vec<RecordBatch> = batches
                .into_iter()
                .map(|b| alter_batch(&old_schema, &new_schema, &b, &ty))
                .collect::<Result<_>>()?;
            let mut opts = self.write_options(&def)?;
            opts.level = part_level(&meta);
            opts.wal_lsn_max = meta.wal_lsn_max;
            write_part(&parts_dir, table, &new_schema, rewritten, &opts)?;
            std::fs::remove_dir_all(&dir)?;
        }
        self.catalog.update_table(&def)?;
        Ok(def.schema.fields.len())
    }

    /// `TRUNCATE TABLE <t>`: drop every part and re-create the empty table
    /// (immutable parts make truncation a drop + recreate).
    pub fn truncate(&self, table: &str) -> Result<()> {
        let def = self.table(table)?;
        self.flush(table)?;
        let parts_dir = self.catalog.parts_dir(table);
        for (dir, _meta) in list_part_metas(&parts_dir)? {
            std::fs::remove_dir_all(dir)?;
        }
        let _ = def;
        Ok(())
    }

    /// Shared-table variant of [`Self::delete_where`]: each part is claimed
    /// in the metastore (like merges) so concurrent merges don't race the
    /// rewrite; a claim conflict fails loudly instead of deleting a subset.
    fn delete_where_shared<F>(&self, table: &str, matches: F) -> Result<u64>
    where
        F: Fn(&RecordBatch) -> Result<BooleanArray> + Send + Sync + 'static,
    {
        let this = self.self_arc()?;
        let table_owned = table.to_string();
        self.block_on_owned(async move {
            let def = this.table_async(&table_owned).await?;
            let shared = this
                .shared()
                .ok_or_else(|| LessError::Engine("shared store unavailable".into()))?
                .clone();
            let ms = this.metastore().clone();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let owner = this.node_id().to_string();
            let parts = this.parts_async(&table_owned).await?;
            let mut deleted = 0u64;
            for part in &parts {
                let PartLocation::Object(prefix) = &part.location else {
                    continue;
                };
                let claim_key = format!("merge-claims/{table_owned}/{}", part.meta.name);
                let claim = serde_json::json!({ "owner": &owner, "expires_at": now + 600 })
                    .to_string()
                    .into_bytes();
                let (ms2, key2) = (ms.clone(), claim_key.clone());
                let claimed = ms2.put_if_absent(&key2, claim).await?;
                if !claimed {
                    return Err(LessError::Engine(format!(
                        "DELETE conflicted with a concurrent merge on part {}; retry",
                        part.meta.name
                    )));
                }
                let result: Result<u64> = async {
                    let batches = this.read_data_part_async(part).await?;
                    let total: usize = batches.iter().map(|b| b.num_rows()).sum();
                    let mut kept_batches = Vec::with_capacity(batches.len());
                    let mut removed = 0usize;
                    for b in &batches {
                        let mask = matches(b)?;
                        removed += mask.true_count();
                        let survivor = arrow::compute::kernels::boolean::not(&mask)?;
                        kept_batches.push(arrow::compute::filter_record_batch(b, &survivor)?);
                    }
                    if removed == 0 {
                        return Ok(0);
                    }
                    let schema = def.arrow_schema();
                    let mut opts = this.write_options(&def)?;
                    opts.level = part.level();
                    opts.wal_lsn_max = part.meta.wal_lsn_max;
                    if removed == total {
                        let _ = shared.delete(&format!("{prefix}/{DATA_FILE}")).await;
                        let _ = shared.delete(&format!("{prefix}/{META_FILE}")).await;
                    } else {
                        write_part_shared(&shared, &table_owned, &schema, kept_batches, &opts)
                            .await?;
                        let _ = shared.delete(&format!("{prefix}/{DATA_FILE}")).await;
                        let _ = shared.delete(&format!("{prefix}/{META_FILE}")).await;
                    }
                    Ok(removed as u64)
                }
                .await;
                let (ms2, key2) = (ms.clone(), claim_key.clone());
                let _ = ms2.delete(&key2).await;
                deleted += result?;
            }
            Ok(deleted)
        })
    }

    /// Update every row of `table` matching the predicate encoded in
    /// `update` — the closure returns `None` for untouched parts and
    /// `Some((rewritten_batch, updated_rows))` when rows changed. Buffered
    /// rows are flushed first; rewritten parts keep the old `wal_lsn_max`.
    /// Returns the number of rows updated.
    pub fn update_where<F>(&self, table: &str, update: F) -> Result<u64>
    where
        F: Fn(&RecordBatch) -> Result<Option<(RecordBatch, u64)>> + Send + Sync + 'static,
    {
        let def = self.table(table)?;
        self.flush(table)?;
        if def.engine.is_shared() {
            return self.update_where_shared(table, update);
        }
        let mut updated = 0u64;
        let parts_dir = self.catalog.parts_dir(table);
        for (dir, meta) in list_part_metas(&parts_dir)? {
            let batches = read_part(&dir, None)?;
            let mut rewritten = Vec::with_capacity(batches.len());
            let mut changed = 0u64;
            for b in &batches {
                match update(b)? {
                    None => rewritten.push(b.clone()),
                    Some((new_batch, n)) => {
                        changed += n;
                        rewritten.push(new_batch);
                    }
                }
            }
            if changed == 0 {
                continue;
            }
            updated += changed;
            let mut opts = self.write_options(&def)?;
            opts.level = part_level(&meta);
            opts.wal_lsn_max = meta.wal_lsn_max;
            write_part(&parts_dir, table, &def.arrow_schema(), rewritten, &opts)?;
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(updated)
    }

    /// Shared-table variant of [`Self::update_where`] (claims like merges).
    fn update_where_shared<F>(&self, table: &str, update: F) -> Result<u64>
    where
        F: Fn(&RecordBatch) -> Result<Option<(RecordBatch, u64)>> + Send + Sync + 'static,
    {
        let this = self.self_arc()?;
        let table_owned = table.to_string();
        self.block_on_owned(async move {
            let def = this.table_async(&table_owned).await?;
            let shared = this
                .shared()
                .ok_or_else(|| LessError::Engine("shared store unavailable".into()))?
                .clone();
            let ms = this.metastore().clone();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let owner = this.node_id().to_string();
            let parts = this.parts_async(&table_owned).await?;
            let mut updated = 0u64;
            for part in &parts {
                let PartLocation::Object(prefix) = &part.location else {
                    continue;
                };
                let claim_key = format!("merge-claims/{table_owned}/{}", part.meta.name);
                let claim = serde_json::json!({ "owner": &owner, "expires_at": now + 600 })
                    .to_string()
                    .into_bytes();
                let (ms2, key2) = (ms.clone(), claim_key.clone());
                if !ms2.put_if_absent(&key2, claim).await? {
                    return Err(LessError::Engine(format!(
                        "UPDATE conflicted with a concurrent merge on part {}; retry",
                        part.meta.name
                    )));
                }
                let result: Result<u64> = async {
                    let batches = this.read_data_part_async(part).await?;
                    let mut rewritten = Vec::with_capacity(batches.len());
                    let mut changed = 0u64;
                    for b in &batches {
                        match update(b)? {
                            None => rewritten.push(b.clone()),
                            Some((new_batch, n)) => {
                                changed += n;
                                rewritten.push(new_batch);
                            }
                        }
                    }
                    if changed == 0 {
                        return Ok(0);
                    }
                    let schema = def.arrow_schema();
                    let mut opts = this.write_options(&def)?;
                    opts.level = part.level();
                    opts.wal_lsn_max = part.meta.wal_lsn_max;
                    write_part_shared(&shared, &table_owned, &schema, rewritten, &opts).await?;
                    let _ = shared.delete(&format!("{prefix}/{DATA_FILE}")).await;
                    let _ = shared.delete(&format!("{prefix}/{META_FILE}")).await;
                    Ok(changed)
                }
                .await;
                let (ms2, key2) = (ms.clone(), claim_key.clone());
                let _ = ms2.delete(&key2).await;
                updated += result?;
            }
            Ok(updated)
        })
    }

    /// Drop rows older than the table's TTL (`ttl_col` before
    /// `now - ttl_secs`). Whole expired parts are removed and mixed parts
    /// rewritten via the same replace-parts machinery as DELETE. Runs
    /// automatically inside `optimize`. Returns rows dropped.
    pub fn enforce_ttl(&self, table: &str) -> Result<u64> {
        let def = self.table(table)?;
        let (Some(col), Some(secs)) = (def.ttl_col.clone(), def.ttl_secs) else {
            return Ok(0);
        };
        let field = def
            .schema
            .fields
            .iter()
            .find(|f| f.name == col)
            .cloned()
            .ok_or_else(|| LessError::Catalog(format!("TTL column '{col}' missing")))?;
        let ty = field.ty;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let cutoff = now.saturating_sub(secs);

        let col_for_closure = col.clone();
        self.delete_where(table, move |batch| {
            let idx = batch
                .schema()
                .index_of(&col_for_closure)
                .map_err(|e| LessError::Engine(format!("TTL column lookup: {e}")))?;
            let arr = batch.column(idx);
            let mask: Vec<bool> = (0..batch.num_rows())
                .map(|r| row_expired(arr.as_ref(), r, cutoff, &ty))
                .collect();
            Ok(BooleanArray::from(mask))
        })
    }

    // ---- reads -----------------------------------------------------------

    /// All parts of a table, oldest first (async; use from async contexts).
    pub async fn parts_async(&self, table: &str) -> Result<Vec<DataPart>> {
        let def = self.table_async(table).await?;
        if def.engine.is_shared() {
            let shared = self
                .shared
                .as_ref()
                .ok_or_else(|| LessError::Engine("shared store unavailable".into()))?;
            let prefix = format!("tables/{table}/parts/");
            let metas = shared.list_part_metas(&prefix).await?;
            Ok(metas
                .into_iter()
                .map(|(key, meta)| DataPart {
                    table: table.to_string(),
                    meta,
                    location: PartLocation::Object(part_prefix_of(&key)),
                })
                .collect())
        } else {
            Ok(list_part_metas(&self.catalog.parts_dir(table))?
                .into_iter()
                .map(|(dir, meta)| DataPart {
                    table: table.to_string(),
                    meta,
                    location: PartLocation::Local(dir),
                })
                .collect())
        }
    }

    /// All parts of a table, oldest first (synchronous convenience; safe to
    /// call from sync contexts — local tables never touch the io runtime).
    pub fn parts(&self, table: &str) -> Result<Vec<DataPart>> {
        let def = self.table(table)?;
        if def.engine.is_shared() {
            let this = self.self_arc()?;
            let table = table.to_string();
            self.block_on_owned(async move { this.parts_async(&table).await })
        } else {
            Ok(list_part_metas(&self.catalog.parts_dir(table))?
                .into_iter()
                .map(|(dir, meta)| DataPart {
                    table: table.to_string(),
                    meta,
                    location: PartLocation::Local(dir),
                })
                .collect())
        }
    }

    /// Read all rows of a part (async; used by merges and direct reads).
    pub async fn read_data_part_async(&self, part: &DataPart) -> Result<Vec<RecordBatch>> {
        match &part.location {
            PartLocation::Local(dir) => read_part(dir, None),
            PartLocation::Object(prefix) => {
                let shared = self
                    .shared
                    .as_ref()
                    .ok_or_else(|| LessError::Engine("shared store unavailable".into()))?;
                let bytes = shared.get(&format!("{prefix}/{DATA_FILE}")).await?;
                read_part_from_bytes(bytes, None)
            }
        }
    }

    /// Read all rows of a part (synchronous convenience; local parts never
    /// touch the io runtime).
    pub fn read_data_part(&self, part: &DataPart) -> Result<Vec<RecordBatch>> {
        match &part.location {
            PartLocation::Local(dir) => read_part(dir, None),
            PartLocation::Object(_) => {
                let this = self.self_arc()?;
                let part = part.clone();
                self.block_on_owned(async move { this.read_data_part_async(&part).await })
            }
        }
    }

    /// Resolve a part's data file to `(store_relative_key, size)` for the
    /// query planner. Local keys are relative to the engine data directory
    /// (`parts/<table>/<part>/data.parquet`); shared keys are relative to
    /// the shared store root (`tables/<table>/parts/<part>/data.parquet`).
    pub async fn part_file_async(&self, part: &DataPart) -> Result<(String, u64)> {
        match &part.location {
            PartLocation::Local(dir) => {
                let rel = dir.strip_prefix(&self.config.data_dir).map_err(|_| {
                    LessError::Engine(format!(
                        "part directory {} lies outside data dir {}",
                        dir.display(),
                        self.config.data_dir.display()
                    ))
                })?;
                let key = format!(
                    "{}/{DATA_FILE}",
                    rel.to_string_lossy().trim_start_matches('/')
                );
                let size = std::fs::metadata(dir.join(DATA_FILE))?.len();
                Ok((key, size))
            }
            PartLocation::Object(prefix) => {
                let shared = self
                    .shared
                    .as_ref()
                    .ok_or_else(|| LessError::Engine("shared store unavailable".into()))?;
                let key = format!("{prefix}/{DATA_FILE}");
                let size = shared.head_size(&key).await?;
                Ok((key, size))
            }
        }
    }

    /// Synchronous convenience wrapper for [`Self::part_file_async`] (local
    /// parts never touch the io runtime).
    pub fn part_file(&self, part: &DataPart) -> Result<(String, u64)> {
        match &part.location {
            PartLocation::Local(dir) => {
                let rel = dir.strip_prefix(&self.config.data_dir).map_err(|_| {
                    LessError::Engine(format!(
                        "part directory {} lies outside data dir {}",
                        dir.display(),
                        self.config.data_dir.display()
                    ))
                })?;
                let key = format!(
                    "{}/{DATA_FILE}",
                    rel.to_string_lossy().trim_start_matches('/')
                );
                let size = std::fs::metadata(dir.join(DATA_FILE))?.len();
                Ok((key, size))
            }
            PartLocation::Object(_) => {
                let this = self.self_arc()?;
                let part = part.clone();
                self.block_on_owned(async move { this.part_file_async(&part).await })
            }
        }
    }

    /// Cumulative statistics for a table.
    pub fn stats(&self, table: &str) -> Result<TableStats> {
        let def = self.table(table)?;
        let parts = self.parts(table)?;
        let mut rows = 0u64;
        let mut disk_bytes = 0u64;
        for p in &parts {
            rows += p.meta.row_count;
            let (_, size) = self.part_file(p)?;
            disk_bytes += size;
        }
        let buffered = self
            .buffers
            .lock()
            .unwrap()
            .get(table)
            .map(|b| b.rows)
            .unwrap_or(0);
        let _ = def;
        Ok(TableStats {
            table: table.to_string(),
            part_count: parts.len(),
            rows,
            buffered_rows: buffered,
            disk_bytes,
        })
    }
}

/// Strip the `meta.json` filename off an object key, leaving the part's
/// directory prefix.
fn part_prefix_of(key: &str) -> String {
    key.trim_end_matches(META_FILE)
        .trim_end_matches('/')
        .to_string()
}

/// Write a part locally, then upload its two files to the shared store.
pub(crate) async fn write_part_shared(
    shared: &SharedStore,
    table: &str,
    schema: &Arc<Schema>,
    batches: Vec<RecordBatch>,
    opts: &WriteOptions,
) -> Result<PartMeta> {
    let tmp = std::env::temp_dir().join(format!("less-part-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp)?;
    let meta = write_part(&tmp, table, schema, batches, opts)?;
    let part_dir = tmp.join(&meta.name);
    let prefix = format!("tables/{table}/parts/{}", meta.name);
    shared
        .put(
            &format!("{prefix}/{DATA_FILE}"),
            Bytes::from(std::fs::read(part_dir.join(DATA_FILE))?),
        )
        .await?;
    // Publish the part by CAS-creating its meta.json: readers only see
    // parts whose metadata exists, and a second writer publishing the same
    // part name loses loudly instead of silently clobbering (part names
    // are uuid-unique, so this is the multi-writer safety net).
    let meta_key = format!("{prefix}/{META_FILE}");
    let meta_bytes = Bytes::from(std::fs::read(part_dir.join(META_FILE))?);
    let published = shared
        .put_if_absent(&meta_key, meta_bytes)
        .await
        .map_err(|e| LessError::Engine(format!("part publication: {e}")))?;
    if !published {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(LessError::Engine(format!(
            "part publication conflict: {meta_key} already exists (concurrent writer); retry"
        )));
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(meta)
}

/// [`write_part_shared`] for already-sorted runs (chunked k-way merge).
pub(crate) async fn write_part_shared_sorted(
    shared: &SharedStore,
    table: &str,
    schema: &Arc<Schema>,
    runs: Vec<RecordBatch>,
    opts: &WriteOptions,
) -> Result<PartMeta> {
    let tmp = std::env::temp_dir().join(format!("less-part-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp)?;
    let meta = write_part_sorted(&tmp, table, schema, runs, opts)?;
    let part_dir = tmp.join(&meta.name);
    let prefix = format!("tables/{table}/parts/{}", meta.name);
    shared
        .put(
            &format!("{prefix}/{DATA_FILE}"),
            Bytes::from(std::fs::read(part_dir.join(DATA_FILE))?),
        )
        .await?;
    let meta_key = format!("{prefix}/{META_FILE}");
    let meta_bytes = Bytes::from(std::fs::read(part_dir.join(META_FILE))?);
    let published = shared
        .put_if_absent(&meta_key, meta_bytes)
        .await
        .map_err(|e| LessError::Engine(format!("part publication: {e}")))?;
    if !published {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(LessError::Engine(format!(
            "part publication conflict: {meta_key} already exists (concurrent writer); retry"
        )));
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(meta)
}

/// Align a batch to the table schema.
///
/// Exact schema match is accepted as-is; otherwise columns may be reordered
/// to match, but names and types must agree exactly (strict typing keeps
/// inserts predictable — cast support is on the roadmap).
pub fn align_batch(table_schema: &Schema, batch: RecordBatch) -> Result<RecordBatch> {
    let bs = batch.schema();
    if bs.fields() == table_schema.fields() {
        return Ok(batch);
    }
    if bs.fields().len() != table_schema.fields().len() {
        return Err(LessError::Engine(format!(
            "insert schema mismatch: table has {} columns, batch has {}",
            table_schema.fields().len(),
            bs.fields().len()
        )));
    }
    let mut indices = Vec::with_capacity(table_schema.fields().len());
    for f in table_schema.fields() {
        match bs.index_of(f.name()) {
            Ok(i) => {
                if bs.field(i).data_type() != f.data_type() {
                    return Err(LessError::Engine(format!(
                        "insert type mismatch for column '{}': table {:?}, batch {:?}",
                        f.name(),
                        f.data_type(),
                        bs.field(i).data_type()
                    )));
                }
                indices.push(i);
            }
            Err(_) => {
                return Err(LessError::Engine(format!(
                    "insert missing column '{}'",
                    f.name()
                )));
            }
        }
    }
    Ok(batch.project(&indices)?)
}

/// Project `batch` (written against `old`) onto `new`, adding the new
/// column with a type-appropriate default when `ty` is Some, or dropping
/// the column when `ty` is None.
fn alter_batch(
    old: &Arc<Schema>,
    new: &Arc<Schema>,
    batch: &RecordBatch,
    ty: &Option<TypeSpec>,
) -> Result<RecordBatch> {
    let mut cols: Vec<Arc<dyn arrow::array::Array>> = Vec::new();
    for f in new.fields() {
        if let Ok(i) = old.index_of(f.name().as_str()) {
            cols.push(batch.column(i).clone());
        } else if let Some(t) = ty {
            cols.push(default_array(t, batch.num_rows())?);
        }
    }
    Ok(RecordBatch::try_new(new.clone(), cols)?)
}

/// A default-filled array for a freshly added column.
fn default_array(ty: &TypeSpec, len: usize) -> Result<Arc<dyn arrow::array::Array>> {
    use arrow::array::{
        BooleanArray, Date32Array, Float64Array, Int8Array, Int16Array, Int32Array, Int64Array,
        StringArray, TimestampMillisecondArray, UInt8Array, UInt16Array, UInt32Array, UInt64Array,
    };
    Ok(match ty {
        TypeSpec::Int8 => Arc::new(Int8Array::from(vec![0i8; len])),
        TypeSpec::Int16 => Arc::new(Int16Array::from(vec![0i16; len])),
        TypeSpec::Int32 => Arc::new(Int32Array::from(vec![0i32; len])),
        TypeSpec::Int64 => Arc::new(Int64Array::from(vec![0i64; len])),
        TypeSpec::UInt8 => Arc::new(UInt8Array::from(vec![0u8; len])),
        TypeSpec::UInt16 => Arc::new(UInt16Array::from(vec![0u16; len])),
        TypeSpec::UInt32 => Arc::new(UInt32Array::from(vec![0u32; len])),
        TypeSpec::UInt64 => Arc::new(UInt64Array::from(vec![0u64; len])),
        TypeSpec::Float32 => Arc::new(arrow::array::Float32Array::from(vec![0f32; len])),
        TypeSpec::Float64 => Arc::new(Float64Array::from(vec![0f64; len])),
        TypeSpec::Bool => Arc::new(BooleanArray::from(vec![false; len])),
        TypeSpec::Utf8 => Arc::new(StringArray::from(vec![""; len])),
        TypeSpec::Date32 => Arc::new(Date32Array::from(vec![0i32; len])),
        TypeSpec::TimestampMs => Arc::new(TimestampMillisecondArray::from(vec![0i64; len])),
        other => {
            return Err(LessError::Engine(format!(
                "ALTER ADD COLUMN: no default for type {other:?} yet — use Int64/Float64/Utf8/…"
            )));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Float64Array, Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, SchemaRef};
    use less_catalog::{EngineKind, FieldSpec, SchemaSpec, TypeSpec};

    fn test_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("city", DataType::Utf8, false),
            Field::new("amount", DataType::Float64, false),
        ]))
    }

    fn batch(ids: Vec<i64>, cities: Vec<&str>, amounts: Vec<f64>) -> RecordBatch {
        RecordBatch::try_new(
            test_schema(),
            vec![
                Arc::new(Int64Array::from(ids)),
                Arc::new(StringArray::from(cities)),
                Arc::new(Float64Array::from(amounts)),
            ],
        )
        .unwrap()
    }

    /// First-column values of every row in a table, sorted.
    fn read_ids_of(engine: &LessEngine, table: &str) -> Vec<i64> {
        let parts = engine.parts(table).unwrap();
        let mut ids = vec![];
        for part in &parts {
            for r in engine.read_data_part(part).unwrap() {
                let col = r.column(0).as_any().downcast_ref::<Int64Array>().unwrap();
                ids.extend((0..r.num_rows()).map(|i| col.value(i)));
            }
        }
        ids.sort_unstable();
        ids
    }

    fn def(name: &str, engine: EngineKind) -> TableDef {
        let mut d = TableDef::new(
            name,
            SchemaSpec {
                fields: vec![
                    FieldSpec::new("id", TypeSpec::Int64),
                    FieldSpec::new("city", TypeSpec::Utf8),
                    FieldSpec::new("amount", TypeSpec::Float64),
                ],
            },
            engine,
        );
        d.sort_key = vec!["city".into(), "id".into()];
        d.unique = vec!["city".into()];
        d
    }

    fn temp_engine() -> (Arc<LessEngine>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("less-engine-{}", uuid::Uuid::new_v4()));
        (
            LessEngine::open(EngineConfig::with_data_dir(&dir)).unwrap(),
            dir,
        )
    }

    #[test]
    fn insert_flush_read_dedup() {
        let (engine, dir) = temp_engine();
        engine
            .create_table(def("events", EngineKind::Firefly))
            .unwrap();

        // First insert: two rows for "berlin" (amount 10 then 20).
        engine
            .insert(
                "events",
                batch(
                    vec![1, 2, 3],
                    vec!["berlin", "paris", "berlin"],
                    vec![10.0, 5.0, 20.0],
                ),
            )
            .unwrap();
        engine.flush("events").unwrap();

        // Second insert: another "berlin" (amount 30) — should win at merge.
        engine
            .insert("events", batch(vec![4], vec!["berlin"], vec![30.0]))
            .unwrap();
        engine.flush("events").unwrap();

        let parts = engine.parts("events").unwrap();
        assert_eq!(parts.len(), 2);

        engine.optimize("events").unwrap();
        let parts = engine.parts("events").unwrap();
        assert_eq!(parts.len(), 1);
        // berlin appears in both parts; the merge keeps the last one, so
        // the table ends with berlin (amount 30) and paris.
        assert_eq!(parts[0].meta.row_count, 2);
        assert!(parts[0].level() >= 1);

        let rows = engine.read_data_part(&parts[0]).unwrap();
        let total: usize = rows.iter().map(|r| r.num_rows()).sum();
        assert_eq!(total, 2);

        let stats = engine.stats("events").unwrap();
        assert_eq!(stats.rows, 2);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn shared_engine_roundtrip() {
        let (engine, dir) = temp_engine();
        engine
            .create_table(def("events_shared", EngineKind::FireflyCloud))
            .unwrap();
        engine
            .insert(
                "events_shared",
                batch(vec![1, 2], vec!["berlin", "paris"], vec![10.0, 5.0]),
            )
            .unwrap();
        engine.flush("events_shared").unwrap();

        let parts = engine.parts("events_shared").unwrap();
        assert_eq!(parts.len(), 1);
        assert!(matches!(parts[0].location, PartLocation::Object(_)));
        let (key, size) = engine.part_file(&parts[0]).unwrap();
        assert!(key.starts_with("tables/events_shared/parts/"));
        assert!(key.ends_with("data.parquet"));
        assert!(size > 0);

        let rows = engine.read_data_part(&parts[0]).unwrap();
        assert_eq!(rows.iter().map(|r| r.num_rows()).sum::<usize>(), 2);

        engine.drop_table("events_shared").unwrap();
        assert!(engine.table("events_shared").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn align_batch_reorders_and_validates() {
        let schema = test_schema();
        let shuffled = Schema::new(vec![
            Field::new("amount", DataType::Float64, false),
            Field::new("id", DataType::Int64, false),
            Field::new("city", DataType::Utf8, false),
        ]);
        let b = RecordBatch::try_new(
            Arc::new(shuffled),
            vec![
                Arc::new(Float64Array::from(vec![9.5])),
                Arc::new(Int64Array::from(vec![7])),
                Arc::new(StringArray::from(vec!["tokyo"])),
            ],
        )
        .unwrap();
        let aligned = align_batch(&schema, b).unwrap();
        assert_eq!(aligned.schema().fields(), schema.fields());
        let ids = aligned
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!(ids.value(0), 7);

        // Wrong type must fail (amount is Int64 here, table wants Float64):
        let bad_schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("city", DataType::Utf8, false),
            Field::new("amount", DataType::Int64, false),
        ]));
        let bad = RecordBatch::try_new(
            bad_schema,
            vec![
                Arc::new(Int64Array::from(vec![1])),
                Arc::new(StringArray::from(vec!["x"])),
                Arc::new(Int64Array::from(vec![2])),
            ],
        )
        .unwrap();
        assert!(align_batch(&schema, bad).is_err());
    }

    #[test]
    fn compute_storage_separation_across_nodes() {
        // One shared storage root, two independent compute nodes with
        // separate local directories: whatever node A writes to shared
        // storage, node B must see — and vice versa.
        let base = std::env::temp_dir().join(format!("less-cloud-{}", uuid::Uuid::new_v4()));
        let shared_root = base.join("shared-root");
        let node_a_dir = base.join("node-a");
        let node_b_dir = base.join("node-b");
        let shared_url = format!("file://{}", shared_root.display());

        // ---- node A: create, insert, flush ------------------------------
        let a = LessEngine::open(EngineConfig::with_shared_url(&node_a_dir, &shared_url)).unwrap();
        a.create_table(def("cloud_events", EngineKind::FireflyCloud))
            .unwrap();
        a.insert(
            "cloud_events",
            batch(vec![1, 2], vec!["berlin", "paris"], vec![10.0, 5.0]),
        )
        .unwrap();
        a.flush("cloud_events").unwrap();

        // ---- node B: fresh compute node, no local knowledge of A --------
        let b = LessEngine::open(EngineConfig::with_shared_url(&node_b_dir, &shared_url)).unwrap();
        assert_eq!(b.tables().unwrap(), vec!["cloud_events".to_string()]);
        let parts = b.parts("cloud_events").unwrap();
        assert_eq!(parts.len(), 1);
        assert!(matches!(parts[0].location, PartLocation::Object(_)));
        let rows = b.read_data_part(&parts[0]).unwrap();
        assert_eq!(rows.iter().map(|r| r.num_rows()).sum::<usize>(), 2);

        // Node A's local disk holds no durable state for the shared table.
        assert!(!node_a_dir.join("parts/cloud_events").exists());

        // ---- drop from node B is visible to node A ----------------------
        b.drop_table("cloud_events").unwrap();
        assert!(a.tables().unwrap().is_empty());
        // No objects remain under the table prefix (S3-style storage has
        // no directories; an empty leftover dir on the file backend is a
        // harmless artifact).
        let store = b.shared().unwrap().clone();
        let keys = b
            .runtime()
            .block_on(store.list_keys("tables/cloud_events/"))
            .unwrap();
        assert!(keys.is_empty(), "part objects should be gone: {keys:?}");
        assert!(
            !b.runtime()
                .block_on(store.exists("catalog/cloud_events.json"))
                .unwrap()
        );
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn block_cache_serves_repeated_shared_part_reads() {
        let base = std::env::temp_dir().join(format!("less-blkcache-{}", uuid::Uuid::new_v4()));
        let shared_root = base.join("shared-root");
        let node_dir = base.join("node");
        let shared_url = format!("file://{}", shared_root.display());
        let mut config = EngineConfig::with_shared_url(&node_dir, &shared_url);
        config.block_cache_bytes = 1 << 20;

        let engine = LessEngine::open(config).unwrap();
        engine
            .create_table(def("cached_t", EngineKind::FireflyCloud))
            .unwrap();
        engine
            .insert(
                "cached_t",
                batch(vec![1, 2], vec!["berlin", "paris"], vec![10.0, 5.0]),
            )
            .unwrap();
        engine.flush("cached_t").unwrap();
        let parts = engine.parts("cached_t").unwrap();

        for _ in 0..3 {
            let rows = engine.read_data_part(&parts[0]).unwrap();
            assert_eq!(rows.iter().map(|r| r.num_rows()).sum::<usize>(), 2);
        }
        let stats = engine.cache_stats().expect("cache configured");
        assert!(
            stats.hits >= 2 && stats.entries >= 1,
            "repeated shared part reads must be served from the block cache: {stats:?}"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    /// Two compute nodes on one shared root, writing concurrently: flushes
    /// never clobber each other (CAS part publication), and concurrent
    /// OPTIMIZE claims prevent double-merging — every row appears exactly
    /// once at the end.
    #[test]
    fn multi_writer_concurrent_flush_and_optimize() {
        let base = std::env::temp_dir().join(format!("less-mw-{}", uuid::Uuid::new_v4()));
        let shared_root = base.join("shared");
        let url = format!("file://{}", shared_root.display());
        let a = LessEngine::open(EngineConfig::with_shared_url(base.join("node-a"), &url)).unwrap();
        a.create_table(def("mw_t", EngineKind::FireflyCloud))
            .unwrap();
        let b = LessEngine::open(EngineConfig::with_shared_url(base.join("node-b"), &url)).unwrap();

        // Concurrent inserts + flushes from both nodes.
        std::thread::scope(|s| {
            s.spawn(|| {
                for i in 0..3 {
                    let name = format!("a{i}");
                    a.insert("mw_t", batch(vec![i], vec![name.as_str()], vec![i as f64]))
                        .unwrap();
                    a.flush("mw_t").unwrap();
                }
            });
            s.spawn(|| {
                for i in 10..13 {
                    let name = format!("b{i}");
                    b.insert("mw_t", batch(vec![i], vec![name.as_str()], vec![i as f64]))
                        .unwrap();
                    b.flush("mw_t").unwrap();
                }
            });
        });

        // Every node sees every part; all 6 rows are present.
        let parts = a.parts("mw_t").unwrap();
        assert_eq!(parts.len(), 6, "six flushes → six parts: {parts:?}");
        let read_ids = |engine: &LessEngine, parts: &[crate::part::DataPart]| {
            use arrow::array::Int64Array;
            let mut ids = vec![];
            for part in parts {
                for r in engine.read_data_part(part).unwrap() {
                    let col = r.column(0).as_any().downcast_ref::<Int64Array>().unwrap();
                    ids.extend((0..r.num_rows()).map(|i| col.value(i)));
                }
            }
            ids
        };
        let ids = read_ids(&a, &parts);
        assert_eq!(ids.len(), 6);
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 6, "no lost or duplicated rows: {ids:?}");

        // Concurrent OPTIMIZE: claims mean no input part is merged twice.
        std::thread::scope(|s| {
            s.spawn(|| {
                let _ = a.optimize("mw_t");
            });
            s.spawn(|| {
                let _ = b.optimize("mw_t");
            });
        });

        let parts = a.parts("mw_t").unwrap();
        assert!(
            (1..=2).contains(&parts.len()),
            "merged parts expected: {parts:?}"
        );
        let ids = read_ids(&a, &parts);
        assert_eq!(ids.len(), 6);
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 6, "no duplicated rows after merge: {ids:?}");
        std::fs::remove_dir_all(&base).ok();
    }

    /// Size-tiered merge policy: similar-sized parts merge together; a big
    /// part is not dragged into small-part merges. `optimize` now converges
    /// over bounded passes in a single call: the two small parts (tier 3)
    /// merge first, then the resulting part merges with the big one (no
    /// tier holds two parts → the two smallest overall), ending at one part.
    #[test]
    fn size_tiered_merge_selects_similar_sized_parts() {
        let (engine, dir) = temp_engine();
        engine
            .create_table(def("tier_t", EngineKind::Firefly))
            .unwrap();
        // Three parts: two small (tier 3), one big (tier 9).
        for round in 0..3 {
            let ids: Vec<i64> = if round < 2 {
                (0..10).map(|k| round * 10 + k).collect()
            } else {
                (1000..2000).collect()
            };
            let cities: Vec<String> = ids.iter().map(|i| format!("c{i}")).collect();
            let cities_ref: Vec<&str> = cities.iter().map(String::as_str).collect();
            let amounts: Vec<f64> = ids.iter().map(|i| *i as f64).collect();
            engine
                .insert("tier_t", batch(ids, cities_ref, amounts))
                .unwrap();
            engine.flush("tier_t").unwrap();
        }
        assert_eq!(engine.parts("tier_t").unwrap().len(), 3);

        // One optimize converges: the last merge combines everything.
        let merged = engine.optimize("tier_t").unwrap().unwrap();
        assert_eq!(merged.row_count, 1020);
        assert_eq!(engine.parts("tier_t").unwrap().len(), 1);
        assert_eq!(engine.stats("tier_t").unwrap().rows, 1020);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// DELETE covers buffered rows too (flush-first), survives reopen, and
    /// works on FireflyCloud tables with cross-node visibility.
    #[test]
    fn delete_and_update_mutations() {
        // ---- local table: buffered rows + reopen persistence ------------
        let (engine, dir) = temp_engine();
        engine
            .create_table(def("mut_t", EngineKind::Firefly))
            .unwrap();
        engine
            .insert(
                "mut_t",
                batch(
                    vec![1, 2, 3, 4],
                    vec!["a", "b", "c", "d"],
                    vec![1.0, 2.0, 3.0, 4.0],
                ),
            )
            .unwrap();
        // Rows are still buffered (no flush yet): the delete must apply to
        // them too.
        let deleted = engine
            .delete_where("mut_t", |b| {
                Ok(BooleanArray::from(
                    (0..b.num_rows())
                        .map(|r| {
                            b.column(0)
                                .as_any()
                                .downcast_ref::<arrow::array::Int64Array>()
                                .unwrap()
                                .value(r)
                                % 2
                                == 0
                        })
                        .collect::<Vec<_>>(),
                ))
            })
            .unwrap();
        assert_eq!(deleted, 2);
        let rows = read_ids_of(engine.as_ref(), "mut_t");
        assert_eq!(rows, vec![1, 3]);

        // UPDATE on the same table.
        let updated = engine
            .update_where("mut_t", |b| {
                let mask = BooleanArray::from(vec![true; b.num_rows()]);
                let mut cols = b.columns().to_vec();
                let v = b
                    .column(2)
                    .as_any()
                    .downcast_ref::<arrow::array::Float64Array>()
                    .unwrap();
                let doubled = arrow::array::Float64Array::from(
                    (0..b.num_rows())
                        .map(|r| v.value(r) * 2.0)
                        .collect::<Vec<_>>(),
                );
                cols[2] = Arc::new(doubled);
                let new_batch = RecordBatch::try_new(b.schema(), cols).unwrap();
                Ok(Some((new_batch, mask.true_count() as u64)))
            })
            .unwrap();
        assert_eq!(updated, 2);

        // Reopen: deletions/updates are durable (wal_lsn_max carried over).
        drop(engine);
        let engine = LessEngine::open(EngineConfig::with_data_dir(&dir)).unwrap();
        let rows = read_ids_of(engine.as_ref(), "mut_t");
        assert_eq!(rows, vec![1, 3]);
        std::fs::remove_dir_all(&dir).ok();

        // ---- shared table: delete + cross-node visibility --------------
        let base = std::env::temp_dir().join(format!("less-mut-{}", uuid::Uuid::new_v4()));
        let url = format!("file://{}", base.join("shared").display());
        let a = LessEngine::open(EngineConfig::with_shared_url(base.join("node-a"), &url)).unwrap();
        a.create_table(def("mut_s", EngineKind::FireflyCloud))
            .unwrap();
        a.insert(
            "mut_s",
            batch(vec![1, 2, 3], vec!["x", "y", "z"], vec![1.0, 2.0, 3.0]),
        )
        .unwrap();
        a.flush("mut_s").unwrap();
        let b = LessEngine::open(EngineConfig::with_shared_url(base.join("node-b"), &url)).unwrap();
        assert_eq!(b.parts("mut_s").unwrap().len(), 1);

        let deleted = b
            .delete_where("mut_s", |batch| {
                Ok(BooleanArray::from(
                    (0..batch.num_rows())
                        .map(|r| {
                            batch
                                .column(0)
                                .as_any()
                                .downcast_ref::<arrow::array::Int64Array>()
                                .unwrap()
                                .value(r)
                                == 2
                        })
                        .collect::<Vec<_>>(),
                ))
            })
            .unwrap();
        assert_eq!(deleted, 1);
        // Node A sees the deletion (new part published, old part removed).
        let ids = read_ids_of(a.as_ref(), "mut_s");
        assert_eq!(ids, vec![1, 3]);
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn wal_recovers_unflushed_inserts() {
        let (engine, dir) = temp_engine();
        engine
            .create_table(def("wal_t", EngineKind::Firefly))
            .unwrap();
        engine
            .insert(
                "wal_t",
                batch(vec![1, 2, 3], vec!["a", "b", "c"], vec![1.0, 2.0, 3.0]),
            )
            .unwrap();
        // "Crash": drop the engine without flushing.
        drop(engine);

        let engine = LessEngine::open(EngineConfig::with_data_dir(&dir)).unwrap();
        let stats = engine.stats("wal_t").unwrap();
        assert_eq!(stats.rows, 3, "WAL replay must recover unflushed rows");
        let parts = engine.parts("wal_t").unwrap();
        assert_eq!(parts.len(), 1, "recovered rows should be flushed on open");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wal_no_duplicates_after_flush() {
        let (engine, dir) = temp_engine();
        engine
            .create_table(def("wal_f", EngineKind::Firefly))
            .unwrap();
        engine
            .insert("wal_f", batch(vec![1, 2], vec!["a", "b"], vec![1.0, 2.0]))
            .unwrap();
        engine.flush("wal_f").unwrap();
        drop(engine);

        let engine = LessEngine::open(EngineConfig::with_data_dir(&dir)).unwrap();
        assert_eq!(engine.stats("wal_f").unwrap().rows, 2);
        let engine = engine;
        let engine = engine;
        drop(engine);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wal_replay_skips_records_covered_by_parts() {
        let (engine, dir) = temp_engine();
        engine
            .create_table(def("wal_s", EngineKind::Firefly))
            .unwrap();
        engine
            .insert("wal_s", batch(vec![1, 2], vec!["a", "b"], vec![1.0, 2.0]))
            .unwrap();
        engine.flush("wal_s").unwrap();
        let part_lsn = engine.parts("wal_s").unwrap()[0].meta.wal_lsn_max.unwrap();

        // Simulate a crash between part-durable and WAL-truncate: re-append
        // a stale record (lsn <= part_lsn) plus a fresh one.
        let wal = crate::wal::Wal::open(&dir.join("wal"), false).unwrap();
        wal.append("wal_s", part_lsn, &batch(vec![9], vec!["stale"], vec![9.0]))
            .unwrap();
        wal.append(
            "wal_s",
            part_lsn + 1,
            &batch(vec![3], vec!["fresh"], vec![3.0]),
        )
        .unwrap();
        drop(engine);

        let engine = LessEngine::open(EngineConfig::with_data_dir(&dir)).unwrap();
        // stale record is covered by the part (skipped); fresh record replays.
        assert_eq!(engine.stats("wal_s").unwrap().rows, 3);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn buffered_rows_are_flushed_automatically() {
        let (engine, dir) = temp_engine();
        let mut d = def("auto", EngineKind::Firefly);
        d.sort_key = vec![];
        d.unique = vec![];
        engine.create_table(d).unwrap();

        let mut config = engine.config.clone();
        config.flush_rows = 10;
        let engine = LessEngine::open(config).unwrap();

        for _ in 0..5 {
            engine
                .insert(
                    "auto",
                    batch(vec![1, 2, 3], vec!["a", "b", "c"], vec![1.0, 2.0, 3.0]),
                )
                .unwrap();
        }
        // 15 rows > 10 -> flushed
        let stats = engine.stats("auto").unwrap();
        assert!(stats.rows > 0);
        std::fs::remove_dir_all(&dir).ok();
    }
}
