//! Cypher execution: backtracking pattern matching over the graph store,
//! implicit aggregation, and write statements.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use less_common::{LessError, Result};
use less_graph::GraphStore;

use crate::ast::*;

/// A bound variable.
#[derive(Debug, Clone)]
pub enum Binding {
    Node(String),
    Edge(u32),
    /// Variable-length path (list of hop objects).
    Path(Vec<Value>),
}

/// Tabular result.
#[derive(Debug, Clone)]
pub struct CypherResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

type Row = HashMap<String, Binding>;

fn node_object(node: &less_graph::Node) -> Value {
    serde_json::json!({
        "key": node.key,
        "labels": node.labels,
        "props": node.props,
    })
}

fn edge_object(edge: &less_graph::Edge) -> Value {
    serde_json::json!({
        "from": edge.from,
        "to": edge.to,
        "type": edge.kind,
        "props": edge.props,
    })
}

fn node_prop(graph: &GraphStore, key: &str, prop: &str) -> Value {
    if prop == "key" {
        return Value::String(key.to_string());
    }
    match graph.get_node(key) {
        Some(node) => node.props.get(prop).cloned().unwrap_or(Value::Null),
        None => Value::Null,
    }
}

/// Execute a parsed statement.
pub fn execute(graph: &mut GraphStore, statement: &Statement) -> Result<CypherResult> {
    match statement {
        Statement::Create(elements) => execute_create(graph, elements),
        Statement::Query(q) => execute_query(graph, q),
    }
}

// ---------------------------------------------------------------------------
// MATCH ... RETURN / DELETE / SET
// ---------------------------------------------------------------------------

fn execute_query(graph: &mut GraphStore, q: &Query) -> Result<CypherResult> {
    let mut rows: Vec<Row> = vec![Row::new()];
    for pattern in &q.patterns {
        let mut next = vec![];
        for row in &rows {
            match_pattern(graph, pattern, row, &mut next);
        }
        rows = next;
        if rows.is_empty() {
            break;
        }
    }
    // WHERE
    if let Some(where_expr) = &q.where_clause {
        rows.retain(|row| eval_expr(graph, row, where_expr) == Some(true));
    }
    // SET
    for (var, prop, value) in &q.sets {
        for row in &rows {
            if let Some(Binding::Node(key)) = row.get(var) {
                let mut props = serde_json::Map::new();
                props.insert(prop.clone(), value.clone());
                graph.upsert_node(key, vec![], props);
            }
        }
    }
    // DELETE
    let mut deleted_nodes: HashSet<String> = HashSet::new();
    let mut deleted_edges: Vec<(String, String, Option<String>)> = vec![];
    for var in &q.deletes {
        for row in &rows {
            match row.get(var) {
                Some(Binding::Node(key)) => {
                    deleted_nodes.insert(key.clone());
                }
                Some(Binding::Edge(id)) => {
                    if let Some(edge) = edge_by_id(graph, *id) {
                        deleted_edges.push((
                            edge.from.clone(),
                            edge.to.clone(),
                            Some(edge.kind.clone()),
                        ));
                    }
                }
                _ => {}
            }
        }
    }
    for key in &deleted_nodes {
        graph.delete_node(key);
    }
    for (from, to, kind) in &deleted_edges {
        graph.delete_edges(from, to, kind.as_deref());
    }

    // RETURN projection
    let mut columns: Vec<String> = vec![];
    let mut projected: Vec<Vec<Value>> = vec![];
    if q.return_items.star {
        let mut vars: Vec<String> = {
            let mut set: HashSet<&str> = HashSet::new();
            let mut out = vec![];
            for row in &rows {
                for v in row.keys() {
                    if set.insert(v) {
                        out.push(v.clone());
                    }
                }
            }
            out
        };
        vars.sort();
        columns.extend(vars.clone());
        for row in &rows {
            projected.push(
                vars.iter()
                    .map(|v| match row.get(v) {
                        Some(Binding::Node(key)) => node_object(graph.get_node(key).unwrap()),
                        Some(Binding::Edge(id)) => edge_object(&edge_by_id(graph, *id).unwrap()),
                        Some(Binding::Path(hops)) => Value::Array(hops.clone()),
                        None => Value::Null,
                    })
                    .collect(),
            );
        }
    } else {
        // Aggregates?
        let has_agg = q.return_items.items.iter().any(is_agg_item);
        let items = &q.return_items.items;
        for item in items {
            columns.push(item.alias_of());
        }
        if has_agg {
            // Implicit GROUP BY on non-aggregate items.
            let mut groups: Vec<(Vec<Value>, Vec<Row>)> = vec![];
            for row in &rows {
                let key: Vec<Value> = items
                    .iter()
                    .filter(|i| !is_agg_item(i))
                    .map(|i| eval_return(graph, row, i))
                    .collect();
                match groups.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, rows)) => rows.push(row.clone()),
                    None => groups.push((key, vec![row.clone()])),
                }
            }
            for (_, group) in groups {
                let mut out = vec![];
                for item in items {
                    out.push(if is_agg_item(item) {
                        eval_aggregate(graph, &group, item)
                    } else {
                        eval_return(graph, &group[0], item)
                    });
                }
                projected.push(out);
            }
        } else {
            for row in &rows {
                projected.push(items.iter().map(|i| eval_return(graph, row, i)).collect());
            }
        }
    }

    // DISTINCT
    if q.distinct {
        let mut seen = HashSet::new();
        projected.retain(|row| seen.insert(row.iter().map(value_key).collect::<Vec<_>>()));
    }
    // ORDER BY (sort keys matched against projected columns)
    if !q.order_by.is_empty() {
        let order_cols: Vec<usize> = q
            .order_by
            .iter()
            .map(|s| {
                columns
                    .iter()
                    .position(|c| *c == s.item.alias_of())
                    .unwrap_or(usize::MAX)
            })
            .collect();
        projected.sort_by(|a, b| {
            for (sort, col) in q.order_by.iter().zip(order_cols.iter()) {
                let (va, vb) = if *col == usize::MAX {
                    (Value::Null, Value::Null)
                } else {
                    (a[*col].clone(), b[*col].clone())
                };
                let ord = compare_values(&va, &vb);
                if ord != std::cmp::Ordering::Equal {
                    return if sort.desc { ord.reverse() } else { ord };
                }
            }
            std::cmp::Ordering::Equal
        });
    }
    // SKIP / LIMIT
    if let Some(skip) = q.skip {
        projected = projected.into_iter().skip(skip).collect();
    }
    if let Some(limit) = q.limit {
        projected.truncate(limit);
    }
    Ok(CypherResult {
        columns,
        rows: projected,
    })
}

