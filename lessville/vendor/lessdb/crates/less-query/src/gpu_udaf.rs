//! GPU-accelerated SQL aggregates with automatic CPU fallback.
//!
//! The existing [`less_gpu::GpuDevice`] wgpu kernels (a masked `filtered_sum`
//! and a `dot` product) are exposed to SQL as two real query-plan aggregate
//! functions (UDAFs):
//!
//! * `gpu_filtered_sum(x)` — `SUM(x)` over the non-null rows of `x`
//! * `gpu_dot(x, y)` — `SUM(x * y)` over the rows where both are non-null
//!
//! Both return `Float64`. They are registered on every [`LessSession`]
//! (`register_udaf`), but the GPU is only a fast path, never a correctness
//! requirement. A batch is dispatched to the GPU only when *all* of:
//!
//! 1. `engine.config.gpu_enabled` is `true` and a device initialized at
//!    session creation (an unavailable adapter / driver / init timeout yields
//!    `None`, which disables the GPU path for the whole session);
//! 2. the input column is `Float64` (the kernels are f32, so `Float64` values
//!    are widened-down to f32 for the kernel and the f32 result widened back;
//!    `Float32` and everything else runs on the CPU);
//! 3. the batch has at least [`GPU_MIN_ROWS`] rows, so tiny batches never pay
//!    the buffer round-trip.
//!
//! On *any* GPU error the accumulator flips a local `gpu_ok` flag and falls
//! back to plain CPU accumulation for that batch and every later batch of the
//! same accumulator, so a lost device cannot fail a query. Because each
//! accumulator owns its fallback decision and the partial state is just a
//! running `f64` sum, the design is single-node: partial results from multiple
//! partitions merge on the CPU exactly like `SUM`.

use std::sync::Arc;
use std::time::Duration;

use arrow::array::{Array, ArrayRef, Float32Array, Float64Array};
use arrow::datatypes::{DataType, Field};
use datafusion::error::{DataFusionError, Result as DFResult};
use datafusion::logical_expr::{
    Accumulator, AccumulatorFactoryFunction, AggregateUDF, Signature, SimpleAggregateUDF,
    Volatility,
};
use datafusion::scalar::ScalarValue;

use less_gpu::GpuDevice;

/// Minimum rows in a single input batch before the GPU path is attempted.
/// Below this the buffer upload/download round-trip costs more than a CPU sum.
pub const GPU_MIN_ROWS: usize = 4096;

/// How long to wait for GPU adapter/device initialization before giving up
/// and falling back to CPU for the lifetime of the session.
const GPU_INIT_TIMEOUT: Duration = Duration::from_secs(5);

/// Initialize the GPU device once per session. Returns `None` when the GPU is
/// disabled, no adapter is present, initialization times out, or the driver
/// errors — callers then take the CPU path unconditionally.
pub async fn init_gpu(gpu_enabled: bool) -> Arc<Option<GpuDevice>> {
    if !gpu_enabled {
        return Arc::new(None);
    }
    match tokio::time::timeout(GPU_INIT_TIMEOUT, GpuDevice::new()).await {
        Ok(Ok(device)) => Arc::new(Some(device)),
        _ => Arc::new(None),
    }
}

/// `gpu_filtered_sum(x)`: `SUM(x)` over non-null rows, `Float64` result.
pub fn filtered_sum_udaf(device: Arc<Option<GpuDevice>>) -> AggregateUDF {
    let accumulator: AccumulatorFactoryFunction =
        Arc::new(move |_args| Ok(Box::new(GpuFilteredSum::new(device.clone()))));
    AggregateUDF::from(SimpleAggregateUDF::new_with_signature(
        "gpu_filtered_sum",
        Signature::uniform(
            1,
            vec![DataType::Float64, DataType::Float32],
            Volatility::Immutable,
        ),
        DataType::Float64,
        accumulator,
        vec![Arc::new(Field::new("sum", DataType::Float64, true))],
    ))
}

