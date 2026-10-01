//! `less context` / `less memory`: the in-memory tier for agent contexts —
//! titled notes, tags, typed links, graph traversal, and SQL over
//! RAM-resident tables. Persisted under `<data_dir>/memory/`.

use std::path::PathBuf;

use clap::Subcommand;

use less_common::Result;
use less_graph::{ContextStore, Direction};
use less_memory::MemoryStore;

fn context_dir(dir: &std::path::Path) -> PathBuf {
    dir.join("memory")
}

#[derive(Subcommand)]
pub enum ContextCmd {
    /// Put (create or update) a context note
    Put {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        /// Stable key, e.g. proj/lessdb or task/123
        key: String,
        title: String,
        text: String,
        #[arg(long)]
        tag: Vec<String>,
        #[arg(long, default_value = "note")]
        kind: String,
    },
    /// Get a context by key
    Get {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        key: String,
    },
    /// Search contexts (key/label/property substring, ranked)
    Find {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        query: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Link two nodes with a typed edge
    Link {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        from: String,
        to: String,
        kind: String,
        #[arg(long)]
        directed: bool,
    },
    /// Remove link(s); all kinds when --kind is omitted
    Unlink {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        from: String,
        to: String,
        #[arg(long)]
        kind: Option<String>,
    },
    /// Graph neighborhood around a key (BFS)
    Neighbors {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        key: String,
        #[arg(long, default_value_t = 1)]
        depth: u32,
        #[arg(long, default_value = "out")]
        direction: String,
    },
    /// Shortest path between two keys
    Path {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        from: String,
        to: String,
    },
    /// Aggregate context/graph statistics
    Stats {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
    },
    /// Delete a context and its links
    Delete {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        key: String,
    },
}

#[derive(Subcommand)]
pub enum MemoryCmd {
    /// Create an in-memory table: --field name:Type (repeatable), --pk col
    Create {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        table: String,
        #[arg(long)]
        field: Vec<String>,
        #[arg(long)]
        pk: Option<String>,
    },
    /// Insert rows from a JSON array (or NDJSON) of objects
    Insert {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        table: String,
        json: String,
    },
    /// Point lookup by primary key (latest row)
    Get {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        table: String,
        key: String,
    },
    /// Run SQL over the in-memory tables
    Sql {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        sql: String,
    },
    /// Deduplicate by primary key (keep last)
    Compact {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        table: String,
    },
    /// List in-memory tables
    Tables {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
    },
}

fn parse_field(spec: &str) -> Result<less_catalog::FieldSpec> {
    let parts: Vec<&str> = spec.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(less_common::LessError::Config(format!(
            "field '{spec}' must be name:Type (e.g. id:Int64)"
        )));
    }
    Ok(less_catalog::FieldSpec::new(
        parts[0].trim(),
        less_catalog::TypeSpec::parse(parts[1].trim())?,
    ))
}

fn parse_key_value(s: &str) -> serde_json::Value {
    match s.parse::<i64>() {
        Ok(v) => serde_json::json!(v),
        Err(_) => serde_json::json!(s),
    }
}

