//! Engine configuration.
//!
//! [`EngineConfig`] is the single configuration object shared by the engine,
//! the query session, the CLI and the SDKs. It serializes to JSON so it can
//! live in config files and be passed through the MCP server.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Global engine configuration.
///
/// All fields have sane defaults tuned for modern NVMe-backed machines; the
/// defaults are intentionally conservative (small buffers, cheap compression
/// level) so first runs stay fast while remaining memory-frugal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    /// Shared storage URL for FireflyCloud tables: `s3://bucket/prefix`,
    /// `gcs://bucket/prefix`, `az://account/container/prefix`,
    /// `file:///path` or `memory://`. `None` (default) keeps a local
    /// object-store root under `data_dir/shared` — useful for development.
    ///
    /// This is the compute/storage separation switch: when set, *all*
    /// durable state of FireflyCloud tables (data parts and table
    /// manifests) lives in the shared store, and the node's local disk
    /// holds only ephemeral compute state (buffers, caches, local
    /// Firefly tables).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_url: Option<String>,

    /// Root directory for the local catalog, local table parts and the
    /// default shared object store.
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,

    /// Number of buffered rows per table before the in-memory buffer is
    /// flushed into an immutable data part.
    #[serde(default = "default_flush_rows")]
    pub flush_rows: usize,

    /// Hard cap on the in-memory buffer; when reached the buffer is flushed
    /// even if the next insert batch would exceed it.
    #[serde(default = "default_max_buffer_rows")]
    pub max_buffer_rows: usize,

    /// Target size of a single data part; background merges combine smaller
    /// parts until parts approach this size.
    #[serde(default = "default_target_part_bytes")]
    pub target_part_bytes: u64,

    /// Trigger an automatic table optimize (merge) once a table reaches this
    /// many parts.
    #[serde(default = "default_auto_merge_parts")]
    pub auto_merge_parts: usize,

    /// Soft cap on the total number of rows materialized in memory per
    /// merge pass. Merges are currently non-streaming on the input side
    /// (selected parts are read into memory as sorted runs), so this bounds
    /// merge memory: an optimize makes repeated passes, merging up to this
    /// many rows of the smallest eligible tier at a time, until no eligible
    /// group remains. Parts larger than the cap stay unmerged (streaming
    /// input merges are on the roadmap).
    #[serde(default = "default_max_merge_rows")]
    pub max_merge_rows: usize,

    /// Default compression codec: `zstd`, `lz4` or `none`.
    #[serde(default = "default_compression")]
    pub compression: String,

    /// Compression level for zstd (1..=22; 0 = library default).
    #[serde(default = "default_zstd_level")]
    pub zstd_level: i32,

    /// False-positive rate for bloom filters used by uniqueness-constraint
    /// pruning.
    #[serde(default = "default_bloom_fp_rate")]
    pub bloom_fp_rate: f64,

    /// Whether GPU kernels are enabled when available.
    #[serde(default)]
    pub gpu_enabled: bool,

    /// Authentication for remote interfaces (HTTP server, future Flight
    /// SQL): LDAP/Active Directory and/or a dev file authenticator. `None`
    /// = no authentication (local dev default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<crate::auth::AuthConfig>,

    /// Write-ahead log for crash-safe inserts (replayed on open).
    #[serde(default = "default_true")]
    pub wal_enabled: bool,

    /// fsync WAL appends before an insert is acknowledged.
    #[serde(default = "default_true")]
    pub wal_fsync: bool,

    /// Memory budget (bytes) for the shared-storage block cache. Immutable
    /// part objects fetched from S3/GCS/Azure/local object storage are
    /// served from this LRU cache, so repeated scans cost no remote reads.
    /// `0` disables the cache.
    #[serde(default = "default_block_cache_bytes")]
    pub block_cache_bytes: usize,

    /// Optional local directory for the persistent disk tier of the block
    /// cache (survives restarts; NVMe-backed in production). `None` keeps
    /// the cache memory-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_cache_dir: Option<PathBuf>,

    /// Query memory limit (bytes) for the DataFusion memory pool. Memory
    /// reservations beyond this fail the query instead of exhausting the
    /// box (`ResourcesExhausted`). `0` = DataFusion's default pool.
    #[serde(default)]
    pub memory_limit: usize,

    /// Read parquet page indexes for page-level predicate skipping.
    /// `true` (default) keeps page skipping on; `false` limits skipping to
    /// row-group statistics — the workaround for a DataFusion/parquet-rs
    /// bug where the Mask selection strategy trips over sparse column
    /// chunks (apache/datafusion#8092) on wide string columns.
    #[serde(default = "default_true")]
    pub parquet_page_index: bool,

    /// PEM certificate chain for HTTPS on `less server` (leaf first).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_cert: Option<PathBuf>,

    /// PEM private key for HTTPS on `less server`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_key: Option<PathBuf>,

    /// Stable identity of this compute node for multi-writer coordination
    /// (merge claims). `None` = a random id per open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
}

fn default_true() -> bool {
    true
}

const fn default_block_cache_bytes() -> usize {
    256 << 20
}

