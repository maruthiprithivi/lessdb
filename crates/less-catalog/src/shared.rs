//! Shared catalog: table manifests stored in the shared object store.
//!
//! This is what makes FireflyCloud compute nodes **stateless**: a node
//! discovers tables by listing `catalog/*.json` in shared storage — no
//! local metadata, no coordination database. Table creation is an atomic
//! object put; single-writer is assumed (a compare-and-swap metastore is
//! the roadmap for concurrent writers).

use bytes::Bytes;
use less_common::{LessError, Result};
use less_storage::SharedStore;

use crate::schema::TableDef;

/// Table manifests live under this prefix in the shared store.
pub const SHARED_CATALOG_PREFIX: &str = "catalog";

/// A catalog of table manifests backed by shared object storage.
#[derive(Clone)]
pub struct SharedCatalog {
    store: SharedStore,
}

impl SharedCatalog {
    pub fn new(store: SharedStore) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &SharedStore {
        &self.store
    }

    fn manifest_key(name: &str) -> String {
        format!("{SHARED_CATALOG_PREFIX}/{name}.json")
    }

    /// Persist a table manifest (atomic object put; last write wins).
    pub async fn put_table(&self, def: &TableDef) -> Result<()> {
        self.store
            .put(
                &Self::manifest_key(&def.name),
                Bytes::from(serde_json::to_vec(def)?),
            )
            .await
    }

    pub async fn get_table(&self, name: &str) -> Result<TableDef> {
        let key = Self::manifest_key(name);
        let bytes = self
            .store
            .get_opt(&key)
            .await?
            .ok_or_else(|| LessError::Catalog(format!("table '{name}' does not exist")))?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub async fn table_exists(&self, name: &str) -> Result<bool> {
        self.store.exists(&Self::manifest_key(name)).await
    }

    pub async fn tables(&self) -> Result<Vec<String>> {
        let mut names = vec![];
        for key in self
            .store
            .list_keys(&format!("{SHARED_CATALOG_PREFIX}/"))
            .await?
        {
            if let Some(stem) = key
                .strip_prefix(&format!("{SHARED_CATALOG_PREFIX}/"))
                .and_then(|rest| rest.strip_suffix(".json"))
            {
                names.push(stem.to_string());
            }
        }
        names.sort();
        Ok(names)
    }

    pub async fn delete_table(&self, name: &str) -> Result<()> {
        self.store.delete(&Self::manifest_key(name)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EngineKind, FieldSpec, SchemaSpec, TypeSpec};

    fn def(name: &str, engine: EngineKind) -> TableDef {
        TableDef::new(
            name,
            SchemaSpec {
                fields: vec![FieldSpec::new("x", TypeSpec::Int64)],
            },
            engine,
        )
    }

    #[tokio::test]
    async fn put_get_list_delete_roundtrip() {
        let dir = std::env::temp_dir().join(format!("less-sharedcat-{}", uuid::Uuid::new_v4()));
        let store = SharedStore::new_local(&dir).unwrap();
        let catalog = SharedCatalog::new(store);

        catalog
            .put_table(&def("alpha", EngineKind::FireflyCloud))
            .await
            .unwrap();
        catalog
            .put_table(&def("beta", EngineKind::FireflyCloud))
            .await
            .unwrap();
        assert!(catalog.table_exists("alpha").await.unwrap());
        assert!(!catalog.table_exists("nope").await.unwrap());
        assert_eq!(catalog.tables().await.unwrap(), vec!["alpha", "beta"]);

        let loaded = catalog.get_table("alpha").await.unwrap();
        assert_eq!(loaded.name, "alpha");
        assert_eq!(loaded.engine, EngineKind::FireflyCloud);

        catalog.delete_table("alpha").await.unwrap();
        assert_eq!(catalog.tables().await.unwrap(), vec!["beta"]);
        assert!(catalog.get_table("alpha").await.is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
