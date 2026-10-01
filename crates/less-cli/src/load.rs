//! Data loading: CSV and JSONL ingestion into Arrow record batches.
//!
//! Both loaders funnel through the Arrow JSON reader, which converts typed
//! JSON values (numbers, strings, booleans) into the table's Arrow schema —
//! robust type coercion without hand-rolled parsing per type.

use std::path::Path;

use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use less_catalog::{TableDef, TypeSpec};
use less_common::{LessError, Result};

/// Load a CSV file into record batches matching the table schema.
/// The CSV header row must contain every table column (any order).
pub fn load_csv(def: &TableDef, path: &Path, batch_rows: usize) -> Result<Vec<RecordBatch>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(path)
        .map_err(|e| LessError::Config(format!("failed to open CSV: {e}")))?;
    let headers = rdr
        .headers()
        .map_err(|e| LessError::Config(format!("failed to read CSV header: {e}")))?
        .clone();
    let schema = def.arrow_schema();

    // Map table columns -> CSV column positions.
    let mut positions = Vec::with_capacity(schema.fields().len());
    for f in schema.fields() {
        let pos = headers
            .iter()
            .position(|h| h.trim() == f.name().as_str())
            .ok_or_else(|| {
                LessError::Config(format!(
                    "CSV header is missing column '{}' (have: {})",
                    f.name(),
                    headers.iter().collect::<Vec<_>>().join(", ")
                ))
            })?;
        positions.push(pos);
    }

    let mut rows: Vec<serde_json::Value> = Vec::new();
    for record in rdr.records() {
        let record = record.map_err(|e| LessError::Config(format!("bad CSV row: {e}")))?;
        let mut map = serde_json::Map::new();
        for (fi, f) in schema.fields().iter().enumerate() {
            let raw = &record[positions[fi]];
            let ty = type_of_field(def, f.name().as_str());
            map.insert(f.name().to_string(), parse_csv_value(raw, ty));
        }
        rows.push(serde_json::Value::Object(map));
    }
    batches_from_json(schema, &serde_json::to_string(&rows)?, batch_rows)
}

/// Stream a Parquet file batch-by-batch (row-group sized chunks, capped
/// at `batch_rows`), so arbitrarily large files load in bounded memory.
/// The caller inserts each batch as it arrives (the engine auto-flushes
/// its buffer, keeping the working set at one part).
pub fn load_parquet_stream(
    def: &TableDef,
    path: &Path,
    batch_rows: usize,
) -> Result<Box<dyn Iterator<Item = Result<RecordBatch>>>> {
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    let file = std::fs::File::open(path)?;
    let reader = ParquetRecordBatchReaderBuilder::try_new(file)
        .map_err(LessError::Parquet)?
        .with_batch_size(batch_rows)
        .build()
        .map_err(LessError::Parquet)?;
    let schema = def.arrow_schema();
    Ok(Box::new(reader.map(move |b| {
        let batch = b.map_err(LessError::Arrow)?;
        less_engine::align_batch(&schema, batch)
    })))
}

/// Load a (small) Parquet file fully into record batches matching the
/// table schema. Large files should use [`load_parquet_stream`].
#[allow(dead_code)]
pub fn load_parquet(def: &TableDef, path: &Path) -> Result<Vec<RecordBatch>> {
    let bytes = std::fs::read(path)?;
    less_storage::read_part_from_bytes(bytes.into(), None).map(|batches| {
        // Reorder to the table schema (storage readers preserve file
        // order; align_batch validates).
        let schema = def.arrow_schema();
        batches
            .into_iter()
            .map(|b| less_engine::align_batch(&schema, b))
            .collect::<Result<Vec<_>>>()
    })?
}

/// Load an Arrow IPC stream file into record batches matching the table
/// schema.
pub fn load_arrow(def: &TableDef, path: &Path) -> Result<Vec<RecordBatch>> {
    let file = std::fs::File::open(path)?;
    let reader = arrow::ipc::reader::StreamReader::try_new(file, None)
        .map_err(less_common::LessError::Arrow)?;
    let schema = def.arrow_schema();
    reader
        .map(|b| {
            let batch = b.map_err(less_common::LessError::Arrow)?;
            less_engine::align_batch(&schema, batch)
        })
        .collect()
}

/// Load a JSONL (or JSON array) file into record batches matching the
/// table schema.
pub fn load_jsonl(def: &TableDef, path: &Path, batch_rows: usize) -> Result<Vec<RecordBatch>> {
    let json = std::fs::read_to_string(path)?;
    batches_from_json(def.arrow_schema(), &json, batch_rows)
}

/// Convert JSON text (a JSON array of objects, or NDJSON) into typed record
/// batches using the Arrow JSON reader. arrow-json 59 accepts NDJSON only,
/// so arrays are converted to line-delimited form first.
pub fn batches_from_json(
    schema: SchemaRef,
    json: &str,
    batch_rows: usize,
) -> Result<Vec<RecordBatch>> {
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
        .with_batch_size(batch_rows)
        .build(std::io::Cursor::new(ndjson.into_bytes()))
        .map_err(LessError::Arrow)?;
    let batches: Vec<RecordBatch> = reader.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(batches)
}

fn type_of_field(def: &TableDef, name: &str) -> TypeSpec {
    def.schema
        .field(name)
        .map(|f| f.ty.clone())
        .unwrap_or(TypeSpec::Utf8)
}

