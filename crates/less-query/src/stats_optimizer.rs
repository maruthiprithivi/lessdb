//! Optimizer rule: answer `COUNT(*)`, `MIN`, and `MAX` over a bare table
//! scan directly from the provider's table statistics (immutable part
//! metadata), so these queries don't open a single parquet file — the
//! DuckDB zone-map trick. Parts are immutable, so the stats are exact.

use datafusion::catalog::default_table_source::DefaultTableSource;
use datafusion::common::stats::Precision;
use datafusion::common::tree_node::Transformed;
use datafusion::error::Result as DFResult;
use datafusion::logical_expr::builder::LogicalPlanBuilder;
use datafusion::logical_expr::expr::AggregateFunction;
use datafusion::logical_expr::{Expr, LogicalPlan};
use datafusion::optimizer::{ApplyOrder, OptimizerConfig, OptimizerRule};
use datafusion::scalar::ScalarValue;

/// Is this `count(Int64(1))` / `count(Int32(1))` — i.e. the canonical
/// `COUNT(*)`?
fn is_count_star(args: &[Expr]) -> bool {
    matches!(
        args,
        [Expr::Literal(ScalarValue::Int64(Some(1)), _)]
            | [Expr::Literal(ScalarValue::Int32(Some(1)), _)]
    )
}

#[derive(Debug, Default)]
pub struct CountMinMaxFromStats;

impl CountMinMaxFromStats {
    pub fn new() -> Self {
        Self
    }
}

impl OptimizerRule for CountMinMaxFromStats {
    fn name(&self) -> &str {
        "lessdb_count_minmax_from_stats"
    }

    fn apply_order(&self) -> Option<ApplyOrder> {
        Some(ApplyOrder::BottomUp)
    }

    fn rewrite(
        &self,
        plan: LogicalPlan,
        _config: &dyn OptimizerConfig,
    ) -> DFResult<Transformed<LogicalPlan>> {
        // Only a bare (non-grouped) aggregate over a bare (unfiltered)
        // table scan.
        let LogicalPlan::Aggregate(agg) = &plan else {
            return Ok(Transformed::no(plan));
        };
        if !agg.group_expr.is_empty() || agg.aggr_expr.is_empty() {
            return Ok(Transformed::no(plan));
        }
        let LogicalPlan::TableScan(scan) = agg.input.as_ref() else {
            return Ok(Transformed::no(plan));
        };
        if !scan.filters.is_empty() {
            return Ok(Transformed::no(plan));
        }
        let Some(source) = scan.source.downcast_ref::<DefaultTableSource>() else {
            return Ok(Transformed::no(plan));
        };
        let Some(stats) = source.table_provider.statistics() else {
            return Ok(Transformed::no(plan));
        };
        let schema = source.table_provider.schema();

        // Every aggregate expression must be COUNT(*), MIN, or MAX so the
        // whole aggregate can become literals; otherwise leave the plan alone.
        let mut replacement = Vec::with_capacity(agg.aggr_expr.len());
        for (i, aggr) in agg.aggr_expr.iter().enumerate() {
            let Expr::AggregateFunction(AggregateFunction { func, params }) = aggr else {
                return Ok(Transformed::no(plan));
            };
            let value = match func.name().to_lowercase().as_str() {
                "count" if !params.distinct && is_count_star(&params.args) => {
                    match stats.num_rows {
                        Precision::Exact(n) => ScalarValue::Int64(Some(n as i64)),
                        _ => return Ok(Transformed::no(plan)),
                    }
                }
                "min" | "max" if !params.distinct && params.args.len() == 1 => {
                    let Expr::Column(c) = &params.args[0] else {
                        return Ok(Transformed::no(plan));
                    };
                    let Some(idx) = schema.fields().iter().position(|f| f.name() == &c.name) else {
                        return Ok(Transformed::no(plan));
                    };
                    let Some(cs) = stats.column_statistics.get(idx) else {
                        return Ok(Transformed::no(plan));
                    };
                    let p = if func.name() == "min" {
                        &cs.min_value
                    } else {
                        &cs.max_value
                    };
                    match p {
                        Precision::Exact(v) => v.clone(),
                        _ => return Ok(Transformed::no(plan)),
                    }
                }
                _ => return Ok(Transformed::no(plan)),
            };
            let field_name = agg.schema.field(i).name().to_string();
            replacement.push(Expr::Literal(value, None).alias(field_name));
        }

        // Replace the aggregate with a one-row projection over an empty
        // relation, aliasing each literal to its aggregate output field name
        // so the parent projection still resolves.
        let new_plan = LogicalPlanBuilder::empty(true)
            .project(replacement)
            .and_then(|b| b.build())
            .map_err(|e| datafusion::common::DataFusionError::Plan(e.to_string()))?;
        Ok(Transformed::yes(new_plan))
    }
}
