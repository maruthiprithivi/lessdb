//! Shared object storage — the backbone of the FireflyCloud engine and
//! the compute/storage separation.
//!
//! FireflyCloud separates
//! compute from state: immutable data parts *and* table manifests live in
//! shared object storage, and every compute node reads the same objects —
//! so scaling horizontally is just adding stateless compute.
//!
//! [`SharedStore`] abstracts the object layer:
//!
//! * `file:///abs/path` — local filesystem (dev, single-box);
//! * `s3://bucket/prefix` — Amazon S3 / any S3-compatible endpoint
//!   (requires the `cloud` feature; region/auth via standard AWS env vars);
//! * `gcs://bucket/prefix`, `az://account/container/prefix` — Google Cloud
//!   Storage / Azure Blob (same `cloud` feature);
//! * `memory://` — in-memory (tests).
//!
//! All keys are relative to the store's root prefix (`tables/...`,
//! `catalog/...`); the store is registered with DataFusion under the
//! custom scheme-only `lessdb-shared://` URL.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use object_store::Result as OsResult;
use object_store::local::LocalFileSystem;
use object_store::{
    Attributes, GetOptions, GetResult, GetResultPayload, ListResult, MultipartUpload, ObjectMeta,
    ObjectStore, ObjectStoreExt, PutMultipartOptions, PutOptions, PutPayload, PutResult,
    path::Path as ObjPath,
};
use url::Url;

use less_common::{LessError, Result};

use crate::block_cache::BlockCache;

/// Custom scheme all LessDB shared stores register under (scheme-only, as
/// DataFusion requires).
pub const SHARED_STORE_URL: &str = "lessdb-shared://";

use crate::part_io::META_FILE;
use crate::part_meta::PartMeta;

/// A shared object store plus its key prefix.
#[derive(Clone)]
pub struct SharedStore {
    /// Scheme-only base URL the store is registered under with DataFusion.
    base_url: Url,
    /// Key prefix all objects live under (e.g. `my-db/` for
    /// `s3://bucket/my-db`). Empty for local filesystem stores.
    key_prefix: ObjPath,
    /// The underlying object store (possibly wrapped in a block cache).
    store: Arc<dyn ObjectStore>,
    /// Block cache behind [`Self::store`], when configured.
    cache: Option<Arc<BlockCache>>,
    /// Human-readable source URL (for diagnostics).
    source: String,
}

impl std::fmt::Debug for SharedStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedStore")
            .field("source", &self.source)
            .finish()
    }
}

impl SharedStore {
    /// Create a shared store rooted at a local directory (object-store
    /// compatible, cloud-swappable via [`Self::new_with_url`]).
    pub fn new_local(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        let root = std::fs::canonicalize(root)?;
        let store: Arc<dyn ObjectStore> = Arc::new(
            LocalFileSystem::new_with_prefix(&root)
                .map_err(|e| LessError::ObjectStore(e.to_string()))?,
        );
        let base_url = Url::parse(SHARED_STORE_URL)
            .map_err(|e| LessError::Config(format!("invalid shared store url: {e}")))?;
        Ok(Self {
            base_url,
            key_prefix: ObjPath::default(),
            store,
            cache: None,
            source: format!("file://{}", root.display()),
        })
    }

