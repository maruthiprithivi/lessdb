//! Distributed query fan-out: a coordinator splits one table scan across
//! `n` LessDB compute nodes and merges the partial results locally.
//!
//! ## How it works
//!
//! 1. **Parse & validate** the SQL against the fan-out v1 subset: a single
//!    base table, projection of group columns + bare `count`/`sum`/`min`/
//!    `max` aggregates, optional `WHERE` (column predicates), `GROUP BY`,
//!    `ORDER BY`, `LIMIT`. No joins, CTEs, unions, subqueries, windows,
//!    `HAVING`, `DISTINCT`, or aggregate expressions — those are rejected
//!    with a clear error.
//! 2. **Shard**: for node `i` of `n`, run
//!    `SELECT <items, aggregates aliased> FROM t
//!     WHERE (<where>) AND lessdb_shard(<col>, i, n) GROUP BY <group>`
//!    — the `lessdb_shard` predicate makes each node scan only the parts
//!    whose names hash to shard `i` (see `less-query::shard`), and pushing
//!    the `GROUP BY` down produces per-node partial aggregates.
//! 3. **Merge**: the partial Arrow batches become a local `partial` table;
//!    the coordinator re-runs the aggregates (`sum(x)` → `sum(<alias>)`,
//!    `count(*)` → `sum(<alias>)`, `min`/`max` re-applied) and applies the
//!    original `ORDER BY`/`LIMIT`.
//!
//! Partial aggregation is exact for `count`/`sum`/`min`/`max` (they are
//! associative + commutative); `avg`/`stddev` etc. need weighted merging
//! and are explicitly rejected in v1.

use std::sync::Arc;

use arrow::ipc::reader::StreamReader;
use arrow::record_batch::RecordBatch;
use datafusion::datasource::MemTable;
use datafusion::execution::context::SessionContext;
use datafusion::sql::sqlparser::ast::{
    Expr as SqlExpr, Function, FunctionArg, FunctionArgExpr, FunctionArguments, GroupByExpr,
    LimitClause, OrderBy, OrderByKind, Query, Select, SelectItem, SetExpr, Statement, TableFactor,
    TableWithJoins,
};
use datafusion::sql::sqlparser::dialect::GenericDialect;
use datafusion::sql::sqlparser::parser::Parser;
use less_common::{LessError, Result};

/// Allowed aggregate functions in the fan-out v1 subset (associative +
/// commutative, so partial results merge exactly).
const ALLOWED_AGGREGATES: &[&str] = &["count", "sum", "min", "max"];

/// One parsed, validated, fan-out-able query.
#[derive(Debug, Clone)]
pub struct FanoutPlan {
    /// Original SQL (for diagnostics).
    sql: String,
    table: String,
    /// Projection items with a deterministic alias each (group-by-only
    /// extras are appended after the first `user_items` entries).
    items: Vec<(String, SqlExpr)>,
    /// How many of `items` are the user's own SELECT list.
    user_items: usize,
    where_clause: Option<SqlExpr>,
    /// (alias, expr) per GROUP BY expression.
    group_by: Vec<(String, SqlExpr)>,
    /// (alias, asc, expr) per ORDER BY expression.
    order_by: Vec<(String, Option<bool>, SqlExpr)>,
    limit: Option<u64>,
}

fn parse_int_literal(e: &SqlExpr) -> Option<i64> {
    use datafusion::sql::sqlparser::ast::{UnaryOperator, Value};
    match e {
        SqlExpr::Value(v) => match &v.value {
            Value::Number(n, _) => n.parse().ok(),
            _ => None,
        },
        SqlExpr::UnaryOp { op, expr } => parse_int_literal(expr).map(|v| match op {
            UnaryOperator::Minus => -v,
            _ => v,
        }),
        _ => None,
    }
}

