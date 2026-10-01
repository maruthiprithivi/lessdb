//! Typed schema definitions and table manifests.
//!
//! Schemas are defined in terms of [`TypeSpec`], a serde-friendly subset of
//! Arrow types that covers the analytical workload sweet spot. Using our own
//! spec (instead of serializing Arrow schemas directly) keeps manifests
//! human-readable JSON that survives format evolution.

use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use less_common::{LessError, Result};
use serde::{Deserialize, Serialize};

/// Supported column types.
///
/// `Uuid` is stored as a validated UTF-8 string for now (readable by
/// agents and tools); a native 16-byte layout is on the roadmap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeSpec {
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float32,
    Float64,
    Bool,
    Utf8,
    Date32,
    TimestampMs,
    TimestampUs,
    TimestampNs,
    Decimal { precision: u8, scale: i8 },
    Uuid,
    Array(Box<TypeSpec>),
    Map(Box<TypeSpec>, Box<TypeSpec>),
}

/// Strip `name(...)` and return the inner text, case-insensitive.
fn strip_parens<'a>(name: &str, lower: &'a str, original: &'a str) -> Option<&'a str> {
    let prefix = format!("{name}(");
    if lower.starts_with(&prefix) && lower.ends_with(')') {
        Some(&original[name.len() + 1..original.len() - 1])
    } else {
        None
    }
}

impl TypeSpec {
    /// Parse a type name, case-insensitive, accepting LessDB names,
    /// SQL and legacy aliases, and nested forms:
    /// `Decimal(18, 2)`, `UUID`, `Array(Int64)`, `Map(Utf8, Float64)`.
    pub fn parse(s: &str) -> Result<Self> {
        let original = s.trim();
        let t = original.to_ascii_lowercase();
        if let Some(inner) = strip_parens("array", &t, original) {
            return Ok(Self::Array(Box::new(Self::parse(inner)?)));
        }
        if let Some(inner) = strip_parens("map", &t, original) {
            let parts: Vec<&str> = inner.splitn(2, ',').collect();
            if parts.len() != 2 {
                return Err(LessError::Catalog(
                    "Map takes two types: Map(key, value)".into(),
                ));
            }
            return Ok(Self::Map(
                Box::new(Self::parse(parts[0])?),
                Box::new(Self::parse(parts[1])?),
            ));
        }
        if let Some(inner) = strip_parens("decimal", &t, original) {
            let parts: Vec<&str> = inner.splitn(2, ',').collect();
            let (p, sc) = match parts.as_slice() {
                [p, sc] => (p.trim(), sc.trim()),
                _ => {
                    return Err(LessError::Catalog(
                        "Decimal takes two numbers: Decimal(precision, scale)".into(),
                    ));
                }
            };
            let precision: u8 = p
                .parse()
                .map_err(|_| LessError::Catalog(format!("bad decimal precision '{p}'")))?;
            let scale: i8 = sc
                .parse()
                .map_err(|_| LessError::Catalog(format!("bad decimal scale '{sc}'")))?;
            if precision == 0 || precision > 38 {
                return Err(LessError::Catalog(
                    "decimal precision must be 1..=38".into(),
                ));
            }
            if scale < 0 || scale as u8 > precision {
                return Err(LessError::Catalog(format!(
                    "decimal scale must be 0..={precision}"
                )));
            }
            return Ok(Self::Decimal { precision, scale });
        }
        Ok(match t.as_str() {
            "int8" | "tinyint" => Self::Int8,
            "int16" | "smallint" => Self::Int16,
            "int32" | "int" | "integer" => Self::Int32,
            "int64" | "bigint" => Self::Int64,
            "uint8" => Self::UInt8,
            "uint16" => Self::UInt16,
            "uint32" => Self::UInt32,
            "uint64" => Self::UInt64,
            "float32" | "float" => Self::Float32,
            "float64" | "double" => Self::Float64,
            "bool" | "boolean" => Self::Bool,
            "utf8" | "string" | "text" | "varchar" => Self::Utf8,
            "date" | "date32" => Self::Date32,
            "datetime" | "datetime64" | "timestamp" | "timestamp_ms" => Self::TimestampMs,
            "timestamp_us" => Self::TimestampUs,
            "timestamp_ns" => Self::TimestampNs,
            "uuid" => Self::Uuid,
            _ => {
                return Err(LessError::Catalog(format!(
                    "unsupported column type '{original}' (supported: int8..int64, uint8..uint64, \
                     float32/64, bool, string, date, datetime/timestamp, decimal(p,s), uuid, \
                     array(t), map(k,v))"
                )));
            }
        })
    }

