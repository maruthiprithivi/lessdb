//! IVF-PQ: inverted-file index with product quantization — the same ANN
//! scheme LanceDB uses (and FAISS before it).
//!
//! * **Training** (k-means, Lloyd's): `nlist` centroids partition the
//!   space; residuals (vector - centroid) are quantized per subspace into
//!   `m` codebooks of `2^nbits` entries each.
//! * **Build**: each vector is assigned to its nearest centroid list and
//!   stored as `m` u8 codes.
//! * **Search** (L2): the query probes the `nprobe` nearest lists;
//!   distances use ADC lookup tables (one per subspace per probed list),
//!   then the final candidates are re-ordered with exact distances.
//! * **Search** (cosine/dot): IVF-flat — exact metric distances over the
//!   probed lists (PQ for non-L2 metrics is roadmap).

use serde::{Deserialize, Serialize};

use crate::metric::Metric;
use crate::rng::Rng;

/// IVF-PQ index parameters.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct IvfPqParams {
    /// Number of inverted lists (centroids).
    pub nlist: usize,
    /// Number of PQ subspaces (`dim` must be divisible by `m`).
    pub m: usize,
    /// Codebook bits per subspace (default 8 → 256 entries).
    pub nbits: u8,
    /// K-means iterations.
    pub niter: usize,
    /// Re-ranking factor: search collects `k * refine_factor` ADC
    /// candidates and re-orders them with exact distances (LanceDB's
    /// `refine_factor`). Default 8 balances recall and cost.
    #[serde(default = "default_refine")]
    pub refine_factor: usize,
}

fn default_refine() -> usize {
    8
}

impl IvfPqParams {
    pub fn for_dim(dim: usize) -> Self {
        let nlist = ((dim as f32).sqrt() as usize).clamp(4, 256);
        Self {
            nlist,
            m: (dim / 2).max(1),
            nbits: 8,
            niter: 10,
            refine_factor: 8,
        }
    }

    pub fn ksub(&self) -> usize {
        1usize << self.nbits
    }
    pub fn subdim(&self, dim: usize) -> usize {
        dim / self.m
    }
}

/// Serialized IVF-PQ state (for `index.bin`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IvfPqSnapshot {
    pub dim: usize,
    pub params: IvfPqParams,
    pub centroids: Vec<f32>,
    pub codebooks: Vec<f32>,
    /// Per list: `(id, codes)`.
    pub lists: Vec<Vec<(u32, Vec<u8>)>>,
    pub next_id: u32,
}

/// The IVF-PQ index.
pub struct IvfPqIndex {
    dim: usize,
    metric: Metric,
    params: IvfPqParams,
    centroids: Vec<f32>,
    codebooks: Vec<f32>,
    lists: Vec<Vec<(u32, Vec<u8>)>>,
    /// All (prepared) vectors, for exact reordering and retraining.
    vectors: Vec<f32>,
    /// Ids appended after the last build (indexed lazily).
    pending: Vec<u32>,
    next_id: u32,
    built: bool,
}

impl IvfPqIndex {
    pub fn new(dim: usize, metric: Metric, params: IvfPqParams) -> Self {
        debug_assert!(dim.is_multiple_of(params.m), "dim must be divisible by m");
        Self {
            dim,
            metric,
            params,
            centroids: vec![],
            codebooks: vec![],
            lists: vec![],
            vectors: vec![],
            pending: vec![],
            next_id: 0,
            built: false,
        }
    }

    pub fn params(&self) -> IvfPqParams {
        self.params
    }

    pub fn len(&self) -> usize {
        self.vectors.len() / self.dim
    }

    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    /// Append prepared vectors; returns their ids.
    pub fn add(&mut self, vectors: &[f32]) -> Vec<u32> {
        let start = self.next_id;
        let n = (vectors.len() / self.dim) as u32;
        self.vectors.extend_from_slice(vectors);
        for id in start..start + n {
            self.pending.push(id);
        }
        self.next_id = start + n;
        (start..start + n).collect()
    }

