//! Data-part metadata.
//!
//! [`PartMeta`] is the sidecar JSON stored next to every part's
//! `data.parquet`. It carries everything the query planner needs to prune
//! parts *without* touching the data file: per-column statistics and bloom
//! filters.
//!
//! Statistics use [`StatValue`], a small serde-friendly, *typed* value
//! wrapper. Typed comparisons are the key correctness property here: pruning
//! never compares strings when the values are numbers, so `9 < 10` cannot be
//! misjudged by lexicographic string ordering.

use std::cmp::Ordering;
use std::fmt;

use arrow::array::{
    Array, BooleanArray, Date32Array, Float32Array, Float64Array, Int8Array, Int16Array,
    Int32Array, Int64Array, LargeStringArray, StringArray, StringViewArray,
    TimestampMicrosecondArray, TimestampMillisecondArray, TimestampNanosecondArray,
    TimestampSecondArray, UInt8Array, UInt16Array, UInt32Array, UInt64Array,
};
use arrow::datatypes::{DataType, TimeUnit};
use serde::{Deserialize, Serialize};

/// A typed scalar value used for part statistics and pruning.
///
/// `Null` doubles as "unknown" for types we don't track yet; comparisons
/// involving `Null` return `None`, which disables pruning (the safe choice).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StatValue {
    Null,
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    Str(String),
    Date32(i32),
    Timestamp { value: i64, unit: String },
}

impl StatValue {
    /// Extract the value at index `i` of an array (assumes `!arr.is_null(i)`).
    pub fn from_array_value(arr: &dyn Array, i: usize) -> StatValue {
        match arr.data_type() {
            DataType::Boolean => StatValue::Bool(
                arr.as_any()
                    .downcast_ref::<BooleanArray>()
                    .unwrap()
                    .value(i),
            ),
            DataType::Int8 => {
                StatValue::I64(arr.as_any().downcast_ref::<Int8Array>().unwrap().value(i) as i64)
            }
            DataType::Int16 => {
                StatValue::I64(arr.as_any().downcast_ref::<Int16Array>().unwrap().value(i) as i64)
            }
            DataType::Int32 => {
                StatValue::I64(arr.as_any().downcast_ref::<Int32Array>().unwrap().value(i) as i64)
            }
            DataType::Int64 => {
                StatValue::I64(arr.as_any().downcast_ref::<Int64Array>().unwrap().value(i))
            }
            DataType::UInt8 => {
                StatValue::U64(arr.as_any().downcast_ref::<UInt8Array>().unwrap().value(i) as u64)
            }
            DataType::UInt16 => {
                StatValue::U64(arr.as_any().downcast_ref::<UInt16Array>().unwrap().value(i) as u64)
            }
            DataType::UInt32 => {
                StatValue::U64(arr.as_any().downcast_ref::<UInt32Array>().unwrap().value(i) as u64)
            }
            DataType::UInt64 => {
                StatValue::U64(arr.as_any().downcast_ref::<UInt64Array>().unwrap().value(i))
            }
            DataType::Float32 => StatValue::F64(
                arr.as_any()
                    .downcast_ref::<Float32Array>()
                    .unwrap()
                    .value(i) as f64,
            ),
            DataType::Float64 => StatValue::F64(
                arr.as_any()
                    .downcast_ref::<Float64Array>()
                    .unwrap()
                    .value(i),
            ),
            DataType::Utf8 => StatValue::Str(
                arr.as_any()
                    .downcast_ref::<StringArray>()
                    .unwrap()
                    .value(i)
                    .to_string(),
            ),
            DataType::LargeUtf8 => StatValue::Str(
                arr.as_any()
                    .downcast_ref::<LargeStringArray>()
                    .unwrap()
                    .value(i)
                    .to_string(),
            ),
            DataType::Utf8View => StatValue::Str(
                arr.as_any()
                    .downcast_ref::<StringViewArray>()
                    .unwrap()
                    .value(i)
                    .to_string(),
            ),
            DataType::Date32 => {
                StatValue::Date32(arr.as_any().downcast_ref::<Date32Array>().unwrap().value(i))
            }
            DataType::Timestamp(TimeUnit::Second, _) => StatValue::Timestamp {
                value: arr
                    .as_any()
                    .downcast_ref::<TimestampSecondArray>()
                    .unwrap()
                    .value(i),
                unit: "s".into(),
            },
            DataType::Timestamp(TimeUnit::Millisecond, _) => StatValue::Timestamp {
                value: arr
                    .as_any()
                    .downcast_ref::<TimestampMillisecondArray>()
                    .unwrap()
                    .value(i),
                unit: "ms".into(),
            },
            DataType::Timestamp(TimeUnit::Microsecond, _) => StatValue::Timestamp {
                value: arr
                    .as_any()
                    .downcast_ref::<TimestampMicrosecondArray>()
                    .unwrap()
                    .value(i),
                unit: "us".into(),
            },
            DataType::Timestamp(TimeUnit::Nanosecond, _) => StatValue::Timestamp {
                value: arr
                    .as_any()
                    .downcast_ref::<TimestampNanosecondArray>()
                    .unwrap()
                    .value(i),
                unit: "ns".into(),
            },
            // Unsupported types become Null ("unknown") and simply don't
            // participate in pruning.
            _ => StatValue::Null,
        }
    }

