//! LessCypher — an openCypher query layer for the LessDB context/graph
//! store.
//!
//! Implements a documented subset (the read core + basic writes):
//!
//! ```cypher
//! MATCH (a:Label {prop: value})-[r:TYPE*1..3]->(b) [, patterns]
//! [WHERE expr] RETURN * | n.prop | n | id(n) | labels(n) | count(*) |
//!   count/collect/sum/avg/min/max(x) [AS alias] [ORDER BY x [DESC]]
//!   [SKIP n] [LIMIT n] [DISTINCT]
//! CREATE (n:Label {..}), (a)-[:TYPE]->(b)
//! MATCH (n) DELETE n
//! MATCH (n) SET n.prop = value
//! ```
//!
//! Semantics follow openCypher: implicit grouping when aggregates are
//! present, `null` comparisons are false, deletes cascade edges, and
//! `id(n)` returns the LessDB node key. Not yet supported: `MERGE`,
//! `WITH`-chained queries, `UNION`, subqueries, list comprehensions,
//! `EXISTS` patterns, shortestPath().

pub mod ast;
pub mod exec;
pub mod lexer;
pub mod parser;

use less_common::{LessError, Result};
use less_graph::GraphStore;

pub use ast::{ChainPart, Direction, Pattern, ReturnItem, Statement};
pub use exec::{CypherResult, execute};
pub use parser::parse;

/// Parse and execute one statement against a graph store.
pub fn run(graph: &mut GraphStore, input: &str) -> Result<CypherResult> {
    let statement = parse(input)?;
    execute(graph, &statement).map_err(|e| LessError::Query(format!("{input}\n{e}")))
}

/// Convenience: execute and render as a JSON array of row objects.
pub fn run_json(graph: &mut GraphStore, input: &str) -> Result<String> {
    let result = run(graph, input)?;
    let rows: Vec<serde_json::Value> = result
        .rows
        .iter()
        .map(|row| {
            result
                .columns
                .iter()
                .zip(row.iter())
                .map(|(c, v)| (c.clone(), v.clone()))
                .collect::<serde_json::Map<_, _>>()
        })
        .map(serde_json::Value::Object)
        .collect();
    Ok(serde_json::to_string(&rows)?)
}