    /// Raw (prepared) vector at id.
    pub fn vector(&self, id: u32) -> &[f32] {
        let base = id as usize * self.dim;
        &self.vectors[base..base + self.dim]
    }

    pub fn raw(&self) -> &[f32] {
        &self.vectors
    }

    /// (Re)train centroids and codebooks and (re)build the lists.
    pub fn build(&mut self, rng: &mut Rng) {
        let n = self.len();
        if n == 0 {
            return;
        }
        let nlist = self.params.nlist.min(n).max(1);
        self.centroids = kmeans(&self.vectors, self.dim, nlist, self.params.niter, rng);

        if self.metric == Metric::L2 {
            let ksub = self.params.ksub();
            let sub = self.params.subdim(self.dim);
            let m = self.params.m;
            let mut codebooks = vec![0.0f32; m * ksub * sub];
            // Quantize residuals: gather them per subspace.
            for s in 0..m {
                let mut residuals: Vec<f32> = Vec::with_capacity(n * sub);
                for i in 0..n {
                    let c = nearest_centroid(&self.vectors, self.dim, &self.centroids, i);
                    let base = i * self.dim + s * sub;
                    let cbase = c * self.dim + s * sub;
                    for j in 0..sub {
                        residuals.push(self.vectors[base + j] - self.centroids[cbase + j]);
                    }
                }
                let cb = kmeans(&residuals, sub, ksub.min(n.max(1)), self.params.niter, rng);
                let start = s * ksub * sub;
                codebooks[start..start + cb.len()].copy_from_slice(&cb);
            }
            self.codebooks = codebooks;
        }

        // Assign every vector to its nearest centroid; store codes (L2).
        let mut lists: Vec<Vec<(u32, Vec<u8>)>> = vec![vec![]; nlist];
        let sub = self.params.subdim(self.dim);
        let m = self.params.m;
        let ksub = self.params.ksub();
        for id in 0..n {
            let c = nearest_centroid(&self.vectors, self.dim, &self.centroids, id);
            let mut codes = vec![0u8; m];
            if self.metric == Metric::L2 {
                let base = id * self.dim;
                let cbase = c * self.dim;
                for (s, code) in codes.iter_mut().enumerate() {
                    let cb_start = s * ksub * sub;
                    let mut best = (f32::MAX, 0u8);
                    for e in 0..ksub {
                        let mut d = 0.0f32;
                        for j in 0..sub {
                            let diff = self.vectors[base + s * sub + j]
                                - self.centroids[cbase + s * sub + j]
                                - self.codebooks[cb_start + e * sub + j];
                            d += diff * diff;
                        }
                        if d < best.0 {
                            best = (d, e as u8);
                        }
                    }
                    *code = best.1;
                }
            }
            lists[c].push((id as u32, codes));
        }
        self.lists = lists;
        self.pending.clear();
        self.built = true;
    }

    fn maybe_build(&mut self) {
        let threshold = (self.params.nlist * 40).max(64);
        if !self.built && self.pending.len() >= threshold {
            let mut rng = Rng::new(0x5EED_7EC7);
            self.build(&mut rng);
        }
    }

    /// Build the ANN index now if enough vectors are pending (called from
    /// the write path; searches themselves never mutate the index).
    pub fn ensure_built(&mut self) {
        self.maybe_build();
    }

