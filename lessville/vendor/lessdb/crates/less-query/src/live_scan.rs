//! Transparent scan/stream ownership: optimizers may replace children, but
//! cannot shed the immutable-file leases needed by this planned generation.
use datafusion::common::tree_node::TreeNodeRecursion;
use datafusion::common::{DataFusionError, Result, Statistics};
use datafusion::physical_expr::PhysicalExpr;
use datafusion::physical_plan::filter_pushdown::{FilterDescription, FilterPushdownPhase};
use datafusion::physical_plan::statistics::{ChildStats, StatisticsArgs};
use datafusion::physical_plan::{
    ChildrenPropertiesMode, DisplayAs, DisplayFormatType, ExecutionPlan, PlanProperties,
    ReplaceChildrenOptions,
};
use datafusion_execution::{SendableRecordBatchStream, TaskContext};
use futures::StreamExt;
use less_engine::DataPart;
use std::sync::Arc;

#[derive(Debug)]
pub(crate) struct LiveScanExec {
    inner: Arc<dyn ExecutionPlan>,
    parts: Vec<DataPart>,
}
impl LiveScanExec {
    pub fn wrap(inner: Arc<dyn ExecutionPlan>, parts: Vec<DataPart>) -> Arc<dyn ExecutionPlan> {
        Arc::new(Self { inner, parts })
    }
}
impl DisplayAs for LiveScanExec {
    fn fmt_as(&self, _: DisplayFormatType, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "LiveScanExec: pinned_parts={}", self.parts.len())
    }
}
impl ExecutionPlan for LiveScanExec {
    fn name(&self) -> &str {
        "LiveScanExec"
    }
    fn properties(&self) -> &Arc<PlanProperties> {
        self.inner.properties()
    }
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        vec![&self.inner]
    }
    fn replace_children(
        self: Arc<Self>,
        mut children: Vec<Arc<dyn ExecutionPlan>>,
        _: ReplaceChildrenOptions,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        if children.len() != 1 {
            return Err(DataFusionError::Internal(
                "LiveScanExec requires one child".into(),
            ));
        }
        Ok(Self::wrap(children.remove(0), self.parts.clone()))
    }
    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        self.replace_children(
            children,
            ReplaceChildrenOptions::new(ChildrenPropertiesMode::Recompute),
        )
    }
    fn execute(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> Result<SendableRecordBatchStream> {
        let pins = self.parts.clone();
        let stream = self.inner.execute(partition, context)?.map(move |batch| {
            let _keep_alive = &pins;
            batch
        });
        Ok(Box::pin(
            datafusion::physical_plan::stream::RecordBatchStreamAdapter::new(
                self.inner.schema(),
                stream,
            ),
        ))
    }
    fn apply_expressions(
        &self,
        _: &mut dyn for<'a> FnMut(&'a Arc<dyn PhysicalExpr>) -> Result<TreeNodeRecursion>,
    ) -> Result<TreeNodeRecursion> {
        Ok(TreeNodeRecursion::Continue)
    }
    fn statistics_from_inputs(
        &self,
        inputs: &[Arc<Statistics>],
        _: &StatisticsArgs,
    ) -> Result<Arc<Statistics>> {
        Ok(inputs[0].clone())
    }
    fn child_stats_requests(&self, partition: Option<usize>) -> Vec<ChildStats> {
        vec![ChildStats::At(partition)]
    }
    fn gather_filters_for_pushdown(
        &self,
        _: FilterPushdownPhase,
        filters: Vec<Arc<dyn PhysicalExpr>>,
        _: &datafusion::common::config::ConfigOptions,
    ) -> Result<FilterDescription> {
        FilterDescription::from_children(filters, &self.children())
    }
}
