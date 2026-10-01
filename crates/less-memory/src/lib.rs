//! LessMemory — a strong in-memory storage tier with SQL access.
//!
//! RAM-resident tables with Arrow-native storage, a hash index on the
//! primary key for O(1) point lookups, snapshot persistence, and full SQL
//! (DataFusion) over the in-memory tables. Together with `less-graph` this
//! is the context/memory tier that agents use instead of Obsidian vaults
//! or a standalone graph database.
//!
//! Semantics (v1):
//! * `insert` appends batches and indexes the primary key — duplicates are
//!   visible to SQL (append log); `compact` deduplicates by primary key
//!   keeping the last row, like the engine's merge;
//! * `point_get` always returns the *latest* row for a key (index points
//!   at the newest occurrence);
//! * tables persist to `<dir>/tables/<name>.json` (JSON array) after every
//!   mutation; `open` reloads them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use arrow::datatypes::{Schema, SchemaRef};
use arrow::record_batch::RecordBatch;
use arrow::row::{RowConverter, SortField};
use datafusion::datasource::MemTable;
use datafusion::execution::context::{SessionConfig, SessionContext};
use serde::{Deserialize, Serialize};

use less_catalog::FieldSpec;
use less_common::{LessError, Result};

/// One in-memory table.
pub struct MemoryTable {
    pub name: String,
    pub schema: SchemaRef,
    pub pk: Option<String>,
    batches: Vec<RecordBatch>,
    /// Primary-key value (normalized bytes) -> (batch idx, row idx) of the
    /// latest occurrence.
    index: HashMap<Box<[u8]>, (usize, usize)>,
}

impl MemoryTable {
    pub fn new(name: &str, schema: SchemaRef, pk: Option<String>) -> Result<Self> {
        if let Some(pk) = &pk
            && schema.field_with_name(pk).is_err()
        {
            return Err(LessError::Config(format!(
                "primary key column '{pk}' not in schema"
            )));
        }
        Ok(Self {
            name: name.to_string(),
            schema,
            pk,
            batches: vec![],
            index: HashMap::new(),
        })
    }

    pub fn rows(&self) -> usize {
        self.batches.iter().map(|b| b.num_rows()).sum()
    }

    /// Append a batch (validated against the schema) and index its rows.
    pub fn insert(&mut self, batch: RecordBatch) -> Result<usize> {
        let aligned = less_engine::align_batch(&self.schema, batch)?;
        let n = aligned.num_rows();
        let batch_idx = self.batches.len();
        if let Some(pk) = &self.pk {
            let idx = self
                .schema
                .index_of(pk)
                .map_err(|_| LessError::Config(format!("pk column '{pk}' missing")))?;
            let col = aligned.column(idx);
            let converter = RowConverter::new(vec![SortField::new(col.data_type().clone())])?;
            let rows = converter.convert_columns(std::slice::from_ref(col))?;
            for r in 0..n {
                let key: Box<[u8]> = rows.row(r).as_ref().into();
                self.index.insert(key, (batch_idx, r));
            }
        }
        self.batches.push(aligned);
        Ok(n)
    }

    /// Latest row for a primary key, as JSON.
    pub fn point_get(&self, key: &serde_json::Value) -> Result<Option<serde_json::Value>> {
        let pk = self.pk.as_ref().ok_or_else(|| {
            LessError::Config(format!("table '{}' has no primary key", self.name))
        })?;
        let idx = self
            .schema
            .index_of(pk)
            .map_err(|_| LessError::Config(format!("pk column '{pk}' missing")))?;
        let col_idx = idx;
        let bytes = scalar_to_row_bytes(key, self.schema.field(col_idx).data_type())?;
        let Some((bi, ri)) = self.index.get(bytes.as_slice()) else {
            return Ok(None);
        };
        let batch = &self.batches[*bi];
        Ok(Some(record_batch_row_to_json(batch, *ri, &self.schema)?))
    }

