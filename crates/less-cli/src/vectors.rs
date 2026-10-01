//! `less vector` — native vector search: spaces (registries), adds, k-NN
//! search, embedding functions. Persisted under `<data_dir>/vectors/`.

use std::path::PathBuf;

use clap::Subcommand;
use less_common::{LessError, Result};
use less_vector::{IndexKind, IvfPqParams, Metric, VectorRegistry};

fn registry_dir(dir: &std::path::Path) -> PathBuf {
    dir.join("vectors")
}

fn parse_vectors_json(s: &str) -> Result<Vec<Vec<f32>>> {
    let v: Vec<Vec<f64>> = serde_json::from_str(s)
        .map_err(|e| LessError::Config(format!("invalid vectors JSON: {e}")))?;
    Ok(v.into_iter()
        .map(|row| row.into_iter().map(|x| x as f32).collect())
        .collect())
}

fn parse_payloads_json(s: &str) -> Result<Vec<serde_json::Value>> {
    let v: Vec<serde_json::Value> = serde_json::from_str(s)
        .map_err(|e| LessError::Config(format!("invalid payloads JSON: {e}")))?;
    Ok(v)
}

#[derive(Subcommand)]
pub enum VectorCmd {
    /// Create a vector space (a named registry of same-dim vectors)
    Create {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        space: String,
        dim: usize,
        #[arg(long, default_value = "l2")]
        metric: String,
        #[arg(long, default_value = "flat")]
        index: String,
        #[arg(long)]
        nlist: Option<usize>,
        #[arg(long)]
        m: Option<usize>,
    },
    /// Add vectors (JSON array of arrays) with optional payloads
    Add {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        space: String,
        /// e.g. '[[0.1,0.2],[0.3,0.4]]'
        #[arg(long)]
        vectors: String,
        /// e.g. '[{"title":"a"}]' — one per vector
        #[arg(long)]
        payloads: Option<String>,
    },
    /// k-nearest-neighbor search
    Search {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        space: String,
        /// e.g. '[0.1,0.2]'
        query: String,
        #[arg(long, default_value_t = 5)]
        k: usize,
        #[arg(long, default_value_t = 8)]
        nprobe: usize,
    },
    /// List vector spaces
    List {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
    },
    /// Embed text with a registered embedding function
    Embed {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        /// e.g. trigram (built-in)
        #[arg(long, default_value = "trigram")]
        embedder: String,
        text: String,
    },
    /// Delete a vector space
    Drop {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        space: String,
    },
}

pub fn run_vector(cmd: &VectorCmd) -> Result<()> {
    match cmd {
        VectorCmd::Create {
            dir,
            space,
            dim,
            metric,
            index,
            nlist,
            m,
        } => {
            let mut reg = VectorRegistry::open(Some(&registry_dir(dir)))?;
            let metric = Metric::parse(metric)?;
            let kind = match index.as_str() {
                "flat" => IndexKind::Flat,
                "ivf_pq" | "ivfpq" => {
                    let mut params = IvfPqParams::for_dim(*dim);
                    if let Some(n) = nlist {
                        params.nlist = *n;
                    }
                    if let Some(mm) = m {
                        params.m = *mm;
                    }
                    IndexKind::IvfPq(params)
                }
                other => {
                    return Err(LessError::Config(format!(
                        "unknown index '{other}' (expected flat | ivf_pq)"
                    )));
                }
            };
            reg.create_space(space, *dim, metric, kind)?;
            println!(
                "created vector space {space} (dim {dim}, metric {}, index {})",
                metric.name(),
                kind.name()
            );
            Ok(())
        }
        VectorCmd::Add {
            dir,
            space,
            vectors,
            payloads,
        } => {
            let vecs = parse_vectors_json(vectors)?;
            let payloads = match payloads {
                Some(p) => parse_payloads_json(p)?,
                None => vec![],
            };
            let mut reg = VectorRegistry::open(Some(&registry_dir(dir)))?;
            let ids = reg.add(space, vecs, payloads)?;
            println!(
                "added {} vectors to {space} (ids {}..{})",
                ids.len(),
                ids.first().copied().unwrap_or(0),
                ids.last().copied().unwrap_or(0)
            );
            Ok(())
        }
        VectorCmd::Search {
            dir,
            space,
            query,
            k,
            nprobe,
        } => {
            let q: Vec<f32> = serde_json::from_str(query)
                .map_err(|e| LessError::Config(format!("invalid query JSON: {e}")))?;
            let reg = VectorRegistry::open(Some(&registry_dir(dir)))?;
            let hits = reg.search(space, q, *k, *nprobe)?;
            if hits.is_empty() {
                println!("(no results)");
            }
            for h in hits {
                println!("id {}  score {:.6}  payload {}", h.id, h.score, h.payload);
            }
            Ok(())
        }
        VectorCmd::List { dir } => {
            let reg = VectorRegistry::open(Some(&registry_dir(dir)))?;
            let infos = reg.list_spaces();
            if infos.is_empty() {
                println!("(no vector spaces)");
            }
            for i in infos {
                println!(
                    "{}  dim={}  metric={}  index={}  count={}",
                    i.name, i.dim, i.metric, i.index, i.count
                );
            }
            Ok(())
        }
        VectorCmd::Embed {
            dir,
            embedder,
            text,
        } => {
            let reg = VectorRegistry::open(Some(&registry_dir(dir)))?;
            let v = reg.embed(embedder, text)?;
            println!("{}", serde_json::to_string(&v)?);
            Ok(())
        }
        VectorCmd::Drop { dir, space } => {
            let mut reg = VectorRegistry::open(Some(&registry_dir(dir)))?;
            match reg.drop_space(space)? {
                true => println!("dropped vector space {space}"),
                false => println!("vector space {space} not found"),
            }
            Ok(())
        }
    }
}