/// Is this expression a bare aggregate call of a fan-out-safe function?
/// Returns the function name (lowercased).
fn aggregate_name(e: &SqlExpr) -> Option<String> {
    if let SqlExpr::Function(Function {
        name,
        args: FunctionArguments::List(list),
        ..
    }) = e
    {
        // DISTINCT / ORDER BY / other clauses inside the argument list are
        // not fan-out-safe.
        if list.duplicate_treatment.is_some() || !list.clauses.is_empty() {
            return None;
        }
        let name = name
            .0
            .last()
            .and_then(|p| p.as_ident())?
            .value
            .to_ascii_lowercase();
        if ALLOWED_AGGREGATES.contains(&name.as_str()) {
            return Some(name);
        }
    }
    None
}

/// Is this expression "simple" — a bare column identifier or a literal/
/// comparison expression with no aggregates and no subqueries?
fn is_simple(e: &SqlExpr) -> bool {
    matches!(
        e,
        SqlExpr::Identifier(_)
            | SqlExpr::CompoundIdentifier(_)
            | SqlExpr::Value(_)
            | SqlExpr::UnaryOp { .. }
            | SqlExpr::BinaryOp { .. }
            | SqlExpr::Nested(_)
            | SqlExpr::IsNull(_)
            | SqlExpr::IsNotNull(_)
            | SqlExpr::Between { .. }
            | SqlExpr::InList { .. }
            | SqlExpr::Like { .. }
            | SqlExpr::ILike { .. }
    )
}

/// Does any sub-expression call an aggregate function?
fn contains_aggregate(e: &SqlExpr) -> bool {
    if aggregate_name(e).is_some() {
        return true;
    }
    match e {
        SqlExpr::BinaryOp { left, right, .. } => {
            contains_aggregate(left) || contains_aggregate(right)
        }
        SqlExpr::UnaryOp { expr, .. } | SqlExpr::Nested(expr) => contains_aggregate(expr),
        SqlExpr::IsNull(expr) | SqlExpr::IsNotNull(expr) => contains_aggregate(expr),
        SqlExpr::Between {
            expr, low, high, ..
        } => contains_aggregate(expr) || contains_aggregate(low) || contains_aggregate(high),
        SqlExpr::InList { expr, list, .. } => {
            contains_aggregate(expr) || list.iter().any(contains_aggregate)
        }
        SqlExpr::Function(f) => match &f.args {
            FunctionArguments::None => false,
            FunctionArguments::Subquery(_) => false,
            FunctionArguments::List(list) => list.args.iter().any(|a| {
                let arg = match a {
                    FunctionArg::Named { arg, .. }
                    | FunctionArg::ExprNamed { arg, .. }
                    | FunctionArg::Unnamed(arg) => arg,
                };
                match arg {
                    FunctionArgExpr::Expr(e) => contains_aggregate(e),
                    _ => false,
                }
            }),
        },
        _ => false,
    }
}

fn contains_subquery(e: &SqlExpr) -> bool {
    match e {
        SqlExpr::Subquery(_) | SqlExpr::Exists { .. } | SqlExpr::InSubquery { .. } => true,
        SqlExpr::BinaryOp { left, right, .. } => {
            contains_subquery(left) || contains_subquery(right)
        }
        SqlExpr::UnaryOp { expr, .. } | SqlExpr::Nested(expr) => contains_subquery(expr),
        SqlExpr::IsNull(expr) | SqlExpr::IsNotNull(expr) => contains_subquery(expr),
        SqlExpr::Between {
            expr, low, high, ..
        } => contains_subquery(expr) || contains_subquery(low) || contains_subquery(high),
        SqlExpr::InList { expr, list, .. } => {
            contains_subquery(expr) || list.iter().any(contains_subquery)
        }
        SqlExpr::Function(f) => match &f.args {
            FunctionArguments::None => false,
            FunctionArguments::Subquery(_) => true,
            FunctionArguments::List(list) => list.args.iter().any(|a| {
                let arg = match a {
                    FunctionArg::Named { arg, .. }
                    | FunctionArg::ExprNamed { arg, .. }
                    | FunctionArg::Unnamed(arg) => arg,
                };
                match arg {
                    FunctionArgExpr::Expr(e) => contains_subquery(e),
                    _ => false,
                }
            }),
        },
        _ => false,
    }
}