/// `gpu_dot(x, y)`: `SUM(x * y)` over rows where both are non-null.
pub fn dot_udaf(device: Arc<Option<GpuDevice>>) -> AggregateUDF {
    let accumulator: AccumulatorFactoryFunction =
        Arc::new(move |_args| Ok(Box::new(GpuDot::new(device.clone()))));
    AggregateUDF::from(SimpleAggregateUDF::new_with_signature(
        "gpu_dot",
        Signature::uniform(
            2,
            vec![DataType::Float64, DataType::Float32],
            Volatility::Immutable,
        ),
        DataType::Float64,
        accumulator,
        vec![Arc::new(Field::new("sum", DataType::Float64, true))],
    ))
}

/// Shared state for both GPU aggregates: a running `f64` sum (`None` until the
/// first non-null value, so empty/all-null input evaluates to SQL `NULL` like
/// `SUM`) plus the per-accumulator GPU-failure latch.
struct GpuRunningSum {
    device: Arc<Option<GpuDevice>>,
    gpu_ok: bool,
    sum: Option<f64>,
}

impl std::fmt::Debug for GpuRunningSum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `GpuDevice` holds wgpu handles with no `Debug`, so print only the
        // availability booleans rather than the device itself.
        f.debug_struct("GpuRunningSum")
            .field("gpu_available", &self.device.is_some())
            .field("gpu_ok", &self.gpu_ok)
            .field("sum", &self.sum)
            .finish()
    }
}

impl GpuRunningSum {
    fn new(device: Arc<Option<GpuDevice>>) -> Self {
        let gpu_ok = device.is_some();
        Self {
            device,
            gpu_ok,
            sum: None,
        }
    }

    /// Accumulate a CPU-computed (or GPU-computed) `f64` partial into the sum.
    fn add(&mut self, v: f64) {
        *self.sum.get_or_insert(0.0) += v;
    }

    fn add_optional(&mut self, v: Option<f64>) {
        if let Some(v) = v {
            self.add(v);
        }
    }

    /// Merge an intermediate state array (a `Float64Array` of partial sums,
    /// `NULL` = an empty partial) — used by multi-phase grouping.
    fn merge_state(&mut self, states: &[ArrayRef]) -> DFResult<()> {
        let state = states[0]
            .as_any()
            .downcast_ref::<Float64Array>()
            .ok_or_else(|| {
                DataFusionError::Internal(format!(
                    "gpu aggregate: expected Float64 state, got {:?}",
                    states[0].data_type()
                ))
            })?;
        self.add_optional(arrow::compute::sum(state));
        Ok(())
    }
}

/// `gpu_filtered_sum(x)` accumulator.
#[derive(Debug)]
struct GpuFilteredSum {
    inner: GpuRunningSum,
}

impl GpuFilteredSum {
    fn new(device: Arc<Option<GpuDevice>>) -> Self {
        Self {
            inner: GpuRunningSum::new(device),
        }
    }
}

impl Accumulator for GpuFilteredSum {
    fn update_batch(&mut self, values: &[ArrayRef]) -> DFResult<()> {
        let arr = &values[0];

        // GPU fast path: Float64 only, batch at/above the threshold, and the
        // GPU has not already failed in this accumulator.
        if self.inner.gpu_ok
            && let Some(device) = self.inner.device.as_ref()
            && let Some(a) = arr.as_any().downcast_ref::<Float64Array>()
            && a.len() >= GPU_MIN_ROWS
        {
            match gpu_sum_f64(device, a) {
                Some(v) => {
                    self.inner.add(v);
                    return Ok(());
                }
                None => self.inner.gpu_ok = false,
            }
        }

        // CPU fallback (SQL `SUM` semantics: ignore nulls).
        let v = if let Some(a) = arr.as_any().downcast_ref::<Float64Array>() {
            arrow::compute::sum(a)
        } else if let Some(a) = arr.as_any().downcast_ref::<Float32Array>() {
            arrow::compute::sum(a).map(|v| v as f64)
        } else {
            return Err(DataFusionError::Internal(format!(
                "gpu_filtered_sum: unsupported input type {:?}",
                arr.data_type()
            )));
        };
        self.inner.add_optional(v);
        Ok(())
    }

