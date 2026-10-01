//! Pluggable metadata store with compare-and-swap primitives — the
//! coordination layer that removes the single-writer assumption from
//! FireflyCloud.
//!
//! Two primitives suffice for multi-writer part publication and merge
//! ownership:
//!
//! * [`MetaStore::put_if_absent`] — atomic "create only if absent": publish
//!   a part's `meta.json` (readers only see parts whose metadata exists),
//!   claim merge ownership of a part set, register a node.
//! * [`MetaStore::compare_and_swap`] — full update-CAS where the backend
//!   supports it (local files via advisory locking; etcd transactions on
//!   the roadmap): takeover of expired claims, manifest version bumps.
//!
//! Implementations:
//!
//! * [`FileMetaStore`] — local filesystem, serialized by `flock` on a
//!   sidecar lock file (correct for all nodes sharing one box).
//! * [`ObjectMetaStore`] — claims and publications as objects in shared
//!   storage (S3/GCS/Azure/`file://`): insert-CAS via conditional copy
//!   (`CopyMode::Create`), so it coordinates writers across boxes.
//!   Update-CAS is not supported (object stores have no transactional
//!   compare; etcd is the roadmap backend for that).

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use fs2::FileExt;
use object_store::{ObjectStore, ObjectStoreExt, path::Path as ObjPath};

use less_common::{LessError, Result};

/// Compare-and-swap key-value coordination store.
#[async_trait]
pub trait MetaStore: Send + Sync + std::fmt::Debug {
    /// Current value, or `None` when the key is absent.
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;

    /// Create `key = value` only if the key does not exist.
    /// `Ok(true)` = written by this call; `Ok(false)` = already existed
    /// (value left untouched).
    async fn put_if_absent(&self, key: &str, value: Vec<u8>) -> Result<bool>;

    /// Set `key = value` iff the current value equals `expected`
    /// (`None` = key must be absent). `Ok(true)` = swapped; `Ok(false)` =
    /// conflict, value unchanged.
    async fn compare_and_swap(
        &self,
        key: &str,
        expected: Option<&[u8]>,
        value: Vec<u8>,
    ) -> Result<bool>;

    /// Remove a key (absent key is not an error).
    async fn delete(&self, key: &str) -> Result<()>;

    /// Can [`Self::compare_and_swap`] succeed on this backend?
    /// (`false` on object stores, which only get insert-CAS.)
    fn supports_update_cas(&self) -> bool {
        true
    }
}

/// Local-filesystem metadata store: one file per key, CAS serialized by an
/// advisory `flock` on a sidecar lock file. Coordinates every process that
/// shares the root directory.
#[derive(Debug)]
pub struct FileMetaStore {
    root: PathBuf,
}

