//! Exact (brute-force) search index — the baseline every space falls back
//! to when an ANN index isn't built yet.

use crate::metric::Metric;

/// Flat index: all vectors in row-major contiguous memory, scanned
/// linearly with a top-k heap.
pub struct FlatIndex {
    pub dim: usize,
    metric: Metric,
    data: Vec<f32>,
}

impl FlatIndex {
    pub fn new(dim: usize, metric: Metric) -> Self {
        Self {
            dim,
            metric,
            data: vec![],
        }
    }

    pub fn len(&self) -> usize {
        self.data.len() / self.dim
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Append prepared vectors; returns their ids.
    pub fn add(&mut self, vectors: &[f32]) -> Vec<u32> {
        let start = self.len() as u32;
        let n = vectors.len() / self.dim;
        self.data.extend_from_slice(vectors);
        (start..start + n as u32).collect()
    }

    /// Top-k smallest distances. Returns `(id, distance)` ascending.
    pub fn search(&self, query: &[f32], k: usize) -> Vec<(u32, f32)> {
        let n = self.len();
        if n == 0 || k == 0 {
            return vec![];
        }
        let mut heap: std::collections::BinaryHeap<(crate::OrderedF32, u32)> =
            std::collections::BinaryHeap::new();
        for id in 0..n {
            let base = id * self.dim;
            let d = self
                .metric
                .distance(query, &self.data[base..base + self.dim]);
            let item = (crate::OrderedF32(d), id as u32);
            if heap.len() < k {
                heap.push(item);
            } else if item < *heap.peek().unwrap() {
                heap.pop();
                heap.push(item);
            }
        }
        // into_sorted_vec is ascending (BinaryHeap is a max-heap, so its
        // sorted order runs smallest → largest).
        heap.into_sorted_vec()
            .into_iter()
            .map(|(d, id)| (id, d.0))
            .collect()
    }

    /// Raw vector at id (prepared form).
    pub fn vector(&self, id: u32) -> &[f32] {
        let base = id as usize * self.dim;
        &self.data[base..base + self.dim]
    }

    pub fn raw(&self) -> &[f32] {
        &self.data
    }

    pub fn into_raw(self) -> Vec<f32> {
        self.data
    }
}
