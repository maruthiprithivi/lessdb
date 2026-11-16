//! Local active-part generations. Physical directory presence is not visibility.
//! Single directory owner; readers retain leases through plans and streams.
use crate::part::{DataPart, PartLocation};
use less_common::{LessError, Result, durability};
use less_storage::{DATA_FILE, META_FILE, PartMeta, list_part_metas, read_part_meta};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub(crate) const ACTIVE_FILE: &str = "ACTIVE.json";

/// Kept alive by every local reader lease, even after the engine is dropped.
#[derive(Debug)]
pub(crate) struct DirectoryOwner {
    _file: File,
    root: PathBuf,
}
impl DirectoryOwner {
    pub fn acquire(root: &Path) -> Result<Arc<Self>> {
        durability::create_dir_all(root)?;
        let root = root.canonicalize()?;
        for child in ["parts", "catalog", "wal", "OWNER.lock"] {
            let path = root.join(child);
            if let Ok(meta) = std::fs::symlink_metadata(&path)
                && meta.file_type().is_symlink()
            {
                return Err(LessError::Engine(
                    "database namespace contains a symlink".into(),
                ));
            }
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("OWNER.lock"))?;
        file.try_lock().map_err(|e| {
            LessError::Engine(format!(
                "database directory already owned or locking unsupported: {e}"
            ))
        })?;
        Ok(Arc::new(Self { _file: file, root }))
    }
}