fn value_key(v: &Value) -> String {
    format!("{:?}", v)
}

fn compare_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x
            .as_f64()
            .partial_cmp(&y.as_f64())
            .unwrap_or(std::cmp::Ordering::Equal),
        (Value::String(x), Value::String(y)) => x.cmp(y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Null, Value::Null) => std::cmp::Ordering::Equal,
        (Value::Null, _) => std::cmp::Ordering::Less,
        (_, Value::Null) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    }
}

fn is_aggregate(name: &str) -> bool {
    matches!(name, "count" | "collect" | "sum" | "avg" | "min" | "max")
}

fn is_agg_item(item: &ReturnItem) -> bool {
    match item {
        ReturnItem::Func(name, _) => is_aggregate(name),
        ReturnItem::Alias(inner, _) => is_agg_item(inner),
        _ => false,
    }
}

fn eval_return(graph: &GraphStore, row: &Row, item: &ReturnItem) -> Value {
    match item {
        ReturnItem::Lit(v) => v.clone(),
        ReturnItem::Var(v) => match row.get(v) {
            Some(Binding::Node(key)) => match graph.get_node(key) {
                Some(node) => node_object(node),
                None => Value::Null,
            },
            Some(Binding::Edge(id)) => match edge_by_id(graph, *id) {
                Some(edge) => edge_object(&edge),
                None => Value::Null,
            },
            Some(Binding::Path(hops)) => Value::Array(hops.clone()),
            None => Value::Null,
        },
        ReturnItem::Prop(v, p) => match row.get(v) {
            Some(Binding::Node(key)) => node_prop(graph, key, p),
            Some(Binding::Edge(id)) => edge_by_id(graph, *id)
                .map(|e| {
                    if p == "type" {
                        Value::String(e.kind.clone())
                    } else {
                        e.props.get(p).cloned().unwrap_or(Value::Null)
                    }
                })
                .unwrap_or(Value::Null),
            _ => Value::Null,
        },
        ReturnItem::Alias(inner, _) => eval_return(graph, row, inner),
        ReturnItem::Func(name, args) => match name.as_str() {
            "id" => match args.first().map(|a| a.as_ref()) {
                Some(ReturnItem::Var(v)) => match row.get(v) {
                    Some(Binding::Node(key)) => Value::String(key.clone()),
                    _ => Value::Null,
                },
                _ => Value::Null,
            },
            "labels" => match args.first().map(|a| a.as_ref()) {
                Some(ReturnItem::Var(v)) => match row.get(v) {
                    Some(Binding::Node(key)) => graph
                        .get_node(key)
                        .map(|n| {
                            Value::Array(
                                n.labels.iter().map(|l| Value::String(l.clone())).collect(),
                            )
                        })
                        .unwrap_or(Value::Null),
                    _ => Value::Null,
                },
                _ => Value::Null,
            },
            // Non-aggregate function fallthrough: evaluate like an item.
            _ => Value::Null,
        },
    }
}

