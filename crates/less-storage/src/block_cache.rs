//! Block cache for shared-storage reads.
//!
//! FireflyCloud parts are immutable once published, so they are perfect
//! cache fodder: fetch an object once from S3/GCS/Azure (or the local
//! object root) and serve every later read — whole-object gets *and* byte
//! ranges, which is exactly what the parquet reader issues — from local
//! memory or NVMe. This persistent block cache
//!
//! * **memory tier** — bounded LRU keyed by object key, evicted by least
//!   recent use once the byte budget is exceeded;
//! * **disk tier** (optional `block_cache_dir`) — fetched objects are also
//!   written to FNV-1a-keyed files so the cache survives restarts;
//! * **soundness** — only immutable part objects (keys containing
//!   `/parts/`) are cached; mutable control-plane objects (catalog
//!   manifests) always go straight to the backing store, and conditional
//!   (etag/versioned) reads bypass the cache.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use bytes::Bytes;

/// One cached object's stats.
#[derive(Debug, Clone, Copy, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub disk_hits: u64,
    pub entries: usize,
    pub total_bytes: usize,
}

struct Entry {
    stamp: u64,
    bytes: Bytes,
}

struct Inner {
    /// Byte budget; entries larger than this are never cached.
    capacity: usize,
    disk_dir: Option<PathBuf>,
    entries: HashMap<String, Entry>,
    total_bytes: usize,
    next_gen: u64,
    hits: u64,
    misses: u64,
    disk_hits: u64,
}

/// Bounded LRU object cache with an optional persistent disk tier.
pub struct BlockCache {
    inner: Mutex<Inner>,
}

/// FNV-1a 64-bit — deterministic across std versions (unlike
/// `DefaultHasher`), used to name disk-cache files.
fn fnv1a(key: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in key.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

impl BlockCache {
    /// `capacity_bytes` = memory budget (0 disables caching). `disk_dir`,
    /// when set, enables the persistent disk tier.
    pub fn new(capacity_bytes: usize, disk_dir: Option<PathBuf>) -> Self {
        Self {
            inner: Mutex::new(Inner {
                capacity: capacity_bytes,
                disk_dir,
                entries: HashMap::new(),
                total_bytes: 0,
                next_gen: 0,
                hits: 0,
                misses: 0,
                disk_hits: 0,
            }),
        }
    }

    /// Is this object key eligible for caching? Only immutable part
    /// objects (`…/parts/…`): data files and their sidecar metadata are
    /// written once at publication. Catalog manifests and every other
    /// mutable control-plane object bypass the cache.
    pub fn is_cacheable(key: &str) -> bool {
        key.contains("/parts/")
    }

    pub fn get(&self, key: &str) -> Option<Bytes> {
        let mut inner = self.inner.lock().unwrap();
        if inner.capacity == 0 {
            return None;
        }
        // Memory tier.
        if let Some(entry) = inner.entries.get(key) {
            let bytes = entry.bytes.clone();
            let stamp = inner.next_gen;
            inner.next_gen += 1;
            if let Some(entry) = inner.entries.get_mut(key) {
                entry.stamp = stamp;
            }
            inner.hits += 1;
            return Some(bytes);
        }
        // Disk tier.
        let from_disk = inner
            .disk_dir
            .as_ref()
            .and_then(|dir| read_disk_entry(dir, key));
        if let Some(bytes) = from_disk {
            inner.disk_hits += 1;
            inner.insert_entry(key.to_string(), bytes.clone());
            return Some(bytes);
        }
        inner.misses += 1;
        None
    }

    pub fn insert(&self, key: &str, bytes: &Bytes) {
        let mut inner = self.inner.lock().unwrap();
        if inner.capacity == 0 || bytes.len() > inner.capacity {
            return;
        }
        inner.insert_entry(key.to_string(), bytes.clone());
        if let Some(dir) = &inner.disk_dir {
            write_disk_entry(dir, key, bytes);
        }
    }

    pub fn stats(&self) -> CacheStats {
        let inner = self.inner.lock().unwrap();
        CacheStats {
            hits: inner.hits,
            misses: inner.misses,
            disk_hits: inner.disk_hits,
            entries: inner.entries.len(),
            total_bytes: inner.total_bytes,
        }
    }
}

impl Inner {
    fn insert_entry(&mut self, key: String, bytes: Bytes) {
        self.total_bytes += bytes.len();
        self.entries.insert(
            key,
            Entry {
                stamp: self.next_gen,
                bytes,
            },
        );
        self.next_gen += 1;
        // Evict least-recently-used entries until within budget.
        while self.total_bytes > self.capacity {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.stamp)
                .map(|(k, _)| k.clone());
            let Some(oldest) = oldest else { break };
            if let Some(e) = self.entries.remove(&oldest) {
                self.total_bytes = self.total_bytes.saturating_sub(e.bytes.len());
            }
        }
    }
}

