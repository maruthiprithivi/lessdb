//! LessVector — native vector search for LessDB.
//!
//! LanceDB-inspired: a registry of named **vector spaces** (dimension +
//! metric space: L2 / cosine / dot), **exact flat** search and **IVF-PQ**
//! approximate nearest-neighbor indexes (k-means inverted lists + product
//! quantization with ADC lookup tables and exact re-ranking), an
//! **embedding-function registry**, and snapshot persistence
//! (`meta.json` + `data.bin` + `index.bin` per space).
//!
//! Multi-space: each space is independent — different dimensions, metrics
//! and index flavors side by side in one registry. SQL access comes via
//! the `vector_search('space', [vector], k)` table function in less-query.

pub mod flat;
pub mod ivf;
pub mod metric;
pub mod registry;
pub mod rng;
pub mod space;

pub use flat::FlatIndex;
pub use ivf::{IvfPqIndex, IvfPqParams};
pub use metric::Metric;
pub use registry::{Embedder, VectorRegistry, trigram_embed};
pub use space::{Index, IndexKind, SearchHit, SpaceInfo, SpaceMeta, VectorSpace};

/// Float wrapper with total order (for heaps). NaN sorts worst.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrderedF32(pub f32);

impl Eq for OrderedF32 {}

impl PartialOrd for OrderedF32 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedF32 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let a = if self.0.is_nan() { f32::MAX } else { self.0 };
        let b = if other.0.is_nan() { f32::MAX } else { other.0 };
        a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
    }
}
