//! Data-part handles returned by the engine.

use std::path::PathBuf;

use less_storage::PartMeta;

/// Where a part's files live.
#[derive(Debug, Clone)]
pub enum PartLocation {
    /// Local directory containing `data.parquet` + `meta.json`.
    Local(PathBuf),
    /// Object-store key prefix of the part directory (relative to the
    /// shared store root), e.g. `tables/events/parts/1730.._ab12_1000_0`.
    Object(String),
}

/// A handle to one immutable data part.
#[derive(Debug, Clone)]
pub struct DataPart {
    pub table: String,
    pub meta: PartMeta,
    pub location: PartLocation,
}

impl DataPart {
    /// Human-friendly part summary for introspection.
    pub fn summary(&self) -> String {
        match &self.location {
            PartLocation::Local(_) => format!(
                "{}  rows={}  compression={}  level={}",
                self.meta.name,
                self.meta.row_count,
                self.meta.compression,
                self.level()
            ),
            PartLocation::Object(key) => format!(
                "{}  rows={}  compression={}  level={}  (shared:{key})",
                self.meta.name,
                self.meta.row_count,
                self.meta.compression,
                self.level()
            ),
        }
    }

    /// Merge level encoded in the part name (`{ts}_{uuid}_{rows}_{level}`).
    pub fn level(&self) -> u32 {
        self.meta
            .name
            .rsplit('_')
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }
}