pub fn run_context(cmd: &ContextCmd) -> Result<()> {
    match cmd {
        ContextCmd::Put {
            dir,
            key,
            title,
            text,
            tag,
            kind,
        } => {
            let mut store = ContextStore::open(Some(&context_dir(dir)))?;
            let ctx = store.put(key, title, text, tag.clone(), kind, Default::default())?;
            println!("put context {}", ctx.key);
            println!("  title: {}", ctx.title);
            println!("  tags:  {}", ctx.tags.join(", "));
            println!("  kind:  {}", ctx.kind);
            Ok(())
        }
        ContextCmd::Get { dir, key } => {
            let store = ContextStore::open(Some(&context_dir(dir)))?;
            match store.get(key) {
                Some(ctx) => {
                    println!("{}", serde_json::to_string_pretty(&ctx)?);
                    Ok(())
                }
                None => Err(less_common::LessError::Catalog(format!(
                    "context '{key}' not found"
                ))),
            }
        }
        ContextCmd::Find { dir, query, limit } => {
            let store = ContextStore::open(Some(&context_dir(dir)))?;
            let hits = store.find(query, *limit);
            if hits.is_empty() {
                println!("(no matches)");
            }
            for h in hits {
                println!("{}  [{}]  (score {})", h.key, h.tags.join(","), h.score);
                println!("  {} — {}", h.title, h.snippet);
            }
            Ok(())
        }
        ContextCmd::Link {
            dir,
            from,
            to,
            kind,
            directed,
        } => {
            let mut store = ContextStore::open(Some(&context_dir(dir)))?;
            let edge = store.link(from, to, kind, *directed, Default::default())?;
            println!(
                "linked {from} -[{}{}]-> {to}",
                edge.kind,
                if edge.directed { "" } else { " (undirected)" }
            );
            Ok(())
        }
        ContextCmd::Unlink {
            dir,
            from,
            to,
            kind,
        } => {
            let mut store = ContextStore::open(Some(&context_dir(dir)))?;
            let n = store.unlink(from, to, kind.as_deref())?;
            println!("removed {n} link(s)");
            Ok(())
        }
        ContextCmd::Neighbors {
            dir,
            key,
            depth,
            direction,
        } => {
            let store = ContextStore::open(Some(&context_dir(dir)))?;
            let dir = Direction::parse(direction)?;
            let neighbors = store.neighbors(key, dir, *depth)?;
            if neighbors.is_empty() {
                println!("(no neighbors)");
            }
            for n in neighbors {
                println!(
                    "depth {}  {}  via {}",
                    n.depth,
                    n.node,
                    n.via.as_deref().unwrap_or("-")
                );
            }
            Ok(())
        }
        ContextCmd::Path { dir, from, to } => {
            let store = ContextStore::open(Some(&context_dir(dir)))?;
            match store.path(from, to) {
                Some(hops) => {
                    if hops.is_empty() {
                        println!("{from} == {to}");
                    } else {
                        println!("{from}");
                        for h in &hops {
                            println!("  -[{}]-> {}", h.kind, h.to);
                        }
                        println!("({} hops)", hops.len());
                    }
                }
                None => println!("no path"),
            }
            Ok(())
        }
        ContextCmd::Stats { dir } => {
            let store = ContextStore::open(Some(&context_dir(dir)))?;
            let stats = store.stats();
            println!("nodes: {}  edges: {}", stats.nodes, stats.edges);
            Ok(())
        }
        ContextCmd::Delete { dir, key } => {
            let mut store = ContextStore::open(Some(&context_dir(dir)))?;
            match store.delete(key)? {
                true => println!("deleted {key}"),
                false => println!("{key} not found"),
            }
            Ok(())
        }
    }
}

pub fn run_memory(cmd: &MemoryCmd) -> Result<()> {
    match cmd {
        MemoryCmd::Create {
            dir,
            table,
            field,
            pk,
        } => {
            let store = MemoryStore::open(Some(&context_dir(dir)))?;
            let fields: Vec<less_catalog::FieldSpec> = field
                .iter()
                .map(|f| parse_field(f))
                .collect::<Result<_>>()?;
            store.create_table(table, fields, pk.clone())?;
            println!("created memory table {table}");
            Ok(())
        }
        MemoryCmd::Insert { dir, table, json } => {
            let store = MemoryStore::open(Some(&context_dir(dir)))?;
            let manifest = store.describe(table)?;
            let schema = less_catalog::SchemaSpec {
                fields: manifest.fields,
            }
            .to_arrow();
            let batches = less_memory::json_to_batches(schema, json)?;
            let mut rows = 0usize;
            for b in batches {
                rows += store.insert(table, b)?;
            }
            println!("inserted {rows} rows into memory table {table}");
            Ok(())
        }
        MemoryCmd::Get { dir, table, key } => {
            let store = MemoryStore::open(Some(&context_dir(dir)))?;
            match store.point_get(table, &parse_key_value(key))? {
                Some(row) => {
                    println!("{}", serde_json::to_string_pretty(&row)?);
                    Ok(())
                }
                None => {
                    println!("(not found)");
                    Ok(())
                }
            }
        }
        MemoryCmd::Sql { dir, sql } => {
            let store = MemoryStore::open(Some(&context_dir(dir)))?;
            let batches = store.sql(sql)?;
            crate::commands::print(&batches)
        }
        MemoryCmd::Compact { dir, table } => {
            let store = MemoryStore::open(Some(&context_dir(dir)))?;
            let removed = store.compact(table)?;
            println!("removed {removed} duplicate row(s) from {table}");
            Ok(())
        }
        MemoryCmd::Tables { dir } => {
            let store = MemoryStore::open(Some(&context_dir(dir)))?;
            for t in store.table_names() {
                println!("{t}");
            }
            Ok(())
        }
    }
}
