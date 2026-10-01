//! AST for the LessDB openCypher subset.
#![allow(clippy::large_enum_variant)]

use serde_json::Value;

/// A parsed Cypher statement.
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Query(Query),
    Create(Vec<CreateElement>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub patterns: Vec<Pattern>,
    pub where_clause: Option<Expr>,
    pub return_items: ReturnClause,
    pub order_by: Vec<SortItem>,
    pub skip: Option<usize>,
    pub limit: Option<usize>,
    pub distinct: bool,
    /// DELETE <vars> after MATCH.
    pub deletes: Vec<String>,
    /// SET var.prop = expr after MATCH.
    pub sets: Vec<(String, String, Value)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SortItem {
    pub item: ReturnItem,
    pub desc: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReturnClause {
    pub items: Vec<ReturnItem>,
    pub star: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReturnItem {
    /// `n.prop`
    Prop(String, String),
    /// bare variable `n`
    Var(String),
    /// `id(n)` / `labels(n)` / `count(*)` etc.
    Func(String, Vec<Box<ReturnItem>>),
    /// literal
    Lit(Value),
    /// `expr AS alias`
    Alias(Box<ReturnItem>, String),
}

impl ReturnItem {
    pub fn alias_of(&self) -> String {
        match self {
            ReturnItem::Prop(v, p) => format!("{v}.{p}"),
            ReturnItem::Var(v) => v.clone(),
            ReturnItem::Func(name, args) => {
                let args_s: Vec<String> = args.iter().map(|a| a.alias_of()).collect();
                format!("{name}({})", args_s.join(", "))
            }
            ReturnItem::Lit(v) => match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            },
            ReturnItem::Alias(_, alias) => alias.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    pub chain: Vec<ChainPart>,
}

/// One node or edge in a pattern chain.
#[derive(Debug, Clone, PartialEq)]
pub enum ChainPart {
    Node {
        var: Option<String>,
        labels: Vec<String>,
        props: Vec<(String, Value)>,
    },
    Edge {
        var: Option<String>,
        types: Vec<String>,
        direction: Direction,
        min_hops: usize,
        max_hops: Option<usize>,
        props: Vec<(String, Value)>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    Out,
    In,
    Both,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Lit(Value),
    Prop(String, String),
    Eq(Box<Expr>, Box<Expr>),
    Neq(Box<Expr>, Box<Expr>),
    Lt(Box<Expr>, Box<Expr>),
    Le(Box<Expr>, Box<Expr>),
    Gt(Box<Expr>, Box<Expr>),
    Ge(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    In(Box<Expr>, Vec<Value>),
    StartsWith(Box<Expr>, Box<Expr>),
    EndsWith(Box<Expr>, Box<Expr>),
    Contains(Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum CreateElement {
    Node {
        var: Option<String>,
        labels: Vec<String>,
        props: Vec<(String, Value)>,
    },
    Edge {
        var: Option<String>,
        from: NodeRef,
        to: NodeRef,
        types: Vec<String>,
        direction: Direction,
        props: Vec<(String, Value)>,
    },
}

/// A CREATE edge endpoint: an existing/created variable or a key literal.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeRef {
    Var(String),
    Key(String),
}
