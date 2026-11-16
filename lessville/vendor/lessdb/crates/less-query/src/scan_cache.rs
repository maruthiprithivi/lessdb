//! In-session cache for small parquet metadata reads (file footers).
//!
//! Unlike the shared-store [`less_storage::CachingObjectStore`], a range read
//! that misses here reads *only* that range from the inner store — columnar
//! scans issue many small range reads, and fetching a whole 4 MiB part for
//! each would regress. Parts are immutable, so cached bytes are always valid.
//! Reads larger than [`MAX_CACHED_READ`] (data pages) bypass the cache.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use futures::StreamExt;
use object_store::Result as OsResult;
use object_store::path::Path as ObjPath;
use object_store::{
    Attributes, GetOptions, GetResult, GetResultPayload, ListResult, MultipartUpload, ObjectMeta,
    ObjectStore, PutMultipartOptions, PutOptions, PutPayload, PutResult,
};

/// Only cache reads up to this size — parquet footers/metadata, never
/// column-chunk data pages.
const MAX_CACHED_READ: usize = 1 << 20; // 1 MiB

/// Range-preserving cache for small object reads.
pub struct ScanCache {
    inner: Arc<dyn ObjectStore>,
    map: Mutex<HashMap<(String, usize, usize), Bytes>>,
    total: Mutex<usize>,
    capacity: usize,
}

impl ScanCache {
    pub fn new(inner: Arc<dyn ObjectStore>, capacity: usize) -> Self {
        Self {
            inner,
            map: Mutex::new(HashMap::new()),
            total: Mutex::new(0),
            capacity,
        }
    }

    fn cached(location: &ObjPath, bytes: Bytes, range: std::ops::Range<usize>) -> GetResult {
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

    async fn collect(
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

    fn get_cached(&self, key: &ObjPath, range: &std::ops::Range<usize>) -> Option<Bytes> {
        self.map
            .lock()
            .unwrap()
            .get(&(key.to_string(), range.start, range.end))
            .cloned()
    }

    fn insert(&self, key: &ObjPath, range: &std::ops::Range<usize>, bytes: &Bytes) {
        if bytes.len() > self.capacity {
            return;
        }
        let mut map = self.map.lock().unwrap();
        let mut total = self.total.lock().unwrap();
        map.insert((key.to_string(), range.start, range.end), bytes.clone());
        *total += bytes.len();
        while *total > self.capacity {
            let Some(k) = map.keys().next().cloned() else {
                break;
            };
            if let Some(v) = map.remove(&k) {
                *total = total.saturating_sub(v.len());
            }
        }
    }
}

#[async_trait::async_trait]
impl ObjectStore for ScanCache {
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
        let conditional = options.if_match.is_some()
            || options.if_none_match.is_some()
            || options.if_modified_since.is_some()
            || options.version.is_some();
        // HEAD and conditional reads bypass the cache; whole-object reads
        // pass straight through (we only cache small metadata ranges).
        if conditional || options.head || options.range.is_none() {
            return self.inner.get_opts(location, options).await;
        }

        let requested = options.range.clone().unwrap();
        let range = requested.as_range(usize::MAX as u64).map_err(|source| {
            object_store::Error::Generic {
                store: "local",
                source: Box::new(source),
            }
        })?;
        let (start, end) = (range.start as usize, range.end as usize);

        // Only small metadata reads are cached; data pages pass through.
        if end.saturating_sub(start) > MAX_CACHED_READ {
            return self.inner.get_opts(location, options).await;
        }

        let bytes = if let Some(b) = self.get_cached(location, &(start..end)) {
            b
        } else {
            let result = self.inner.get_opts(location, options).await?;
            let (bytes, _meta, _range, _attributes) = Self::collect(result).await?;
            self.insert(location, &(start..end), &bytes);
            bytes
        };
        Ok(Self::cached(location, bytes, start..end))
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

impl std::fmt::Display for ScanCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ScanCache({} bytes)", self.capacity)
    }
}

impl std::fmt::Debug for ScanCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScanCache")
            .field("capacity", &self.capacity)
            .finish()
    }
}