    /// Deduplicate by primary key keeping the last row per key.
    pub fn compact(&mut self) -> Result<usize> {
        let Some(pk) = &self.pk else {
            return Ok(0);
        };
        let before: usize = self.rows();
        let all: Vec<RecordBatch> = std::mem::take(&mut self.batches);
        if all.is_empty() {
            return Ok(0);
        }
        let schema = self.schema.clone();
        let idx = schema
            .index_of(pk)
            .map_err(|_| LessError::Config(format!("pk column '{pk}' missing")))?;
        // Concatenate, then keep-last per pk using the normalized rows.
        let combined = arrow::compute::concat_batches(&schema, &all)?;
        let col = combined.column(idx);
        let converter = RowConverter::new(vec![SortField::new(col.data_type().clone())])?;
        let rows = converter.convert_columns(std::slice::from_ref(col))?;
        let n = combined.num_rows();
        let mut keep = vec![false; n];
        let mut i = 0usize;
        while i < n {
            let mut j = i + 1;
            while j < n && rows.row(j).as_ref() == rows.row(i).as_ref() {
                j += 1;
            }
            keep[j - 1] = true;
            i = j;
        }
        let filtered = arrow::compute::filter_record_batch(
            &combined,
            &arrow::array::BooleanArray::from(keep),
        )?;
        self.batches = vec![filtered];
        self.index.clear();
        // Reindex the compacted batch.
        let col = self.batches[0].column(idx);
        let rows = converter.convert_columns(std::slice::from_ref(col))?;
        for r in 0..self.batches[0].num_rows() {
            let key: Box<[u8]> = rows.row(r).as_ref().into();
            self.index.insert(key, (0, r));
        }
        Ok(before - self.rows())
    }

    /// All rows (append order) as a DataFusion MemTable.
    pub fn to_memtable(&self) -> Result<MemTable> {
        MemTable::try_new(self.schema.clone(), vec![self.batches.clone()])
            .map_err(|e| LessError::Query(e.to_string()))
    }

    /// Serialize to a JSON array of row objects.
    pub fn to_json(&self) -> Result<serde_json::Value> {
        let mut rows = vec![];
        for b in &self.batches {
            for r in 0..b.num_rows() {
                rows.push(record_batch_row_to_json(b, r, &self.schema)?);
            }
        }
        Ok(serde_json::Value::Array(rows))
    }
}

fn scalar_to_row_bytes(
    value: &serde_json::Value,
    ty: &arrow::datatypes::DataType,
) -> Result<Vec<u8>> {
    use arrow::array::*;
    use arrow::datatypes::DataType as T;
    let arr: ArrayRef = match ty {
        T::Int64 => Arc::new(Int64Array::from(vec![
            value
                .as_i64()
                .or_else(|| value.as_str().and_then(|s| s.parse().ok())),
        ])),
        T::Int32 => Arc::new(Int32Array::from(vec![value.as_i64().map(|v| v as i32)])),
        T::Utf8 => Arc::new(StringArray::from(vec![
            value.as_str().map(|s| s.to_string()),
        ])),
        T::Float64 => Arc::new(Float64Array::from(vec![
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|s| s.parse().ok())),
        ])),
        T::Boolean => Arc::new(BooleanArray::from(vec![value.as_bool()])),
        other => {
            return Err(LessError::Config(format!(
                "unsupported primary key type {other:?}"
            )));
        }
    };
    let converter = RowConverter::new(vec![SortField::new(ty.clone())])?;
    let rows = converter.convert_columns(&[arr])?;
    Ok(rows.row(0).as_ref().to_vec())
}

fn record_batch_row_to_json(
    batch: &RecordBatch,
    row: usize,
    schema: &Schema,
) -> Result<serde_json::Value> {
    use arrow::array::*;
    use arrow::datatypes::DataType as T;
    use serde_json::{Value, json};
    let mut map = serde_json::Map::new();
    for (ci, field) in schema.fields().iter().enumerate() {
        let col = batch.column(ci);
        let value: Value = if col.is_null(row) {
            Value::Null
        } else {
            match field.data_type() {
                T::Int64 => json!(
                    col.as_any()
                        .downcast_ref::<Int64Array>()
                        .unwrap()
                        .value(row)
                ),
                T::Int32 => json!(
                    col.as_any()
                        .downcast_ref::<Int32Array>()
                        .unwrap()
                        .value(row)
                ),
                T::Float64 => json!(
                    col.as_any()
                        .downcast_ref::<Float64Array>()
                        .unwrap()
                        .value(row)
                ),
                T::Float32 => json!(
                    col.as_any()
                        .downcast_ref::<Float32Array>()
                        .unwrap()
                        .value(row)
                ),
                T::Boolean => json!(
                    col.as_any()
                        .downcast_ref::<BooleanArray>()
                        .unwrap()
                        .value(row)
                ),
                T::Utf8 => json!(
                    col.as_any()
                        .downcast_ref::<StringArray>()
                        .unwrap()
                        .value(row)
                ),
                T::LargeUtf8 => json!(
                    col.as_any()
                        .downcast_ref::<LargeStringArray>()
                        .unwrap()
                        .value(row)
                ),
                T::Utf8View => json!(
                    col.as_any()
                        .downcast_ref::<StringViewArray>()
                        .unwrap()
                        .value(row)
                ),
                other => Value::String(format!("<unsupported:{other:?}>")),
            }
        };
        map.insert(field.name().to_string(), value);
    }
    Ok(Value::Object(map))
}

