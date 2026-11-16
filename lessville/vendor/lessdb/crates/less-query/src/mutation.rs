//! DELETE/UPDATE execution support: a tiny `DmlResultExec` (rows-affected
//! reporter, mirroring DataFusion's private one) plus helpers that turn
//! physical filter expressions into per-batch boolean masks.

use std::fmt;
use std::sync::Arc;

use arrow::array::{BooleanArray, UInt64Array};
use arrow::compute;
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow::record_batch::RecordBatch;
use datafusion::common::{DFSchema, Result as DFResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_expr::PhysicalExpr;
use datafusion::physical_plan::{
    DisplayAs, DisplayFormatType, ExecutionPlan, Partitioning, PlanProperties,
    ReplaceChildrenOptions,
    execution_plan::{Boundedness, EmissionType},
};
use datafusion::scalar::ScalarValue;
use datafusion_execution::TaskContext;
use datafusion_session::Session;

use less_common::LessError;

/// Build physical expressions for logical filters against the table schema.
pub fn physical_filters(
    state: &dyn Session,
    schema: &Schema,
    filters: &[Expr],
) -> DFResult<Vec<Arc<dyn PhysicalExpr>>> {
    let df_schema = DFSchema::try_from(schema.clone())?;
    filters
        .iter()
        .map(|f| state.create_physical_expr(f.clone(), &df_schema))
        .collect()
}

/// Evaluate a columnar value as a boolean mask of `rows` entries.
fn columnar_to_mask(
    v: &datafusion::logical_expr::ColumnarValue,
    rows: usize,
) -> DFResult<BooleanArray> {
    use datafusion::logical_expr::ColumnarValue;
    match v {
        ColumnarValue::Array(a) => {
            if a.data_type() == &DataType::Boolean {
                let b = a.as_any().downcast_ref::<BooleanArray>().ok_or_else(|| {
                    datafusion::error::DataFusionError::Internal("not bool".into())
                })?;
                Ok(b.clone())
            } else {
                let cast = compute::cast(a, &DataType::Boolean)?;
                let b = cast
                    .as_any()
                    .downcast_ref::<BooleanArray>()
                    .ok_or_else(|| {
                        datafusion::error::DataFusionError::Internal("cast failed".into())
                    })?;
                Ok(b.clone())
            }
        }
        ColumnarValue::Scalar(ScalarValue::Boolean(Some(b))) => {
            Ok(BooleanArray::from(vec![*b; rows]))
        }
        // NULL predicate → no row matches (SQL three-valued logic).
        ColumnarValue::Scalar(ScalarValue::Null) => Ok(BooleanArray::from(vec![false; rows])),
        other => Err(datafusion::error::DataFusionError::Internal(format!(
            "filter evaluated to non-boolean {other:?}"
        ))),
    }
}

/// AND every filter into one per-row boolean mask (empty filters = all
/// rows match).
pub fn filters_to_mask(
    exprs: &[Arc<dyn PhysicalExpr>],
    batch: &RecordBatch,
) -> DFResult<BooleanArray> {
    let rows = batch.num_rows();
    if exprs.is_empty() {
        return Ok(BooleanArray::from(vec![true; rows]));
    }
    let mut mask: Option<BooleanArray> = None;
    for e in exprs {
        let v = e.evaluate(batch)?;
        let arr = columnar_to_mask(&v, rows)?;
        mask = Some(match mask {
            None => arr,
            Some(prev) => compute::kernels::boolean::and(&prev, &arr)?,
        });
    }
    Ok(mask.expect("at least one filter"))
}

/// Apply an UPDATE assignment (column name → new-value expression) to a
/// batch, casting each result to the column's declared type.
pub fn apply_assignments(
    assignments: &[(String, Arc<dyn PhysicalExpr>)],
    batch: &RecordBatch,
) -> less_common::Result<RecordBatch> {
    let mut columns: Vec<Arc<dyn arrow::array::Array>> = batch.columns().to_vec();
    for (col, expr) in assignments {
        let idx = batch
            .schema()
            .index_of(col)
            .map_err(|e| LessError::Query(format!("UPDATE column {col}: {e}")))?;
        let target_type = batch.schema().field(idx).data_type().clone();
        let value = expr
            .evaluate(batch)
            .map_err(|e| LessError::Query(format!("UPDATE {col}: {e}")))?;
        let arr = match value {
            datafusion::logical_expr::ColumnarValue::Array(a) => Arc::new(a),
            datafusion::logical_expr::ColumnarValue::Scalar(s) => s
                .to_array_of_size(batch.num_rows())
                .map_err(|e| LessError::Query(format!("UPDATE {col}: {e}")))?,
        };
        let cast = if arr.data_type() == &target_type {
            arr
        } else {
            compute::cast(&arr, &target_type)
                .map_err(|e| LessError::Query(format!("UPDATE {col}: {e}")))?
        };
        columns[idx] = cast;
    }
    RecordBatch::try_new(batch.schema(), columns).map_err(LessError::Arrow)
}

/// Reports a single `count` column with the number of affected rows —
/// the physical plan a DELETE/UPDATE returns.
#[derive(Debug)]
pub struct DmlResultExec {
    rows_affected: u64,
    schema: SchemaRef,
    properties: Arc<PlanProperties>,
}

impl DmlResultExec {
    pub fn new(rows_affected: u64) -> Self {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "count",
            DataType::UInt64,
            false,
        )]));
        let properties = PlanProperties::new(
            datafusion::physical_expr::EquivalenceProperties::new(Arc::clone(&schema)),
            Partitioning::UnknownPartitioning(1),
            EmissionType::Final,
            Boundedness::Bounded,
        );
        Self {
            rows_affected,
            schema,
            properties: Arc::new(properties),
        }
    }
}