fn eval_aggregate(graph: &GraphStore, rows: &[Row], item: &ReturnItem) -> Value {
    let item = match item {
        ReturnItem::Alias(inner, _) => inner.as_ref(),
        other => other,
    };
    let ReturnItem::Func(name, args) = item else {
        return Value::Null;
    };
    match name.as_str() {
        "count" => match args.first().map(|a| a.as_ref()) {
            Some(ReturnItem::Lit(v)) if v.as_str() == Some("*") => Value::from(rows.len() as i64),
            Some(arg) => Value::from(
                rows.iter()
                    .filter(|r| eval_return(graph, r, arg) != Value::Null)
                    .count() as i64,
            ),
            None => Value::from(rows.len() as i64),
        },
        "collect" => match args.first() {
            Some(arg) => Value::Array(rows.iter().map(|r| eval_return(graph, r, arg)).collect()),
            None => Value::Null,
        },
        "sum" | "avg" | "min" | "max" => {
            let values: Vec<f64> = args
                .first()
                .map(|arg| {
                    rows.iter()
                        .filter_map(|r| match eval_return(graph, r, arg) {
                            Value::Number(n) => n.as_f64(),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default();
            match name.as_str() {
                "sum" => Value::from(values.iter().sum::<f64>()),
                "avg" => {
                    if values.is_empty() {
                        Value::Null
                    } else {
                        Value::from(values.iter().sum::<f64>() / values.len() as f64)
                    }
                }
                "min" => values
                    .iter()
                    .copied()
                    .fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.min(v))))
                    .map(Value::from)
                    .unwrap_or(Value::Null),
                _ => values
                    .iter()
                    .copied()
                    .fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v))))
                    .map(Value::from)
                    .unwrap_or(Value::Null),
            }
        }
        _ => Value::Null,
    }
}

// ---------------------------------------------------------------------------
// Pattern matching (backtracking)
// ---------------------------------------------------------------------------