/// Coerce a raw CSV string cell into a typed JSON value.
/// Empty cells and `\N`/`NULL` become JSON null.
fn parse_csv_value(raw: &str, ty: TypeSpec) -> serde_json::Value {
    let s = raw.trim();
    if s.is_empty() || s == "\\N" || s.eq_ignore_ascii_case("null") {
        return serde_json::Value::Null;
    }
    match ty {
        TypeSpec::Int8 | TypeSpec::Int16 | TypeSpec::Int32 | TypeSpec::Int64 => {
            match s.parse::<i64>() {
                Ok(v) => serde_json::json!(v),
                Err(_) => serde_json::Value::Null,
            }
        }
        TypeSpec::UInt8 | TypeSpec::UInt16 | TypeSpec::UInt32 | TypeSpec::UInt64 => {
            match s.parse::<u64>() {
                Ok(v) => serde_json::json!(v),
                Err(_) => serde_json::Value::Null,
            }
        }
        TypeSpec::Float32 | TypeSpec::Float64 => match s.parse::<f64>() {
            Ok(v) => serde_json::json!(v),
            Err(_) => serde_json::Value::Null,
        },
        TypeSpec::Bool => match s.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => serde_json::json!(true),
            "false" | "0" | "no" => serde_json::json!(false),
            _ => serde_json::Value::Null,
        },
        // Strings, dates, timestamps and UUIDs pass through as text; the
        // Arrow JSON reader parses them per the schema. UUIDs are validated.
        TypeSpec::Utf8
        | TypeSpec::Date32
        | TypeSpec::TimestampMs
        | TypeSpec::TimestampUs
        | TypeSpec::TimestampNs => {
            serde_json::json!(s)
        }
        TypeSpec::Uuid => match valid_uuid(s) {
            true => serde_json::json!(s),
            false => serde_json::Value::Null,
        },
        // Decimals are plain numbers; the JSON reader coerces to the
        // declared precision/scale.
        TypeSpec::Decimal { .. } => match s.parse::<f64>() {
            Ok(v) => serde_json::json!(v),
            Err(_) => serde_json::Value::Null,
        },
        // Array/Map cells are JSON values themselves (e.g. "[1,2,3]").
        TypeSpec::Array(_) | TypeSpec::Map(_, _) => {
            serde_json::from_str(s).unwrap_or(serde_json::Value::Null)
        }
    }
}

/// 8-4-4-4-12 hex UUID format check.
fn valid_uuid(s: &str) -> bool {
    let bytes = s.as_bytes();
    let dash = |i: usize| bytes.get(i) == Some(&b'-');
    let hex = |i: usize| matches!(bytes.get(i), Some(b) if b.is_ascii_hexdigit());
    bytes.len() == 36
        && hex(0)
        && hex(1)
        && hex(2)
        && hex(3)
        && hex(4)
        && hex(5)
        && hex(6)
        && hex(7)
        && dash(8)
        && hex(9)
        && hex(10)
        && hex(11)
        && hex(12)
        && dash(13)
        && hex(14)
        && hex(15)
        && hex(16)
        && hex(17)
        && dash(18)
        && hex(19)
        && hex(20)
        && hex(21)
        && hex(22)
        && dash(23)
        && hex(24)
        && hex(25)
        && hex(26)
        && hex(27)
        && hex(28)
        && hex(29)
        && hex(30)
        && hex(31)
        && hex(32)
        && hex(33)
        && hex(34)
        && hex(35)
}

#[cfg(test)]
mod tests {
    use super::*;
    use less_catalog::{EngineKind, FieldSpec, SchemaSpec};

    fn def() -> TableDef {
        TableDef::new(
            "t",
            SchemaSpec {
                fields: vec![
                    FieldSpec::new("id", TypeSpec::Int64),
                    FieldSpec::new("name", TypeSpec::Utf8),
                    FieldSpec::new("amount", TypeSpec::Float64),
                    FieldSpec::new("ok", TypeSpec::Bool),
                ],
            },
            EngineKind::Firefly,
        )
    }

    #[test]
    fn csv_value_coercion() {
        assert_eq!(
            parse_csv_value("42", TypeSpec::Int64),
            serde_json::json!(42)
        );
        assert_eq!(
            parse_csv_value("", TypeSpec::Int64),
            serde_json::Value::Null
        );
        assert_eq!(
            parse_csv_value("\\N", TypeSpec::Utf8),
            serde_json::Value::Null
        );
        assert_eq!(
            parse_csv_value("TRUE", TypeSpec::Bool),
            serde_json::json!(true)
        );
        assert_eq!(
            parse_csv_value("3.5", TypeSpec::Float64),
            serde_json::json!(3.5)
        );
        assert_eq!(
            parse_csv_value("hello", TypeSpec::Utf8),
            serde_json::json!("hello")
        );
    }

    #[test]
    fn csv_roundtrip_to_batches() {
        let dir = std::env::temp_dir().join(format!("less-csv-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.csv");
        std::fs::write(
            &path,
            "name,id,amount,ok\napple,1,1.5,true\npear,2,2.5,false\n,3,,true\n",
        )
        .unwrap();
        let batches = load_csv(&def(), &path, 100).unwrap();
        assert_eq!(batches.len(), 1);
        let b = &batches[0];
        assert_eq!(b.num_rows(), 3);
        assert_eq!(b.num_columns(), 4);
        // Column order follows the table schema, not the CSV header.
        assert_eq!(b.schema().field(0).name(), "id");
        std::fs::remove_dir_all(&dir).ok();
    }
}
