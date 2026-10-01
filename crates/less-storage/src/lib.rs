//! LessDB storage layer: compression codecs, bloom filters, data-part
//! metadata and the immutable data-part format.
//!
//! A **data part** is the unit of immutability in LessDB. It is a directory (or object-store
//! prefix) containing:
//!
//! * `data.parquet` — columnar data, one row group per part, with the
//!   configured per-column compression codec;
//! * `meta.json` — [`PartMeta`]: per-column statistics (min/max/null count),
//!   bloom filters for uniqueness columns, sort-key info.
//!
//! Parts are written once and never mutated; updates happen by merging parts
//! into new, larger parts and deleting the inputs.

pub mod block_cache;
pub mod bloom;
pub mod codec;
pub mod object;
pub mod part_io;
pub mod part_meta;

pub use block_cache::{BlockCache, CacheStats};
pub use bloom::Bloom;
pub use codec::Compression;
pub use object::{CachingObjectStore, SharedStore};
pub use part_io::{
    DATA_FILE, META_FILE, WriteOptions, list_part_metas, read_part, read_part_from_bytes,
    read_part_meta, sort_batch, write_part, write_part_sorted,
};
pub use part_meta::{ColumnStats, PartMeta, StatValue};
