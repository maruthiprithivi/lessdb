//! [`LessTableProvider`]: the DataFusion `TableProvider` for engine tables.
//!
//! DF 55 architecture notes: a scan is a [`ParquetSource`] (file source with
//! the table's schema) + a [`FileScanConfig`] built with
//! [`FileScanConfigBuilder`], wrapped in a [`DataSourceExec`]. Part files are
//! addressed as store-relative paths; the object store is resolved from the
//! config's [`ObjectStoreUrl`] against the session runtime environment.

use std::sync::Arc;

use arrow::datatypes::{Schema, SchemaRef};
use async_trait::async_trait;
use chrono::Utc;

use datafusion::common::SchemaExt;
use datafusion::common::stats::{ColumnStatistics, Precision, Statistics};
use datafusion::common::{DFSchema, DataFusionError};
use datafusion::datasource::TableProvider;
use datafusion::datasource::provider::{TableProviderFilterPushDown, TableType};
use datafusion::error::Result as DFResult;
use datafusion::logical_expr::Expr;
use datafusion::logical_expr::dml::InsertOp;
use datafusion::logical_expr::utils::conjunction;
use datafusion::physical_expr::PhysicalExpr;
use datafusion::physical_plan::ExecutionPlan;
use datafusion::physical_plan::empty::EmptyExec;
use datafusion::scalar::ScalarValue;
use datafusion_datasource::PartitionedFile;
use datafusion_datasource::file_groups::FileGroup;
use datafusion_datasource::file_scan_config::FileScanConfigBuilder;
use datafusion_datasource::sink::DataSinkExec;
use datafusion_datasource::source::DataSourceExec;
use datafusion_datasource_parquet::source::ParquetSource;
use datafusion_execution::object_store::ObjectStoreUrl;
use datafusion_session::Session;
use object_store::ObjectMeta;

use less_catalog::TableDef;
use less_common::LessError;
use less_engine::LessEngine;
use less_storage::StatValue;

use crate::insert::LessTableSink;
use crate::prune::prune_part;

fn df_err(e: impl std::fmt::Display) -> DataFusionError {
    DataFusionError::Execution(format!("{e}"))
}

/// Split `lessdb_shard(i, n)` marker predicates out of a filter list,
/// returning the first marker found plus the remaining filters.
fn split_shard_filters(filters: &[Expr]) -> (Option<(i64, i64)>, Vec<Expr>) {
    use datafusion::scalar::ScalarValue;
    let mut shard = None;
    let mut rest = Vec::with_capacity(filters.len());
    for f in filters {
        let Expr::ScalarFunction(func) = f else {
            rest.push(f.clone());
            continue;
        };
        if func.func.name() != "lessdb_shard" || func.args.len() != 3 {
            rest.push(f.clone());
            continue;
        }
        // args[0] is any table column (keeps the predicate pushable);
        // args[1]/args[2] carry the shard index and count.
        let ints: Option<Vec<i64>> = func.args[1..]
            .iter()
            .map(|a| match a {
                Expr::Literal(ScalarValue::Int64(Some(v)), _) => Some(*v),
                _ => None,
            })
            .collect();
        match ints {
            Some(v) if v[1] > 0 => {
                if shard.is_none() {
                    shard = Some((v[0], v[1]));
                }
            }
            _ => rest.push(f.clone()),
        }
    }
    (shard, rest)
}

/// Object-store URL for local (on-disk) parts; the store is a
/// `LocalFileSystem` rooted at the engine's data directory.
pub const LOCAL_STORE_URL: &str = "lessdb://local";

/// A DataFusion table backed by LessDB parts.
pub struct LessTableProvider {
    engine: Arc<LessEngine>,
    table: String,
    def: TableDef,
    schema: SchemaRef,
}

impl LessTableProvider {
    pub fn new(engine: Arc<LessEngine>, table: String, def: TableDef, schema: SchemaRef) -> Self {
        Self {
            engine,
            table,
            def,
            schema,
        }
    }

    pub fn table_name(&self) -> &str {
        &self.table
    }

    pub fn definition(&self) -> &TableDef {
        &self.def
    }