    /// Create a shared store from a URL:
    /// `file:///path`, `s3://bucket/prefix`, `gcs://bucket/prefix`,
    /// `az://account/container/prefix`, `memory://`.
    ///
    /// Cloud schemes require the `cloud` feature (enables the S3/GCS/Azure
    /// object_store backends). Credentials come from the standard
    /// environment (AWS_* / GOOGLE_* / AZURE_*).
    pub fn new_with_url(url_str: &str) -> Result<Self> {
        let url = Url::parse(url_str).map_err(|e| {
            LessError::Config(format!("invalid shared storage url '{url_str}': {e}"))
        })?;
        match url.scheme() {
            "file" => {
                let root = url
                    .to_file_path()
                    .map_err(|_| LessError::Config(format!("invalid file url '{url_str}'")))?;
                Self::new_local(&root)
            }
            scheme => {
                // Cloud backends build from environment + URL so standard
                // AWS_*/GOOGLE_*/AZURE_* variables (credentials, endpoint
                // overrides like AWS_ENDPOINT/AWS_ALLOW_HTTP) apply —
                // `parse_url` alone would ignore them.
                let built: Option<object_store::Result<Box<dyn ObjectStore>>> = match scheme {
                    #[cfg(feature = "cloud")]
                    "s3" => Some(
                        object_store::aws::AmazonS3Builder::from_env()
                            .with_url(url_str)
                            .build()
                            .map(|s| Box::new(s) as Box<dyn ObjectStore>),
                    ),
                    #[cfg(feature = "cloud")]
                    "gcs" => Some(
                        object_store::gcp::GoogleCloudStorageBuilder::from_env()
                            .with_url(url_str)
                            .build()
                            .map(|s| Box::new(s) as Box<dyn ObjectStore>),
                    ),
                    #[cfg(feature = "cloud")]
                    "az" => Some(
                        object_store::azure::MicrosoftAzureBuilder::from_env()
                            .with_url(url_str)
                            .build()
                            .map(|s| Box::new(s) as Box<dyn ObjectStore>),
                    ),
                    _ => None,
                };
                let (store, parsed_prefix) = match built {
                    Some(built) => (
                        built.map_err(|e| {
                            LessError::Config(format!("failed to build '{scheme}' store: {e}"))
                        })?,
                        ObjPath::default(),
                    ),
                    None => object_store::parse_url(&url).map_err(|_| {
                        LessError::Config(format!(
                            "unsupported shared storage scheme '{scheme}' (supported: file, s3, gcs, az, memory; \
                             s3/gcs/az require building with the `cloud` feature)"
                        ))
                    })?,
                };
                // Cloud builders root the store at the bucket/container;
                // the URL path is the key prefix (the same convention as
                // `parse_url`).
                let url_prefix = url.path().trim_start_matches('/');
                let key_prefix = if url_prefix.is_empty() {
                    parsed_prefix
                } else {
                    ObjPath::from(url_prefix)
                };
                let base_url = Url::parse(SHARED_STORE_URL)
                    .map_err(|e| LessError::Config(format!("invalid shared store url: {e}")))?;
                Ok(Self {
                    base_url,
                    key_prefix,
                    store: store.into(),
                    cache: None,
                    source: url_str.to_string(),
                })
            }
        }
    }

    /// Wrap this store's reads in a block cache (see [`BlockCache`]).
    /// Idempotent: calling twice keeps the first cache.
    pub fn with_cache(mut self, cache: Arc<BlockCache>) -> Self {
        if self.cache.is_none() {
            self.store = Arc::new(CachingObjectStore::new(self.store.clone(), cache.clone()));
            self.cache = Some(cache);
        }
        self
    }

    /// The block cache backing this store's reads, when configured.
    pub fn cache(&self) -> Option<&Arc<BlockCache>> {
        self.cache.as_ref()
    }

    pub fn base_url(&self) -> Url {
        self.base_url.clone()
    }

    pub fn store(&self) -> Arc<dyn ObjectStore> {
        self.store.clone()
    }

    /// The key prefix all objects live under (empty for stores rooted at
    /// the bucket/container directly).
    pub fn key_prefix(&self) -> ObjPath {
        self.key_prefix.clone()
    }

    /// Human-readable source of this store (bucket/prefix or path).
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Join the store key prefix with a relative key.
    fn full_key(&self, key: &str) -> ObjPath {
        let mut parts: Vec<_> = self.key_prefix.parts().collect();
        parts.extend(
            key.split('/')
                .filter(|p| !p.is_empty())
                .map(object_store::path::PathPart::from),
        );
        ObjPath::from_iter(parts)
    }

    pub async fn put(&self, key: &str, bytes: Bytes) -> Result<()> {
        self.store
            .put(&self.full_key(key), bytes.into())
            .await
            .map_err(|e| LessError::ObjectStore(e.to_string()))?;
        Ok(())
    }

    /// Atomically publish `key = bytes` only if the key is absent
    /// (conditional create: `PutMode::Create` — `If-None-Match` on S3/GCS/
    /// Azure, create-exclusive locally).
    /// `Ok(true)` = published by this call; `Ok(false)` = already existed.
    pub async fn put_if_absent(&self, key: &str, bytes: Bytes) -> Result<bool> {
        let opts = object_store::PutOptions::from(object_store::PutMode::Create);
        match self
            .store
            .put_opts(&self.full_key(key), bytes.into(), opts)
            .await
        {
            Ok(_) => Ok(true),
            Err(object_store::Error::AlreadyExists { .. }) => Ok(false),
            Err(e) => Err(LessError::ObjectStore(e.to_string())),
        }
    }

