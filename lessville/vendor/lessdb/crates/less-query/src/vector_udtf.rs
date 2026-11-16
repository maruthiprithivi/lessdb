//! The `vector_search` table function — LanceDB-style SQL access to the
//! vector registry.
//!
//! ```sql
//! SELECT * FROM vector_search('docs', [0.1, 0.2, 0.3, 0.4], 5);
//! SELECT * FROM vector_search('docs', '[0.1,0.2,0.3,0.4]', 5, 8 /* nprobe */);
//! ```
//!
//! Returns columns `id` (u32), `score` (f32, smaller = closer) and
//! `payload` (JSON string). Implemented as a DataFusion table function:
//! `call_with_args` runs the search at plan time and hands back a MemTable
//! — the same shape LanceDB uses over DataFusion.

use std::sync::{Arc, RwLock};

use arrow::array::{Float32Array, StringArray, UInt32Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use datafusion::catalog::{TableFunctionArgs, TableFunctionImpl};
use datafusion::common::DataFusionError;
use datafusion::datasource::{MemTable, TableProvider};
use datafusion::error::Result as DFResult;
use datafusion::logical_expr::Expr;
use datafusion::scalar::ScalarValue;

use less_vector::VectorRegistry;

fn df_err(e: impl std::fmt::Display) -> DataFusionError {
    DataFusionError::Execution(format!("{e}"))
}

/// Extract a float vector from a query literal: List/FixedSizeList of
/// numbers, or a JSON string like "[0.1, 0.2]".
fn literal_to_vec(sv: &ScalarValue) -> Option<Vec<f32>> {
    match sv {
        ScalarValue::Utf8(Some(s)) | ScalarValue::LargeUtf8(Some(s)) => {
            serde_json::from_str::<Vec<f32>>(s).ok()
        }
        ScalarValue::List(arr) => {
            let values = arr.values();
            if let Some(f) = values.as_any().downcast_ref::<arrow::array::Float64Array>() {
                return Some(f.values().iter().map(|v| *v as f32).collect());
            }
            if let Some(f) = values.as_any().downcast_ref::<arrow::array::Float32Array>() {
                return Some(f.values().to_vec());
            }
            if let Some(f) = values.as_any().downcast_ref::<arrow::array::Int64Array>() {
                return Some(f.values().iter().map(|v| *v as f32).collect());
            }
            None
        }
        ScalarValue::LargeList(arr) => {
            let values = arr.values();
            if let Some(f) = values.as_any().downcast_ref::<arrow::array::Float64Array>() {
                return Some(f.values().iter().map(|v| *v as f32).collect());
            }
            if let Some(f) = values.as_any().downcast_ref::<arrow::array::Float32Array>() {
                return Some(f.values().to_vec());
            }
            if let Some(f) = values.as_any().downcast_ref::<arrow::array::Int64Array>() {
                return Some(f.values().iter().map(|v| *v as f32).collect());
            }
            None
        }
        ScalarValue::FixedSizeList(arr) => {
            let values = arr.values();
            if let Some(f) = values.as_any().downcast_ref::<arrow::array::Float32Array>() {
                return Some(f.values().to_vec());
            }
            if let Some(f) = values.as_any().downcast_ref::<arrow::array::Float64Array>() {
                return Some(f.values().iter().map(|v| *v as f32).collect());
            }
            None
        }
        _ => None,
    }
}

fn literal_to_usize(sv: &ScalarValue) -> Option<usize> {
    match sv {
        ScalarValue::Int8(Some(v)) => Some(*v as usize),
        ScalarValue::Int16(Some(v)) => Some(*v as usize),
        ScalarValue::Int32(Some(v)) => Some(*v as usize),
        ScalarValue::Int64(Some(v)) => Some(*v as usize),
        ScalarValue::UInt8(Some(v)) => Some(*v as usize),
        ScalarValue::UInt16(Some(v)) => Some(*v as usize),
        ScalarValue::UInt32(Some(v)) => Some(*v as usize),
        ScalarValue::UInt64(Some(v)) => Some(*v as usize),
        _ => None,
    }
}

/// `vector_search(space, query, k [, nprobe])`.
pub struct VectorSearchTableFunction {
    pub vectors: Arc<RwLock<VectorRegistry>>,
}

impl std::fmt::Debug for VectorSearchTableFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VectorSearchTableFunction").finish()
    }
}

impl TableFunctionImpl for VectorSearchTableFunction {
    fn call_with_args(&self, args: TableFunctionArgs) -> DFResult<Arc<dyn TableProvider>> {
        let exprs: Vec<&Expr> = args.exprs().iter().collect();
        let literal = |i: usize| match exprs.get(i).map(|e| (*e).clone()) {
            Some(Expr::Literal(v, _)) => Ok(v),
            other => Err(df_err(format!(
                "vector_search argument {i} must be a literal, got {:?}",
                other.map(|e| e.to_string())
            ))),
        };
        let space_sv = literal(0)?;
        let space = match space_sv {
            ScalarValue::Utf8(Some(s)) => s,
            other => {
                return Err(df_err(format!(
                    "vector_search argument 0 must be a space name string, got {other:?}"
                )));
            }
        };
        let query_sv = literal(1)?;
        let query = literal_to_vec(&query_sv).ok_or_else(|| {
            df_err(format!(
                "vector_search argument 1 must be a vector (array or JSON string), got {query_sv:?}"
            ))
        })?;
        let k_sv = literal(2)?;
        let k = literal_to_usize(&k_sv).ok_or_else(|| {
            df_err(format!(
                "vector_search argument 2 must be an integer k, got {k_sv:?}"
            ))
        })?;
        let nprobe = match exprs.get(3) {
            Some(Expr::Literal(v, _)) => literal_to_usize(v).unwrap_or(8),
            Some(other) => {
                return Err(df_err(format!(
                    "vector_search argument 3 must be an integer nprobe, got {}",
                    other
                )));
            }
            None => 8,
        };

        let registry = self.vectors.read().unwrap();
        let hits = registry.search(&space, query, k, nprobe).map_err(df_err)?;

        let mut ids = Vec::with_capacity(hits.len());
        let mut scores = Vec::with_capacity(hits.len());
        let mut payloads = Vec::with_capacity(hits.len());
        for h in hits {
            ids.push(h.id);
            scores.push(h.score);
            payloads.push(h.payload.to_string());
        }
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::UInt32, false),
            Field::new("score", DataType::Float32, false),
            Field::new("payload", DataType::Utf8, true),
        ]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(UInt32Array::from(ids)),
                Arc::new(Float32Array::from(scores)),
                Arc::new(StringArray::from(payloads)),
            ],
        )
        .map_err(df_err)?;
        let memtable = MemTable::try_new(schema, vec![vec![batch]]).map_err(df_err)?;
        Ok(Arc::new(memtable))
    }
}
