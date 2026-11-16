//! Vector registry: named vector spaces + embedding-function registry +
//! persistence — the LanceDB-style "registry and search" surface.
//!
//! Layout on disk (`<dir>/`):
//! ```text
//! <dir>/<space>/meta.json   SpaceMeta (name, dim, metric, index, payloads)
//! <dir>/<space>/data.bin    prepared vectors, row-major f32 (bincode)
//! <dir>/<space>/index.bin   IVF-PQ snapshot (bincode; flat spaces: absent)
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

use less_common::{LessError, Result};

use crate::metric::Metric;
use crate::space::{IndexKind, SearchHit, SpaceInfo, VectorSpace};

/// An embedding function: text → vector.
pub type Embedder = Arc<dyn Fn(&str) -> Result<Vec<f32>> + Send + Sync>;

/// The multi-space vector registry.
pub struct VectorRegistry {
    dir: Option<PathBuf>,
    spaces: BTreeMap<String, VectorSpace>,
    embedders: BTreeMap<String, Embedder>,
}

impl VectorRegistry {
    /// Open (creating if needed) a registry at `dir`, loading all spaces.
    pub fn open(dir: Option<&Path>) -> Result<Self> {
        let mut registry = Self {
            dir: dir.map(|p| p.to_path_buf()),
            spaces: BTreeMap::new(),
            embedders: BTreeMap::new(),
        };
        let trigram =
            Arc::new(move |text: &str| -> Result<Vec<f32>> { Ok(trigram_embed(text, 128)) });
        registry.register_embedder("trigram", trigram);
        registry.load()?;
        Ok(registry)
    }