    pub async fn get(&self, key: &str) -> Result<Bytes> {
        let result = self
            .store
            .get(&self.full_key(key))
            .await
            .map_err(|e| LessError::ObjectStore(e.to_string()))?;
        result
            .bytes()
            .await
            .map_err(|e| LessError::ObjectStore(e.to_string()))
    }

    /// Like [`Self::get`], but returns `None` instead of an error when the
    /// object does not exist.
    pub async fn get_opt(&self, key: &str) -> Result<Option<Bytes>> {
        match self.get(key).await {
            Ok(b) => Ok(Some(b)),
            Err(LessError::ObjectStore(msg)) if msg.to_ascii_lowercase().contains("not found") => {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        self.store
            .delete(&self.full_key(key))
            .await
            .map_err(|e| LessError::ObjectStore(e.to_string()))
    }

    /// Does an object exist?
    pub async fn exists(&self, key: &str) -> Result<bool> {
        match self.store.head(&self.full_key(key)).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(LessError::ObjectStore(e.to_string())),
        }
    }

    /// Size in bytes of an object under the store prefix (store-relative
    /// key), or an error when it does not exist.
    pub async fn head_size(&self, key: &str) -> Result<u64> {
        let meta = self
            .store
            .head(&self.full_key(key))
            .await
            .map_err(|e| LessError::ObjectStore(e.to_string()))?;
        Ok(meta.size as u64)
    }

    /// List store-relative keys under `prefix` (recursive).
    pub async fn list_keys(&self, prefix: &str) -> Result<Vec<String>> {
        let mut out = vec![];
        let mut stream = self.store.list(Some(&self.full_key(prefix)));
        while let Some(item) = stream.next().await {
            let item = item.map_err(|e| LessError::ObjectStore(e.to_string()))?;
            out.push(self.rel_key(&item.location)?);
        }
        out.sort();
        Ok(out)
    }

    /// Delete every object under `prefix` (used by table drops).
    pub async fn delete_prefix(&self, prefix: &str) -> Result<usize> {
        let keys = self.list_keys(prefix).await?;
        let mut removed = 0usize;
        for key in keys {
            self.delete(&key).await?;
            removed += 1;
        }
        Ok(removed)
    }

    /// List part metadata under `prefix` (e.g. `tables/t/parts/`),
    /// returning `(part_dir_key, PartMeta)` ordered oldest-first.
    pub async fn list_part_metas(&self, prefix: &str) -> Result<Vec<(String, PartMeta)>> {
        let mut out: Vec<(String, PartMeta)> = vec![];
        let mut stream = self.store.list(Some(&self.full_key(prefix)));
        while let Some(item) = stream.next().await {
            let item = item.map_err(|e| LessError::ObjectStore(e.to_string()))?;
            if item.location.filename() == Some(META_FILE) {
                let result = self
                    .store
                    .get(&item.location)
                    .await
                    .map_err(|e| LessError::ObjectStore(e.to_string()))?;
                let bytes = result
                    .bytes()
                    .await
                    .map_err(|e| LessError::ObjectStore(e.to_string()))?;
                let meta: PartMeta = serde_json::from_slice(&bytes)?;
                let key = self.rel_key(&item.location)?;
                out.push((key, meta));
            }
        }
        out.sort_by(|a, b| {
            a.1.created_at
                .cmp(&b.1.created_at)
                .then(a.1.name.cmp(&b.1.name))
        });
        Ok(out)
    }

    /// Convert a store-listing location into a LessDB-relative key (strips
    /// the store's key prefix).
    fn rel_key(&self, location: &ObjPath) -> Result<String> {
        let rel: String = location.as_ref().trim_start_matches('/').to_string();
        let prefix_len = self.key_prefix.parts().count();
        if prefix_len == 0 {
            return Ok(rel);
        }
        let parts: Vec<&str> = rel.split('/').collect();
        if parts.len() <= prefix_len {
            return Ok(rel);
        }
        Ok(parts[prefix_len..].join("/"))
    }
}

/// An [`ObjectStore`] decorator that serves immutable part objects
/// (`…/parts/…`) from a [`BlockCache`].
///
/// Whole-object gets hit the memory/disk tiers directly. Byte-range reads
/// (what the parquet reader issues) fetch the full object once, cache it,
/// and slice the requested range — every subsequent range of the same part
/// is served locally. Conditional (etag/versioned) reads and non-part keys
/// bypass the cache entirely, so mutable control-plane objects are never
/// stale.
pub struct CachingObjectStore {
    inner: Arc<dyn ObjectStore>,
    cache: Arc<BlockCache>,
}

impl std::fmt::Debug for CachingObjectStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachingObjectStore")
            .field("inner", &self.inner)
            .field("stats", &self.cache.stats())
            .finish()
    }
}

