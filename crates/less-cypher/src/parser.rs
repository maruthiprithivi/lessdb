//! Recursive-descent parser for the LessDB openCypher subset.
//!
//! Supported (documented subset):
//! ```cypher
//! MATCH (a:Label {prop: value})-[r:TYPE*1..3]->(b) [, more patterns]
//! [WHERE <expr>]
//! RETURN * | n.prop | n | id(n) | labels(n) | count(*) | count(x) |
//!        collect(x) | sum/avg/min/max(x) [AS alias] [, ...]
//! [ORDER BY item [ASC|DESC] [, ...]] [SKIP n] [LIMIT n] [DISTINCT]
//! CREATE (n:Label {..}), (a)-[:TYPE]->(b), ...
//! MATCH ... DELETE n
//! MATCH ... SET n.prop = value
//! ```
//! Expressions: comparisons (`=` `<>` `<` `>` `<=` `>=`), `IN [..]`,
#![allow(clippy::type_complexity)]

//! `CONTAINS` / `STARTS WITH` / `ENDS WITH`, `AND`/`OR`/`NOT`, literals.

use serde_json::Value;

use less_common::{LessError, Result};

use crate::ast::*;
use crate::lexer::{Kw, Tok, tokenize};

pub fn parse(input: &str) -> Result<Statement> {
    let toks = tokenize(input)?;
    Parser { toks, pos: 0 }.statement()
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos]
    }

    fn next(&mut self) -> Tok {
        let t = self.toks[self.pos].clone();
        self.pos += 1;
        t
    }

    fn eat_kw(&mut self, kw: Kw) -> bool {
        if self.peek() == &Tok::Kw(kw) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_kw(&mut self, kw: Kw, what: &str) -> Result<()> {
        if self.eat_kw(kw) {
            Ok(())
        } else {
            Err(LessError::Query(format!(
                "cypher: expected {what}, found {:?}",
                self.peek()
            )))
        }
    }

    fn expect_ident(&mut self) -> Result<String> {
        match self.next() {
            Tok::Ident(s) => Ok(s),
            other => Err(LessError::Query(format!(
                "cypher: expected identifier, found {other:?}"
            ))),
        }
    }

    fn expect_int(&mut self) -> Result<usize> {
        match self.next() {
            Tok::Int(n) if n >= 0 => Ok(n as usize),
            other => Err(LessError::Query(format!(
                "cypher: expected non-negative integer, found {other:?}"
            ))),
        }
    }

    fn statement(&mut self) -> Result<Statement> {
        if self.eat_kw(Kw::Create) {
            return Ok(Statement::Create(self.create_elements()?));
        }
        self.expect_kw(Kw::Match, "MATCH")?;
        let patterns = self.pattern_list()?;
        let where_clause = if self.eat_kw(Kw::Where) {
            Some(self.expr()?)
        } else {
            None
        };
        let mut return_clause: Option<(
            ReturnClause,
            Vec<SortItem>,
            Option<usize>,
            Option<usize>,
            bool,
        )> = None;
        let mut deletes = vec![];
        let mut sets = vec![];
        loop {
            if return_clause.is_none() && self.eat_kw(Kw::Return) {
                return_clause = Some(self.return_clause()?);
                continue;
            }
            if self.eat_kw(Kw::Delete) {
                loop {
                    deletes.push(self.expect_ident()?);
                    if !self.eat_tok(&Tok::Comma) {
                        break;
                    }
                }
                continue;
            }
            if self.eat_kw(Kw::Set) {
                loop {
                    let var = self.expect_ident()?;
                    if self.peek() != &Tok::Dot {
                        return Err(LessError::Query(
                            "cypher: SET expects n.prop = value".into(),
                        ));
                    }
                    self.pos += 1; // .
                    let prop = self.expect_ident()?;
                    if self.peek() != &Tok::Eq {
                        return Err(LessError::Query("cypher: SET expects '='".into()));
                    }
                    self.pos += 1; // =
                    let value = self.value()?;
                    sets.push((var, prop, value));
                    if !self.eat_tok(&Tok::Comma) {
                        break;
                    }
                }
                continue;
            }
            break;
        }
        if return_clause.is_none() && deletes.is_empty() && sets.is_empty() {
            return Err(LessError::Query(
                "cypher: MATCH requires a RETURN, DELETE or SET clause".into(),
            ));
        }
        let (return_items, order_by, skip, limit, distinct) = return_clause.unwrap_or((
            ReturnClause {
                items: vec![],
                star: false,
            },
            vec![],
            None,
            None,
            false,
        ));
        if self.peek() != &Tok::Eof {
            return Err(LessError::Query(format!(
                "cypher: unexpected trailing input {:?}",
                self.peek()
            )));
        }
        Ok(Statement::Query(Query {
            patterns,
            where_clause,
            return_items,
            order_by,
            skip,
            limit,
            distinct,
            deletes,
            sets,
        }))
    }

    fn eat_tok(&mut self, t: &Tok) -> bool {
        if self.peek() == t {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn pattern_list(&mut self) -> Result<Vec<Pattern>> {
        let mut out = vec![self.pattern()?];
        while self.eat_tok(&Tok::Comma) {
            out.push(self.pattern()?);
        }
        Ok(out)
    }

    fn pattern(&mut self) -> Result<Pattern> {
        let mut chain = vec![self.node_part(true)?];
        while matches!(self.peek(), Tok::Minus | Tok::ArrowL) {
            let edge = self.edge_part()?;
            let node = self.node_part(false)?;
            chain.push(edge);
            chain.push(node);
        }
        Ok(Pattern { chain })
    }

    /// Parse `(var:Label:Label2 {props})`. `required` for the first node.
    fn node_part(&mut self, required: bool) -> Result<ChainPart> {
        if self.peek() != &Tok::LParen {
            if required {
                return Err(LessError::Query(format!(
                    "cypher: expected node pattern, found {:?}",
                    self.peek()
                )));
            }
            return Ok(ChainPart::Node {
                var: None,
                labels: vec![],
                props: vec![],
            });
        }
        self.pos += 1; // (
        let mut var = None;
        if let Tok::Ident(_) = self.peek().clone() {
            var = Some(self.expect_ident()?);
        }
        let mut labels = vec![];
        while self.eat_tok(&Tok::Colon) {
            labels.push(self.expect_ident()?);
        }
        let mut props = vec![];
        if self.eat_tok(&Tok::LBrace) {
            props = self.props_until_rbrace()?;
        }
        self.expect_tok(&Tok::RParen, "')'")?;
        Ok(ChainPart::Node { var, labels, props })
    }

    fn edge_part(&mut self) -> Result<ChainPart> {
        let direction = if self.eat_tok(&Tok::ArrowL) {
            Direction::In
        } else {
            // '-'
            self.expect_tok(&Tok::Minus, "'-'")?;
            if self.eat_tok(&Tok::Minus) {
                // undirected shorthand '--'
                return Ok(ChainPart::Edge {
                    var: None,
                    types: vec![],
                    direction: Direction::Both,
                    min_hops: 1,
                    max_hops: Some(1),
                    props: vec![],
                });
            }
            Direction::Out
        };
        self.expect_tok(&Tok::LBracket, "'['")?;
        let mut var = None;
        if let Tok::Ident(_) = self.peek().clone() {
            var = Some(self.expect_ident()?);
        }
        let mut types = vec![];
        while self.eat_tok(&Tok::Colon) {
            types.push(self.expect_ident()?);
        }
        let mut min_hops = 1;
        let mut max_hops = Some(1);
        if self.eat_tok(&Tok::Star) {
            // * or *min..max or *..max or *min..
            min_hops = 0;
            max_hops = Some(usize::MAX / 2);
            if let Tok::Int(n) = self.peek().clone()
                && n >= 0
            {
                min_hops = n as usize;
                self.pos += 1;
            }
            // check for '..'
            if self.peek() == &Tok::Dot {
                // lexer produced Dot only for single '.', a bare '..' is an
                // error; support the range form by peeking two dots.
                self.pos += 1;
                if self.peek() == &Tok::Dot {
                    self.pos += 1;
                    max_hops = None;
                    if let Tok::Int(n) = self.peek().clone()
                        && n >= 0
                    {
                        max_hops = Some(n as usize);
                        self.pos += 1;
                    }
                } else {
                    return Err(LessError::Query(
                        "cypher: expected '..' in variable-length pattern".into(),
                    ));
                }
            }
        }
        let mut props = vec![];
        if self.eat_tok(&Tok::LBrace) {
            props = self.props_until_rbrace()?;
        }
        self.expect_tok(&Tok::RBracket, "']'")?;
        if direction == Direction::Out {
            self.expect_tok(&Tok::Arrow, "'->'")?;
        } else {
            self.expect_tok(&Tok::Minus, "'-'")?;
        }
        Ok(ChainPart::Edge {
            var,
            types,
            direction,
            min_hops,
            max_hops: max_hops.map(|m| m.min(usize::MAX / 2)),
            props,
        })
    }

    fn expect_tok(&mut self, t: &Tok, what: &str) -> Result<()> {
        if self.peek() == t {
            self.pos += 1;
            Ok(())
        } else {
            Err(LessError::Query(format!(
                "cypher: expected {what}, found {:?}",
                self.peek()
            )))
        }
    }

    fn props_until_rbrace(&mut self) -> Result<Vec<(String, Value)>> {
        let mut props = vec![];
        loop {
            if self.eat_tok(&Tok::RBrace) {
                return Ok(props);
            }
            let key = self.expect_ident()?;
            self.expect_tok(&Tok::Colon, "':'")?;
            let value = self.value()?;
            props.push((key, value));
            if !self.eat_tok(&Tok::Comma) {
                self.expect_tok(&Tok::RBrace, "'}'")?;
                return Ok(props);
            }
        }
    }

    fn value(&mut self) -> Result<Value> {
        match self.next() {
            Tok::String(s) => Ok(Value::String(s)),
            Tok::Number(f) => Ok(Value::from(f)),
            Tok::Int(i) => Ok(Value::from(i)),
            Tok::Kw(Kw::True) => Ok(Value::Bool(true)),
            Tok::Kw(Kw::False) => Ok(Value::Bool(false)),
            Tok::Kw(Kw::Null) => Ok(Value::Null),
            Tok::LBracket => {
                let mut list = vec![];
                loop {
                    if self.eat_tok(&Tok::RBracket) {
                        return Ok(Value::Array(list));
                    }
                    list.push(self.value()?);
                    if !self.eat_tok(&Tok::Comma) {
                        self.expect_tok(&Tok::RBracket, "']'")?;
                        return Ok(Value::Array(list));
                    }
                }
            }
            Tok::LBrace => {
                let mut map = serde_json::Map::new();
                for (k, v) in self.props_until_rbrace()? {
                    map.insert(k, v);
                }
                Ok(Value::Object(map))
            }
            other => Err(LessError::Query(format!(
                "cypher: expected value, found {other:?}"
            ))),
        }
    }

    fn return_clause(
        &mut self,
    ) -> Result<(
        ReturnClause,
        Vec<SortItem>,
        Option<usize>,
        Option<usize>,
        bool,
    )> {
        let distinct = self.eat_kw(Kw::Distinct);
        let mut items = vec![];
        let mut star = false;
        loop {
            if self.eat_tok(&Tok::Star) {
                star = true;
            } else {
                let mut item = self.return_item()?;
                if self.eat_kw(Kw::As) {
                    let alias = self.expect_ident()?;
                    item = ReturnItem::Alias(Box::new(item), alias);
                }
                items.push(item);
            }
            if !self.eat_tok(&Tok::Comma) {
                break;
            }
        }
        let mut order_by = vec![];
        if self.eat_kw(Kw::Order) {
            self.expect_kw(Kw::By, "BY")?;
            loop {
                let item = self.return_item()?;
                let desc = if self.eat_kw(Kw::Desc) {
                    true
                } else {
                    self.eat_kw(Kw::Asc);
                    false
                };
                order_by.push(SortItem { item, desc });
                if !self.eat_tok(&Tok::Comma) {
                    break;
                }
            }
        }
        let skip = if self.eat_kw(Kw::Skip) {
            Some(self.expect_int()?)
        } else {
            None
        };
        let limit = if self.eat_kw(Kw::Limit) {
            Some(self.expect_int()?)
        } else {
            None
        };
        Ok((
            ReturnClause { items, star },
            order_by,
            skip,
            limit,
            distinct,
        ))
    }

    fn return_item(&mut self) -> Result<ReturnItem> {
        match self.peek().clone() {
            Tok::Ident(name) => {
                self.pos += 1;
                // function call?
                if self.eat_tok(&Tok::LParen) {
                    let mut args = vec![];
                    if self.eat_tok(&Tok::Star) {
                        args.push(Box::new(ReturnItem::Lit(Value::String("*".into()))));
                        self.expect_tok(&Tok::RParen, "')'")?;
                        return Ok(ReturnItem::Func(name, args));
                    } else if self.eat_tok(&Tok::RParen) {
                        // no args (count()? tolerate)
                    } else {
                        loop {
                            args.push(Box::new(self.return_item()?));
                            if !self.eat_tok(&Tok::Comma) {
                                break;
                            }
                        }
                        self.expect_tok(&Tok::RParen, "')'")?;
                    }
                    return Ok(ReturnItem::Func(name, args));
                }
                if self.eat_tok(&Tok::Dot) {
                    let prop = self.expect_ident()?;
                    return Ok(ReturnItem::Prop(name, prop));
                }
                Ok(ReturnItem::Var(name))
            }
            Tok::String(s) => {
                self.pos += 1;
                Ok(ReturnItem::Lit(Value::String(s)))
            }
            Tok::Int(i) => {
                self.pos += 1;
                Ok(ReturnItem::Lit(Value::from(i)))
            }
            Tok::Number(f) => {
                self.pos += 1;
                Ok(ReturnItem::Lit(Value::from(f)))
            }
            other => Err(LessError::Query(format!(
                "cypher: expected return item, found {other:?}"
            ))),
        }
    }

    fn expr(&mut self) -> Result<Expr> {
        self.or_expr()
    }

    fn or_expr(&mut self) -> Result<Expr> {
        let mut left = self.and_expr()?;
        while self.eat_kw(Kw::Or) {
            let right = self.and_expr()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn and_expr(&mut self) -> Result<Expr> {
        let mut left = self.not_expr()?;
        while self.eat_kw(Kw::And) {
            let right = self.not_expr()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn not_expr(&mut self) -> Result<Expr> {
        if self.eat_kw(Kw::Not) {
            return Ok(Expr::Not(Box::new(self.not_expr()?)));
        }
        self.cmp_expr()
    }

    fn cmp_expr(&mut self) -> Result<Expr> {
        let left = self.operand()?;
        let op = match self.peek().clone() {
            Tok::Eq => Some("="),
            Tok::Neq => Some("<>"),
            Tok::Lt => Some("<"),
            Tok::Gt => Some(">"),
            Tok::Le => Some("<="),
            Tok::Ge => Some(">="),
            _ => None,
        };
        if let Some(op) = op {
            self.pos += 1;
            let right = self.operand()?;
            return Ok(match op {
                "=" => Expr::Eq(Box::new(left), Box::new(right)),
                "<>" => Expr::Neq(Box::new(left), Box::new(right)),
                "<" => Expr::Lt(Box::new(left), Box::new(right)),
                ">" => Expr::Gt(Box::new(left), Box::new(right)),
                "<=" => Expr::Le(Box::new(left), Box::new(right)),
                _ => Expr::Ge(Box::new(left), Box::new(right)),
            });
        }
        if self.eat_kw(Kw::In) {
            let mut list = vec![];
            self.expect_tok(&Tok::LBracket, "'['")?;
            loop {
                if self.eat_tok(&Tok::RBracket) {
                    break;
                }
                list.push(self.value()?);
                if !self.eat_tok(&Tok::Comma) {
                    self.expect_tok(&Tok::RBracket, "']'")?;
                    break;
                }
            }
            return Ok(Expr::In(Box::new(left), list));
        }
        if self.eat_kw(Kw::Starts) {
            self.expect_kw(Kw::With, "WITH")?;
            return Ok(Expr::StartsWith(Box::new(left), Box::new(self.operand()?)));
        }
        if self.eat_kw(Kw::Ends) {
            self.expect_kw(Kw::With, "WITH")?;
            return Ok(Expr::EndsWith(Box::new(left), Box::new(self.operand()?)));
        }
        if self.eat_kw(Kw::Contains) {
            return Ok(Expr::Contains(Box::new(left), Box::new(self.operand()?)));
        }
        Ok(left)
    }

    fn operand(&mut self) -> Result<Expr> {
        match self.peek().clone() {
            Tok::Ident(name) => {
                self.pos += 1;
                if self.eat_tok(&Tok::Dot) {
                    let prop = self.expect_ident()?;
                    Ok(Expr::Prop(name, prop))
                } else {
                    Err(LessError::Query(
                        "cypher: WHERE expressions support n.prop comparisons".into(),
                    ))
                }
            }
            Tok::String(s) => {
                self.pos += 1;
                Ok(Expr::Lit(Value::String(s)))
            }
            Tok::Int(i) => {
                self.pos += 1;
                Ok(Expr::Lit(Value::from(i)))
            }
            Tok::Number(f) => {
                self.pos += 1;
                Ok(Expr::Lit(Value::from(f)))
            }
            Tok::Kw(Kw::True) => {
                self.pos += 1;
                Ok(Expr::Lit(Value::Bool(true)))
            }
            Tok::Kw(Kw::False) => {
                self.pos += 1;
                Ok(Expr::Lit(Value::Bool(false)))
            }
            Tok::Kw(Kw::Null) => {
                self.pos += 1;
                Ok(Expr::Lit(Value::Null))
            }
            Tok::LParen => {
                self.pos += 1;
                let e = self.expr()?;
                self.expect_tok(&Tok::RParen, "')'")?;
                Ok(e)
            }
            other => Err(LessError::Query(format!(
                "cypher: expected expression operand, found {other:?}"
            ))),
        }
    }

    fn create_elements(&mut self) -> Result<Vec<CreateElement>> {
        let mut out = vec![self.create_element()?];
        while self.eat_tok(&Tok::Comma) {
            out.push(self.create_element()?);
        }
        if self.peek() != &Tok::Eof {
            return Err(LessError::Query(format!(
                "cypher: unexpected trailing input {:?}",
                self.peek()
            )));
        }
        Ok(out)
    }

    fn create_element(&mut self) -> Result<CreateElement> {
        self.expect_tok(&Tok::LParen, "'('")?;
        let mut var = None;
        if let Tok::Ident(_) = self.peek().clone() {
            var = Some(self.expect_ident()?);
        }
        let mut labels = vec![];
        while self.eat_tok(&Tok::Colon) {
            labels.push(self.expect_ident()?);
        }
        let mut props = vec![];
        if self.eat_tok(&Tok::LBrace) {
            props = self.props_until_rbrace()?;
        }
        self.expect_tok(&Tok::RParen, "')'")?;

        // Is this a node, or an edge pattern `(a)-[:R]->(b)`?
        match self.peek().clone() {
            Tok::Minus | Tok::ArrowL => {
                let direction = if self.eat_tok(&Tok::ArrowL) {
                    Direction::In
                } else {
                    self.pos += 1; // -
                    Direction::Out
                };
                let mut edge_var = None;
                let mut types = vec![];
                if self.eat_tok(&Tok::LBracket) {
                    if let Tok::Ident(_) = self.peek().clone() {
                        edge_var = Some(self.expect_ident()?);
                    }
                    while self.eat_tok(&Tok::Colon) {
                        types.push(self.expect_ident()?);
                    }
                    let mut edge_props = vec![];
                    if self.eat_tok(&Tok::LBrace) {
                        edge_props = self.props_until_rbrace()?;
                    }
                    self.expect_tok(&Tok::RBracket, "']'")?;
                    props = edge_props;
                }
                if direction == Direction::Out {
                    self.expect_tok(&Tok::Arrow, "'->'")?;
                } else {
                    self.expect_tok(&Tok::Minus, "'-'")?;
                }
                self.expect_tok(&Tok::LParen, "'('")?;
                let mut to_var = None;
                if let Tok::Ident(_) = self.peek().clone() {
                    to_var = Some(self.expect_ident()?);
                }
                let mut to_labels = vec![];
                while self.eat_tok(&Tok::Colon) {
                    to_labels.push(self.expect_ident()?);
                }
                let mut to_props = vec![];
                if self.eat_tok(&Tok::LBrace) {
                    to_props = self.props_until_rbrace()?;
                }
                self.expect_tok(&Tok::RParen, "')'")?;
                let _ = to_labels;
                let _ = to_props;
                Ok(CreateElement::Edge {
                    var: edge_var,
                    from: match var {
                        Some(v) => NodeRef::Var(v),
                        None => NodeRef::Key(
                            props
                                .iter()
                                .find(|(k, _)| k == "key")
                                .and_then(|(_, v)| v.as_str())
                                .ok_or_else(|| {
                                    LessError::Query(
                                        "cypher: CREATE edge endpoints need a variable or a key property"
                                            .into(),
                                    )
                                })?
                                .to_string(),
                        ),
                    },
                    to: match to_var {
                        Some(v) => NodeRef::Var(v),
                        None => NodeRef::Key(
                            to_props
                                .iter()
                                .find(|(k, _)| k == "key")
                                .and_then(|(_, v)| v.as_str())
                                .ok_or_else(|| {
                                    LessError::Query(
                                        "cypher: CREATE edge endpoints need a variable or a key property"
                                            .into(),
                                    )
                                })?
                                .to_string(),
                        ),
                    },
                    types,
                    direction,
                    props,
                })
            }
            _ => Ok(CreateElement::Node { var, labels, props }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_match_return() {
        let stmt =
            parse("MATCH (n:Person) RETURN n.name, count(*) AS c ORDER BY c DESC LIMIT 5").unwrap();
        match stmt {
            Statement::Query(q) => {
                assert_eq!(q.patterns.len(), 1);
                assert_eq!(q.limit, Some(5));
                assert_eq!(q.order_by.len(), 1);
                assert!(q.order_by[0].desc);
                assert_eq!(q.return_items.items.len(), 2);
            }
            other => panic!("expected query, got {other:?}"),
        }
    }

    #[test]
    fn parses_chain_with_var_length() {
        let stmt =
            parse("MATCH (a:Person {name: 'Ann'})-[r:KNOWS*1..3]->(b) WHERE b.age > 30 RETURN b")
                .unwrap();
        match stmt {
            Statement::Query(q) => {
                let chain = &q.patterns[0].chain;
                assert_eq!(chain.len(), 3);
                assert!(matches!(
                    &chain[1],
                    ChainPart::Edge {
                        min_hops: 1,
                        max_hops: Some(3),
                        ..
                    }
                ));
                assert!(q.where_clause.is_some());
            }
            other => panic!("expected query, got {other:?}"),
        }
    }

    #[test]
    fn parses_undirected_and_incoming() {
        let stmt = parse("MATCH (a)--(b) RETURN a").unwrap();
        match stmt {
            Statement::Query(q) => {
                assert!(matches!(
                    &q.patterns[0].chain[1],
                    ChainPart::Edge {
                        direction: Direction::Both,
                        ..
                    }
                ));
            }
            other => panic!("expected query, got {other:?}"),
        }
        let stmt = parse("MATCH (a)<-[r:PART_OF]-(b) RETURN r").unwrap();
        match stmt {
            Statement::Query(q) => {
                assert!(matches!(
                    &q.patterns[0].chain[1],
                    ChainPart::Edge {
                        direction: Direction::In,
                        ..
                    }
                ));
            }
            other => panic!("expected query, got {other:?}"),
        }
    }

    #[test]
    fn parses_create_and_delete_and_set() {
        let stmt = parse("CREATE (n:Task {key: 't/1', title: 'x'}), (m:Task {key: 't/2'}), (n)-[:DEPENDS_ON]->(m)").unwrap();
        assert!(matches!(stmt, Statement::Create(ref v) if v.len() == 3));
        let stmt = parse("MATCH (n:Task) WHERE n.done = false DELETE n").unwrap();
        match stmt {
            Statement::Query(q) => assert_eq!(q.deletes, vec!["n".to_string()]),
            other => panic!("expected query, got {other:?}"),
        }
        let stmt =
            parse("MATCH (n {key: 't/1'}) SET n.done = true, n.note = 'ok' RETURN n.done").unwrap();
        match stmt {
            Statement::Query(q) => assert_eq!(q.sets.len(), 2),
            other => panic!("expected query, got {other:?}"),
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("SELECT 1").is_err());
        assert!(parse("MATCH (n)").is_err());
        assert!(parse("MATCH (n) RETRN n").is_err());
    }
}
