//! LessDB table engine: Firefly (local) and FireflyCloud (object
//! storage) table engines with buffered inserts, immutable data parts,
//! automatic merging and uniqueness constraints.
//!
//! * **Firefly** — parts live on local disk under `<data_dir>/parts/`.
//! * **FireflyCloud** — the same parts live in the shared object store
//!   under `tables/<table>/parts/`, so any number of stateless compute
//!   nodes can serve the same data. The architecture uses no
//!   replicas, no quorum writes — just shared storage + shared metadata.

mod engine;
mod merge;
mod part;
mod wal;

pub use engine::{LessEngine, TableStats, align_batch};
pub use merge::merge_all;
pub use part::{DataPart, PartLocation};