fn disk_path(dir: &Path, key: &str) -> PathBuf {
    dir.join(format!("{:016x}.blk", fnv1a(key)))
}

fn read_disk_entry(dir: &Path, key: &str) -> Option<Bytes> {
    std::fs::read(disk_path(dir, key)).ok().map(Bytes::from)
}

/// Best-effort atomic write (tmp + rename); disk-tier failures never
/// fail the read path.
fn write_disk_entry(dir: &Path, key: &str, bytes: &Bytes) {
    let _ = std::fs::create_dir_all(dir).and_then(|_| {
        let tmp = dir.join(format!("{:016x}.tmp", fnv1a(key)));
        std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, disk_path(dir, key)))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("less-blkcache-{tag}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn memory_hit_miss_and_eviction() {
        let cache = BlockCache::new(10, None);
        assert_eq!(cache.get("tables/t/parts/p1/data.parquet"), None);
        cache.insert(
            "tables/t/parts/p1/data.parquet",
            &Bytes::from_static(b"0123456789"),
        );
        assert_eq!(
            &cache.get("tables/t/parts/p1/data.parquet").unwrap()[..],
            b"0123456789"
        );
        // Exceeds capacity → never cached.
        cache.insert(
            "tables/t/parts/p2/data.parquet",
            &Bytes::from_static(b"0123456789A"),
        );
        assert_eq!(cache.get("tables/t/parts/p2/data.parquet"), None);
        // Fits but evicts the older entry.
        cache.insert(
            "tables/t/parts/p2/data.parquet",
            &Bytes::from_static(b"01234"),
        );
        assert_eq!(
            &cache.get("tables/t/parts/p2/data.parquet").unwrap()[..],
            b"01234"
        );
        assert_eq!(cache.get("tables/t/parts/p1/data.parquet"), None);
        let stats = cache.stats();
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.total_bytes, 5);
    }

    #[test]
    fn lru_order_respects_recency() {
        let cache = BlockCache::new(4, None);
        cache.insert("a", &Bytes::from_static(b"11"));
        cache.insert("b", &Bytes::from_static(b"22"));
        // Touch a, then insert c → b is the LRU victim despite a being older.
        let _ = cache.get("a");
        cache.insert("c", &Bytes::from_static(b"33"));
        assert!(cache.get("a").is_some());
        assert!(cache.get("b").is_none());
        assert!(cache.get("c").is_some());
    }

    #[test]
    fn disk_tier_survives_restart() {
        let dir = tmpdir("disk");
        let cache = BlockCache::new(1 << 20, Some(dir.clone()));
        cache.insert(
            "tables/t/parts/p1/data.parquet",
            &Bytes::from_static(b"payload"),
        );
        drop(cache);

        let cache2 = BlockCache::new(1 << 20, Some(dir.clone()));
        assert_eq!(
            &cache2.get("tables/t/parts/p1/data.parquet").unwrap()[..],
            &b"payload"[..]
        );
        assert_eq!(cache2.stats().disk_hits, 1);
        // Now in memory too — a second get is a plain hit.
        assert_eq!(
            &cache2.get("tables/t/parts/p1/data.parquet").unwrap()[..],
            &b"payload"[..]
        );
        let stats = cache2.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.disk_hits, 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cacheable_only_for_parts() {
        assert!(BlockCache::is_cacheable("tables/t/parts/p1/data.parquet"));
        assert!(BlockCache::is_cacheable("tables/t/parts/p1/meta.json"));
        assert!(!BlockCache::is_cacheable("catalog/t.json"));
        assert!(!BlockCache::is_cacheable("tables/t/manifest.json"));
    }
}