    /// Total order across comparable, same-type pairs; `None` when the two
    /// values are not comparable (different types, different timestamp
    /// units, or either side is unknown).
    pub fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (StatValue::Bool(a), StatValue::Bool(b)) => a.partial_cmp(b),
            (StatValue::I64(a), StatValue::I64(b)) => a.partial_cmp(b),
            (StatValue::U64(a), StatValue::U64(b)) => a.partial_cmp(b),
            (StatValue::F64(a), StatValue::F64(b)) => a.partial_cmp(b),
            (StatValue::Str(a), StatValue::Str(b)) => a.partial_cmp(b),
            (StatValue::Date32(a), StatValue::Date32(b)) => a.partial_cmp(b),
            (
                StatValue::Timestamp { value: a, unit: au },
                StatValue::Timestamp { value: b, unit: bu },
            ) if au == bu => a.partial_cmp(b),
            _ => None,
        }
    }

    /// Canonical byte encoding, used to feed bloom filters. Encoding must be
    /// consistent between writers (array values) and readers (query
    /// literals) — both sides go through `StatValue`, which guarantees it.
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            StatValue::Null => vec![0x00],
            StatValue::Bool(b) => vec![*b as u8],
            StatValue::I64(v) => v.to_le_bytes().to_vec(),
            StatValue::U64(v) => v.to_le_bytes().to_vec(),
            StatValue::F64(v) => v.to_le_bytes().to_vec(),
            StatValue::Str(s) => s.as_bytes().to_vec(),
            StatValue::Date32(v) => v.to_le_bytes().to_vec(),
            StatValue::Timestamp { value, unit } => {
                let mut b = value.to_le_bytes().to_vec();
                b.extend_from_slice(unit.as_bytes());
                b
            }
        }
    }

    /// Render as a JSON value for describe/introspection output.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            StatValue::Null => serde_json::Value::Null,
            StatValue::Bool(b) => serde_json::Value::Bool(*b),
            StatValue::I64(v) => serde_json::json!(v),
            StatValue::U64(v) => serde_json::json!(v),
            StatValue::F64(v) => serde_json::json!(v),
            StatValue::Str(s) => serde_json::json!(s),
            StatValue::Date32(v) => serde_json::json!(v),
            StatValue::Timestamp { value, unit } => {
                serde_json::json!({ "value": value, "unit": unit })
            }
        }
    }
}

impl fmt::Display for StatValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StatValue::Null => write!(f, "null"),
            StatValue::Bool(b) => write!(f, "{b}"),
            StatValue::I64(v) => write!(f, "{v}"),
            StatValue::U64(v) => write!(f, "{v}"),
            StatValue::F64(v) => write!(f, "{v}"),
            StatValue::Str(s) => write!(f, "{s}"),
            StatValue::Date32(v) => write!(f, "{v}"),
            StatValue::Timestamp { value, unit } => write!(f, "{value}{unit}"),
        }
    }
}

/// Per-column statistics stored in part metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnStats {
    pub name: String,
    /// Arrow type name (for introspection).
    pub ty: String,
    pub null_count: u64,
    /// Minimum value seen in the part (unknown types: `None`).
    pub min: Option<StatValue>,
    /// Maximum value seen in the part (unknown types: `None`).
    pub max: Option<StatValue>,
    /// Base64-encoded bloom filter of this column's values, present for
    /// uniqueness-constraint columns.
    pub bloom: Option<String>,
    /// Does this (float) column contain NaN? NaN is invisible to min/max
    /// but *is* ordered above everything by the query engine's comparison
    /// semantics, so pruning must treat such parts conservatively.
    #[serde(default)]
    pub has_nan: bool,
}

