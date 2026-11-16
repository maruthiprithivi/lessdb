//! Vector spaces: a named collection of same-dimension vectors with one
//! metric and one index — the "multi-space" unit of the registry.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use less_common::{LessError, Result};

use crate::flat::FlatIndex;
use crate::ivf::{IvfPqIndex, IvfPqParams};
use crate::metric::Metric;
use crate::rng::Rng;

/// Index flavor for a space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum IndexKind {
    /// Exact brute-force search.
    Flat,
    /// IVF-PQ approximate nearest neighbors (trained lazily).
    IvfPq(IvfPqParams),
}

impl IndexKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::IvfPq(_) => "ivf_pq",
        }
    }
}

/// The space's index, by flavor.
pub enum Index {
    Flat(FlatIndex),
    IvfPq(IvfPqIndex),
}

/// One search result.
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    /// Row id within the space (dense, insertion order).
    pub id: u32,
    /// Distance: smaller = closer.
    pub score: f32,
    /// Attached payload, if any.
    pub payload: Value,
}

/// Space summary for registry listing.
#[derive(Debug, Clone, Serialize)]
pub struct SpaceInfo {
    pub name: String,
    pub dim: usize,
    pub metric: String,
    pub index: String,
    pub count: usize,
}

/// Serialized space metadata (vectors live in `data.bin`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceMeta {
    pub name: String,
    pub dim: usize,
    pub metric: Metric,
    pub index_kind: IndexKind,
    pub payloads: Vec<Value>,
}

/// A named vector space.
pub struct VectorSpace {
    pub name: String,
    pub dim: usize,
    pub metric: Metric,
    pub index_kind: IndexKind,
    index: Index,
    payloads: Vec<Value>,
}

impl VectorSpace {
    pub fn create(name: &str, dim: usize, metric: Metric, kind: IndexKind) -> Result<Self> {
        if dim == 0 {
            return Err(LessError::Config("vector dimension must be > 0".into()));
        }
        let index = match kind {
            IndexKind::Flat => Index::Flat(FlatIndex::new(dim, metric)),
            IndexKind::IvfPq(params) => {
                if !dim.is_multiple_of(params.m) {
                    return Err(LessError::Config(format!(
                        "dimension {dim} must be divisible by m={}",
                        params.m
                    )));
                }
                Index::IvfPq(IvfPqIndex::new(dim, metric, params))
            }
        };
        Ok(Self {
            name: name.to_string(),
            dim,
            metric,
            index_kind: kind,
            index,
            payloads: vec![],
        })
    }

    pub fn count(&self) -> usize {
        match &self.index {
            Index::Flat(f) => f.len(),
            Index::IvfPq(i) => i.len(),
        }
    }

    fn push_payloads(&mut self, payloads: Vec<Value>) {
        self.payloads.extend(payloads);
    }

    /// Add vectors (validated, metric-prepared) and optional payloads.
    /// Returns the assigned row ids.
    pub fn add(&mut self, vectors: Vec<Vec<f32>>, payloads: Vec<Value>) -> Result<Vec<u32>> {
        let n = vectors.len();
        if !payloads.is_empty() && payloads.len() != n {
            return Err(LessError::Config(format!(
                "got {} payloads for {n} vectors",
                payloads.len()
            )));
        }
        let mut flat: Vec<f32> = Vec::with_capacity(n * self.dim);
        for mut v in vectors {
            if v.len() != self.dim {
                return Err(LessError::Config(format!(
                    "vector has {} dims, space '{}' expects {}",
                    v.len(),
                    self.name,
                    self.dim
                )));
            }
            self.metric.prepare(&mut v);
            flat.extend_from_slice(&v);
        }
        let ids = match &mut self.index {
            Index::Flat(f) => f.add(&flat),
            Index::IvfPq(i) => i.add(&flat),
        };
        let payloads = if payloads.is_empty() {
            vec![Value::Null; n]
        } else {
            payloads
        };
        self.push_payloads(payloads);
        Ok(ids)
    }

    /// Build the ANN index now if enough vectors are pending.
    pub fn ensure_built(&mut self) {
        if let Index::IvfPq(i) = &mut self.index {
            i.ensure_built();
        }
    }

    /// Top-k nearest neighbors. Smaller score = closer.
    pub fn search(&self, query: Vec<f32>, k: usize, nprobe: usize) -> Result<Vec<SearchHit>> {
        if k == 0 {
            return Err(LessError::Config("k must be > 0".into()));
        }
        if query.len() != self.dim {
            return Err(LessError::Config(format!(
                "query has {} dims, space '{}' expects {}",
                query.len(),
                self.name,
                self.dim
            )));
        }
        let mut q = query;
        self.metric.prepare(&mut q);
        let hits: Vec<(u32, f32)> = match &self.index {
            Index::Flat(f) => f.search(&q, k),
            Index::IvfPq(i) => i.search(&q, k, nprobe),
        };
        Ok(hits
            .into_iter()
            .map(|(id, score)| SearchHit {
                id,
                score,
                payload: self
                    .payloads
                    .get(id as usize)
                    .cloned()
                    .unwrap_or(Value::Null),
            })
            .collect())
    }

    /// (Re)build the ANN index now (e.g. after bulk loading).
    pub fn train(&mut self) {
        if let Index::IvfPq(i) = &mut self.index {
            let mut rng = Rng::new(0x5EED_7EC7);
            i.build(&mut rng);
        }
    }

    pub fn info(&self) -> SpaceInfo {
        SpaceInfo {
            name: self.name.clone(),
            dim: self.dim,
            metric: self.metric.name().to_string(),
            index: self.index_kind.name().to_string(),
            count: self.count(),
        }
    }

    pub fn meta(&self) -> SpaceMeta {
        SpaceMeta {
            name: self.name.clone(),
            dim: self.dim,
            metric: self.metric,
            index_kind: self.index_kind,
            payloads: self.payloads.clone(),
        }
    }

    /// Raw prepared vectors (row-major) + index snapshot for persistence.
    pub fn data_and_snapshot(&self) -> (Vec<f32>, Option<crate::ivf::IvfPqSnapshot>) {
        match &self.index {
            Index::Flat(f) => (f.raw().to_vec(), None),
            Index::IvfPq(i) => (i.raw().to_vec(), Some(i.snapshot())),
        }
    }

    pub fn restore(
        meta: SpaceMeta,
        data: Vec<f32>,
        snapshot: Option<crate::ivf::IvfPqSnapshot>,
    ) -> Result<Self> {
        let index = match (&meta.index_kind, snapshot) {
            (IndexKind::Flat, _) => {
                let mut f = FlatIndex::new(meta.dim, meta.metric);
                f.add(&data);
                Index::Flat(f)
            }
            (IndexKind::IvfPq(_), Some(snap)) => {
                Index::IvfPq(IvfPqIndex::restore(snap, data, meta.metric))
            }
            (IndexKind::IvfPq(_), None) => {
                let mut i = IvfPqIndex::new(
                    meta.dim,
                    meta.metric,
                    match meta.index_kind {
                        IndexKind::IvfPq(p) => p,
                        _ => unreachable!(),
                    },
                );
                i.add(&data);
                Index::IvfPq(i)
            }
        };
        Ok(Self {
            name: meta.name,
            dim: meta.dim,
            metric: meta.metric,
            index_kind: meta.index_kind,
            index,
            payloads: meta.payloads,
        })
    }
}
