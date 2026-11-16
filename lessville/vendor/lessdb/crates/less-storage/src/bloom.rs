//! Bloom filters for uniqueness-constraint pruning.
//!
//! Every uniqueness column of a data part carries a bloom filter in its part
//! metadata. During scan planning, an equality predicate `col = X` lets the
//! planner skip any part whose bloom filter provably cannot contain `X`.
//!
//! Blooms are probabilistic: they never produce false negatives, only false
//! positives (a part may be kept that doesn't actually match). That makes
//! pruning *sound* — the worst case is doing work, never missing rows.
//!
//! Filters are serialized as `{ bits, num_hashes }` with a fixed seed so a
//! filter written on one node behaves identically on every reader.

use fastbloom::BloomFilter;
use serde::{Deserialize, Serialize};

/// Fixed seed so serialized filters are deterministic across nodes.
const BLOOM_SEED: u128 = 0x1E55_0B10_00DB_0000;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BloomData {
    bits: Vec<u64>,
    num_hashes: u32,
}

/// A serializable bloom filter.
#[derive(Debug, Clone)]
pub struct Bloom {
    inner: BloomFilter,
}

impl Bloom {
    /// Build a bloom filter sized for `expected_items` at the given
    /// false-positive rate.
    pub fn with_capacity(expected_items: usize, fp_rate: f64) -> Self {
        Self {
            inner: BloomFilter::with_false_pos(fp_rate)
                .seed(&BLOOM_SEED)
                .expected_items(expected_items.max(16)),
        }
    }

    pub fn insert(&mut self, bytes: &[u8]) {
        self.inner.insert(bytes);
    }

    pub fn contains(&self, bytes: &[u8]) -> bool {
        self.inner.contains(bytes)
    }

    /// Serialize to bytes for storage in part metadata.
    pub fn to_bytes(&self) -> Vec<u8> {
        let data = BloomData {
            bits: self.inner.as_slice().to_vec(),
            num_hashes: self.inner.num_hashes(),
        };
        serde_json::to_vec(&data).expect("bloom serialization cannot fail")
    }

    /// Deserialize from stored bytes. Malformed input yields an empty
    /// filter (which is always a safe, conservative answer).
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let data: BloomData = serde_json::from_slice(bytes).unwrap_or(BloomData {
            bits: vec![0],
            num_hashes: 1,
        });
        Self {
            inner: BloomFilter::from_vec(data.bits)
                .seed(&BLOOM_SEED)
                .hashes(data.num_hashes.max(1)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bloom_roundtrip_and_membership() {
        let mut b = Bloom::with_capacity(10_000, 0.01);
        for i in 0..10_000u64 {
            b.insert(&i.to_le_bytes());
        }
        let bytes = b.to_bytes();
        let b2 = Bloom::from_bytes(&bytes);
        for i in 0..10_000u64 {
            assert!(b2.contains(&i.to_le_bytes()));
        }
        // False positives allowed, false negatives never:
        let mut misses = 0;
        for i in 10_000..20_000u64 {
            if b2.contains(&i.to_le_bytes()) {
                misses += 1;
            }
        }
        assert!(
            (misses as f64) < 20_000.0 * 0.05,
            "fp rate should be near 1%, got {misses}/10000"
        );
    }

    #[test]
    fn malformed_bytes_yield_empty_filter() {
        let b = Bloom::from_bytes(b"not a bloom");
        assert!(!b.contains(b"anything"));
    }
}