    fn load(&mut self) -> Result<()> {
        let Some(dir) = &self.dir else { return Ok(()) };
        if !dir.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if !path.is_dir() {
                continue;
            }
            let meta_path = path.join("meta.json");
            if !meta_path.exists() {
                continue;
            }
            let meta: crate::space::SpaceMeta =
                serde_json::from_slice(&std::fs::read(&meta_path)?)?;
            let data: Vec<f32> = bincode::serde::decode_from_slice(
                &std::fs::read(path.join("data.bin"))?,
                bincode::config::standard(),
            )
            .map_err(|e| LessError::Config(format!("bad data.bin for '{}': {e}", meta.name)))?
            .0;
            let snapshot = if path.join("index.bin").exists() {
                Some(
                    bincode::serde::decode_from_slice(
                        &std::fs::read(path.join("index.bin"))?,
                        bincode::config::standard(),
                    )
                    .map_err(|e| {
                        LessError::Config(format!("bad index.bin for '{}': {e}", meta.name))
                    })?
                    .0,
                )
            } else {
                None
            };
            let space = VectorSpace::restore(meta, data, snapshot)?;
            self.spaces.insert(space.name.clone(), space);
        }
        Ok(())
    }

    /// Persist one space (atomic-ish: tmp file + rename per file).
    pub fn persist(&self, name: &str) -> Result<()> {
        let Some(dir) = &self.dir else { return Ok(()) };
        let space = self
            .spaces
            .get(name)
            .ok_or_else(|| LessError::Catalog(format!("vector space '{name}' does not exist")))?;
        let space_dir = dir.join(name);
        std::fs::create_dir_all(&space_dir)?;
        let (data, snapshot) = space.data_and_snapshot();
        let write_atomic = |path: &Path, bytes: &[u8]| -> Result<()> {
            let tmp = path.with_extension("tmp");
            std::fs::write(&tmp, bytes)?;
            std::fs::rename(&tmp, path)?;
            Ok(())
        };
        write_atomic(
            &space_dir.join("meta.json"),
            &serde_json::to_vec(&space.meta())?,
        )?;
        write_atomic(
            &space_dir.join("data.bin"),
            &bincode::serde::encode_to_vec(&data, bincode::config::standard())
                .map_err(|e| LessError::Config(e.to_string()))?,
        )?;
        if let Some(snap) = snapshot {
            write_atomic(
                &space_dir.join("index.bin"),
                &bincode::serde::encode_to_vec(&snap, bincode::config::standard())
                    .map_err(|e| LessError::Config(e.to_string()))?,
            )?;
        }
        Ok(())
    }

    // ---- spaces ----------------------------------------------------------

    /// Create a new vector space (fails if it exists).
    pub fn create_space(
        &mut self,
        name: &str,
        dim: usize,
        metric: Metric,
        kind: IndexKind,
    ) -> Result<()> {
        if !is_identifier_like(name) {
            return Err(LessError::Catalog(format!(
                "invalid space name '{name}' ([A-Za-z0-9_.-]+)"
            )));
        }
        if self.spaces.contains_key(name) {
            return Err(LessError::Catalog(format!(
                "vector space '{name}' already exists"
            )));
        }
        let space = VectorSpace::create(name, dim, metric, kind)?;
        self.spaces.insert(name.to_string(), space);
        self.persist(name)?;
        Ok(())
    }

    pub fn drop_space(&mut self, name: &str) -> Result<bool> {
        let removed = self.spaces.remove(name).is_some();
        if removed && let Some(dir) = &self.dir {
            let _ = std::fs::remove_dir_all(dir.join(name));
        }
        Ok(removed)
    }

    pub fn space(&self, name: &str) -> Option<&VectorSpace> {
        self.spaces.get(name)
    }

    pub fn space_mut(&mut self, name: &str) -> Option<&mut VectorSpace> {
        self.spaces.get_mut(name)
    }

    pub fn list_spaces(&self) -> Vec<SpaceInfo> {
        let mut infos: Vec<SpaceInfo> = self.spaces.values().map(|s| s.info()).collect();
        infos.sort_by(|a, b| a.name.cmp(&b.name));
        infos
    }

    /// Add vectors (+optional payloads) to a space; persists. Returns ids.
    pub fn add(
        &mut self,
        name: &str,
        vectors: Vec<Vec<f32>>,
        payloads: Vec<Value>,
    ) -> Result<Vec<u32>> {
        let space = self
            .space_mut(name)
            .ok_or_else(|| LessError::Catalog(format!("vector space '{name}' does not exist")))?;
        let ids = space.add(vectors, payloads)?;
        space.ensure_built();
        self.persist(name)?;
        Ok(ids)
    }

    /// Top-k search in a space (read-only: safe to run concurrently).
    pub fn search(
        &self,
        name: &str,
        query: Vec<f32>,
        k: usize,
        nprobe: usize,
    ) -> Result<Vec<SearchHit>> {
        let space = self
            .space(name)
            .ok_or_else(|| LessError::Catalog(format!("vector space '{name}' does not exist")))?;
        space.search(query, k, nprobe)
    }

    /// Force ANN training for a space (e.g. after bulk load).
    pub fn train(&mut self, name: &str) -> Result<()> {
        let space = self
            .space_mut(name)
            .ok_or_else(|| LessError::Catalog(format!("vector space '{name}' does not exist")))?;
        space.train();
        self.persist(name)
    }

    // ---- embedding functions ---------------------------------------------

    /// Register a named embedding function (LanceDB-style embedding
    /// registry). Built-in: `trigram` (deterministic lexical hashing).
    pub fn register_embedder(&mut self, name: &str, f: Embedder) {
        self.embedders.insert(name.to_string(), f);
    }

    pub fn embedders(&self) -> Vec<String> {
        let mut names: Vec<String> = self.embedders.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn embed(&self, embedder: &str, text: &str) -> Result<Vec<f32>> {
        let f = self.embedders.get(embedder).ok_or_else(|| {
            LessError::Catalog(format!(
                "unknown embedder '{embedder}' (available: {})",
                self.embedders().join(", ")
            ))
        })?;
        f(text)
    }
}