    /// Map an Arrow data type to the closest LessDB type. Used by
    /// `CREATE TABLE … AS SELECT` so query results become durable tables.
    pub fn from_arrow(ty: &DataType) -> Result<Self> {
        use DataType::*;
        let t = match ty {
            Int8 => Self::Int8,
            Int16 => Self::Int16,
            Int32 => Self::Int32,
            Int64 => Self::Int64,
            UInt8 => Self::UInt8,
            UInt16 => Self::UInt16,
            UInt32 => Self::UInt32,
            UInt64 => Self::UInt64,
            Float16 => Self::Float32,
            Float32 => Self::Float32,
            Float64 => Self::Float64,
            Boolean => Self::Bool,
            Utf8 | LargeUtf8 | Utf8View => Self::Utf8,
            Date32 => Self::Date32,
            Date64 => Self::TimestampMs,
            Timestamp(TimeUnit::Millisecond, _) => Self::TimestampMs,
            Timestamp(TimeUnit::Microsecond, _) => Self::TimestampUs,
            Timestamp(TimeUnit::Nanosecond, _) => Self::TimestampNs,
            Decimal128(p, s) | Decimal256(p, s) => Self::Decimal {
                precision: *p,
                scale: (*s).max(0),
            },
            List(f) | FixedSizeList(f, _) | LargeList(f) => {
                Self::Array(Box::new(Self::from_arrow(f.data_type())?))
            }
            Map(f, _) => {
                let DataType::Struct(entries) = f.data_type() else {
                    return Err(LessError::Catalog(format!(
                        "unsupported Arrow map entry type for CREATE TABLE AS: {}",
                        f.data_type()
                    )));
                };
                if entries.len() != 2 {
                    return Err(LessError::Catalog(
                        "Arrow maps must have key/value entries".into(),
                    ));
                }
                Self::Map(
                    Box::new(Self::from_arrow(entries[0].data_type())?),
                    Box::new(Self::from_arrow(entries[1].data_type())?),
                )
            }
            Null => Self::Utf8, // untyped NULL literal → String
            other => {
                return Err(LessError::Catalog(format!(
                    "unsupported Arrow type for CREATE TABLE AS: {other}"
                )));
            }
        };
        Ok(t)
    }

    pub fn to_arrow(&self) -> DataType {
        match self {
            Self::Int8 => DataType::Int8,
            Self::Int16 => DataType::Int16,
            Self::Int32 => DataType::Int32,
            Self::Int64 => DataType::Int64,
            Self::UInt8 => DataType::UInt8,
            Self::UInt16 => DataType::UInt16,
            Self::UInt32 => DataType::UInt32,
            Self::UInt64 => DataType::UInt64,
            Self::Float32 => DataType::Float32,
            Self::Float64 => DataType::Float64,
            Self::Bool => DataType::Boolean,
            Self::Utf8 | Self::Uuid => DataType::Utf8,
            Self::Date32 => DataType::Date32,
            Self::TimestampMs => DataType::Timestamp(TimeUnit::Millisecond, None),
            Self::TimestampUs => DataType::Timestamp(TimeUnit::Microsecond, None),
            Self::TimestampNs => DataType::Timestamp(TimeUnit::Nanosecond, None),
            Self::Decimal { precision, scale } => DataType::Decimal128(*precision, *scale),
            Self::Array(inner) => {
                DataType::List(Arc::new(Field::new("item", inner.to_arrow(), true)))
            }
            Self::Map(k, v) => DataType::Map(
                Arc::new(Field::new(
                    "entries",
                    DataType::Struct(
                        vec![
                            Field::new("key", k.to_arrow(), false),
                            Field::new("value", v.to_arrow(), true),
                        ]
                        .into(),
                    ),
                    false,
                )),
                false,
            ),
        }
    }

    pub fn name(&self) -> String {
        match self {
            Self::Int8 => "Int8".into(),
            Self::Int16 => "Int16".into(),
            Self::Int32 => "Int32".into(),
            Self::Int64 => "Int64".into(),
            Self::UInt8 => "UInt8".into(),
            Self::UInt16 => "UInt16".into(),
            Self::UInt32 => "UInt32".into(),
            Self::UInt64 => "UInt64".into(),
            Self::Float32 => "Float32".into(),
            Self::Float64 => "Float64".into(),
            Self::Bool => "Bool".into(),
            Self::Utf8 => "String".into(),
            Self::Date32 => "Date".into(),
            Self::TimestampMs => "DateTime".into(),
            Self::TimestampUs => "DateTime64(6)".into(),
            Self::TimestampNs => "DateTime64(9)".into(),
            Self::Decimal { precision, scale } => format!("Decimal({precision}, {scale})"),
            Self::Uuid => "UUID".into(),
            Self::Array(inner) => format!("Array({})", inner.name()),
            Self::Map(k, v) => format!("Map({}, {})", k.name(), v.name()),
        }
    }
}