impl FanoutPlan {
    /// Parse and validate SQL against the fan-out v1 subset.
    pub fn parse(sql: &str) -> Result<Self> {
        let dialect = GenericDialect {};
        let mut stmts =
            Parser::parse_sql(&dialect, sql).map_err(|e| LessError::Query(format!("{e}")))?;
        if stmts.len() != 1 {
            return Err(LessError::Query(
                "fan-out takes exactly one statement".into(),
            ));
        }
        let Statement::Query(query) = stmts.remove(0) else {
            return Err(LessError::Query(
                "fan-out v1 supports SELECT queries only".into(),
            ));
        };
        let Query {
            with,
            body,
            order_by,
            limit_clause,
            ..
        } = *query;
        if with.is_some() {
            return Err(LessError::Query(
                "fan-out v1: CTEs (WITH) unsupported".into(),
            ));
        }
        let limit = match limit_clause {
            None => None,
            Some(LimitClause::LimitOffset {
                limit: Some(l),
                offset: None,
                ..
            }) => {
                let n = parse_int_literal(&l).ok_or_else(|| {
                    LessError::Query("fan-out v1: LIMIT must be a literal".into())
                })?;
                Some(n as u64)
            }
            Some(LimitClause::LimitOffset {
                limit: None,
                offset: None,
                ..
            }) => None,
            Some(_) => {
                return Err(LessError::Query("fan-out v1: OFFSET unsupported".into()));
            }
        };
        let order_by = match order_by {
            None => Vec::new(),
            Some(OrderBy {
                kind: OrderByKind::Expressions(exprs),
                ..
            }) => exprs,
            Some(OrderBy {
                kind: OrderByKind::All(_),
                ..
            }) => {
                return Err(LessError::Query(
                    "fan-out v1: ORDER BY ALL unsupported".into(),
                ));
            }
        };

        let SetExpr::Select(select) = *body else {
            return Err(LessError::Query(
                "fan-out v1: only plain SELECT (no UNION) supported".into(),
            ));
        };
        let Select {
            distinct,
            projection,
            from,
            selection,
            group_by,
            having,
            named_window,
            qualify,
            ..
        } = *select;
        if distinct.is_some() {
            return Err(LessError::Query("fan-out v1: DISTINCT unsupported".into()));
        }
        if having.is_some() {
            return Err(LessError::Query("fan-out v1: HAVING unsupported".into()));
        }
        if !named_window.is_empty() {
            return Err(LessError::Query("fan-out v1: windows unsupported".into()));
        }
        if qualify.is_some() {
            return Err(LessError::Query("fan-out v1: QUALIFY unsupported".into()));
        }
        if from.len() != 1 {
            return Err(LessError::Query(
                "fan-out v1: exactly one base table required".into(),
            ));
        }
        let TableWithJoins { relation, joins } = &from[0];
        if !joins.is_empty() {
            return Err(LessError::Query("fan-out v1: joins unsupported".into()));
        }
        let TableFactor::Table { name, .. } = relation else {
            return Err(LessError::Query(
                "fan-out v1: only base tables supported".into(),
            ));
        };
        let table = name
            .0
            .last()
            .and_then(|p| p.as_ident())
            .map(|i| i.value.clone())
            .unwrap_or_default();

        // Projection: every item gets a deterministic alias. Items must be
        // bare aggregates or simple expressions (no wrapped aggregates).
        let mut items: Vec<(String, SqlExpr)> = Vec::new();
        for (i, item) in projection.iter().enumerate() {
            let (expr, alias) = match item {
                SelectItem::UnnamedExpr(expr) => (expr.clone(), None),
                SelectItem::ExprWithAlias { expr, alias } => {
                    (expr.clone(), Some(alias.value.clone()))
                }
                _ => {
                    return Err(LessError::Query(
                        "fan-out v1: SELECT * and qualified wildcards unsupported".into(),
                    ));
                }
            };
            if contains_subquery(&expr) {
                return Err(LessError::Query(
                    "fan-out v1: subqueries unsupported".into(),
                ));
            }
            let is_agg = aggregate_name(&expr).is_some();
            if !is_agg && (!is_simple(&expr) || contains_aggregate(&expr)) {
                return Err(LessError::Query(format!(
                    "fan-out v1: unsupported projection expression `{expr}` \
                     (bare group columns and count/sum/min/max only)"
                )));
            }
            let alias = alias.or_else(|| match &expr {
                SqlExpr::Identifier(id) => Some(id.value.clone()),
                _ => None,
            });
            let alias = alias.unwrap_or_else(|| format!("_f{i}"));
            items.push((alias, expr));
        }

        let user_items = items.len();

        let where_clause = match selection {
            Some(e) => {
                if contains_subquery(&e) {
                    return Err(LessError::Query(
                        "fan-out v1: subqueries unsupported".into(),
                    ));
                }
                Some(e)
            }
            None => None,
        };

        // Group-by: each expression must be simple and is referred to by
        // its projection alias (appended to the projection when missing).
        let group_exprs: Vec<SqlExpr> = group_by_exprs(&group_by)?.unwrap_or_default();
        let mut group_by: Vec<(String, SqlExpr)> = Vec::new();
        for (k, g) in group_exprs.into_iter().enumerate() {
            if !is_simple(&g) || contains_subquery(&g) || contains_aggregate(&g) {
                return Err(LessError::Query(format!(
                    "fan-out v1: unsupported GROUP BY expression `{g}`"
                )));
            }
            let alias = items
                .iter()
                .find(|(_, e)| e == &g)
                .map(|(a, _)| a.clone())
                .unwrap_or_else(|| {
                    let a = format!("_g{k}");
                    items.push((a.clone(), g.clone()));
                    a
                });
            group_by.push((alias, g));
        }

        // Order-by: identifier or bare aggregate only, mapped to aliases.
        let mut order_by_out = Vec::new();
        for o in order_by {
            let expr = o.expr;
            if !(matches!(expr, SqlExpr::Identifier(_)) || aggregate_name(&expr).is_some()) {
                return Err(LessError::Query(format!(
                    "fan-out v1: unsupported ORDER BY expression `{expr}`"
                )));
            }
            let alias = match &expr {
                SqlExpr::Identifier(id) => {
                    let name = id.value.clone();
                    items
                        .iter()
                        .find(|(a, _)| a == &name)
                        .map(|(a, _)| a.clone())
                        .unwrap_or(name)
                }
                _ => {
                    let idx = items.iter().position(|(_, e)| e == &expr).ok_or_else(|| {
                        LessError::Query(
                            "fan-out v1: ORDER BY aggregate must appear in SELECT".into(),
                        )
                    })?;
                    items[idx].0.clone()
                }
            };
            order_by_out.push((alias, o.options.asc, expr));
        }

        Ok(Self {
            sql: sql.to_string(),
            table,
            items,
            where_clause,
            user_items,
            group_by,
            order_by: order_by_out,
            limit,
        })
    }

