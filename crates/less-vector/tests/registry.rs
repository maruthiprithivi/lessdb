//! Registry-level regression tests: persistence roundtrip, multi-space
//! isolation, edge cases, and index rebuild.

use std::collections::HashSet;

use less_vector::ivf::IvfPqParams;
use less_vector::{IndexKind, Metric, SearchHit, VectorRegistry};

mod common;

fn ivf_params() -> IvfPqParams {
    IvfPqParams {
        nlist: 4,
        m: 4,
        nbits: 5,
        niter: 6,
        refine_factor: 8,
    }
}

fn assert_hits_eq(a: &[SearchHit], b: &[SearchHit]) {
    assert_eq!(a.len(), b.len(), "hit count mismatch");
    for (x, y) in a.iter().zip(b.iter()) {
        assert_eq!(x.id, y.id, "hit id mismatch");
        assert!(
            (x.score - y.score).abs() < 1e-5,
            "hit score mismatch: {} vs {}",
            x.score,
            y.score
        );
        assert_eq!(x.payload, y.payload, "hit payload mismatch");
    }
}

#[test]
fn registry_persistence_roundtrip_multi_space() {
    let dir = common::temp_dir("roundtrip");
    let dim = 8;

    let a = common::clustered_data(dim, 4, 20, 5.0, 0.5, Metric::L2, 100);
    let b = common::clustered_data(dim, 4, 20, 5.0, 0.5, Metric::Cosine, 101);
    let c = common::clustered_data(dim, 4, 20, 5.0, 0.5, Metric::Dot, 102);
    let payloads = |space: &str, n: usize| -> Vec<serde_json::Value> {
        (0..n)
            .map(|i| serde_json::json!({"space": space, "i": i}))
            .collect()
    };

    // Phase 1: build a 3-space registry (different metric/index flavors) and
    // record its full observable state.
    let before = {
        let mut reg = VectorRegistry::open(Some(&dir)).unwrap();
        reg.create_space("l2_flat", dim, Metric::L2, IndexKind::Flat)
            .unwrap();
        reg.create_space(
            "cos_ivf",
            dim,
            Metric::Cosine,
            IndexKind::IvfPq(ivf_params()),
        )
        .unwrap();
        reg.create_space("dot_flat", dim, Metric::Dot, IndexKind::Flat)
            .unwrap();

        let ids_a = reg
            .add("l2_flat", a.clone(), payloads("l2_flat", a.len()))
            .unwrap();
        let ids_b = reg
            .add("cos_ivf", b.clone(), payloads("cos_ivf", b.len()))
            .unwrap();
        let ids_c = reg
            .add("dot_flat", c.clone(), payloads("dot_flat", c.len()))
            .unwrap();
        // ids are dense and insertion-ordered.
        assert_eq!(ids_a, (0..a.len() as u32).collect::<Vec<_>>());
        assert_eq!(ids_b, (0..b.len() as u32).collect::<Vec<_>>());
        assert_eq!(ids_c, (0..c.len() as u32).collect::<Vec<_>>());

        // Build the ANN so the persisted `index.bin` snapshot path is what we
        // reopen (not the lazy exact-scan fallback).
        reg.train("cos_ivf").unwrap();

        let hits_a = reg.search("l2_flat", a[0].clone(), 10, 4).unwrap();
        let hits_b = reg.search("cos_ivf", b[0].clone(), 10, 4).unwrap();
        let hits_c = reg.search("dot_flat", c[0].clone(), 10, 4).unwrap();

        let raw_a = reg.space("l2_flat").unwrap().data_and_snapshot().0;
        let raw_b = reg.space("cos_ivf").unwrap().data_and_snapshot().0;
        let raw_c = reg.space("dot_flat").unwrap().data_and_snapshot().0;

        let infos = reg.list_spaces();
        assert_eq!(infos.len(), 3);
        assert_eq!(infos[0].count, 80);
        assert_eq!(infos[1].count, 80);
        assert_eq!(infos[2].count, 80);

        (raw_a, raw_b, raw_c, hits_a, hits_b, hits_c)
    };

    // Phase 2: reopen from the same directory and assert everything matches.
    let reg = VectorRegistry::open(Some(&dir)).unwrap();
    let infos = reg.list_spaces();
    assert_eq!(
        infos
            .iter()
            .map(|i| (
                i.name.as_str(),
                i.dim,
                i.metric.as_str(),
                i.index.as_str(),
                i.count
            ))
            .collect::<Vec<_>>(),
        vec![
            ("cos_ivf", 8, "cosine", "ivf_pq", 80),
            ("dot_flat", 8, "dot", "flat", 80),
            ("l2_flat", 8, "l2", "flat", 80),
        ]
    );

    // Raw vectors identical.
    assert_eq!(
        reg.space("l2_flat").unwrap().data_and_snapshot().0,
        before.0
    );
    assert_eq!(
        reg.space("cos_ivf").unwrap().data_and_snapshot().0,
        before.1
    );
    assert_eq!(
        reg.space("dot_flat").unwrap().data_and_snapshot().0,
        before.2
    );

    // Search results identical.
    assert_hits_eq(
        &reg.search("l2_flat", a[0].clone(), 10, 4).unwrap(),
        &before.3,
    );
    assert_hits_eq(
        &reg.search("cos_ivf", b[0].clone(), 10, 4).unwrap(),
        &before.4,
    );
    assert_hits_eq(
        &reg.search("dot_flat", c[0].clone(), 10, 4).unwrap(),
        &before.5,
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn multi_space_isolation_and_drop() {
    let dir = common::temp_dir("isolation");
    let dim = 8;

    let mut reg = VectorRegistry::open(Some(&dir)).unwrap();
    reg.create_space("a", dim, Metric::L2, IndexKind::Flat)
        .unwrap();
    reg.create_space("b", dim, Metric::L2, IndexKind::Flat)
        .unwrap();

    // Space A vectors sit near +1, space B near -1; payloads tag the origin.
    let mut rng = less_vector::rng::Rng::new(1);
    let mut make = |sign: f32, space: &str| -> (Vec<Vec<f32>>, Vec<serde_json::Value>) {
        let mut vecs = Vec::with_capacity(50);
        let mut payloads = Vec::with_capacity(50);
        for i in 0..50 {
            let mut v = vec![sign; dim];
            for x in v.iter_mut() {
                *x += (rng.f() - 0.5) * 0.2;
            }
            vecs.push(v);
            payloads.push(serde_json::json!({"space": space, "i": i}));
        }
        (vecs, payloads)
    };
    let (a_vecs, a_payloads) = make(1.0, "a");
    let (b_vecs, b_payloads) = make(-1.0, "b");
    reg.add("a", a_vecs, a_payloads).unwrap();
    reg.add("b", b_vecs, b_payloads).unwrap();

    // Searches on A return only A vectors (never B), and vice versa.
    let hits_a = reg.search("a", vec![1.0; dim], 20, 4).unwrap();
    assert_eq!(hits_a.len(), 20);
    assert!(hits_a.iter().all(|h| h.payload["space"] == "a"));

    let hits_b = reg.search("b", vec![-1.0; dim], 20, 4).unwrap();
    assert_eq!(hits_b.len(), 20);
    assert!(hits_b.iter().all(|h| h.payload["space"] == "b"));

    // Dropping A leaves B fully intact.
    assert!(reg.drop_space("a").unwrap());
    let infos = reg.list_spaces();
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].name, "b");
    assert_eq!(infos[0].count, 50);

    let hits_b2 = reg.search("b", vec![-1.0; dim], 10, 4).unwrap();
    assert_eq!(hits_b2.len(), 10);
    assert!(hits_b2.iter().all(|h| h.payload["space"] == "b"));
    assert!(reg.space("a").is_none());
    assert!(reg.search("a", vec![1.0; dim], 5, 4).is_err());

    // B also survives a reopen after A's directory was removed.
    drop(reg);
    let reg = VectorRegistry::open(Some(&dir)).unwrap();
    assert_eq!(reg.list_spaces().len(), 1);
    let hits_b3 = reg.search("b", vec![-1.0; dim], 5, 4).unwrap();
    assert_eq!(hits_b3.len(), 5);
    assert!(hits_b3.iter().all(|h| h.payload["space"] == "b"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn empty_and_edge_cases() {
    let dir = common::temp_dir("edge");
    let mut reg = VectorRegistry::open(Some(&dir)).unwrap();

    // Zero-dimension spaces are rejected cleanly.
    assert!(
        reg.create_space("zero", 0, Metric::L2, IndexKind::Flat)
            .is_err()
    );
    // IVF requires dim % m == 0.
    assert!(
        reg.create_space(
            "badivf",
            7,
            Metric::L2,
            IndexKind::IvfPq(IvfPqParams {
                nlist: 4,
                m: 4,
                nbits: 5,
                niter: 4,
                refine_factor: 8,
            })
        )
        .is_err()
    );

    // Empty space search returns empty.
    reg.create_space("empty", 4, Metric::L2, IndexKind::Flat)
        .unwrap();
    assert!(
        reg.search("empty", vec![1.0, 0.0, 0.0, 0.0], 10, 4)
            .unwrap()
            .is_empty()
    );

    // Single-vector space.
    reg.create_space("single", 4, Metric::L2, IndexKind::Flat)
        .unwrap();
    reg.add("single", vec![vec![1.0, 0.0, 0.0, 0.0]], vec![])
        .unwrap();
    let hits = reg
        .search("single", vec![1.0, 0.0, 0.0, 0.0], 10, 4)
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, 0);
    assert!(hits[0].score.abs() < 1e-6);

    // k > n returns n results (not k).
    reg.create_space("few", 4, Metric::L2, IndexKind::Flat)
        .unwrap();
    reg.add(
        "few",
        vec![
            vec![1.0, 0.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0, 0.0],
            vec![0.0, 0.0, 1.0, 0.0],
        ],
        vec![],
    )
    .unwrap();
    let hits = reg.search("few", vec![1.0, 0.0, 0.0, 0.0], 10, 4).unwrap();
    assert_eq!(hits.len(), 3);
    assert_eq!(hits[0].id, 0);

    // k == 0 is rejected cleanly.
    assert!(reg.search("few", vec![1.0, 0.0, 0.0, 0.0], 0, 4).is_err());

    // Mismatched-dimension query is rejected cleanly.
    assert!(reg.search("few", vec![1.0, 0.0], 10, 4).is_err());
    // Mismatched-dimension insert is rejected cleanly (and leaves the space
    // unchanged).
    assert!(reg.add("few", vec![vec![1.0, 0.0]], vec![]).is_err());
    assert_eq!(reg.space("few").unwrap().count(), 3);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn index_rebuild_after_growth() {
    // There is no per-vector delete API in the registry, so "rebuild after
    // mutation" is exercised as: build, append a second disjoint cluster
    // batch, then `train()` (full rebuild over all vectors) and verify the
    // results match the flat ground truth.
    let dir = common::temp_dir("rebuild");
    let dim = 16;
    let params = IvfPqParams {
        nlist: 8,
        m: 8,
        nbits: 5,
        niter: 8,
        refine_factor: 8,
    };
    let mut reg = VectorRegistry::open(Some(&dir)).unwrap();
    reg.create_space("v", dim, Metric::L2, IndexKind::IvfPq(params))
        .unwrap();

    let batch1 = common::clustered_data(dim, 4, 100, 10.0, 1.0, Metric::L2, 201);
    let batch2 = common::clustered_data(dim, 4, 100, 10.0, 1.0, Metric::L2, 202);
    reg.add("v", batch1.clone(), vec![]).unwrap();
    reg.train("v").unwrap();
    reg.add("v", batch2.clone(), vec![]).unwrap();
    reg.train("v").unwrap(); // rebuild over batch1 ∪ batch2

    let all: Vec<Vec<f32>> = batch1.iter().chain(batch2.iter()).cloned().collect();
    let mut flat = less_vector::FlatIndex::new(dim, Metric::L2);
    flat.add(&common::flatten(&all));

    let k = 10;
    let mut query_rng = less_vector::rng::Rng::new(303);
    let mut hits = 0usize;
    let mut total = 0usize;
    for _ in 0..40 {
        let base = (query_rng.next_u64() as usize) % all.len();
        let mut q = all[base].clone();
        for x in q.iter_mut() {
            *x += common::gaussian(&mut query_rng) * 1.0;
        }
        let exact: HashSet<u32> = flat.search(&q, k).into_iter().map(|(id, _)| id).collect();
        let approx: HashSet<u32> = reg
            .search("v", q, k, 8)
            .unwrap()
            .into_iter()
            .map(|h| h.id)
            .collect();
        hits += exact.intersection(&approx).count();
        total += k;
    }
    let recall = hits as f64 / total as f64;
    eprintln!("rebuild recall@10: {recall:.3}");
    assert!(recall >= 0.7, "rebuild recall too low: {recall:.3}");

    // The rebuilt index also survives persistence/reopen and still matches
    // the flat ground truth.
    drop(reg);
    let reg = VectorRegistry::open(Some(&dir)).unwrap();
    let q = all[0].clone();
    let exact: HashSet<u32> = flat.search(&q, k).into_iter().map(|(id, _)| id).collect();
    let approx: HashSet<u32> = reg
        .search("v", q, k, 8)
        .unwrap()
        .into_iter()
        .map(|h| h.id)
        .collect();
    assert_eq!(exact, approx, "reopened rebuilt index differs from flat");

    let _ = std::fs::remove_dir_all(&dir);
}