/// A single column definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldSpec {
    pub name: String,
    pub ty: TypeSpec,
    #[serde(default = "default_nullable")]
    pub nullable: bool,
}

fn default_nullable() -> bool {
    true
}

impl FieldSpec {
    pub fn new(name: impl Into<String>, ty: TypeSpec) -> Self {
        Self {
            name: name.into(),
            ty,
            nullable: true,
        }
    }
}

/// Column set of a table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaSpec {
    pub fields: Vec<FieldSpec>,
}

impl SchemaSpec {
    pub fn to_arrow(&self) -> SchemaRef {
        Arc::new(Schema::new(
            self.fields
                .iter()
                .map(|f| Field::new(&f.name, f.ty.to_arrow(), f.nullable))
                .collect::<Vec<_>>(),
        ))
    }

    pub fn field(&self, name: &str) -> Option<&FieldSpec> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Validate sort key and uniqueness constraint against the schema.
    ///
    /// Rules (guaranteeing that "keep last per unique key after a stable
    /// sort" is exact):
    /// * all referenced columns must exist;
    /// * the uniqueness columns must be a prefix of the sort key;
    /// * a uniqueness constraint requires a sort key.
    pub fn validate_keys(&self, sort_key: &[String], unique: &[String]) -> Result<()> {
        for col in sort_key.iter().chain(unique.iter()) {
            if self.field(col).is_none() {
                return Err(LessError::Catalog(format!("unknown key column '{col}'")));
            }
        }
        if !unique.is_empty() && sort_key.is_empty() {
            return Err(LessError::Catalog(
                "a UNIQUE constraint requires an ORDER BY / PRIMARY KEY".into(),
            ));
        }
        if unique.len() > sort_key.len() || unique != &sort_key[..unique.len()] {
            return Err(LessError::Catalog(
                "UNIQUE columns must be a prefix of the ORDER BY / PRIMARY KEY columns".into(),
            ));
        }
        Ok(())
    }
}

/// Engine kind of a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    /// Local Firefly: parts on local disk, catalog local.
    Firefly,
    /// FireflyCloud: parts in shared object storage, stateless compute.
    FireflyCloud,
}

impl EngineKind {
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "firefly" => Ok(Self::Firefly),
            "fireflycloud" | "firefly_cloud" | "shared" => Ok(Self::FireflyCloud),
            _ => Err(LessError::Catalog(format!(
                "unknown engine '{s}' (supported: Firefly, FireflyCloud)"
            ))),
        }
    }

    pub fn is_shared(&self) -> bool {
        matches!(self, Self::FireflyCloud)
    }
}

/// Persistent definition of a table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDef {
    pub name: String,
    pub schema: SchemaSpec,
    pub engine: EngineKind,
    pub sort_key: Vec<String>,
    pub unique: Vec<String>,
    /// Per-table compression override.
    pub compression: Option<String>,
    pub bloom_fp_rate: f64,
    pub created_at: String,
    /// TTL retention: rows older than `ttl_secs` (measured on
    /// `ttl_col`, a Timestamp/Date column) are dropped by OPTIMIZE.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_col: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_secs: Option<u64>,
}

impl TableDef {
    pub fn new(name: impl Into<String>, schema: SchemaSpec, engine: EngineKind) -> Self {
        Self {
            name: name.into(),
            schema,
            engine,
            sort_key: vec![],
            unique: vec![],
            compression: None,
            bloom_fp_rate: 0.01,
            created_at: chrono::Utc::now().to_rfc3339(),
            ttl_col: None,
            ttl_secs: None,
        }
    }

    pub fn arrow_schema(&self) -> SchemaRef {
        self.schema.to_arrow()
    }

