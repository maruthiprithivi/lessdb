//! Recall@k regression tests: IVF-PQ approximate search vs the exact
//! flat-scan ground truth (LanceDB-style ANN recall).
//!
//! PQ is exercised for L2 (ADC lookup tables + exact re-ranking). Cosine and
//! dot use the IVF-flat path (exact metric distances over probed lists) since
//! product quantization is currently L2-only; both are covered so the whole
//! `Metric` surface is regression-tested.

use less_vector::ivf::IvfPqParams;
use less_vector::metric::Metric;

mod common;

// Parameters chosen to stay fast in an unoptimized `cargo test` build while
// still exercising real PQ: 2000 vectors in 32 dims, 4 well-separated
// gaussian clusters. `nbits = 5` keeps the 32-entry codebooks' k-means cheap
// (the k-means++ init is O(k^2) in codebook size, so 8-bit PQ is far slower
// here without adding meaningful signal for these thresholds).
const DIM: usize = 32;
const N_CLUSTERS: usize = 4;
const N_PER: usize = 500; // 4 * 500 = 2000 vectors
const K: usize = 10;
const N_QUERIES: usize = 50;

fn base_params() -> IvfPqParams {
    IvfPqParams {
        nlist: 16,
        m: 8,     // 32 / 8 = 4 dims per subspace
        nbits: 5, // 32-entry codebooks
        niter: 8,
        refine_factor: 8, // LanceDB default
    }
}

#[test]
fn ivf_pq_l2_recall_default_refine() {
    let metric = Metric::L2;
    let data = common::clustered_data(DIM, N_CLUSTERS, N_PER, 10.0, 1.0, metric, 42);
    let recall = common::recall_at_k(DIM, metric, base_params(), &data, 4, K, N_QUERIES, 7, 1.0);
    eprintln!("L2 recall@10 (default refine_factor=8): {recall:.3}");
    assert!(
        recall >= 0.7,
        "L2 IVF-PQ recall too low: {recall:.3} (want >= 0.7)"
    );
}

#[test]
fn ivf_pq_l2_recall_small_refine() {
    let metric = Metric::L2;
    let mut params = base_params();
    params.refine_factor = 2; // small refine factor
    let data = common::clustered_data(DIM, N_CLUSTERS, N_PER, 10.0, 1.0, metric, 42);
    let recall = common::recall_at_k(DIM, metric, params, &data, 4, K, N_QUERIES, 7, 1.0);
    eprintln!("L2 recall@10 (refine_factor=2): {recall:.3}");
    assert!(
        recall >= 0.5,
        "L2 IVF-PQ recall (refine=2) too low: {recall:.3} (want >= 0.5)"
    );
}

#[test]
fn ivf_pq_l2_recall_more_probes() {
    let metric = Metric::L2;
    let mut params = base_params();
    params.nlist = 32;
    let data = common::clustered_data(DIM, N_CLUSTERS, N_PER, 10.0, 1.0, metric, 42);
    let recall = common::recall_at_k(DIM, metric, params, &data, 8, K, N_QUERIES, 7, 1.0);
    eprintln!("L2 recall@10 (nlist=32, nprobe=8): {recall:.3}");
    assert!(
        recall >= 0.7,
        "L2 IVF-PQ recall (nprobe=8) too low: {recall:.3} (want >= 0.7)"
    );
}

#[test]
fn ivf_cosine_recall() {
    let metric = Metric::Cosine;
    let data = common::clustered_data(DIM, N_CLUSTERS, N_PER, 10.0, 1.0, metric, 43);
    let recall = common::recall_at_k(DIM, metric, base_params(), &data, 4, K, N_QUERIES, 8, 0.1);
    eprintln!("cosine recall@10: {recall:.3}");
    assert!(
        recall >= 0.7,
        "cosine IVF recall too low: {recall:.3} (want >= 0.7)"
    );
}

#[test]
fn ivf_dot_recall() {
    let metric = Metric::Dot;
    let data = common::clustered_data(DIM, N_CLUSTERS, N_PER, 10.0, 1.0, metric, 44);
    let recall = common::recall_at_k(DIM, metric, base_params(), &data, 4, K, N_QUERIES, 9, 0.1);
    eprintln!("dot recall@10: {recall:.3}");
    assert!(
        recall >= 0.7,
        "dot IVF recall too low: {recall:.3} (want >= 0.7)"
    );
}
