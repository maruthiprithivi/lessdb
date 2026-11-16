//! Compression codecs.
//!
//! LessDB compresses columns independently. The codec set is `zstd`, `lz4`
//! and `none`; additional encodings (delta, double-delta, gorilla, dictionary) are on
//! the roadmap as additional parquet encodings.
//!
//! Both codecs are applied in *raw* form here and handed to the parquet
//! writer which owns the framing, so a LessDB part is readable by any
//! parquet-capable tool — part of the DuckDB-style "it just works with your
//! existing stack" integration story.

use less_common::{LessError, Result};
use parquet::basic::{Compression as ParquetCompression, ZstdLevel};
use serde::{Deserialize, Serialize};

/// Supported compression codecs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    /// No compression.
    None,
    /// LZ4 (raw block format) — extremely fast, decent ratio.
    Lz4,
    /// Zstandard — the default compression codec.
    Zstd,
}

impl Compression {
    /// Parse a codec name, case-insensitive.
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" | "uncompressed" | "" => Ok(Self::None),
            "lz4" | "lz4_raw" => Ok(Self::Lz4),
            "zstd" | "zstandard" => Ok(Self::Zstd),
            other => Err(LessError::Config(format!(
                "unknown compression codec '{other}' (expected one of: zstd, lz4, none)"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Lz4 => "lz4",
            Self::Zstd => "zstd",
        }
    }

    /// Compress raw bytes (used by future custom codecs; data parts go
    /// through parquet's own writer instead).
    pub fn compress(&self, data: &[u8], level: i32) -> Result<Vec<u8>> {
        match self {
            Self::None => Ok(data.to_vec()),
            Self::Lz4 => Ok(lz4_flex::compress_prepend_size(data)),
            Self::Zstd => Ok(zstd::bulk::compress(data, level)?),
        }
    }

    /// Decompress raw bytes.
    pub fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        match self {
            Self::None => Ok(data.to_vec()),
            Self::Lz4 => lz4_flex::decompress_size_prepended(data)
                .map_err(|e| LessError::Compression(e.to_string())),
            Self::Zstd => {
                zstd::stream::decode_all(data).map_err(|e| LessError::Compression(e.to_string()))
            }
        }
    }

    /// Map to the parquet codec used when writing parts.
    pub fn to_parquet(&self, level: i32) -> ParquetCompression {
        match self {
            Self::None => ParquetCompression::UNCOMPRESSED,
            // LZ4_RAW avoids the Hadoop framing overhead; it is fully
            // supported by arrow/parquet readers and tools like DuckDB.
            Self::Lz4 => ParquetCompression::LZ4_RAW,
            Self::Zstd => ParquetCompression::ZSTD(ZstdLevel::try_new(level).unwrap_or_default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(c: Compression, data: &[u8]) {
        let compressed = c.compress(data, 3).unwrap();
        let restored = c.decompress(&compressed).unwrap();
        assert_eq!(data, restored.as_slice());
    }

    #[test]
    fn codec_roundtrips() {
        let data: Vec<u8> = (0..100_000u32).flat_map(|i| i.to_le_bytes()).collect();
        for c in [Compression::None, Compression::Lz4, Compression::Zstd] {
            roundtrip(c, &data);
        }
    }

    #[test]
    fn compression_actually_compresses() {
        let data = vec![42u8; 1 << 20];
        let compressed = Compression::Zstd.compress(&data, 3).unwrap();
        assert!(
            compressed.len() < data.len() / 10,
            "zstd should crush uniform data"
        );
        let compressed = Compression::Lz4.compress(&data, 0).unwrap();
        assert!(
            compressed.len() < data.len() / 10,
            "lz4 should crush uniform data"
        );
    }

    #[test]
    fn parse_names() {
        assert_eq!(Compression::parse("ZSTD").unwrap(), Compression::Zstd);
        assert_eq!(Compression::parse("lz4").unwrap(), Compression::Lz4);
        assert_eq!(Compression::parse("none").unwrap(), Compression::None);
        assert!(Compression::parse("gzip").is_err());
    }
}