impl ColumnStats {
    /// Compute stats for a column array.
    pub fn compute(name: &str, arr: &dyn Array) -> ColumnStats {
        let mut null_count = 0u64;
        let mut min: Option<StatValue> = None;
        let mut max: Option<StatValue> = None;
        let mut has_nan = false;
        for i in 0..arr.len() {
            if arr.is_null(i) {
                null_count += 1;
                continue;
            }
            let v = StatValue::from_array_value(arr, i);
            if matches!(v, StatValue::F64(x) if x.is_nan()) {
                // NaN compares unordered with everything: min/max skip it
                // (Arrow's min/max do too), but its presence matters for
                // pruning soundness.
                has_nan = true;
                continue;
            }
            min = Some(match &min {
                None => v.clone(),
                Some(m) => match m.partial_cmp(&v) {
                    Some(Ordering::Greater) => v.clone(),
                    _ => m.clone(),
                },
            });
            max = Some(match &max {
                None => v.clone(),
                Some(m) => match m.partial_cmp(&v) {
                    Some(Ordering::Less) => v.clone(),
                    _ => m.clone(),
                },
            });
        }
        ColumnStats {
            name: name.to_string(),
            ty: format!("{:?}", arr.data_type()),
            null_count,
            min,
            max,
            bloom: None,
            has_nan,
        }
    }

    /// Base64-decode the bloom filter, if present.
    pub fn bloom_bytes(&self) -> Option<Vec<u8>> {
        use base64::Engine;
        self.bloom
            .as_ref()
            .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok())
    }
}

/// Metadata for one immutable data part.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartMeta {
    /// Stable unique part id (uuid).
    pub id: String,
    /// Part name: `{ts_ms}_{uuid8}_{rows}_{level}`.
    pub name: String,
    /// Owning table name.
    pub table: String,
    /// Total rows in the part.
    pub row_count: u64,
    /// RFC-3339 creation timestamp.
    pub created_at: String,
    /// Compression codec used for this part.
    pub compression: String,
    /// Sort-key columns (in order).
    pub sort_key: Vec<String>,
    /// Uniqueness-constraint columns (a prefix of the sort key).
    pub unique: Vec<String>,
    /// Per-column statistics.
    pub columns: Vec<ColumnStats>,
    /// Highest WAL sequence number whose rows are included in this part
    /// (set when written from a WAL-buffered flush). Recovery skips WAL
    /// records `<=` this value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wal_lsn_max: Option<u64>,
}

impl PartMeta {
    /// Generate a part name from its ingredients.
    pub fn new_part_name(ts_ms: i64, uuid_short: &str, rows: u64, level: u32) -> String {
        format!("{ts_ms}_{uuid_short}_{rows}_{level}")
    }

    /// Look up a column's stats.
    pub fn column(&self, name: &str) -> Option<&ColumnStats> {
        self.columns.iter().find(|c| c.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use arrow::array::{Int64Array, StringArray};

    #[test]
    fn typed_comparisons_are_not_lexicographic() {
        let nine = StatValue::I64(9);
        let ten = StatValue::I64(10);
        assert_eq!(nine.partial_cmp(&ten), Some(Ordering::Less));
        // The string versions order differently — which is exactly why
        // pruning must never compare strings for numbers.
        assert_eq!(
            StatValue::Str("9".into()).partial_cmp(&StatValue::Str("10".into())),
            Some(Ordering::Greater)
        );
    }

    #[test]
    fn incomparable_pairs_return_none() {
        assert_eq!(StatValue::I64(1).partial_cmp(&StatValue::F64(1.0)), None);
        assert_eq!(StatValue::Null.partial_cmp(&StatValue::I64(1)), None);
        assert_eq!(
            StatValue::Timestamp {
                value: 1,
                unit: "ms".into()
            }
            .partial_cmp(&StatValue::Timestamp {
                value: 1000,
                unit: "us".into()
            }),
            None
        );
    }

    #[test]
    fn stats_compute_min_max_and_nulls() {
        let arr = Int64Array::from(vec![Some(3), None, Some(1), Some(9)]);
        let stats = ColumnStats::compute("x", &arr);
        assert_eq!(stats.null_count, 1);
        assert_eq!(stats.min, Some(StatValue::I64(1)));
        assert_eq!(stats.max, Some(StatValue::I64(9)));

        let s = StringArray::from(vec!["b", "a", "c"]);
        let stats = ColumnStats::compute("s", &s);
        assert_eq!(stats.min, Some(StatValue::Str("a".into())));
        assert_eq!(stats.max, Some(StatValue::Str("c".into())));
    }
}
