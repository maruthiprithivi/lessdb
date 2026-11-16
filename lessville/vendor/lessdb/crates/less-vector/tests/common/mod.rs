//! Shared helpers for the less-vector integration tests.
//!
//! Everything here is deterministic (seeded xorshift RNG) so the recall and
//! persistence assertions are reproducible across runs.

#![allow(dead_code)]

use std::collections::HashSet;

use less_vector::flat::FlatIndex;
use less_vector::ivf::{IvfPqIndex, IvfPqParams};
use less_vector::metric::Metric;
use less_vector::rng::Rng;

/// Standard-normal sample via Box-Muller, driven by the deterministic RNG.
pub fn gaussian(rng: &mut Rng) -> f32 {
    // u1 in (0, 1] so ln() never sees 0.
    let u1 = 1.0 - rng.f();
    let u2 = rng.f();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos()
}

/// Generate `n_clusters` gaussian clusters of `n_per` points each in `dim`.
///
/// Cluster centers are random unit directions scaled to a common radius
/// `scale`; each point adds isotropic gaussian noise of std `sigma`. Vectors
/// are metric-prepared (unit-normalized for cosine) before returning.
pub fn clustered_data(
    dim: usize,
    n_clusters: usize,
    n_per: usize,
    scale: f32,
    sigma: f32,
    metric: Metric,
    seed: u64,
) -> Vec<Vec<f32>> {
    let mut rng = Rng::new(seed);
    let mut centers = Vec::with_capacity(n_clusters);
    for _ in 0..n_clusters {
        let mut center = vec![0.0f32; dim];
        let mut norm = 0.0f32;
        for x in center.iter_mut() {
            *x = gaussian(&mut rng);
            norm += *x * *x;
        }
        norm = norm.sqrt().max(1e-6);
        for x in center.iter_mut() {
            *x = *x / norm * scale;
        }
        centers.push(center);
    }

    let mut out = Vec::with_capacity(n_clusters * n_per);
    for center in &centers {
        for _ in 0..n_per {
            let mut v = center.clone();
            for x in v.iter_mut() {
                *x += gaussian(&mut rng) * sigma;
            }
            metric.prepare(&mut v);
            out.push(v);
        }
    }
    out
}

/// Flatten row-major vectors into one contiguous `Vec<f32>`.
pub fn flatten(data: &[Vec<f32>]) -> Vec<f32> {
    let mut out = Vec::with_capacity(data.len() * data.first().map_or(0, Vec::len));
    for v in data {
        out.extend_from_slice(v);
    }
    out
}

/// Build both a flat ground-truth index and an IVF-PQ index over `data`
/// (already metric-prepared), then measure mean recall@k over `n_queries`
/// noisy queries drawn from random data points.
#[allow(clippy::too_many_arguments)]
pub fn recall_at_k(
    dim: usize,
    metric: Metric,
    params: IvfPqParams,
    data: &[Vec<f32>],
    nprobe: usize,
    k: usize,
    n_queries: usize,
    query_seed: u64,
    query_sigma: f32,
) -> f64 {
    let flat_vec = flatten(data);
    let mut flat = FlatIndex::new(dim, metric);
    flat.add(&flat_vec);
    let mut index = IvfPqIndex::new(dim, metric, params);
    index.add(&flat_vec);
    let mut build_rng = Rng::new(0x5EED_7EC7);
    index.build(&mut build_rng);

    let n = data.len();
    let mut query_rng = Rng::new(query_seed);
    let mut hits = 0usize;
    let mut total = 0usize;
    for _ in 0..n_queries {
        let base_id = (query_rng.next_u64() as usize) % n;
        let mut q = data[base_id].clone();
        for x in q.iter_mut() {
            *x += gaussian(&mut query_rng) * query_sigma;
        }
        metric.prepare(&mut q);

        let exact: HashSet<u32> = flat.search(&q, k).into_iter().map(|(id, _)| id).collect();
        let approx: HashSet<u32> = index
            .search(&q, k, nprobe)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        hits += exact.intersection(&approx).count();
        total += k;
    }
    hits as f64 / total as f64
}

/// A unique temp directory for one test.
pub fn temp_dir(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("less-vector-{tag}-{}", uuid::Uuid::new_v4()))
}