    fn merge_batch(&mut self, states: &[ArrayRef]) -> DFResult<()> {
        self.inner.merge_state(states)
    }

    fn evaluate(&mut self) -> DFResult<ScalarValue> {
        Ok(ScalarValue::Float64(self.inner.sum))
    }

    fn state(&mut self) -> DFResult<Vec<ScalarValue>> {
        Ok(vec![self.evaluate()?])
    }

    fn size(&self) -> usize {
        std::mem::size_of_val(self)
    }
}

/// `gpu_dot(x, y)` accumulator.
#[derive(Debug)]
struct GpuDot {
    inner: GpuRunningSum,
}

impl GpuDot {
    fn new(device: Arc<Option<GpuDevice>>) -> Self {
        Self {
            inner: GpuRunningSum::new(device),
        }
    }
}

impl Accumulator for GpuDot {
    fn update_batch(&mut self, values: &[ArrayRef]) -> DFResult<()> {
        let a = &values[0];
        let b = &values[1];

        // GPU fast path: both Float64, batch at/above the threshold.
        if self.inner.gpu_ok
            && let Some(device) = self.inner.device.as_ref()
            && let Some(x) = a.as_any().downcast_ref::<Float64Array>()
            && let Some(y) = b.as_any().downcast_ref::<Float64Array>()
            && x.len() >= GPU_MIN_ROWS
        {
            match gpu_dot_f64(device, x, y) {
                Some(v) => {
                    self.inner.add(v);
                    return Ok(());
                }
                None => self.inner.gpu_ok = false,
            }
        }

        // CPU fallback: pair rows only where both sides are non-null.
        let v = if let (Some(x), Some(y)) = (
            a.as_any().downcast_ref::<Float64Array>(),
            b.as_any().downcast_ref::<Float64Array>(),
        ) {
            cpu_dot_f64(x, y)
        } else if let (Some(x), Some(y)) = (
            a.as_any().downcast_ref::<Float32Array>(),
            b.as_any().downcast_ref::<Float32Array>(),
        ) {
            cpu_dot_f32(x, y)
        } else {
            return Err(DataFusionError::Internal(format!(
                "gpu_dot: unsupported input types {:?}, {:?}",
                a.data_type(),
                b.data_type()
            )));
        };
        self.inner.add_optional(v);
        Ok(())
    }

    fn merge_batch(&mut self, states: &[ArrayRef]) -> DFResult<()> {
        self.inner.merge_state(states)
    }

    fn evaluate(&mut self) -> DFResult<ScalarValue> {
        Ok(ScalarValue::Float64(self.inner.sum))
    }

    fn state(&mut self) -> DFResult<Vec<ScalarValue>> {
        Ok(vec![self.evaluate()?])
    }

    fn size(&self) -> usize {
        std::mem::size_of_val(self)
    }
}

/// Run the GPU filtered-sum kernel over a `Float64Array`. Non-null slots are
/// widened down to f32 and masked `true`; null slots contribute nothing.
fn gpu_sum_f64(device: &GpuDevice, a: &Float64Array) -> Option<f64> {
    let n = a.len();
    let mut values = Vec::with_capacity(n);
    let mut mask = Vec::with_capacity(n);
    for i in 0..n {
        let valid = a.is_valid(i);
        values.push(if valid { a.value(i) as f32 } else { 0.0 });
        mask.push(valid);
    }
    device
        .filtered_sum_f32(&values, &mask)
        .ok()
        .map(|v| v as f64)
}

/// Run the GPU dot kernel over two `Float64Array`s. Slots where either side is
/// null are zeroed on both sides so they contribute `0.0` to the product.
fn gpu_dot_f64(device: &GpuDevice, a: &Float64Array, b: &Float64Array) -> Option<f64> {
    let n = a.len().min(b.len());
    let mut x = Vec::with_capacity(n);
    let mut y = Vec::with_capacity(n);
    for i in 0..n {
        let valid = a.is_valid(i) && b.is_valid(i);
        x.push(if valid { a.value(i) as f32 } else { 0.0 });
        y.push(if valid { b.value(i) as f32 } else { 0.0 });
    }
    device.dot_f32(&x, &y).ok().map(|v| v as f64)
}

