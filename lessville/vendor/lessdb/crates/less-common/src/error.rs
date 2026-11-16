//! Central error type for LessDB.
//!
//! Every crate in the workspace converts its own error types into
//! [`LessError`] so that callers (CLI, SDKs, MCP server, HTTP server) have a
//! single, uniform error surface.

use std::io;

use arrow::error::ArrowError;
use parquet::errors::ParquetError;

/// The one error type used across LessDB.
#[derive(Debug, thiserror::Error)]
pub enum LessError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("arrow error: {0}")]
    Arrow(#[from] ArrowError),

    #[error("parquet error: {0}")]
    Parquet(#[from] ParquetError),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("compression error: {0}")]
    Compression(String),

    #[error("object store error: {0}")]
    ObjectStore(String),

    #[error("query error: {0}")]
    Query(String),

    #[error("engine error: {0}")]
    Engine(String),

    #[error("catalog error: {0}")]
    Catalog(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("gpu error: {0}")]
    Gpu(String),

    #[error("server error: {0}")]
    Server(String),

    #[error("not implemented: {0}")]
    NotImplemented(String),
}

/// Convenience alias used everywhere.
pub type Result<T> = std::result::Result<T, LessError>;

impl LessError {
    pub fn engine(msg: impl Into<String>) -> Self {
        Self::Engine(msg.into())
    }

    pub fn catalog(msg: impl Into<String>) -> Self {
        Self::Catalog(msg.into())
    }

    pub fn query(msg: impl Into<String>) -> Self {
        Self::Query(msg.into())
    }
}