impl std::fmt::Display for CachingObjectStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CachingObjectStore({})", self.inner)
    }
}

impl CachingObjectStore {
    pub fn new(inner: Arc<dyn ObjectStore>, cache: Arc<BlockCache>) -> Self {
        Self { inner, cache }
    }

    fn cached_result(location: &ObjPath, bytes: Bytes, range: std::ops::Range<usize>) -> GetResult {
        let len = bytes.len() as u64;
        GetResult {
            payload: GetResultPayload::Stream(Box::pin(futures::stream::once(
                async move { Ok(bytes) },
            ))),
            meta: ObjectMeta {
                location: location.clone(),
                last_modified: chrono::Utc::now(),
                size: len,
                e_tag: None,
                version: None,
            },
            range: range.start as u64..range.end as u64,
            attributes: Attributes::default(),
        }
    }

    /// Collect a get result's payload into memory (handles both the stream
    /// and local-file variants).
    async fn collect_payload(
        result: GetResult,
    ) -> OsResult<(Bytes, ObjectMeta, std::ops::Range<u64>, Attributes)> {
        let GetResult {
            payload,
            meta,
            range,
            attributes,
        } = result;
        let bytes = match payload {
            GetResultPayload::Stream(mut s) => {
                let mut out = Vec::with_capacity(range.end.saturating_sub(range.start) as usize);
                while let Some(chunk) = s.next().await {
                    out.extend_from_slice(&chunk?);
                }
                Bytes::from(out)
            }
            GetResultPayload::File(_file, path) => {
                // Re-read the object range ourselves with positional I/O so
                // the cached bytes are independent of the store's file
                // handle state.
                use std::os::unix::fs::FileExt;
                let start = range.start as usize;
                let len = (range.end - range.start) as usize;
                let mut buf = vec![0u8; len];
                let file =
                    std::fs::File::open(&path).map_err(|e| object_store::Error::Generic {
                        store: "local",
                        source: Box::new(e),
                    })?;
                file.read_exact_at(&mut buf, start as u64).map_err(|e| {
                    object_store::Error::Generic {
                        store: "local",
                        source: Box::new(e),
                    }
                })?;
                Bytes::from(buf)
            }
        };
        Ok((bytes, meta, range, attributes))
    }
}

#[async_trait]
impl ObjectStore for CachingObjectStore {
    async fn put_opts(
        &self,
        location: &ObjPath,
        payload: PutPayload,
        opts: PutOptions,
    ) -> OsResult<PutResult> {
        self.inner.put_opts(location, payload, opts).await
    }

    async fn put_multipart_opts(
        &self,
        location: &ObjPath,
        opts: PutMultipartOptions,
    ) -> OsResult<Box<dyn MultipartUpload>> {
        self.inner.put_multipart_opts(location, opts).await
    }

    async fn get_opts(&self, location: &ObjPath, options: GetOptions) -> OsResult<GetResult> {
        let key = location.as_ref();
        let conditional = options.if_match.is_some()
            || options.if_none_match.is_some()
            || options.if_modified_since.is_some()
            || options.version.is_some();
        // HEAD requests carry no body: caching their empty payload would
        // poison the entry for the real object. Pass them through.
        if conditional || options.head || !BlockCache::is_cacheable(key) {
            return self.inner.get_opts(location, options).await;
        }

        // Whole-object read: serve from cache when possible.
        if options.range.is_none() {
            if let Some(bytes) = self.cache.get(key) {
                let len = bytes.len();
                return Ok(Self::cached_result(location, bytes, 0..len));
            }
            let result = self.inner.get_opts(location, options).await?;
            let (bytes, meta, range, attributes) = Self::collect_payload(result).await?;
            if !bytes.is_empty() {
                self.cache.insert(key, &bytes);
            }
            return Ok(GetResult {
                payload: GetResultPayload::Stream(Box::pin(futures::stream::once(async move {
                    Ok(bytes)
                }))),
                meta,
                range,
                attributes,
            });
        }

        // Range read: fetch the full object once and slice it.
        let requested = options.range.clone().unwrap();
        let bytes = match self.cache.get(key) {
            Some(bytes) => bytes,
            None => {
                let result = self.inner.get_opts(location, GetOptions::default()).await?;
                let (bytes, _meta, _range, _attributes) = Self::collect_payload(result).await?;
                if !bytes.is_empty() {
                    self.cache.insert(key, &bytes);
                }
                bytes
            }
        };
        let range = requested.as_range(bytes.len() as u64).map_err(|source| {
            object_store::Error::Generic {
                store: "object",
                source: Box::new(source),
            }
        })?;
        let (start, end) = (range.start as usize, range.end as usize);
        Ok(Self::cached_result(
            location,
            bytes.slice(start..end),
            start..end,
        ))
    }