/// CPU dot product over two `Float64Array`s, pairing only rows where both are
/// non-null. Returns `None` when no valid pair exists (SQL `NULL`).
fn cpu_dot_f64(a: &Float64Array, b: &Float64Array) -> Option<f64> {
    let n = a.len().min(b.len());
    let mut sum = 0.0f64;
    let mut seen = false;
    for i in 0..n {
        if a.is_valid(i) && b.is_valid(i) {
            sum += a.value(i) * b.value(i);
            seen = true;
        }
    }
    seen.then_some(sum)
}

/// CPU dot product over two `Float32Array`s, widened to f64 for accumulation.
fn cpu_dot_f32(a: &Float32Array, b: &Float32Array) -> Option<f64> {
    let n = a.len().min(b.len());
    let mut sum = 0.0f64;
    let mut seen = false;
    for i in 0..n {
        if a.is_valid(i) && b.is_valid(i) {
            sum += a.value(i) as f64 * b.value(i) as f64;
            seen = true;
        }
    }
    seen.then_some(sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtered_sum_cpu_ignores_nulls_and_handles_empty() {
        let mut acc = GpuFilteredSum::new(Arc::new(None));
        let arr: ArrayRef = Arc::new(Float64Array::from(vec![
            Some(1.0),
            None,
            Some(2.0),
            Some(-0.5),
        ]));
        acc.update_batch(&[arr]).unwrap();
        assert_eq!(acc.evaluate().unwrap(), ScalarValue::Float64(Some(2.5)));

        // All-null input behaves like `SUM`: NULL.
        let mut acc = GpuFilteredSum::new(Arc::new(None));
        let arr: ArrayRef = Arc::new(Float64Array::from(vec![None::<f64>, None::<f64>]));
        acc.update_batch(&[arr]).unwrap();
        assert_eq!(acc.evaluate().unwrap(), ScalarValue::Float64(None));

        // Empty input: NULL.
        let mut acc = GpuFilteredSum::new(Arc::new(None));
        let arr: ArrayRef = Arc::new(Float64Array::from(Vec::<f64>::new()));
        acc.update_batch(&[arr]).unwrap();
        assert_eq!(acc.evaluate().unwrap(), ScalarValue::Float64(None));
    }

    #[test]
    fn filtered_sum_cpu_float32() {
        let mut acc = GpuFilteredSum::new(Arc::new(None));
        let arr: ArrayRef = Arc::new(Float32Array::from(vec![Some(1.5f32), None, Some(2.5f32)]));
        acc.update_batch(&[arr]).unwrap();
        assert_eq!(acc.evaluate().unwrap(), ScalarValue::Float64(Some(4.0)));
    }

    #[test]
    fn dot_cpu_pairs_valid_rows() {
        let mut acc = GpuDot::new(Arc::new(None));
        let x: ArrayRef = Arc::new(Float64Array::from(vec![
            Some(2.0),
            None,
            Some(3.0),
            Some(-1.0),
        ]));
        let y: ArrayRef = Arc::new(Float64Array::from(vec![
            Some(1.5),
            Some(10.0),
            None,
            Some(4.0),
        ]));
        acc.update_batch(&[x, y]).unwrap();
        // 2.0*1.5 + (-1.0)*4.0 = 3.0 - 4.0 = -1.0 (null rows skipped).
        assert_eq!(acc.evaluate().unwrap(), ScalarValue::Float64(Some(-1.0)));
    }

    #[test]
    fn merge_state_sums_partials() {
        let mut acc = GpuFilteredSum::new(Arc::new(None));
        let partial: ArrayRef = Arc::new(Float64Array::from(vec![Some(1.0), None, Some(2.0)]));
        acc.merge_batch(&[partial]).unwrap();
        assert_eq!(acc.evaluate().unwrap(), ScalarValue::Float64(Some(3.0)));
    }
}