impl FileMetaStore {
    pub fn new(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn path(&self, key: &str) -> PathBuf {
        self.root.join(key)
    }

    fn lock_path(&self, key: &str) -> PathBuf {
        self.root.join(format!("{key}.lock"))
    }
}

#[async_trait]
impl MetaStore for FileMetaStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        match std::fs::read(self.path(key)) {
            Ok(v) => Ok(Some(v)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn put_if_absent(&self, key: &str, value: Vec<u8>) -> Result<bool> {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.path(key))
        {
            Ok(mut f) => {
                use std::io::Write;
                f.write_all(&value)?;
                Ok(true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    async fn compare_and_swap(
        &self,
        key: &str,
        expected: Option<&[u8]>,
        value: Vec<u8>,
    ) -> Result<bool> {
        // Serialize readers and writers on an advisory lock; the whole
        // read-compare-write is then atomic w.r.t. every cooperating
        // process (flock is per open-file-description).
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(self.lock_path(key))?;
        lock.lock_exclusive()?;
        let result = (|| -> Result<bool> {
            let current = match std::fs::read(self.path(key)) {
                Ok(v) => Some(v),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(e.into()),
            };
            if current.as_deref() != expected {
                return Ok(false);
            }
            let tmp = self.root.join(format!("{key}.tmp"));
            std::fs::write(&tmp, &value)?;
            std::fs::rename(&tmp, self.path(key))?;
            Ok(true)
        })();
        let _ = lock.unlock();
        result
    }

    async fn delete(&self, key: &str) -> Result<()> {
        match std::fs::remove_file(self.path(key)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

/// Object-storage metadata store: keys are objects under a prefix in the
/// shared store, so writers on different machines coordinate through the
/// same S3/GCS/Azure bucket (or shared `file://` root).
#[derive(Debug)]
pub struct ObjectMetaStore {
    store: Arc<dyn ObjectStore>,
    prefix: String,
}

impl ObjectMetaStore {
    pub fn new(store: Arc<dyn ObjectStore>, prefix: impl Into<String>) -> Self {
        Self {
            store,
            prefix: prefix.into(),
        }
    }

    fn key(&self, key: &str) -> ObjPath {
        ObjPath::from(format!("{}{key}", self.prefix))
    }
}

#[async_trait]
impl MetaStore for ObjectMetaStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        match self.store.get(&self.key(key)).await {
            Ok(result) => {
                let bytes = result
                    .bytes()
                    .await
                    .map_err(|e| LessError::ObjectStore(e.to_string()))?;
                Ok(Some(bytes.to_vec()))
            }
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(LessError::ObjectStore(e.to_string())),
        }
    }

    async fn put_if_absent(&self, key: &str, value: Vec<u8>) -> Result<bool> {
        // Conditional create via PutMode::Create: If-None-Match on
        // S3/GCS/Azure, create-exclusive locally.
        let opts = object_store::PutOptions::from(object_store::PutMode::Create);
        match self
            .store
            .put_opts(&self.key(key), Bytes::from(value).into(), opts)
            .await
        {
            Ok(_) => Ok(true),
            Err(object_store::Error::AlreadyExists { .. }) => Ok(false),
            Err(e) => Err(LessError::ObjectStore(e.to_string())),
        }
    }

    async fn compare_and_swap(
        &self,
        _key: &str,
        _expected: Option<&[u8]>,
        _value: Vec<u8>,
    ) -> Result<bool> {
        Err(LessError::NotImplemented(
            "update-CAS on object stores (no transactional compare); use a FileMetaStore \
             (shared host) or etcd (planned)"
                .into(),
        ))
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.store
            .delete(&self.key(key))
            .await
            .map_err(|e| LessError::ObjectStore(e.to_string()))
    }

    fn supports_update_cas(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("less-meta-{tag}-{}", uuid::Uuid::new_v4()))
    }

    #[tokio::test]
    async fn file_cas_semantics() {
        let ms = FileMetaStore::new(tmpdir("file")).unwrap();
        // put_if_absent: first wins.
        assert!(ms.put_if_absent("k", b"one".to_vec()).await.unwrap());
        assert!(!ms.put_if_absent("k", b"two".to_vec()).await.unwrap());
        assert_eq!(ms.get("k").await.unwrap().unwrap(), b"one");
        // CAS: wrong expected → conflict; right expected → swap.
        assert!(
            !ms.compare_and_swap("k", Some(b"nope"), b"x".to_vec())
                .await
                .unwrap()
        );
        assert_eq!(ms.get("k").await.unwrap().unwrap(), b"one");
        assert!(
            ms.compare_and_swap("k", Some(b"one"), b"three".to_vec())
                .await
                .unwrap()
        );
        assert_eq!(ms.get("k").await.unwrap().unwrap(), b"three");
        // CAS on absent key.
        assert!(
            ms.compare_and_swap("fresh", None, b"v".to_vec())
                .await
                .unwrap()
        );
        assert_eq!(ms.get("fresh").await.unwrap().unwrap(), b"v");
        assert!(ms.get("missing").await.unwrap().is_none());
        ms.delete("k").await.unwrap();
        assert!(ms.get("k").await.unwrap().is_none());
        std::fs::remove_dir_all(ms.root).ok();
    }

    #[tokio::test]
    async fn object_cas_semantics() {
        let store: Arc<dyn ObjectStore> = Arc::new(object_store::memory::InMemory::new());
        let ms = ObjectMetaStore::new(store, "metastore/");
        assert!(ms.put_if_absent("a", b"1".to_vec()).await.unwrap());
        assert!(!ms.put_if_absent("a", b"2".to_vec()).await.unwrap());
        assert_eq!(ms.get("a").await.unwrap().unwrap(), b"1");
        assert!(!ms.supports_update_cas());
        assert!(
            ms.compare_and_swap("a", Some(b"1"), b"3".to_vec())
                .await
                .is_err()
        );
        ms.delete("a").await.unwrap();
        assert!(ms.get("a").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn file_cas_contention_two_writers() {
        // Two "processes" sharing the same root: only one insert-CAS wins,
        // and interleaved CAS never corrupts.
        let root = tmpdir("race");
        let a = FileMetaStore::new(root.clone()).unwrap();
        let b = FileMetaStore::new(root).unwrap();
        let (ra, rb) = tokio::join!(
            a.put_if_absent("k", b"a".to_vec()),
            b.put_if_absent("k", b"b".to_vec())
        );
        assert!(ra.unwrap() ^ rb.unwrap());
        let v = a.get("k").await.unwrap().unwrap();
        assert!(v == b"a" || v == b"b");
        std::fs::remove_dir_all(&a.root).ok();
    }
}