fn match_pattern(graph: &GraphStore, pattern: &Pattern, row: &Row, out: &mut Vec<Row>) {
    // Seed: if the first node var is bound, start from it; else all nodes.
    let mut work: Vec<Row> = vec![row.clone()];
    let mut step = 0usize;
    while step < pattern.chain.len() {
        let part = &pattern.chain[step];
        let mut next = vec![];
        for r in &work {
            match part {
                ChainPart::Node { var, labels, props } => {
                    match var {
                        Some(v) => match r.get(v) {
                            Some(Binding::Node(existing)) => {
                                if node_matches(graph, existing, labels, props) {
                                    next.push(r.clone());
                                }
                            }
                            _ => {
                                for node in graph_node_iter(graph) {
                                    if node_matches(graph, &node.key, labels, props) {
                                        let mut r2 = r.clone();
                                        r2.insert(v.clone(), Binding::Node(node.key.clone()));
                                        next.push(r2);
                                    }
                                }
                            }
                        },
                        None => {
                            // Anonymous node between edges: just constrain on
                            // the adjacent edges' endpoints (no binding).
                            next.push(r.clone());
                        }
                    }
                }
                ChainPart::Edge {
                    var,
                    types,
                    direction,
                    min_hops,
                    max_hops,
                    props,
                } => {
                    // The source node is the previous node part's var.
                    let prev_var = match &pattern.chain[step - 1] {
                        ChainPart::Node { var: Some(v), .. } => Some(v.clone()),
                        _ => None,
                    };
                    let Some(prev) = prev_var else {
                        return;
                    };
                    let Some(Binding::Node(from)) = r.get(&prev).cloned() else {
                        return;
                    };
                    let max = max_hops.unwrap_or(8).min(8);
                    if *min_hops == 1 && max == 1 {
                        // Single hop.
                        for cand in edge_candidates(graph, &from, *direction) {
                            if edge_matches(graph, cand, types, props) {
                                let mut r2 = r.clone();
                                if let Some(v) = var {
                                    r2.insert(v.clone(), Binding::Edge(cand));
                                }
                                let to = edge_to(graph, cand, &from);
                                if let Some(ChainPart::Node { var: Some(nv), .. }) =
                                    pattern.chain.get(step + 1)
                                {
                                    r2.insert(nv.clone(), Binding::Node(to));
                                }
                                next.push(r2);
                            }
                        }
                    } else {
                        // Variable-length BFS.
                        for (to, path) in bfs_paths(graph, &from, *direction, *min_hops, max) {
                            let mut r2 = r.clone();
                            if let Some(v) = var {
                                r2.insert(v.clone(), Binding::Path(path));
                            }
                            if let Some(ChainPart::Node { var: Some(nv), .. }) =
                                pattern.chain.get(step + 1)
                            {
                                r2.insert(nv.clone(), Binding::Node(to));
                            }
                            next.push(r2);
                        }
                    }
                }
            }
        }
        work = next;
        if work.is_empty() {
            return;
        }
        step += 1; // walk the whole chain: node, edge, node, edge...
    }
    out.extend(work);
}

fn node_matches(
    graph: &GraphStore,
    key: &str,
    labels: &[String],
    props: &[(String, Value)],
) -> bool {
    let Some(node) = graph.get_node(key) else {
        return false;
    };
    if !labels.iter().all(|l| node.labels.contains(l)) {
        return false;
    }
    props.iter().all(|(k, v)| {
        if k == "key" {
            Value::String(key.to_string()) == *v
        } else {
            node.props.get(k) == Some(v)
        }
    })
}

fn edge_matches(graph: &GraphStore, id: u32, types: &[String], props: &[(String, Value)]) -> bool {
    let Some(edge) = edge_by_id(graph, id) else {
        return false;
    };
    if !types.is_empty() && !types.contains(&edge.kind) {
        return false;
    }
    props.iter().all(|(k, v)| edge.props.get(k) == Some(v))
}

/// Enumerate candidate edge ids from a node in the given direction.
fn edge_candidates(graph: &GraphStore, from: &str, direction: Direction) -> Vec<u32> {
    let mut ids = vec![];
    if direction != Direction::In {
        for id in graph.out_edges(from) {
            ids.push(id);
        }
    }
    if direction != Direction::Out {
        for id in graph.in_edges(from) {
            ids.push(id);
        }
    }
    ids
}

fn edge_by_id(graph: &GraphStore, id: u32) -> Option<less_graph::Edge> {
    graph.edge_by_id(id)
}

fn edge_to(graph: &GraphStore, id: u32, from: &str) -> String {
    match edge_by_id(graph, id) {
        Some(e) if e.from == from => e.to.clone(),
        Some(e) => e.from.clone(),
        None => from.to_string(),
    }
}

/// BFS over the graph returning `(target_key, path_hops)` for depths in
/// `[min, max]`.
fn bfs_paths(
    graph: &GraphStore,
    start: &str,
    direction: Direction,
    min: usize,
    max: usize,
) -> Vec<(String, Vec<Value>)> {
    let mut out = vec![];
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(start.to_string());
    let mut frontier: Vec<(String, Vec<Value>)> = vec![(start.to_string(), vec![])];
    for depth in 1..=max {
        let mut next = vec![];
        for (node, path) in &frontier {
            for id in edge_candidates(graph, node, direction) {
                let to = edge_to(graph, id, node);
                if !visited.contains(&to) {
                    visited.insert(to.clone());
                    let edge = edge_by_id(graph, id);
                    let mut p2 = path.clone();
                    p2.push(serde_json::json!({
                        "from": node,
                        "to": to,
                        "type": edge.as_ref().map(|e| e.kind.as_str()).unwrap_or(""),
                    }));
                    if depth >= min {
                        out.push((to.clone(), p2.clone()));
                    }
                    next.push((to.clone(), p2));
                }
            }
        }
        frontier = next;
        if frontier.is_empty() {
            break;
        }
    }
    out
}