    async fn list_with_delimiter(&self, prefix: Option<&ObjPath>) -> OsResult<ListResult> {
        self.inner.list_with_delimiter(prefix).await
    }

    fn delete_stream(
        &self,
        locations: futures::stream::BoxStream<'static, OsResult<ObjPath>>,
    ) -> futures::stream::BoxStream<'static, OsResult<ObjPath>> {
        self.inner.delete_stream(locations)
    }

    fn list(
        &self,
        prefix: Option<&ObjPath>,
    ) -> futures::stream::BoxStream<'static, OsResult<ObjectMeta>> {
        self.inner.list(prefix)
    }

    async fn copy_opts(
        &self,
        from: &ObjPath,
        to: &ObjPath,
        options: object_store::CopyOptions,
    ) -> OsResult<()> {
        self.inner.copy_opts(from, to, options).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::memory::InMemory;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A pass-through store counting get_opts calls (to assert caching).
    struct CountingStore {
        inner: InMemory,
        gets: AtomicUsize,
    }

    impl std::fmt::Display for CountingStore {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "CountingStore")
        }
    }
    impl std::fmt::Debug for CountingStore {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "CountingStore")
        }
    }

    #[async_trait]
    impl ObjectStore for CountingStore {
        async fn put_opts(
            &self,
            location: &ObjPath,
            payload: PutPayload,
            opts: PutOptions,
        ) -> OsResult<PutResult> {
            self.inner.put_opts(location, payload, opts).await
        }
        async fn put_multipart_opts(
            &self,
            location: &ObjPath,
            opts: PutMultipartOptions,
        ) -> OsResult<Box<dyn MultipartUpload>> {
            self.inner.put_multipart_opts(location, opts).await
        }
        async fn get_opts(&self, location: &ObjPath, options: GetOptions) -> OsResult<GetResult> {
            self.gets.fetch_add(1, Ordering::SeqCst);
            self.inner.get_opts(location, options).await
        }
        async fn list_with_delimiter(&self, prefix: Option<&ObjPath>) -> OsResult<ListResult> {
            self.inner.list_with_delimiter(prefix).await
        }
        fn delete_stream(
            &self,
            locations: futures::stream::BoxStream<'static, OsResult<ObjPath>>,
        ) -> futures::stream::BoxStream<'static, OsResult<ObjPath>> {
            self.inner.delete_stream(locations)
        }
        fn list(
            &self,
            prefix: Option<&ObjPath>,
        ) -> futures::stream::BoxStream<'static, OsResult<ObjectMeta>> {
            self.inner.list(prefix)
        }
        async fn copy_opts(
            &self,
            from: &ObjPath,
            to: &ObjPath,
            options: object_store::CopyOptions,
        ) -> OsResult<()> {
            self.inner.copy_opts(from, to, options).await
        }
    }

    fn cached_store() -> (Arc<CachingObjectStore>, Arc<BlockCache>, Arc<CountingStore>) {
        let inner = Arc::new(CountingStore {
            inner: InMemory::new(),
            gets: AtomicUsize::new(0),
        });
        let cache = Arc::new(BlockCache::new(1 << 20, None));
        let wrapped = Arc::new(CachingObjectStore::new(inner.clone(), cache.clone()));
        (wrapped, cache, inner)
    }

    #[tokio::test]
    async fn whole_object_gets_are_cached() {
        let (store, cache, inner) = cached_store();
        let key = ObjPath::from("tables/t/parts/p1/data.parquet");
        store
            .put(&key, Bytes::from_static(b"hello world").into())
            .await
            .unwrap();
        for _ in 0..3 {
            let got = store.get(&key).await.unwrap().bytes().await.unwrap();
            assert_eq!(&got[..], b"hello world");
        }
        assert_eq!(inner.gets.load(Ordering::SeqCst), 1);
        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.entries, 1);
    }

    #[tokio::test]
    async fn range_reads_fetch_full_object_once() {
        let (store, cache, inner) = cached_store();
        let key = ObjPath::from("tables/t/parts/p1/data.parquet");
        store
            .put(&key, Bytes::from_static(b"0123456789").into())
            .await
            .unwrap();
        for _ in 0..3 {
            let got = store.get_range(&key, 2..6).await.unwrap();
            assert_eq!(&got[..], b"2345");
        }
        // One full-object fetch to prime the cache, then all slices local.
        assert_eq!(inner.gets.load(Ordering::SeqCst), 1);
        assert_eq!(cache.stats().hits, 2);
        // Suffix-style range also works and stays within bounds.
        let tail = store.get_range(&key, 8..100).await.unwrap();
        assert_eq!(&tail[..], b"89");
    }

    #[tokio::test]
    async fn non_part_keys_bypass_the_cache() {
        let (store, cache, inner) = cached_store();
        let key = ObjPath::from("catalog/t.json");
        store
            .put(&key, Bytes::from_static(b"{}").into())
            .await
            .unwrap();
        let _ = store.get(&key).await.unwrap().bytes().await.unwrap();
        let _ = store.get(&key).await.unwrap().bytes().await.unwrap();
        assert_eq!(inner.gets.load(Ordering::SeqCst), 2);
        assert_eq!(cache.stats().entries, 0);
    }

    #[tokio::test]
    async fn conditional_reads_bypass_the_cache() {
        let (store, _, inner) = cached_store();
        let key = ObjPath::from("tables/t/parts/p1/meta.json");
        store
            .put(&key, Bytes::from_static(b"{}").into())
            .await
            .unwrap();
        let _ = store.get(&key).await.unwrap().bytes().await.unwrap();
        // Whatever the precondition outcome, the conditional read must have
        // reached the backing store instead of the cache.
        let _conditional = store
            .get_opts(
                &key,
                GetOptions::new().with_if_match(Some::<String>("etag".into())),
            )
            .await;
        assert_eq!(inner.gets.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn put_get_delete_roundtrip() {
        let dir = std::env::temp_dir().join(format!("less-shared-{}", uuid::Uuid::new_v4()));
        let store = SharedStore::new_local(&dir).unwrap();
        store
            .put(
                "tables/t/parts/p1/data.parquet",
                Bytes::from_static(b"hello"),
            )
            .await
            .unwrap();
        let got = store.get("tables/t/parts/p1/data.parquet").await.unwrap();
        assert_eq!(&got[..], b"hello");
        store
            .delete("tables/t/parts/p1/data.parquet")
            .await
            .unwrap();
        assert!(store.get("tables/t/parts/p1/data.parquet").await.is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn file_url_constructor_and_listing() {
        let dir = std::env::temp_dir().join(format!("less-shared-url-{}", uuid::Uuid::new_v4()));
        let url = format!("file://{}", dir.display());
        let store = SharedStore::new_with_url(&url).unwrap();
        store
            .put("catalog/t.json", Bytes::from_static(b"{}"))
            .await
            .unwrap();
        store
            .put("tables/t/parts/p1/data.parquet", Bytes::from_static(b"x"))
            .await
            .unwrap();
        store
            .put("tables/t/parts/p1/meta.json", Bytes::from_static(b"{}"))
            .await
            .unwrap();
        assert!(store.exists("catalog/t.json").await.unwrap());
        assert!(!store.exists("catalog/nope.json").await.unwrap());
        let keys = store.list_keys("tables/t/").await.unwrap();
        assert_eq!(keys.len(), 2);
        let removed = store.delete_prefix("tables/t/").await.unwrap();
        assert_eq!(removed, 2);
        assert!(store.list_keys("tables/t/").await.unwrap().is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn memory_url_constructor() {
        let store = SharedStore::new_with_url("memory://").unwrap();
        store.put("k", Bytes::from_static(b"v")).await.unwrap();
        assert_eq!(&store.get("k").await.unwrap()[..], b"v");
    }
}