    /// Object-store URL holding this table's parts.
    fn object_store_url(&self) -> DFResult<ObjectStoreUrl> {
        if self.def.engine.is_shared() {
            let shared = self
                .engine
                .shared()
                .ok_or_else(|| df_err("shared store unavailable for FireflyCloud table"))?;
            ObjectStoreUrl::parse(shared.base_url().as_str()).map_err(df_err)
        } else {
            ObjectStoreUrl::parse(LOCAL_STORE_URL).map_err(df_err)
        }
    }

    /// Collect every column referenced by an expression tree.
    fn expr_columns(expr: &Expr, out: &mut Vec<String>) {
        match expr {
            Expr::Column(c) => out.push(c.name.clone()),
            Expr::BinaryExpr(b) => {
                Self::expr_columns(&b.left, out);
                Self::expr_columns(&b.right, out);
            }
            Expr::Cast(c) => Self::expr_columns(&c.expr, out),
            Expr::IsNull(i) => Self::expr_columns(i, out),
            Expr::IsNotNull(i) => Self::expr_columns(i, out),
            Expr::Not(n) => Self::expr_columns(n, out),
            Expr::Negative(n) => Self::expr_columns(n, out),
            Expr::InList(l) => {
                Self::expr_columns(&l.expr, out);
                for e in &l.list {
                    Self::expr_columns(e, out);
                }
            }
            Expr::Between(b) => {
                Self::expr_columns(&b.expr, out);
                Self::expr_columns(&b.low, out);
                Self::expr_columns(&b.high, out);
            }
            Expr::Like(l) => Self::expr_columns(&l.expr, out),
            _ => {}
        }
    }
}

#[async_trait]
impl TableProvider for LessTableProvider {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    /// Table-level statistics aggregated from immutable part metadata, so
    /// the optimizer can answer `COUNT(*)`, `MIN`, and `MAX` (and prune
    /// better) without opening any parquet file. Parts are immutable, so a
    /// row count or a min/max derived from part metas is exact.
    fn statistics(&self) -> Option<Statistics> {
        // Shared tables read parts from object storage (async path); skip
        // statistics there for now — local Firefly tables get the fast path.
        if self.def.engine.is_shared() {
            return None;
        }
        let parts = self.engine.parts(&self.table).ok()?;
        let schema = self.def.arrow_schema();
        let mut column_statistics = Statistics::unknown_column(&schema);

        let mut num_rows = 0usize;
        for p in &parts {
            num_rows += p.meta.row_count as usize;
        }

        for (idx, field) in schema.fields().iter().enumerate() {
            let name = field.name();
            let mut gmin: Option<StatValue> = None;
            let mut gmax: Option<StatValue> = None;
            let mut null_count = 0usize;
            let mut complete = true;
            for p in &parts {
                let Some(cs) = p.meta.column(name) else {
                    complete = false;
                    break;
                };
                // NaN is invisible to min/max but orders above everything;
                // treat such parts as unknown so we never over-prune.
                if cs.has_nan {
                    complete = false;
                    break;
                }
                null_count += cs.null_count as usize;
                match (&cs.min, &cs.max) {
                    (Some(mn), Some(mx)) => {
                        gmin = Some(match gmin {
                            None => mn.clone(),
                            Some(cur) if mn.partial_cmp(&cur) == Some(std::cmp::Ordering::Less) => {
                                mn.clone()
                            }
                            Some(cur) => cur,
                        });
                        gmax = Some(match gmax {
                            None => mx.clone(),
                            Some(cur)
                                if mx.partial_cmp(&cur) == Some(std::cmp::Ordering::Greater) =>
                            {
                                mx.clone()
                            }
                            Some(cur) => cur,
                        });
                    }
                    _ => {
                        complete = false;
                        break;
                    }
                }
            }
            if complete
                && let (Some(mn), Some(mx)) = (&gmin, &gmax)
                && let (Some(smin), Some(smax)) = (stat_to_scalar(mn), stat_to_scalar(mx))
            {
                column_statistics[idx] = ColumnStatistics::new_unknown()
                    .with_null_count(Precision::Exact(null_count))
                    .with_min_value(Precision::Exact(smin))
                    .with_max_value(Precision::Exact(smax));
            }
        }

        Some(Statistics {
            num_rows: Precision::Exact(num_rows),
            total_byte_size: Precision::Absent,
            column_statistics,
        })
    }

    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> DFResult<Vec<TableProviderFilterPushDown>> {
        // Part-level pruning happens in scan(); row-level filtering is
        // re-applied by DataFusion (and by the parquet reader when the
        // predicate is safe to push). "Inexact" is deliberately
        // conservative.
        Ok(vec![TableProviderFilterPushDown::Inexact; filters.len()])
    }

