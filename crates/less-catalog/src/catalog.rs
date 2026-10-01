//! Persistent catalog: JSON manifests for tables under a root directory.

use std::path::{Path, PathBuf};

use less_common::{LessError, Result};

use crate::schema::TableDef;

/// Directory layout:
/// ```text
/// <root>/
///   catalog/<table>.json   table manifest
///   parts/<table>/         data parts (local engine only)
///   shared/                shared object store root (shared engine)
/// ```
#[derive(Debug, Clone)]
pub struct Catalog {
    root: PathBuf,
    catalog_dir: PathBuf,
    parts_root: PathBuf,
}

impl Catalog {
    /// Open (creating if needed) a catalog at `root`.
    pub fn open(root: &Path) -> Result<Self> {
        let catalog_dir = root.join("catalog");
        let parts_root = root.join("parts");
        std::fs::create_dir_all(&catalog_dir)?;
        std::fs::create_dir_all(&parts_root)?;
        Ok(Self {
            root: root.to_path_buf(),
            catalog_dir,
            parts_root,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Directory holding local data parts for a table.
    pub fn parts_dir(&self, table: &str) -> PathBuf {
        self.parts_root.join(table)
    }

    fn manifest_path(&self, table: &str) -> PathBuf {
        self.catalog_dir.join(format!("{table}.json"))
    }

    /// Persist a new table (fails if it already exists).
    pub fn create_table(&self, def: &TableDef) -> Result<()> {
        let path = self.manifest_path(&def.name);
        if path.exists() {
            return Err(LessError::Catalog(format!(
                "table '{}' already exists",
                def.name
            )));
        }
        self.write_manifest(def)
    }

    /// Overwrite the manifest for an existing table.
    pub fn update_table(&self, def: &TableDef) -> Result<()> {
        let path = self.manifest_path(&def.name);
        if !path.exists() {
            return Err(LessError::Catalog(format!(
                "table '{}' does not exist",
                def.name
            )));
        }
        self.write_manifest(def)
    }

    fn write_manifest(&self, def: &TableDef) -> Result<()> {
        let path = self.manifest_path(&def.name);
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(def)?)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn load_table(&self, name: &str) -> Result<TableDef> {
        let path = self.manifest_path(name);
        if !path.exists() {
            return Err(LessError::Catalog(format!("table '{name}' does not exist")));
        }
        let bytes = std::fs::read(&path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn tables(&self) -> Result<Vec<String>> {
        let mut out = vec![];
        for entry in std::fs::read_dir(&self.catalog_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json")
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            {
                out.push(stem.to_string());
            }
        }
        out.sort();
        Ok(out)
    }

    /// Remove a table manifest and its local parts directory.
    pub fn drop_table(&self, name: &str) -> Result<()> {
        let path = self.manifest_path(name);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        let parts = self.parts_dir(name);
        if parts.exists() {
            std::fs::remove_dir_all(&parts)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{EngineKind, FieldSpec, SchemaSpec, TypeSpec};

    fn def(name: &str) -> TableDef {
        TableDef::new(
            name,
            SchemaSpec {
                fields: vec![FieldSpec::new("a", TypeSpec::Int64)],
            },
            EngineKind::Firefly,
        )
    }

    #[test]
    fn create_load_list_drop() {
        let dir = std::env::temp_dir().join(format!("less-catalog-{}", uuid::Uuid::new_v4()));
        let catalog = Catalog::open(&dir).unwrap();
        catalog.create_table(&def("t1")).unwrap();
        assert!(catalog.create_table(&def("t1")).is_err());
        catalog.create_table(&def("t2")).unwrap();
        assert_eq!(
            catalog.tables().unwrap(),
            vec!["t1".to_string(), "t2".to_string()]
        );
        let loaded = catalog.load_table("t1").unwrap();
        assert_eq!(loaded.name, "t1");
        catalog.drop_table("t1").unwrap();
        assert_eq!(catalog.tables().unwrap(), vec!["t2".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