    pub fn validate(&self) -> Result<()> {
        if !is_identifier(&self.name) {
            return Err(LessError::Catalog(format!(
                "invalid table name '{}' (expected [A-Za-z_][A-Za-z0-9_]*)",
                self.name
            )));
        }
        if self.schema.fields.is_empty() {
            return Err(LessError::Catalog(
                "table must have at least one column".into(),
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for f in &self.schema.fields {
            if !is_identifier(&f.name) {
                return Err(LessError::Catalog(format!(
                    "invalid column name '{}'",
                    f.name
                )));
            }
            if !seen.insert(f.name.clone()) {
                return Err(LessError::Catalog(format!("duplicate column '{}'", f.name)));
            }
        }
        self.schema.validate_keys(&self.sort_key, &self.unique)?;
        if let Some(col) = &self.ttl_col {
            let field = self
                .schema
                .fields
                .iter()
                .find(|f| &f.name == col)
                .ok_or_else(|| LessError::Catalog(format!("TTL column '{col}' does not exist")))?;
            if !matches!(
                field.ty,
                TypeSpec::TimestampMs | TypeSpec::TimestampNs | TypeSpec::Date32
            ) {
                return Err(LessError::Catalog(format!(
                    "TTL column '{col}' must be a Timestamp or Date column"
                )));
            }
            if self.ttl_secs.unwrap_or(0) == 0 {
                return Err(LessError::Catalog(
                    "TTL requires a positive interval".into(),
                ));
            }
        }
        Ok(())
    }

    /// Effective compression for this table.
    pub fn effective_compression(&self, engine_default: &str) -> String {
        self.compression
            .clone()
            .unwrap_or_else(|| engine_default.to_string())
    }
}

/// Validate a bare identifier (table/column names).
pub fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl TableDef {
    /// Render the LessDB DDL for this table (`SHOW CREATE TABLE` and the
    /// `describe` command share this).
    pub fn to_ddl(&self) -> String {
        let mut out = format!("CREATE TABLE {} (\n", self.name);
        for (i, f) in self.schema.fields.iter().enumerate() {
            out.push_str(&format!("    {} {}", f.name, f.ty.name()));
            if i + 1 < self.schema.fields.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str(&format!(
            ") ENGINE={}",
            match self.engine {
                EngineKind::Firefly => "Firefly",
                EngineKind::FireflyCloud => "FireflyCloud",
            }
        ));
        if !self.sort_key.is_empty() {
            out.push_str(&format!(" ORDER BY ({})", self.sort_key.join(", ")));
        }
        if !self.unique.is_empty() {
            out.push_str(&format!(" UNIQUE ({})", self.unique.join(", ")));
        }
        if let (Some(col), Some(secs)) = (&self.ttl_col, self.ttl_secs) {
            let (n, unit) = if secs % 86_400 == 0 {
                (secs / 86_400, "DAY")
            } else if secs % 3_600 == 0 {
                (secs / 3_600, "HOUR")
            } else {
                (secs, "SECOND")
            };
            out.push_str(&format!(" TTL {col} INTERVAL {n} {unit}"));
        }
        if let Some(c) = &self.compression {
            out.push_str(&format!(" COMPRESSION='{c}'"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_parse_and_names() {
        assert_eq!(TypeSpec::parse("INT64").unwrap(), TypeSpec::Int64);
        assert_eq!(TypeSpec::parse("string").unwrap(), TypeSpec::Utf8);
        assert_eq!(TypeSpec::parse("DateTime").unwrap(), TypeSpec::TimestampMs);
        assert_eq!(
            TypeSpec::parse("Decimal(18, 2)").unwrap(),
            TypeSpec::Decimal {
                precision: 18,
                scale: 2
            }
        );
        assert_eq!(
            TypeSpec::parse("Array(Int64)").unwrap(),
            TypeSpec::Array(Box::new(TypeSpec::Int64))
        );
        assert_eq!(
            TypeSpec::parse("Map(Utf8, Float64)").unwrap(),
            TypeSpec::Map(Box::new(TypeSpec::Utf8), Box::new(TypeSpec::Float64))
        );
        assert_eq!(TypeSpec::parse("UUID").unwrap(), TypeSpec::Uuid);
        assert!(TypeSpec::parse("decimal(40,2)").is_err());
        assert!(TypeSpec::parse("decimal(10,11)").is_err());
        assert!(TypeSpec::parse("nope").is_err());
        assert_eq!(TypeSpec::Int64.name(), "Int64");
        assert_eq!(
            TypeSpec::Decimal {
                precision: 18,
                scale: 2
            }
            .name(),
            "Decimal(18, 2)"
        );
        assert_eq!(
            TypeSpec::Array(Box::new(TypeSpec::Int64)).name(),
            "Array(Int64)"
        );
        // Arrow mapping for the nested types.
        assert_eq!(
            TypeSpec::Array(Box::new(TypeSpec::Int64)).to_arrow(),
            DataType::List(Arc::new(Field::new("item", DataType::Int64, true)))
        );
        assert_eq!(
            TypeSpec::Decimal {
                precision: 18,
                scale: 2
            }
            .to_arrow(),
            DataType::Decimal128(18, 2)
        );
    }

    #[test]
    fn unique_must_be_sort_prefix() {
        let schema = SchemaSpec {
            fields: vec![
                FieldSpec::new("a", TypeSpec::Int64),
                FieldSpec::new("b", TypeSpec::Int64),
                FieldSpec::new("c", TypeSpec::Utf8),
            ],
        };
        assert!(
            schema
                .validate_keys(&["a".into(), "b".into()], &["a".into()])
                .is_ok()
        );
        assert!(schema.validate_keys(&["a".into()], &["a".into()]).is_ok());
        // b is not a prefix of [a]:
        assert!(schema.validate_keys(&["a".into()], &["b".into()]).is_err());
        // unique without sort key:
        assert!(schema.validate_keys(&[], &["a".into()]).is_err());
    }
}