    /// Top-k search. `nprobe` lists probed; results re-ordered with exact
    /// distances. Falls back to an exact scan while unbuilt.
    pub fn search(&self, query: &[f32], k: usize, nprobe: usize) -> Vec<(u32, f32)> {
        let mut q = query.to_vec();
        self.metric.prepare(&mut q);
        let n = self.len();
        if n == 0 || k == 0 {
            return vec![];
        }
        if !self.built {
            // Exact scan fallback.
            return exact_scan(&self.vectors, self.dim, &self.metric, &q, k);
        }
        let nlist = self.lists.len();
        let nprobe = nprobe.clamp(1, nlist);

        // Nearest centroids.
        let mut centroid_dists: Vec<(crate::OrderedF32, usize)> = (0..nlist)
            .map(|c| {
                let d = self
                    .metric
                    .distance(&q, &self.centroids[c * self.dim..(c + 1) * self.dim]);
                (crate::OrderedF32(d), c)
            })
            .collect();
        centroid_dists.sort();
        let probed: Vec<usize> = centroid_dists
            .into_iter()
            .take(nprobe)
            .map(|(_, c)| c)
            .collect();

        // Top-k heap over candidates: keep `k * refine_factor` so exact
        // re-ranking has a margin (ADC ordering is approximate).
        let pool = k * self.params.refine_factor.max(1);
        let mut heap: std::collections::BinaryHeap<(crate::OrderedF32, u32)> =
            std::collections::BinaryHeap::new();
        let push_candidate = |heap: &mut std::collections::BinaryHeap<(crate::OrderedF32, u32)>,
                              item: (crate::OrderedF32, u32),
                              limit: usize| {
            if heap.len() < limit {
                heap.push(item);
            } else if item < *heap.peek().unwrap() {
                heap.pop();
                heap.push(item);
            }
        };

        let sub = self.params.subdim(self.dim);
        let m = self.params.m;
        let ksub = self.params.ksub();
        for &c in &probed {
            if self.metric == Metric::L2 {
                // ADC lookup tables: per subspace, distance from
                // (query - centroid) to each codebook entry.
                let cbase = c * self.dim;
                let mut tables = vec![0.0f32; m * ksub];
                for s in 0..m {
                    let cb_start = s * ksub * sub;
                    for e in 0..ksub {
                        let mut d = 0.0f32;
                        for j in 0..sub {
                            let diff = q[s * sub + j]
                                - self.centroids[cbase + s * sub + j]
                                - self.codebooks[cb_start + e * sub + j];
                            d += diff * diff;
                        }
                        tables[s * ksub + e] = d;
                    }
                }
                for (id, codes) in &self.lists[c] {
                    let mut d = 0.0f32;
                    for s in 0..m {
                        d += tables[s * ksub + codes[s] as usize];
                    }
                    push_candidate(&mut heap, (crate::OrderedF32(d), *id), pool);
                }
            } else {
                // IVF-flat for cosine/dot: exact distances in probed lists.
                for (id, _) in &self.lists[c] {
                    let d = self.metric.distance(&q, self.vector(*id));
                    push_candidate(&mut heap, (crate::OrderedF32(d), *id), pool);
                }
            }
        }

        // Exact re-order of the finalists.
        let mut finalists: Vec<(u32, f32)> = heap
            .into_iter()
            .map(|(_d, id)| {
                let exact = self.metric.distance(&q, self.vector(id));
                (id, exact)
            })
            .collect();
        finalists.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        finalists.truncate(k);
        finalists
    }

    /// Serialize the index state (excluding full vectors; those live in
    /// `data.bin` alongside it).
    pub fn snapshot(&self) -> IvfPqSnapshot {
        IvfPqSnapshot {
            dim: self.dim,
            params: self.params,
            centroids: self.centroids.clone(),
            codebooks: self.codebooks.clone(),
            lists: self.lists.clone(),
            next_id: self.next_id,
        }
    }

    pub fn restore(snapshot: IvfPqSnapshot, vectors: Vec<f32>, metric: Metric) -> Self {
        let built = !snapshot.lists.is_empty() || !snapshot.centroids.is_empty();
        let n = (vectors.len() / snapshot.dim) as u32;
        let mut pending = vec![];
        if built {
            // Ids in vectors but not in any list are pending.
            let n = vectors.len() / snapshot.dim;
            let indexed: std::collections::HashSet<u32> = snapshot
                .lists
                .iter()
                .flat_map(|l| l.iter().map(|(id, _)| *id))
                .collect();
            for id in 0..n as u32 {
                if !indexed.contains(&id) {
                    pending.push(id);
                }
            }
        } else {
            let n = (vectors.len() / snapshot.dim) as u32;
            pending.extend(0..n);
        }
        Self {
            dim: snapshot.dim,
            metric,
            params: snapshot.params,
            centroids: snapshot.centroids,
            codebooks: snapshot.codebooks,
            lists: snapshot.lists,
            vectors,
            pending,
            next_id: snapshot.next_id.max(n),
            built,
        }
    }
}

