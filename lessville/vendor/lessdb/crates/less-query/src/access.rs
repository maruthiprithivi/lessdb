//! Restricted SQL for untrusted read-capability callers.
//! Validate before native statement dispatch, file registration, or planning.
use std::ops::ControlFlow;

use datafusion::sql::sqlparser::ast::{Query, Select, Statement, TableFactor, Visit, Visitor};
use datafusion::sql::sqlparser::dialect::GenericDialect;
use datafusion::sql::sqlparser::parser::Parser;
use less_common::{LessError, Result};

/// Permit one query over registered tables, including CTEs and subqueries.
/// External table functions, SELECT INTO, locks and nested writes are denied.
/// This is a capability boundary, not row/table-level tenant isolation.
pub fn validate_read_query(sql: &str) -> Result<()> {
    let statements = Parser::parse_sql(&GenericDialect {}, sql)
        .map_err(|e| LessError::Query(format!("read query rejected: {e}")))?;
    if statements.len() != 1 || !matches!(statements[0], Statement::Query(_)) {
        return Err(denied());
    }
    if statements.visit(&mut ReadOnly).is_break() {
        return Err(denied());
    }
    Ok(())
}

fn denied() -> LessError {
    LessError::Query(
        "access denied: SQL capability permits one read query over registered tables only".into(),
    )
}

struct ReadOnly;
impl Visitor for ReadOnly {
    type Break = ();

    fn pre_visit_statement(&mut self, statement: &Statement) -> ControlFlow<()> {
        if matches!(statement, Statement::Query(_)) {
            ControlFlow::Continue(())
        } else {
            ControlFlow::Break(())
        }
    }

    fn pre_visit_select(&mut self, select: &Select) -> ControlFlow<()> {
        if select.into.is_some() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }

    fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<()> {
        if query.locks.is_empty() {
            ControlFlow::Continue(())
        } else {
            ControlFlow::Break(())
        }
    }

    fn pre_visit_table_factor(&mut self, factor: &TableFactor) -> ControlFlow<()> {
        match factor {
            TableFactor::Table {
                name, args: None, ..
            } if !name.to_string().contains(['/', '\\']) => ControlFlow::Continue(()),
            TableFactor::Derived { .. } | TableFactor::NestedJoin { .. } => {
                ControlFlow::Continue(())
            }
            _ => ControlFlow::Break(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_capability_accepts_queries_and_rejects_side_effects() {
        for sql in [
            "SELECT 1",
            "-- DELETE\nSELECT 'DROP TABLE t'",
            "WITH q AS (SELECT 1 AS n) SELECT n FROM q",
            "SELECT * FROM (SELECT 1) q",
            "SELECT COUNT(*) FROM t WHERE id = 1",
        ] {
            assert!(validate_read_query(sql).is_ok(), "{sql}");
        }
        for sql in [
            "",
            "SELECT 1; DROP TABLE t",
            "/* SELECT */ DELETE FROM t",
            "INSERT INTO t VALUES (1)",
            "CREATE TABLE t (id INT)",
            "VACUUM t",
            "COPY t TO '/tmp/out'",
            "SELECT * INTO new_table FROM t",
            "SELECT * FROM read_csv('/tmp/secret.csv')",
            "SELECT * FROM read_parquet('/tmp/secret.parquet')",
            "SELECT * FROM '/tmp/secret.csv'",
            "SELECT * FROM t FOR UPDATE",
            "SET x = 1",
        ] {
            assert!(validate_read_query(sql).is_err(), "{sql}");
        }
    }
}
