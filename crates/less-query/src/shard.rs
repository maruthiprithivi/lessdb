//! `lessdb_shard(i, n)` — the fan-out hook.
//!
//! This scalar function always returns `true`; its real work happens in
//! [`crate::provider::LessTableProvider::scan`], which recognizes the
//! predicate during planning and scans only the parts whose names hash to
//! shard `i` of `n`. That lets a coordinator split one table scan across
//! `n` compute nodes with plain SQL:
//!
//! ```sql
//! -- node 0 of 3 runs:
//! SELECT count(*) FROM events WHERE lessdb_shard(any_column, 0, 3);
//! ```
//!
//! The first argument is any table column (referencing a column is what
//! makes DataFusion push the predicate to the table provider). The provider
//! strips the marker before pruning, so it costs nothing per row.

use std::sync::Arc;

use datafusion::arrow::datatypes::DataType;
use datafusion::common::Result as DFResult;
use datafusion::logical_expr::{
    ColumnarValue, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, Volatility,
};
use datafusion::scalar::ScalarValue;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShardUdf {
    signature: Signature,
}

impl Default for ShardUdf {
    fn default() -> Self {
        Self::new()
    }
}

impl ShardUdf {
    pub fn new() -> Self {
        Self {
            // Immutable: with a column argument the marker can never be
            // constant-folded away, and *non-volatile* predicates are what
            // DataFusion pushes down to table providers (volatile ones are
            // never pushed — see push_down_filter.rs). The provider strips
            // the marker before row filtering, so its always-true result
            // costs nothing.
            // Three args: any table column (keeps the predicate pushable to
            // the table provider), then the shard index and shard count.
            signature: Signature::any(3, Volatility::Immutable),
        }
    }
}

impl ScalarUDFImpl for ShardUdf {
    fn name(&self) -> &str {
        "lessdb_shard"
    }

    fn signature(&self) -> &Signature {
        &self.signature
    }

    fn return_type(&self, _arg_types: &[DataType]) -> DFResult<DataType> {
        Ok(DataType::Boolean)
    }

    fn invoke_with_args(&self, _args: ScalarFunctionArgs) -> DFResult<ColumnarValue> {
        Ok(ColumnarValue::Scalar(ScalarValue::Boolean(Some(true))))
    }
}

/// The registered UDF (an always-true marker; the provider interprets it).
pub fn shard_udf() -> ScalarUDF {
    ScalarUDF::new_from_shared_impl(Arc::new(ShardUdf::new()))
}

/// FNV-1a 64-bit over the part name — stable across runs and platforms,
/// so every node assigns the same part to the same shard.
pub fn shard_of(name: &str, n: u64) -> u64 {
    debug_assert!(n > 0);
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h % n
}