impl DisplayAs for DmlResultExec {
    fn fmt_as(&self, t: DisplayFormatType, f: &mut fmt::Formatter) -> fmt::Result {
        match t {
            DisplayFormatType::Default
            | DisplayFormatType::Verbose
            | DisplayFormatType::TreeRender => {
                write!(f, "DmlResultExec: rows_affected={}", self.rows_affected)
            }
        }
    }
}

impl ExecutionPlan for DmlResultExec {
    fn name(&self) -> &str {
        "DmlResultExec"
    }

    fn schema(&self) -> SchemaRef {
        Arc::clone(&self.schema)
    }

    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
    }

    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        vec![]
    }

    fn replace_children(
        self: Arc<Self>,
        _: Vec<Arc<dyn ExecutionPlan>>,
        _: ReplaceChildrenOptions,
    ) -> DFResult<Arc<dyn ExecutionPlan>> {
        Ok(self)
    }

    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> DFResult<Arc<dyn ExecutionPlan>> {
        self.replace_children(
            children,
            ReplaceChildrenOptions::new(
                datafusion::physical_plan::ChildrenPropertiesMode::Recompute,
            ),
        )
    }

    fn execute(
        &self,
        _partition: usize,
        _context: Arc<TaskContext>,
    ) -> DFResult<datafusion_execution::SendableRecordBatchStream> {
        let batch = RecordBatch::try_new(
            Arc::clone(&self.schema),
            vec![Arc::new(UInt64Array::from(vec![self.rows_affected]))],
        )?;
        let stream = futures::stream::iter(vec![Ok(batch)]);
        Ok(Box::pin(
            datafusion::physical_plan::stream::RecordBatchStreamAdapter::new(
                Arc::clone(&self.schema),
                stream,
            ),
        ))
    }

    fn apply_expressions(
        &self,
        _f: &mut dyn for<'a> FnMut(
            &'a Arc<dyn PhysicalExpr>,
        )
            -> DFResult<datafusion::common::tree_node::TreeNodeRecursion>,
    ) -> DFResult<datafusion::common::tree_node::TreeNodeRecursion> {
        Ok(datafusion::common::tree_node::TreeNodeRecursion::Continue)
    }
}