/// Serializable manifest of a memory table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableManifest {
    pub name: String,
    pub fields: Vec<FieldSpec>,
    pub pk: Option<String>,
}

/// A collection of in-memory tables with SQL access and persistence.
pub struct MemoryStore {
    tables: Mutex<HashMap<String, MemoryTable>>,
    ctx: SessionContext,
    dir: Option<PathBuf>,
    runtime: Option<Arc<tokio::runtime::Runtime>>,
}

impl Drop for MemoryStore {
    fn drop(&mut self) {
        if let Some(rt) = self.runtime.take() {
            std::thread::spawn(move || drop(rt));
        }
    }
}

impl MemoryStore {
    /// Open a store, loading persisted tables from `dir` if present.
    pub fn open(dir: Option<&Path>) -> Result<Self> {
        let ctx =
            SessionContext::new_with_config(SessionConfig::new().with_information_schema(true));
        let mut store = Self {
            tables: Mutex::new(HashMap::new()),
            ctx,
            dir: dir.map(|p| p.to_path_buf()),
            runtime: Some(Arc::new(
                tokio::runtime::Builder::new_current_thread()
                    .thread_name("less-memory")
                    .enable_all()
                    .build()
                    .map_err(|e| LessError::Engine(format!("tokio: {e}")))?,
            )),
        };
        store.load()?;
        Ok(store)
    }