fn is_identifier_like(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// Built-in deterministic embedder: hashes character trigrams (with edge
/// padding) into `dim` buckets and unit-normalizes. No ML, but captures
/// lexical similarity — fine for demos and smoke tests; production
/// embedders register their own functions.
pub fn trigram_embed(text: &str, dim: usize) -> Vec<f32> {
    let mut v = vec![0.0f32; dim];
    let s = format!("  {} ", text.to_ascii_lowercase());
    let bytes = s.as_bytes();
    if bytes.len() >= 3 {
        for w in bytes.windows(3) {
            let mut h: u64 = 0xcbf29ce484222325;
            for b in w {
                h ^= *b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
            v[(h as usize) % dim] += 1.0;
        }
    }
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metric::Metric;

    #[test]
    fn registry_create_add_search_drop() {
        let dir = std::env::temp_dir().join(format!("less-vec-{}", uuid::Uuid::new_v4()));
        let mut reg = VectorRegistry::open(Some(&dir)).unwrap();
        reg.create_space("docs", 4, Metric::L2, IndexKind::Flat)
            .unwrap();
        reg.add(
            "docs",
            vec![
                vec![1.0, 0.0, 0.0, 0.0],
                vec![0.0, 1.0, 0.0, 0.0],
                vec![0.9, 0.1, 0.0, 0.0],
            ],
            vec![
                serde_json::json!({"title": "a"}),
                serde_json::json!({"title": "b"}),
                serde_json::json!({"title": "c"}),
            ],
        )
        .unwrap();
        let hits = reg.search("docs", vec![1.0, 0.0, 0.0, 0.0], 2, 4).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id, 0); // exact match first
        assert_eq!(hits[1].id, 2); // then the near one
        assert_eq!(hits[0].payload["title"], "a");

        let infos = reg.list_spaces();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].count, 3);

        // Persistence roundtrip.
        drop(reg);
        let mut reg = VectorRegistry::open(Some(&dir)).unwrap();
        let hits = reg.search("docs", vec![0.0, 1.0, 0.0, 0.0], 1, 4).unwrap();
        assert_eq!(hits[0].id, 1);

        assert!(reg.drop_space("docs").unwrap());
        assert!(reg.list_spaces().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ivf_pq_space_persists_and_searches() {
        let dir = std::env::temp_dir().join(format!("less-vecivf-{}", uuid::Uuid::new_v4()));
        let mut reg = VectorRegistry::open(Some(&dir)).unwrap();
        reg.create_space(
            "big",
            8,
            Metric::L2,
            IndexKind::IvfPq(crate::ivf::IvfPqParams {
                nlist: 8,
                m: 4,
                nbits: 8,
                niter: 5,
                refine_factor: 8,
            }),
        )
        .unwrap();
        // 8 clusters × 50 points.
        let mut rng = crate::rng::Rng::new(11);
        let mut batch = vec![];
        for c in 0..8usize {
            for _ in 0..50 {
                let mut v = vec![0.0f32; 8];
                v[c % 8] = 10.0 * (c as f32 + 1.0);
                for x in v.iter_mut() {
                    *x += (rng.f() - 0.5) * 2.0;
                }
                batch.push(v);
            }
        }
        reg.add("big", batch, vec![]).unwrap();
        reg.train("big").unwrap();

        let mut q = vec![0.0f32; 8];
        q[0] = 10.0;
        let hits = reg.search("big", q.clone(), 10, 4).unwrap();
        assert_eq!(hits.len(), 10);
        // Nearest hits must be from cluster 0 (ids 0..50).
        assert!(hits.iter().take(8).all(|h| h.id < 50));

        // Reload and search again (snapshot restore path).
        drop(reg);
        let reg = VectorRegistry::open(Some(&dir)).unwrap();
        let hits = reg.search("big", q.clone(), 5, 4).unwrap();
        assert!(hits.iter().all(|h| h.id < 50));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn embedder_registry_builtin() {
        let reg = VectorRegistry::open(None).unwrap();
        assert_eq!(reg.embedders(), vec!["trigram".to_string()]);
        let v = reg.embed("trigram", "hello world").unwrap();
        assert_eq!(v.len(), 128);
        // Unit length.
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4);
        // Similar texts produce closer vectors than dissimilar ones.
        let a = reg.embed("trigram", "hello world").unwrap();
        let b = reg.embed("trigram", "hello world!").unwrap();
        let c = reg.embed("trigram", "quantum chromodynamics").unwrap();
        let dab = Metric::L2.distance(&a, &b);
        let dac = Metric::L2.distance(&a, &c);
        assert!(dab < dac, "similar texts should be closer ({dab} vs {dac})");
    }
}
