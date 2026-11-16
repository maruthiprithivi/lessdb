//! LessDB catalog: typed table schemas and persistent table manifests.
//!
//! The catalog is deliberately boring: one JSON manifest per table under
//! `<data_dir>/catalog/<table>.json`. This keeps the metadata story simple
//! for the local engine; the shared engine stores the same manifests and
//! additionally keeps part metadata in the shared object store, so any
//! compute node can discover tables and parts without a coordination
//! database. (A real distributed metadata store — FoundationDB/etcd — is on
//! the roadmap for multi-writer shared clusters.)

pub mod catalog;
pub mod ddl;
pub mod metastore;
pub mod schema;
pub mod shared;

pub use catalog::Catalog;
pub use metastore::{FileMetaStore, MetaStore, ObjectMetaStore};
pub use schema::EngineKind;
pub use schema::{FieldSpec, SchemaSpec, TableDef, TypeSpec};
pub use shared::{SHARED_CATALOG_PREFIX, SharedCatalog};