fn default_data_dir() -> PathBuf {
    PathBuf::from(".less")
}
const fn default_flush_rows() -> usize {
    262_144
}
const fn default_max_buffer_rows() -> usize {
    1_048_576
}
const fn default_target_part_bytes() -> u64 {
    256 << 20
}
const fn default_auto_merge_parts() -> usize {
    8
}
const fn default_max_merge_rows() -> usize {
    4 << 20
}
fn default_compression() -> String {
    "zstd".to_string()
}
const fn default_zstd_level() -> i32 {
    3
}
const fn default_bloom_fp_rate() -> f64 {
    0.01
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            shared_url: None,
            data_dir: default_data_dir(),
            flush_rows: default_flush_rows(),
            max_buffer_rows: default_max_buffer_rows(),
            target_part_bytes: default_target_part_bytes(),
            auto_merge_parts: default_auto_merge_parts(),
            max_merge_rows: default_max_merge_rows(),
            compression: default_compression(),
            zstd_level: default_zstd_level(),
            bloom_fp_rate: default_bloom_fp_rate(),
            gpu_enabled: false,
            auth: None,
            wal_enabled: true,
            wal_fsync: true,
            block_cache_bytes: default_block_cache_bytes(),
            block_cache_dir: None,
            memory_limit: 0,
            parquet_page_index: true,
            tls_cert: None,
            tls_key: None,
            node_id: None,
        }
    }
}

impl EngineConfig {
    /// Config rooted at `data_dir` with defaults for everything else.
    pub fn with_data_dir(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            ..Self::default()
        }
    }

    /// Config rooted at `data_dir` with a shared storage URL
    /// (see [`EngineConfig::shared_url`]).
    pub fn with_shared_url(data_dir: impl Into<PathBuf>, shared_url: impl Into<String>) -> Self {
        Self {
            shared_url: Some(shared_url.into()),
            ..Self::with_data_dir(data_dir.into())
        }
    }

    /// Validate the configuration (called on load, save, and `less init`):
    /// fail fast with a clear message instead of misbehaving later.
    pub fn validate(&self) -> crate::Result<()> {
        if !matches!(self.compression.as_str(), "zstd" | "lz4" | "none") {
            return Err(crate::LessError::Config(format!(
                "invalid compression '{}' (expected zstd, lz4, or none)",
                self.compression
            )));
        }
        if !(0..=22).contains(&self.zstd_level) {
            return Err(crate::LessError::Config(format!(
                "invalid zstd_level {} (expected 0..=22)",
                self.zstd_level
            )));
        }
        if self.flush_rows == 0 {
            return Err(crate::LessError::Config(
                "flush_rows must be at least 1".into(),
            ));
        }
        if self.max_buffer_rows < self.flush_rows {
            return Err(crate::LessError::Config(format!(
                "max_buffer_rows ({}) must be >= flush_rows ({})",
                self.max_buffer_rows, self.flush_rows
            )));
        }
        if self.max_merge_rows == 0 {
            return Err(crate::LessError::Config(
                "max_merge_rows must be at least 1".into(),
            ));
        }
        if !(0.0..1.0).contains(&self.bloom_fp_rate) {
            return Err(crate::LessError::Config(format!(
                "bloom_fp_rate must be in (0, 1), got {}",
                self.bloom_fp_rate
            )));
        }
        Ok(())
    }

    /// Load `<data_dir>/config.json` when present (e.g. written by
    /// `less init --shared s3://...`), else defaults. The data directory is
    /// always the caller-supplied one. Invalid persisted config fails fast.
    pub fn load_or_default(data_dir: impl Into<PathBuf>) -> Self {
        let dir = data_dir.into();
        let mut config = std::fs::read(dir.join("config.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<EngineConfig>(&bytes).ok())
            .unwrap_or_default();
        config.data_dir = dir;
        config
    }

    /// Persist this config to `<data_dir>/config.json` (atomic write).
    pub fn save(&self) -> crate::Result<()> {
        self.validate()?;
        std::fs::create_dir_all(&self.data_dir)?;
        let path = self.data_dir.join("config.json");
        let tmp = self.data_dir.join("config.json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> EngineConfig {
        EngineConfig::default()
    }

    #[test]
    fn validate_accepts_defaults() {
        assert!(valid().validate().is_ok());
    }

    #[test]
    fn validate_rejects_bad_values() {
        let mut c = valid();
        c.compression = "snappy".into();
        assert!(
            c.validate()
                .unwrap_err()
                .to_string()
                .contains("compression")
        );

        c = valid();
        c.zstd_level = 23;
        assert!(c.validate().unwrap_err().to_string().contains("zstd_level"));

        c = valid();
        c.flush_rows = 0;
        assert!(c.validate().is_err());

        c = valid();
        c.max_buffer_rows = 100;
        assert!(
            c.validate()
                .unwrap_err()
                .to_string()
                .contains("max_buffer_rows")
        );

        c = valid();
        c.bloom_fp_rate = 1.5;
        assert!(
            c.validate()
                .unwrap_err()
                .to_string()
                .contains("bloom_fp_rate")
        );
    }

    #[test]
    fn save_roundtrips_and_persists() {
        let dir = std::env::temp_dir().join(format!(
            "less-config-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut c = EngineConfig::with_data_dir(&dir);
        c.compression = "lz4".into();
        c.flush_rows = 123_456;
        c.save().unwrap();
        let loaded = EngineConfig::load_or_default(&dir);
        assert_eq!(loaded.compression, "lz4");
        assert_eq!(loaded.flush_rows, 123_456);
        std::fs::remove_dir_all(&dir).ok();
    }
}