/// Exact scan over raw vectors (row-major).
fn exact_scan(
    vectors: &[f32],
    dim: usize,
    metric: &Metric,
    q: &[f32],
    k: usize,
) -> Vec<(u32, f32)> {
    let n = vectors.len() / dim;
    let mut heap: std::collections::BinaryHeap<(crate::OrderedF32, u32)> =
        std::collections::BinaryHeap::new();
    for id in 0..n {
        let d = metric.distance(q, &vectors[id * dim..(id + 1) * dim]);
        let item = (crate::OrderedF32(d), id as u32);
        if heap.len() < k {
            heap.push(item);
        } else if item < *heap.peek().unwrap() {
            heap.pop();
            heap.push(item);
        }
    }
    heap.into_sorted_vec()
        .into_iter()
        .map(|(d, id)| (id, d.0))
        .collect()
}

/// Nearest centroid id for vector `i`.
fn nearest_centroid(vectors: &[f32], dim: usize, centroids: &[f32], i: usize) -> usize {
    let base = i * dim;
    let mut best = (f32::MAX, 0usize);
    for c in 0..centroids.len() / dim {
        let mut d = 0.0f32;
        for j in 0..dim {
            let diff = vectors[base + j] - centroids[c * dim + j];
            d += diff * diff;
        }
        if d < best.0 {
            best = (d, c);
        }
    }
    best.1
}

fn l2sq(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| {
            let d = x - y;
            d * d
        })
        .sum()
}

