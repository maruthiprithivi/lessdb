//! LessDB query layer: a DataFusion-backed SQL engine with a custom table
//! provider.
//!
//! [`LessSession`] wraps a DataFusion `SessionContext` and registers every
//! engine table through [`LessTableProvider`], which:
//!
//! * prunes data parts with part metadata (bloom filters for uniqueness
//!   columns, typed min/max statistics for range predicates) *before* any
//!   I/O;
//! * plans a parallel `ParquetExec` scan per surviving part;
//! * pushes row-level predicates into the parquet reader when safe.
//!
//! Everything above the scan — the optimizer, hash joins (StarRocks-style),
//! aggregations, window functions, `EXPLAIN` — comes from DataFusion, which
//! is also what makes the engine DuckDB-style embeddable.

pub mod gpu_udaf;
pub mod insert;
pub mod mutation;
pub mod provider;
pub mod prune;
pub mod scan_cache;
pub mod session;
pub mod shard;
pub mod stats_optimizer;
pub mod vector_udtf;

pub use provider::LessTableProvider;
pub use prune::prune_part;
pub use session::LessSession;