    /// The SQL run on node `i` of `n` (partial aggregates pushed down).
    pub fn sharded_sql(&self, i: usize, n: usize, first_column: &str) -> Result<String> {
        let items: Vec<String> = self
            .items
            .iter()
            .map(|(alias, expr)| {
                let bare = *alias == expr.to_string();
                if bare {
                    expr.to_string()
                } else {
                    format!("{expr} AS {alias}")
                }
            })
            .collect();
        let mut sql = format!("SELECT {} FROM {}", items.join(", "), self.table);
        match &self.where_clause {
            Some(w) => {
                sql.push_str(&format!(
                    " WHERE ({w}) AND lessdb_shard({first_column}, {i}, {n})"
                ));
            }
            None => {
                sql.push_str(&format!(" WHERE lessdb_shard({first_column}, {i}, {n})"));
            }
        }
        if !self.group_by.is_empty() {
            let groups: Vec<&str> = self.group_by.iter().map(|(a, _)| a.as_str()).collect();
            sql.push_str(&format!(" GROUP BY {}", groups.join(", ")));
        }
        Ok(sql)
    }

    /// The coordinator-side SQL re-aggregating the `partial` table.
    pub fn final_sql(&self) -> String {
        let items: Vec<String> = self
            .items
            .iter()
            .take(self.user_items)
            .map(|(alias, expr)| {
                if let Some(name) = aggregate_name(expr) {
                    match name.as_str() {
                        // Partial aggregates merge by re-applying the same
                        // function over the partial column — except count,
                        // which sums counts.
                        "count" => format!("sum({alias}) AS {alias}"),
                        _ => format!("{name}({alias}) AS {alias}"),
                    }
                } else {
                    alias.clone()
                }
            })
            .collect();
        let mut sql = format!("SELECT {} FROM partial", items.join(", "));
        if !self.group_by.is_empty() {
            let groups: Vec<&str> = self.group_by.iter().map(|(a, _)| a.as_str()).collect();
            sql.push_str(&format!(" GROUP BY {}", groups.join(", ")));
        }
        if !self.order_by.is_empty() {
            let orders: Vec<String> = self
                .order_by
                .iter()
                .map(|(alias, asc, _)| {
                    let dir = if *asc == Some(false) { " DESC" } else { "" };
                    format!("{alias}{dir}")
                })
                .collect();
            sql.push_str(&format!(" ORDER BY {}", orders.join(", ")));
        }
        if let Some(limit) = self.limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }
        sql
    }

    /// Original SQL, for diagnostics.
    pub fn sql(&self) -> &str {
        &self.sql
    }
}

