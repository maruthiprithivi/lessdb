//! SQL write path: `INSERT INTO <table> …` and `COPY <table> FROM …` land
//! here — a DataFusion `DataSink` streaming batches into the engine
//! (buffered + WAL'd + flushed at completion).

use std::fmt::Display;
use std::sync::Arc;

use arrow::datatypes::SchemaRef;
use async_trait::async_trait;
use datafusion::error::Result as DFResult;
use datafusion::execution::TaskContext;
use datafusion::physical_plan::SendableRecordBatchStream;
use datafusion::physical_plan::display::{DisplayAs, DisplayFormatType};
use datafusion_datasource::sink::DataSink;
use futures::StreamExt;

use less_engine::LessEngine;

/// Sink that inserts streamed batches into a LessDB table.
pub struct LessTableSink {
    engine: Arc<LessEngine>,
    table: String,
    schema: SchemaRef,
}

impl LessTableSink {
    pub fn new(engine: Arc<LessEngine>, table: &str, schema: SchemaRef) -> Self {
        Self {
            engine,
            table: table.to_string(),
            schema,
        }
    }
}

impl DisplayAs for LessTableSink {
    fn fmt_as(&self, _t: DisplayFormatType, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "LessTableSink({})", self.table)
    }
}

impl Display for LessTableSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LessTableSink({})", self.table)
    }
}

impl std::fmt::Debug for LessTableSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LessTableSink({})", self.table)
    }
}

#[async_trait]
impl DataSink for LessTableSink {
    fn schema(&self) -> &SchemaRef {
        &self.schema
    }

    async fn write_all(
        &self,
        data: SendableRecordBatchStream,
        _context: &Arc<TaskContext>,
    ) -> DFResult<u64> {
        let mut rows = 0u64;
        let mut stream = data;
        while let Some(batch) = stream.next().await {
            let batch = batch?;
            rows += batch.num_rows() as u64;
            self.engine
                .insert(&self.table, batch)
                .map_err(|e| datafusion::error::DataFusionError::Execution(e.to_string()))?;
        }
        // Flush: written rows become an immutable part and are queryable.
        self.engine
            .flush(&self.table)
            .map_err(|e| datafusion::error::DataFusionError::Execution(e.to_string()))?;
        Ok(rows)
    }
}