#[derive(Debug)]
pub(crate) struct PartLease {
    path: PathBuf,
    retired: AtomicBool,
    failed: Arc<AtomicBool>,
    _owner: Arc<DirectoryOwner>,
}
impl Drop for PartLease {
    fn drop(&mut self) {
        if self.retired.load(Ordering::Acquire) {
            // Retire only after a durable generation commit. Failure leaves
            // retryable garbage and makes subsequent engine operations fail.
            let namespace = self._owner.root.join("parts");
            for directory in [
                &namespace,
                self.path.parent().expect("part has parent"),
                &self.path,
            ] {
                if !std::fs::symlink_metadata(directory).is_ok_and(|meta| meta.is_dir()) {
                    self.failed.store(true, Ordering::Release);
                    return;
                }
            }
            let result = std::fs::remove_dir_all(&self.path)
                .and_then(|()| durability::sync_dir(self.path.parent().expect("part has parent")));
            if result.is_err() {
                self.failed.store(true, Ordering::Release);
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    generation: u64,
    wal_highwater: u64,
    parts: Vec<String>,
}
struct State {
    manifest: Manifest,
    parts: Vec<DataPart>,
}
pub(crate) struct LiveParts {
    directory: PathBuf,
    table: String,
    owner: Arc<DirectoryOwner>,
    failed: Arc<AtomicBool>,
    state: Mutex<State>,
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 240
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}
fn validate_part(directory: &Path, table: &str, name: &str) -> Result<PartMeta> {
    if !valid_name(name) {
        return Err(LessError::Engine("invalid active part name".into()));
    }
    let path = directory.join(name);
    if !std::fs::symlink_metadata(&path)?.is_dir() {
        return Err(LessError::Engine(
            "active part is not a real directory".into(),
        ));
    }
    for file in [DATA_FILE, META_FILE] {
        if !std::fs::symlink_metadata(path.join(file))?.is_file() {
            return Err(LessError::Engine(
                "active part file is not a real regular file".into(),
            ));
        }
    }
    let meta = read_part_meta(&path)?;
    if meta.name != name || meta.table != table || meta.row_count == 0 {
        return Err(LessError::Engine(
            "active part metadata identity mismatch".into(),
        ));
    }
    Ok(meta)
}

impl LiveParts {
    pub fn open(
        directory: PathBuf,
        table: String,
        owner: Arc<DirectoryOwner>,
        failed: Arc<AtomicBool>,
    ) -> Result<Self> {
        if directory != owner.root.join("parts").join(&table) {
            return Err(LessError::Engine(
                "active table namespace escapes database root".into(),
            ));
        }
        for path in [owner.root.join("parts"), directory.clone()] {
            if let Ok(meta) = std::fs::symlink_metadata(path)
                && !meta.is_dir()
            {
                return Err(LessError::Engine(
                    "active table container is not a real directory".into(),
                ));
            }
        }
        durability::create_dir_all(&directory)?;
        let manifest = if directory.join(ACTIVE_FILE).exists() {
            let path = directory.join(ACTIVE_FILE);
            if !std::fs::symlink_metadata(&path)?.is_file()
                || path.metadata()?.len() > 16 * 1024 * 1024
            {
                return Err(LessError::Engine(
                    "invalid/oversized active manifest".into(),
                ));
            }
            serde_json::from_slice::<Manifest>(&std::fs::read(path)?)?
        } else {
            if !list_part_metas(&directory)?.is_empty() {
                return Err(LessError::Engine("legacy nonempty table lacks ACTIVE.json; explicit audited migration required (do not infer generations from part levels/LSNs)".into()));
            }
            let manifest = Manifest {
                version: 1,
                generation: 0,
                wal_highwater: 0,
                parts: vec![],
            };
            Self::publish(&directory, &manifest)?;
            manifest
        };
        if manifest.version != 1
            || manifest.parts.len() > 100_000
            || manifest.parts.iter().collect::<HashSet<_>>().len() != manifest.parts.len()
        {
            return Err(LessError::Engine(
                "unsupported or invalid active manifest".into(),
            ));
        }
        // Validate every referenced file before any destructive orphan cleanup.
        let metas = manifest
            .parts
            .iter()
            .map(|name| validate_part(&directory, &table, name))
            .collect::<Result<Vec<_>>>()?;
        if metas
            .iter()
            .filter_map(|m| m.wal_lsn_max)
            .any(|n| n > manifest.wal_highwater)
        {
            return Err(LessError::Engine(
                "active manifest WAL high-water mismatch".into(),
            ));
        }
        let parts = metas
            .into_iter()
            .map(|meta| Self::handle(&directory, &table, meta, &owner, &failed))
            .collect();
        let live = Self {
            directory,
            table,
            owner,
            failed,
            state: Mutex::new(State { manifest, parts }),
        };
        let names: BTreeSet<_> = live
            .state
            .lock()
            .unwrap()
            .manifest
            .parts
            .iter()
            .cloned()
            .collect();
        for entry in std::fs::read_dir(&live.directory)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type()?.is_dir() && valid_name(&name) && !names.contains(&name) {
                std::fs::remove_dir_all(entry.path())?;
            }
        }
        durability::sync_dir(&live.directory)?;
        Ok(live)
    }

    fn handle(
        directory: &Path,
        table: &str,
        meta: PartMeta,
        owner: &Arc<DirectoryOwner>,
        failed: &Arc<AtomicBool>,
    ) -> DataPart {
        let path = directory.join(&meta.name);
        DataPart {
            table: table.to_owned(),
            meta,
            location: PartLocation::Local(path.clone()),
            lease: Some(Arc::new(PartLease {
                path,
                retired: AtomicBool::new(false),
                failed: failed.clone(),
                _owner: owner.clone(),
            })),
        }
    }

    fn publish(directory: &Path, manifest: &Manifest) -> Result<()> {
        if manifest.parts.len() > 100_000 {
            return Err(LessError::Engine(
                "active part count exceeds supported limit".into(),
            ));
        }
        let bytes = serde_json::to_vec(manifest)?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(LessError::Engine(
                "active manifest exceeds supported size".into(),
            ));
        }
        let temporary = directory.join(format!("ACTIVE-{}.tmp", uuid::Uuid::new_v4()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        durability::sync_file(&file)?;
        std::fs::rename(&temporary, directory.join(ACTIVE_FILE))?;
        durability::sync_dir(directory)?;
        Ok(())
    }

    pub fn parts(&self) -> Result<Vec<DataPart>> {
        let mut parts = self.state.lock().unwrap().parts.clone();
        for part in &mut parts {
            part.meta = validate_part(&self.directory, &self.table, &part.meta.name)?;
        }
        Ok(parts)
    }
    pub fn highwater(&self) -> u64 {
        self.state.lock().unwrap().manifest.wal_highwater
    }

    /// Caller serializes writes. Readers see old or new, never both. On an
    /// ambiguous failed rename/barrier, keep all files and require reopen.
    pub fn replace(&self, removed: &[DataPart], added: &[PartMeta]) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        let names: HashSet<_> = removed.iter().map(|p| p.meta.name.as_str()).collect();
        if names.len() != removed.len()
            || removed
                .iter()
                .any(|p| !state.manifest.parts.contains(&p.meta.name))
        {
            return Err(LessError::Engine("active generation conflict".into()));
        }
        let mut manifest = state.manifest.clone();
        manifest.generation = manifest
            .generation
            .checked_add(1)
            .ok_or_else(|| LessError::Engine("active generation exhausted".into()))?;
        manifest.parts.retain(|name| !names.contains(name.as_str()));
        for meta in added {
            validate_part(&self.directory, &self.table, &meta.name)?;
            if manifest.parts.contains(&meta.name) {
                return Err(LessError::Engine("duplicate active part".into()));
            }
            manifest.parts.push(meta.name.clone());
            manifest.wal_highwater = manifest.wal_highwater.max(meta.wal_lsn_max.unwrap_or(0));
        }
        manifest.parts.sort();
        if let Err(error) = Self::publish(&self.directory, &manifest) {
            self.failed.store(true, Ordering::Release);
            return Err(error);
        }
        // All barriers passed: retire inputs only now. Existing readers hold
        // their same owner lock and files until the last lease is dropped.
        for part in &state.parts {
            if names.contains(part.meta.name.as_str()) {
                part.lease
                    .as_ref()
                    .expect("local lease")
                    .retired
                    .store(true, Ordering::Release);
            }
        }
        state
            .parts
            .retain(|p| !names.contains(p.meta.name.as_str()));
        state.parts.extend(added.iter().cloned().map(|meta| {
            Self::handle(
                &self.directory,
                &self.table,
                meta,
                &self.owner,
                &self.failed,
            )
        }));
        state.parts.sort_by(|a, b| a.meta.name.cmp(&b.meta.name));
        state.manifest = manifest;
        Ok(())
    }
}