/// Lloyd's k-means with k-means++ initialization and farthest-point
/// reseeding of empty clusters (avoids duplicate-centroid local optima).
pub fn kmeans(data: &[f32], dim: usize, k: usize, niter: usize, rng: &mut Rng) -> Vec<f32> {
    let n = data.len() / dim;
    assert!(n > 0 && k > 0);
    let k = k.min(n);
    let mut centroids = vec![0.0f32; k * dim];

    // k-means++ init: pick successive centroids from points far away from
    // the ones already chosen.
    let first = (rng.next_u64() % n as u64) as usize;
    centroids[..dim].copy_from_slice(&data[first * dim..(first + 1) * dim]);
    for c in 1..k {
        let mut best = (f32::MIN, 0usize);
        for i in 0..n {
            let mut dmin = f32::MAX;
            for cc in 0..c {
                dmin = dmin.min(l2sq(
                    &data[i * dim..(i + 1) * dim],
                    &centroids[cc * dim..(cc + 1) * dim],
                ));
            }
            if dmin > best.0 {
                best = (dmin, i);
            }
        }
        centroids[c * dim..(c + 1) * dim].copy_from_slice(&data[best.1 * dim..(best.1 + 1) * dim]);
    }

    let mut assign = vec![0usize; n];
    for _ in 0..niter {
        // Assign.
        for (i, slot) in assign.iter_mut().enumerate() {
            *slot = nearest_centroid(data, dim, &centroids, i);
        }
        // Update.
        let mut sums = vec![0.0f32; k * dim];
        let mut counts = vec![0usize; k];
        for i in 0..n {
            let c = assign[i];
            counts[c] += 1;
            for j in 0..dim {
                sums[c * dim + j] += data[i * dim + j];
            }
        }
        for c in 0..k {
            if counts[c] == 0 {
                // Re-seed at the point farthest from its own centroid.
                let mut farthest = (f32::MIN, 0usize);
                for i in 0..n {
                    let d = l2sq(
                        &data[i * dim..(i + 1) * dim],
                        &centroids[assign[i] * dim..(assign[i] + 1) * dim],
                    );
                    if d > farthest.0 {
                        farthest = (d, i);
                    }
                }
                centroids[c * dim..(c + 1) * dim]
                    .copy_from_slice(&data[farthest.1 * dim..(farthest.1 + 1) * dim]);
            } else {
                for j in 0..dim {
                    centroids[c * dim + j] = sums[c * dim + j] / counts[c] as f32;
                }
            }
        }
    }
    centroids
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clustered_data(n_per_cluster: usize, nclusters: usize, dim: usize) -> Vec<f32> {
        let mut rng = Rng::new(42);
        let mut data = vec![];
        for c in 0..nclusters {
            // Cluster center along a random direction, magnitude 10.
            let mut center = vec![0.0f32; dim];
            let dir = c % dim;
            center[dir] = 10.0 * (c as f32 + 1.0);
            for _ in 0..n_per_cluster {
                for x in center.iter() {
                    let noise = (rng.f() - 0.5) * 2.0;
                    data.push(x + noise);
                }
            }
        }
        data
    }

    #[test]
    fn kmeans_finds_cluster_centers() {
        let data = clustered_data(50, 4, 8);
        let mut rng = Rng::new(7);
        let centroids = kmeans(&data, 8, 4, 20, &mut rng);
        assert_eq!(centroids.len(), 4 * 8);
        // Each true center direction (0..4 → dims 0..3) should be covered
        // by some centroid with large magnitude.
        for dir in 0..4usize {
            let found = (0..4).any(|c| centroids[c * 8 + dir].abs() > 5.0);
            assert!(found, "no centroid found along dim {dir}");
        }
    }

    #[test]
    fn ivf_pq_recall_vs_flat() {
        let dim = 8;
        let data = clustered_data(200, 16, dim);
        let mut flat = crate::flat::FlatIndex::new(dim, Metric::L2);
        flat.add(&data);

        let params = IvfPqParams {
            nlist: 16,
            m: 4,
            nbits: 8,
            niter: 20,
            refine_factor: 8,
        };
        let mut index = IvfPqIndex::new(dim, Metric::L2, params);
        index.add(&data);
        let mut rng = Rng::new(3);
        index.build(&mut rng);

        // Query with points near random data points.
        let mut hits = 0usize;
        let mut trials = 0usize;
        let mut query_rng = Rng::new(99);
        for _ in 0..50 {
            let base = (query_rng.next_u64() as usize % (data.len() / dim)) * dim;
            let mut q = data[base..base + dim].to_vec();
            for x in q.iter_mut() {
                *x += (query_rng.f() - 0.5) * 2.0;
            }
            let exact: std::collections::HashSet<u32> =
                flat.search(&q, 10).into_iter().map(|(id, _)| id).collect();
            let approx: std::collections::HashSet<u32> = index
                .search(&q, 10, 4)
                .into_iter()
                .map(|(id, _)| id)
                .collect();
            hits += exact.intersection(&approx).count();
            trials += 10;
        }
        let recall = hits as f64 / trials as f64;
        assert!(
            recall > 0.85,
            "IVF-PQ recall too low: {recall:.2} (expected > 0.85)"
        );
    }

    #[test]
    fn cosine_space_uses_ivf_flat() {
        let dim = 4;
        let mut data = vec![];
        for i in 0..100 {
            let mut v = vec![0.0f32; dim];
            v[i % dim] = 1.0;
            Metric::Cosine.prepare(&mut v);
            data.extend(v);
        }
        let mut index = IvfPqIndex::new(
            dim,
            Metric::Cosine,
            IvfPqParams {
                nlist: 8,
                m: 2,
                nbits: 8,
                niter: 5,
                refine_factor: 8,
            },
        );
        index.add(&data);
        let mut rng = Rng::new(1);
        index.build(&mut rng);
        let mut q = vec![1.0f32, 0.0, 0.0, 0.0];
        Metric::Cosine.prepare(&mut q);
        let hits = index.search(&q, 5, 4);
        // Nearest should be dim-0 cluster members (ids 0,4,8,...) — ties
        // among exact matches are broken arbitrarily.
        assert!(hits[0].1 < 1e-6);
        assert_eq!(hits[0].0 % 4, 0);
    }
}