    async fn insert_into(
        &self,
        _state: &dyn Session,
        input: Arc<dyn ExecutionPlan>,
        insert_op: InsertOp,
    ) -> DFResult<Arc<dyn ExecutionPlan>> {
        if insert_op != InsertOp::Append {
            return Err(DataFusionError::NotImplemented(format!(
                "{insert_op} is not implemented for LessDB tables (append only)"
            )));
        }
        self.schema()
            .logically_equivalent_names_and_types(&input.schema())?;
        let sink = LessTableSink::new(self.engine.clone(), &self.table, self.schema.clone());
        Ok(Arc::new(DataSinkExec::new(input, Arc::new(sink), None)))
    }

    async fn delete_from(
        &self,
        state: &dyn Session,
        filters: Vec<Expr>,
    ) -> DFResult<Arc<dyn ExecutionPlan>> {
        let phys = crate::mutation::physical_filters(state, &self.schema, &filters)?;
        let engine = self.engine.clone();
        let table = self.table.clone();
        let deleted = engine
            .delete_where(&table, move |batch| {
                crate::mutation::filters_to_mask(&phys, batch)
                    .map_err(|e| LessError::Query(format!("DELETE evaluation: {e}")))
            })
            .map_err(df_err)?;
        Ok(Arc::new(crate::mutation::DmlResultExec::new(deleted)))
    }

    async fn update(
        &self,
        state: &dyn Session,
        assignments: Vec<(String, Expr)>,
        filters: Vec<Expr>,
    ) -> DFResult<Arc<dyn ExecutionPlan>> {
        let phys_filters = crate::mutation::physical_filters(state, &self.schema, &filters)?;
        let df_schema = DFSchema::try_from(self.schema.clone())?;
        let phys_assignments: Vec<(String, Arc<dyn PhysicalExpr>)> = assignments
            .iter()
            .map(|(col, expr)| {
                state
                    .create_physical_expr(expr.clone(), &df_schema)
                    .map(|e| (col.clone(), e))
            })
            .collect::<DFResult<_>>()?;
        let engine = self.engine.clone();
        let table = self.table.clone();
        let updated = engine
            .update_where(&table, move |batch| {
                let mask = crate::mutation::filters_to_mask(&phys_filters, batch)
                    .map_err(|e| LessError::Query(format!("UPDATE evaluation: {e}")))?;
                if mask.true_count() == 0 {
                    return Ok(None);
                }
                let new_batch = crate::mutation::apply_assignments(&phys_assignments, batch)?;
                Ok(Some((new_batch, mask.true_count() as u64)))
            })
            .map_err(df_err)?;
        Ok(Arc::new(crate::mutation::DmlResultExec::new(updated)))
    }