    fn load(&mut self) -> Result<()> {
        let Some(dir) = &self.dir else { return Ok(()) };
        let tables_dir = dir.join("tables");
        if !tables_dir.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(&tables_dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "json") {
                let bytes = std::fs::read(&path)?;
                let manifest: TableManifest = serde_json::from_slice(&bytes)?;
                let schema = less_catalog::SchemaSpec {
                    fields: manifest.fields,
                }
                .to_arrow();
                let mut table = MemoryTable::new(&manifest.name, schema, manifest.pk)?;
                let rows_path = path.with_extension("rows.jsonl");
                if rows_path.exists() {
                    let json = std::fs::read_to_string(&rows_path)?;
                    let batches = json_to_batches(table.schema.clone(), &json)?;
                    for b in batches {
                        table.insert(b)?;
                    }
                }
                self.tables
                    .lock()
                    .unwrap()
                    .insert(manifest.name.clone(), table);
            }
        }
        Ok(())
    }

    fn persist(&self, name: &str) -> Result<()> {
        let Some(dir) = &self.dir else { return Ok(()) };
        let tables_dir = dir.join("tables");
        std::fs::create_dir_all(&tables_dir)?;
        let tables = self.tables.lock().unwrap();
        let table = tables
            .get(name)
            .ok_or_else(|| LessError::Catalog(format!("table '{name}' does not exist")))?;
        let fields: Vec<FieldSpec> = table
            .schema
            .fields()
            .iter()
            .map(|f| {
                Ok(FieldSpec {
                    name: f.name().to_string(),
                    ty: type_spec_from_arrow(f.data_type())?,
                    nullable: f.is_nullable(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let manifest = TableManifest {
            name: name.to_string(),
            fields,
            pk: table.pk.clone(),
        };
        let tmp = tables_dir.join(format!("{name}.json.tmp"));
        std::fs::write(&tmp, serde_json::to_string_pretty(&manifest)?)?;
        std::fs::rename(&tmp, tables_dir.join(format!("{name}.json")))?;
        let rows = table.to_json()?;
        let mut ndjson = String::new();
        if let serde_json::Value::Array(rows) = rows {
            for r in rows {
                ndjson.push_str(&r.to_string());
                ndjson.push('\n');
            }
        }
        let tmp = tables_dir.join(format!("{name}.rows.jsonl.tmp"));
        std::fs::write(&tmp, ndjson)?;
        std::fs::rename(&tmp, tables_dir.join(format!("{name}.rows.jsonl")))?;
        Ok(())
    }

    /// Create a table from field specs.
    pub fn create_table(
        &self,
        name: &str,
        fields: Vec<FieldSpec>,
        pk: Option<String>,
    ) -> Result<()> {
        if !less_catalog::schema::is_identifier(name) {
            return Err(LessError::Catalog(format!("invalid table name '{name}'")));
        }
        let schema = less_catalog::SchemaSpec { fields }.to_arrow();
        let table = MemoryTable::new(name, schema, pk)?;
        let mut tables = self.tables.lock().unwrap();
        if tables.contains_key(name) {
            return Err(LessError::Catalog(format!("table '{name}' already exists")));
        }
        tables.insert(name.to_string(), table);
        drop(tables);
        self.persist(name)
    }

    /// Insert a batch (aligned to the table schema) into a table.
    pub fn insert(&self, name: &str, batch: RecordBatch) -> Result<usize> {
        let mut tables = self.tables.lock().unwrap();
        let table = tables
            .get_mut(name)
            .ok_or_else(|| LessError::Catalog(format!("table '{name}' does not exist")))?;
        let n = table.insert(batch)?;
        drop(tables);
        self.persist(name)?;
        Ok(n)
    }

    /// Latest row for a primary key, as JSON.
    pub fn point_get(
        &self,
        name: &str,
        key: &serde_json::Value,
    ) -> Result<Option<serde_json::Value>> {
        let tables = self.tables.lock().unwrap();
        let table = tables
            .get(name)
            .ok_or_else(|| LessError::Catalog(format!("table '{name}' does not exist")))?;
        table.point_get(key)
    }

    /// Deduplicate by primary key, keeping the last row per key.
    pub fn compact(&self, name: &str) -> Result<usize> {
        let mut tables = self.tables.lock().unwrap();
        let table = tables
            .get_mut(name)
            .ok_or_else(|| LessError::Catalog(format!("table '{name}' does not exist")))?;
        let removed = table.compact()?;
        drop(tables);
        if removed > 0 {
            self.persist(name)?;
        }
        Ok(removed)
    }

    pub fn table_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tables.lock().unwrap().keys().cloned().collect();
        names.sort();
        names
    }

    pub fn describe(&self, name: &str) -> Result<TableManifest> {
        let tables = self.tables.lock().unwrap();
        let table = tables
            .get(name)
            .ok_or_else(|| LessError::Catalog(format!("table '{name}' does not exist")))?;
        let fields: Vec<FieldSpec> = table
            .schema
            .fields()
            .iter()
            .map(|f| {
                Ok(FieldSpec {
                    name: f.name().to_string(),
                    ty: type_spec_from_arrow(f.data_type())?,
                    nullable: f.is_nullable(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(TableManifest {
            name: name.to_string(),
            fields,
            pk: table.pk.clone(),
        })
    }

    /// Run SQL over all memory tables (async; use from async contexts).
    pub async fn sql_async(&self, sql: &str) -> Result<Vec<RecordBatch>> {
        self.register_memtables()?;
        self.ctx
            .sql(sql)
            .await
            .map_err(|e| LessError::Query(e.to_string()))?
            .collect()
            .await
            .map_err(|e| LessError::Query(e.to_string()))
    }

    /// Run SQL over all memory tables (synchronous convenience).
    pub fn sql(&self, sql: &str) -> Result<Vec<RecordBatch>> {
        self.runtime
            .as_ref()
            .expect("runtime taken during drop")
            .block_on(self.sql_async(sql))
    }

    /// (Re)register every memory table as a DataFusion MemTable.
    fn register_memtables(&self) -> Result<()> {
        let memtables: Vec<(String, MemTable)> = {
            let tables = self.tables.lock().unwrap();
            tables
                .iter()
                .map(|(name, t)| Ok((name.clone(), t.to_memtable()?)))
                .collect::<Result<Vec<_>>>()?
        };
        for (name, mt) in memtables {
            let _ = self.ctx.deregister_table(&name);
            self.ctx
                .register_table(&name, Arc::new(mt))
                .map_err(|e| LessError::Query(e.to_string()))?;
        }
        Ok(())
    }
}

/// Parse JSON (array or NDJSON) into batches for a schema.
pub fn json_to_batches(schema: SchemaRef, json: &str) -> Result<Vec<RecordBatch>> {
    let ndjson: String = if json.trim_start().starts_with('[') {
        let values: Vec<serde_json::Value> = serde_json::from_str(json)?;
        let mut out = String::with_capacity(json.len());
        for v in values {
            out.push_str(&v.to_string());
            out.push('\n');
        }
        out
    } else {
        json.to_string()
    };
    let reader = arrow_json::reader::ReaderBuilder::new(schema)
        .with_batch_size(65536)
        .build(std::io::Cursor::new(ndjson.into_bytes()))
        .map_err(LessError::Arrow)?;
    Ok(reader.collect::<std::result::Result<Vec<_>, _>>()?)
}

fn type_spec_from_arrow(ty: &arrow::datatypes::DataType) -> Result<less_catalog::TypeSpec> {
    use arrow::datatypes::{DataType as T, TimeUnit};
    use less_catalog::TypeSpec as S;
    Ok(match ty {
        T::Int8 => S::Int8,
        T::Int16 => S::Int16,
        T::Int32 => S::Int32,
        T::Int64 => S::Int64,
        T::UInt8 => S::UInt8,
        T::UInt16 => S::UInt16,
        T::UInt32 => S::UInt32,
        T::UInt64 => S::UInt64,
        T::Float32 => S::Float32,
        T::Float64 => S::Float64,
        T::Boolean => S::Bool,
        T::Utf8 | T::LargeUtf8 | T::Utf8View => S::Utf8,
        T::Date32 => S::Date32,
        T::Timestamp(TimeUnit::Millisecond, _) => S::TimestampMs,
        T::Timestamp(TimeUnit::Microsecond, _) => S::TimestampUs,
        T::Timestamp(TimeUnit::Nanosecond, _) => S::TimestampNs,
        other => {
            return Err(LessError::Config(format!(
                "unsupported memory table type {other:?}"
            )));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Float64Array, Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field};
    use less_catalog::TypeSpec;

    fn schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, false),
            Field::new("score", DataType::Float64, false),
        ]))
    }

    fn batch(ids: Vec<i64>, names: Vec<&str>, scores: Vec<f64>) -> RecordBatch {
        RecordBatch::try_new(
            schema(),
            vec![
                Arc::new(Int64Array::from(ids)),
                Arc::new(StringArray::from(names)),
                Arc::new(Float64Array::from(scores)),
            ],
        )
        .unwrap()
    }

    #[test]
    fn insert_point_get_compact_sql() {
        let dir = std::env::temp_dir().join(format!("less-mem-{}", uuid::Uuid::new_v4()));
        let store = MemoryStore::open(Some(&dir)).unwrap();
        store
            .create_table(
                "people",
                vec![
                    FieldSpec::new("id", TypeSpec::Int64),
                    FieldSpec::new("name", TypeSpec::Utf8),
                    FieldSpec::new("score", TypeSpec::Float64),
                ],
                Some("id".into()),
            )
            .unwrap();
        store
            .insert(
                "people",
                batch(vec![1, 2], vec!["ada", "grace"], vec![9.5, 9.9]),
            )
            .unwrap();
        store
            .insert("people", batch(vec![2], vec!["grace-v2"], vec![10.0]))
            .unwrap();

        // point_get returns the LATEST row for a pk.
        let row = store
            .point_get("people", &serde_json::json!(2))
            .unwrap()
            .unwrap();
        assert_eq!(row["name"], "grace-v2");
        assert!(
            store
                .point_get("people", &serde_json::json!(3))
                .unwrap()
                .is_none()
        );

        // SQL sees the append log (2 + 1 rows).
        let batches = store.sql("SELECT count(*) AS n FROM people").unwrap();
        let n = batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .value(0);
        assert_eq!(n, 3);

        // Compact dedups by pk keeping last.
        let removed = store.compact("people").unwrap();
        assert_eq!(removed, 1);
        let batches = store.sql("SELECT count(*) AS n FROM people").unwrap();
        let n = batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .value(0);
        assert_eq!(n, 2);

        // Persistence roundtrip.
        drop(store);
        let store = MemoryStore::open(Some(&dir)).unwrap();
        let row = store
            .point_get("people", &serde_json::json!(2))
            .unwrap()
            .unwrap();
        assert_eq!(row["name"], "grace-v2");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