fn graph_node_iter(graph: &GraphStore) -> Vec<less_graph::Node> {
    graph.nodes()
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

fn eval_expr(graph: &GraphStore, row: &Row, expr: &Expr) -> Option<bool> {
    match expr {
        Expr::Lit(Value::Bool(b)) => Some(*b),
        Expr::Lit(_) => None,
        Expr::Prop(v, p) => match row.get(v) {
            Some(Binding::Node(key)) => match node_prop(graph, key, p) {
                Value::Bool(b) => Some(b),
                _ => None,
            },
            _ => None,
        },
        Expr::Eq(a, b) => {
            let (va, vb) = (eval_operand(graph, row, a), eval_operand(graph, row, b));
            if va.is_null() || vb.is_null() {
                None
            } else {
                Some(va == vb)
            }
        }
        Expr::Neq(a, b) => eval_eq(graph, row, a, b).map(|x| !x),
        Expr::Lt(a, b) => cmp(graph, row, a, b).map(|o| o == std::cmp::Ordering::Less),
        Expr::Le(a, b) => cmp(graph, row, a, b).map(|o| o != std::cmp::Ordering::Greater),
        Expr::Gt(a, b) => cmp(graph, row, a, b).map(|o| o == std::cmp::Ordering::Greater),
        Expr::Ge(a, b) => cmp(graph, row, a, b).map(|o| o != std::cmp::Ordering::Less),
        Expr::And(a, b) => match (eval_expr(graph, row, a), eval_expr(graph, row, b)) {
            (Some(false), _) | (_, Some(false)) => Some(false),
            (Some(true), Some(true)) => Some(true),
            _ => None,
        },
        Expr::Or(a, b) => match (eval_expr(graph, row, a), eval_expr(graph, row, b)) {
            (Some(true), _) | (_, Some(true)) => Some(true),
            (Some(false), Some(false)) => Some(false),
            _ => None,
        },
        Expr::Not(a) => eval_expr(graph, row, a).map(|x| !x),
        Expr::In(a, list) => {
            let v = eval_operand(graph, row, a);
            if v.is_null() {
                None
            } else {
                Some(list.contains(&v))
            }
        }
        Expr::StartsWith(a, b) => string_op(graph, row, a, b, |s, p| s.starts_with(p)),
        Expr::EndsWith(a, b) => string_op(graph, row, a, b, |s, p| s.ends_with(p)),
        Expr::Contains(a, b) => string_op(graph, row, a, b, |s, p| s.contains(p)),
    }
}

fn eval_eq(graph: &GraphStore, row: &Row, a: &Expr, b: &Expr) -> Option<bool> {
    match (eval_operand(graph, row, a), eval_operand(graph, row, b)) {
        (va, vb) if va.is_null() || vb.is_null() => None,
        (va, vb) => Some(va == vb),
    }
}

fn cmp(graph: &GraphStore, row: &Row, a: &Expr, b: &Expr) -> Option<std::cmp::Ordering> {
    let (va, vb) = (eval_operand(graph, row, a), eval_operand(graph, row, b));
    if va.is_null() || vb.is_null() {
        return None;
    }
    Some(compare_values(&va, &vb))
}

fn string_op(
    graph: &GraphStore,
    row: &Row,
    a: &Expr,
    b: &Expr,
    f: impl Fn(&str, &str) -> bool,
) -> Option<bool> {
    match (eval_operand(graph, row, a), eval_operand(graph, row, b)) {
        (Value::String(s), Value::String(p)) => Some(f(&s, &p)),
        _ => None,
    }
}

fn eval_operand(graph: &GraphStore, row: &Row, expr: &Expr) -> Value {
    match expr {
        Expr::Lit(v) => v.clone(),
        Expr::Prop(v, p) => match row.get(v) {
            Some(Binding::Node(key)) => node_prop(graph, key, p),
            Some(Binding::Edge(id)) => edge_by_id(graph, *id)
                .map(|e| e.props.get(p).cloned().unwrap_or(Value::Null))
                .unwrap_or(Value::Null),
            _ => Value::Null,
        },
        _ => Value::Null,
    }
}

// ---------------------------------------------------------------------------
// CREATE
// ---------------------------------------------------------------------------

fn execute_create(graph: &mut GraphStore, elements: &[CreateElement]) -> Result<CypherResult> {
    let mut keys: HashMap<String, String> = HashMap::new(); // var -> key
    let mut counter = graph.node_count();
    // Nodes first.
    for el in elements {
        if let CreateElement::Node { var, labels, props } = el {
            let key = props
                .iter()
                .find(|(k, _)| k == "key")
                .and_then(|(_, v)| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    counter += 1;
                    format!("node-{counter}")
                });
            let mut node_props = serde_json::Map::new();
            for (k, v) in props {
                if k != "key" {
                    node_props.insert(k.clone(), v.clone());
                }
            }
            graph.upsert_node(&key, labels.clone(), node_props);
            if let Some(v) = var {
                keys.insert(v.clone(), key);
            }
        }
    }
    // Edges next (endpoints resolve to vars or key props; auto-create when
    // the endpoint was an anonymous node pattern).
    for el in elements {
        if let CreateElement::Edge {
            from,
            to,
            types,
            direction,
            props,
            ..
        } = el
        {
            let mut resolve = |r: &NodeRef| -> Result<String> {
                match r {
                    NodeRef::Var(v) => keys.get(v).cloned().ok_or_else(|| {
                        LessError::Query(format!("cypher: unknown node variable '{v}' in CREATE"))
                    }),
                    NodeRef::Key(k) => {
                        if graph.get_node(k).is_none() {
                            graph.upsert_node(k, vec![], serde_json::Map::new());
                        }
                        Ok(k.clone())
                    }
                }
            };
            let from_key = resolve(from)?;
            let to_key = resolve(to)?;
            let kind = types
                .first()
                .cloned()
                .ok_or_else(|| LessError::Query("cypher: CREATE edge needs a type".into()))?;
            let mut edge_props = serde_json::Map::new();
            for (k, v) in props {
                edge_props.insert(k.clone(), v.clone());
            }
            graph.upsert_edge(
                &from_key,
                &to_key,
                &kind,
                edge_props,
                *direction == Direction::Out,
            )?;
        }
    }
    Ok(CypherResult {
        columns: vec![],
        rows: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixture: ann -KNOWS-> bob -KNOWS-> carl; ann -KNOWS-> dave; bob:age 34,
    /// carl:age 41, dave:age 29; all :Person.
    fn fixture() -> GraphStore {
        let mut g = GraphStore::new();
        for (key, age) in [("ann", 30), ("bob", 34), ("carl", 41), ("dave", 29)] {
            let mut props = serde_json::Map::new();
            props.insert("name".into(), Value::String(key.to_string()));
            props.insert("age".into(), Value::from(age));
            g.upsert_node(key, vec!["Person".into()], props);
        }
        g.upsert_edge("ann", "bob", "KNOWS", serde_json::Map::new(), true)
            .unwrap();
        g.upsert_edge("bob", "carl", "KNOWS", serde_json::Map::new(), true)
            .unwrap();
        g.upsert_edge("ann", "dave", "KNOWS", serde_json::Map::new(), true)
            .unwrap();
        g
    }

    fn run(g: &mut GraphStore, q: &str) -> CypherResult {
        execute(g, &crate::parser::parse(q).unwrap()).unwrap()
    }

    #[test]
    fn match_nodes_and_labels() {
        let mut g = fixture();
        let r = run(&mut g, "MATCH (n:Person) RETURN n.name ORDER BY n.name");
        assert_eq!(r.columns, vec!["n.name".to_string()]);
        assert_eq!(r.rows.len(), 4);
        assert_eq!(r.rows[0][0], Value::String("ann".into()));
    }

    #[test]
    fn match_edge_and_filter() {
        let mut g = fixture();
        let r = run(
            &mut g,
            "MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE b.age > 30 RETURN a.name, b.name ORDER BY b.name",
        );
        assert_eq!(r.rows.len(), 2); // bob->carl, ann->bob
        assert_eq!(r.rows[0][0], Value::String("ann".into()));
        assert_eq!(r.rows[0][1], Value::String("bob".into()));
        assert_eq!(r.rows[1][1], Value::String("carl".into()));
    }

    #[test]
    fn variable_length_and_undirected() {
        let mut g = fixture();
        let r = run(
            &mut g,
            "MATCH (a:Person)-[:KNOWS*1..2]->(b:Person) WHERE a.name = 'ann' RETURN b.name ORDER BY b.name",
        );
        let names: Vec<&str> = r.rows.iter().map(|row| row[0].as_str().unwrap()).collect();
        assert_eq!(names, vec!["bob", "carl", "dave"]);

        let r = run(
            &mut g,
            "MATCH (a:Person {name: 'carl'})--(b:Person) RETURN b.name",
        );
        assert_eq!(r.rows.len(), 1);
        assert_eq!(r.rows[0][0], Value::String("bob".into()));
    }

    #[test]
    fn aggregates_grouping_order_limit_skip_distinct() {
        let mut g = fixture();
        let r = run(
            &mut g,
            "MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, count(b) AS friends ORDER BY friends DESC, a.name LIMIT 2",
        );
        assert_eq!(r.columns, vec!["a.name".to_string(), "friends".to_string()]);
        assert_eq!(r.rows.len(), 2);
        assert_eq!(r.rows[0][0], Value::String("ann".into()));
        assert_eq!(r.rows[0][1], Value::from(2));

        let r = run(&mut g, "MATCH (n:Person) RETURN collect(n.age) AS ages");
        assert_eq!(r.rows[0][0].as_array().unwrap().len(), 4);

        let r = run(
            &mut g,
            "MATCH (n:Person) RETURN n.name ORDER BY n.name SKIP 1 LIMIT 2",
        );
        let names: Vec<&str> = r.rows.iter().map(|row| row[0].as_str().unwrap()).collect();
        assert_eq!(names, vec!["bob", "carl"]);

        let r = run(
            &mut g,
            "MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN DISTINCT a.name",
        );
        assert_eq!(r.rows.len(), 2);
    }

    #[test]
    fn create_set_delete_roundtrip() {
        let mut g = GraphStore::new();
        let r = run(
            &mut g,
            "CREATE (n:Task {key: 't/1', title: 'one'}), (m:Task {key: 't/2'}), (n)-[:DEPENDS_ON]->(m)",
        );
        assert!(r.rows.is_empty());
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 1);

        let r = run(
            &mut g,
            "MATCH (n:Task {key: 't/1'}) RETURN n.title, n.key, id(n), labels(n)",
        );
        assert_eq!(r.rows[0][0], Value::String("one".into()));
        assert_eq!(r.rows[0][1], Value::String("t/1".into()));
        assert_eq!(r.rows[0][2], Value::String("t/1".into()));
        assert_eq!(
            r.rows[0][3],
            Value::Array(vec![Value::String("Task".into())])
        );

        run(&mut g, "MATCH (n:Task {key: 't/1'}) SET n.done = true");
        let r = run(&mut g, "MATCH (n:Task {key: 't/1'}) RETURN n.done");
        assert_eq!(r.rows[0][0], Value::Bool(true));

        run(&mut g, "MATCH (n:Task {key: 't/1'}) DELETE n");
        assert_eq!(g.node_count(), 1);
        assert_eq!(g.edge_count(), 0, "deleting a node cascades its edges");
    }

    #[test]
    fn contains_and_in_operators() {
        let mut g = fixture();
        let r = run(
            &mut g,
            "MATCH (n:Person) WHERE n.name CONTAINS 'ar' RETURN n.name",
        );
        assert_eq!(r.rows.len(), 1);
        assert_eq!(r.rows[0][0], Value::String("carl".into()));
        let r = run(
            &mut g,
            "MATCH (n:Person) WHERE n.name IN ['ann', 'dave'] RETURN n.name ORDER BY n.name",
        );
        assert_eq!(r.rows.len(), 2);
    }
}