/// Extract plain GROUP BY expressions; reject modifiers/ALL.
fn group_by_exprs(g: &GroupByExpr) -> Result<Option<Vec<SqlExpr>>> {
    match g {
        GroupByExpr::Expressions(exprs, modifiers) if modifiers.is_empty() => {
            Ok(Some(exprs.clone()))
        }
        GroupByExpr::Expressions(_, _) => Err(LessError::Query(
            "fan-out v1: GROUP BY modifiers (ROLLUP etc.) unsupported".into(),
        )),
        GroupByExpr::All(_) => Err(LessError::Query(
            "fan-out v1: GROUP BY ALL unsupported".into(),
        )),
    }
}

/// Ask a node's describe endpoint for the table's first column name
/// (needed as the `lessdb_shard` marker column).
async fn first_column(base: &str, table: &str) -> Result<String> {
    let url = format!("{base}/v1/describe/{table}");
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| LessError::Query(format!("{url}: {e}")))?;
    if !resp.status().is_success() {
        return Err(LessError::Query(format!("{url}: HTTP {}", resp.status())));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| LessError::Query(format!("{url}: bad JSON: {e}")))?;
    body["schema"]["fields"]
        .as_array()
        .and_then(|f| f.first())
        .and_then(|f| f["name"].as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| LessError::Query(format!("{url}: could not find a table column")))
}