    async fn scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DFResult<Arc<dyn ExecutionPlan>> {
        let full_schema = self.def.arrow_schema();
        let projected: SchemaRef = match projection {
            Some(p) => Arc::new(Schema::new(
                p.iter()
                    .filter_map(|i| full_schema.fields().get(*i).map(|f| f.as_ref().clone()))
                    .collect::<Vec<arrow::datatypes::Field>>(),
            )),
            None => full_schema.clone(),
        };

        // Part-level pruning: bloom filters + typed min/max stats.
        // `lessdb_shard(i, n)` markers are intercepted here (see shard.rs):
        // they restrict the scan to part names hashing to shard i of n and
        // are stripped from the filters passed to pruning/pushdown.
        let (shard, filters): (Option<(i64, i64)>, Vec<Expr>) = split_shard_filters(filters);
        let all_parts = self.engine.parts_async(&self.table).await.map_err(df_err)?;
        let total = all_parts.len();
        let parts: Vec<_> = all_parts
            .into_iter()
            .filter(|p| match shard {
                Some((i, n)) if n > 1 => crate::shard::shard_of(&p.meta.name, n as u64) == i as u64,
                _ => true,
            })
            .filter(|p| prune_part(&p.meta, &filters))
            .collect();
        less_telemetry::global()
            .parts_pruned
            .add((total - parts.len()) as u64);
        less_telemetry::global()
            .parts_scanned
            .add(parts.len() as u64);

        if parts.is_empty() {
            return Ok(Arc::new(EmptyExec::new(projected)));
        }

        // Push the predicate into the parquet reader (row-group stats
        // pruning + row filtering) only when every referenced column is
        // part of the projected schema.
        let predicate = if filters.is_empty() {
            None
        } else {
            let mut needed = vec![];
            for f in &filters {
                Self::expr_columns(f, &mut needed);
            }
            let all_present = needed.iter().all(|c| {
                projected
                    .fields()
                    .iter()
                    .any(|f| f.name().as_str() == c.as_str())
            });
            if all_present {
                let conj = conjunction(filters.to_vec())
                    .ok_or_else(|| df_err("failed to combine filter predicates"))?;
                let df_schema = DFSchema::try_from(full_schema.clone()).map_err(df_err)?;
                Some(state.create_physical_expr(conj, &df_schema)?)
            } else {
                None
            }
        };

        let source = ParquetSource::new(full_schema.clone()).with_pushdown_filters(true);
        let source = match predicate {
            Some(p) => source.with_predicate(p),
            None => source,
        };

        let mut builder = FileScanConfigBuilder::new(self.object_store_url()?, Arc::new(source));

        // One file group per part = one parallel scan task per part.
        let mut groups: Vec<FileGroup> = Vec::with_capacity(parts.len());
        for part in &parts {
            let (rel_key, size) = self.engine.part_file_async(part).await.map_err(df_err)?;
            groups.push(FileGroup::new(vec![PartitionedFile::new_from_meta(
                ObjectMeta {
                    location: rel_key.into(),
                    last_modified: Utc::now(),
                    size,
                    e_tag: None,
                    version: None,
                },
            )]));
        }
        builder = builder
            .with_file_groups(groups)
            .with_projection_indices(projection.cloned())
            .map_err(df_err)?;
        builder = builder.with_limit(limit);

        Ok(DataSourceExec::from_data_source(builder.build()))
    }
}

/// Convert a part-metadata [`StatValue`] into an Arrow [`ScalarValue`] for
/// the optimizer's column statistics.
fn stat_to_scalar(v: &StatValue) -> Option<ScalarValue> {
    use ScalarValue::*;
    Some(match v {
        StatValue::Null => return None,
        StatValue::Bool(b) => Boolean(Some(*b)),
        StatValue::I64(v) => Int64(Some(*v)),
        StatValue::U64(v) => UInt64(Some(*v)),
        StatValue::F64(v) => Float64(Some(*v)),
        StatValue::Str(s) => Utf8(Some(s.clone())),
        StatValue::Date32(d) => Date32(Some(*d)),
        StatValue::Timestamp { value, unit } => match unit.as_str() {
            "ms" | "millisecond" => TimestampMillisecond(Some(*value), None),
            "us" | "microsecond" => TimestampMicrosecond(Some(*value), None),
            _ => TimestampNanosecond(Some(*value), None),
        },
    })
}

impl std::fmt::Debug for LessTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LessTableProvider({})", self.table)?;
        // Surface uniqueness info in EXPLAIN output.
        if !self.def.unique.is_empty() {
            write!(f, " unique={}", self.def.unique.join(","))?;
        }
        Ok(())
    }
}

/// Convenience for callers that hold a `LessError`.
#[allow(dead_code)]
pub(crate) fn less_err(e: LessError) -> DataFusionError {
    df_err(e)
}