/// Run one sharded query against a node; returns its Arrow IPC batches.
async fn query_node(base: &str, sql: &str) -> Result<Vec<RecordBatch>> {
    let url = format!("{base}/v1/sql");
    let resp = reqwest::Client::new()
        .post(&url)
        .json(&serde_json::json!({ "sql": sql, "format": "arrow" }))
        .send()
        .await
        .map_err(|e| LessError::Query(format!("{url}: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(LessError::Query(format!("{url}: HTTP {status}: {text}")));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| LessError::Query(format!("{url}: {e}")))?;
    let reader = StreamReader::try_new(std::io::Cursor::new(bytes.as_ref()), None)
        .map_err(LessError::Arrow)?;
    let mut out = Vec::new();
    for batch in reader {
        out.push(batch.map_err(LessError::Arrow)?);
    }
    Ok(out)
}

/// Fan a SQL query out across `nodes` (base URLs like `http://host:7080`)
/// and return the merged result batches.
pub async fn fanout(nodes: &[String], sql: &str) -> Result<Vec<RecordBatch>> {
    if nodes.is_empty() {
        return Err(LessError::Query("fan-out needs at least one node".into()));
    }
    let plan = FanoutPlan::parse(sql)?;
    let first_column = first_column(&nodes[0], &plan.table).await?;
    let n = nodes.len();

    let mut per_node = Vec::with_capacity(n);
    for (i, node) in nodes.iter().enumerate() {
        let sharded = plan.sharded_sql(i, n, &first_column)?;
        per_node.push(query_node(node, &sharded).await?);
    }

    // Empty result: still have a schema from the first node's IPC stream.
    let schema = per_node
        .iter()
        .find_map(|b| b.first().map(|b| b.schema()))
        .ok_or_else(|| LessError::Query("fan-out: no schema returned".into()))?;

    // Merge partial batches into one local `partial` table.
    let mut all: Vec<RecordBatch> = Vec::new();
    for batches in &per_node {
        all.extend(batches.iter().cloned());
    }
    let ctx = SessionContext::new();
    let partial = MemTable::try_new(schema, vec![all])
        .map_err(|e| LessError::Query(format!("partial table: {e}")))?;
    ctx.register_table("partial", Arc::new(partial))
        .map_err(|e| LessError::Query(format!("register partial: {e}")))?;

    let final_sql = plan.final_sql();
    let df = ctx
        .sql(&final_sql)
        .await
        .map_err(|e| LessError::Query(format!("{final_sql}\n{e}")))?;
    df.collect()
        .await
        .map_err(|e| LessError::Query(format!("{final_sql}\n{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_rewrites_sharded_and_final() {
        let plan =
            FanoutPlan::parse("SELECT kind, count(*) AS n, sum(v) AS total FROM events WHERE v > 1 GROUP BY kind ORDER BY n DESC LIMIT 10")
                .unwrap();
        assert_eq!(
            plan.sharded_sql(0, 2, "id").unwrap(),
            "SELECT kind, count(*) AS n, sum(v) AS total FROM events \
             WHERE (v > 1) AND lessdb_shard(id, 0, 2) GROUP BY kind"
        );
        assert_eq!(
            plan.final_sql(),
            "SELECT kind, sum(n) AS n, sum(total) AS total FROM partial \
             GROUP BY kind ORDER BY n DESC LIMIT 10"
        );
    }

    #[test]
    fn plan_auto_aliases_aggregates() {
        let plan = FanoutPlan::parse("SELECT count(*) FROM t").unwrap();
        assert_eq!(
            plan.sharded_sql(0, 1, "id").unwrap(),
            "SELECT count(*) AS _f0 FROM t WHERE lessdb_shard(id, 0, 1)"
        );
        assert_eq!(plan.final_sql(), "SELECT sum(_f0) AS _f0 FROM partial");
    }

    #[test]
    fn plan_group_by_column_not_in_projection() {
        let plan = FanoutPlan::parse("SELECT count(*) FROM t GROUP BY kind").unwrap();
        let sharded = plan.sharded_sql(0, 1, "id").unwrap();
        assert!(sharded.contains("kind AS _g0"), "{sharded}");
        assert!(sharded.ends_with("GROUP BY _g0"), "{sharded}");
        assert_eq!(
            plan.final_sql(),
            "SELECT sum(_f0) AS _f0 FROM partial GROUP BY _g0"
        );
    }

    #[test]
    fn plan_rejects_unsupported() {
        for bad in [
            "SELECT avg(v) FROM t",
            "SELECT * FROM t",
            "SELECT a FROM t JOIN u ON t.id = u.id",
            "SELECT sum(v) + 1 FROM t",
            "SELECT v FROM t GROUP BY v HAVING sum(v) > 1",
            "WITH x AS (SELECT 1) SELECT * FROM x",
            "SELECT DISTINCT v FROM t",
            "SELECT v FROM t WHERE id IN (SELECT id FROM u)",
            "SELECT v FROM t LIMIT 2 OFFSET 1",
        ] {
            assert!(FanoutPlan::parse(bad).is_err(), "should reject: {bad}");
        }
    }
}
