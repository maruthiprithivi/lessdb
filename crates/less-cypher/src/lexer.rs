//! Tokenizer for the LessDB openCypher subset.

use less_common::{LessError, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    String(String),
    Number(f64),
    Int(i64),
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Comma,
    Colon,
    Dot,
    Eq,
    Neq,
    Lt,
    Gt,
    Le,
    Ge,
    Arrow,  // ->
    ArrowL, // <-
    Minus,
    Star,
    Kw(Kw),
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kw {
    Match,
    Where,
    Return,
    Order,
    By,
    Skip,
    Limit,
    Distinct,
    Create,
    Delete,
    Set,
    As,
    And,
    Or,
    Not,
    In,
    Starts,
    Ends,
    Contains,
    With,
    True,
    False,
    Null,
    Asc,
    Desc,
}

pub fn keyword(s: &str) -> Option<Kw> {
    Some(match s.to_ascii_lowercase().as_str() {
        "match" => Kw::Match,
        "where" => Kw::Where,
        "return" => Kw::Return,
        "order" => Kw::Order,
        "by" => Kw::By,
        "skip" => Kw::Skip,
        "limit" => Kw::Limit,
        "distinct" => Kw::Distinct,
        "create" => Kw::Create,
        "delete" => Kw::Delete,
        "set" => Kw::Set,
        "as" => Kw::As,
        "and" => Kw::And,
        "or" => Kw::Or,
        "not" => Kw::Not,
        "in" => Kw::In,
        "starts" => Kw::Starts,
        "ends" => Kw::Ends,
        "contains" => Kw::Contains,
        "with" => Kw::With,
        "true" => Kw::True,
        "false" => Kw::False,
        "null" => Kw::Null,
        "asc" => Kw::Asc,
        "desc" => Kw::Desc,
        _ => return None,
    })
}

pub struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            chars: src.chars().peekable(),
            pos: 0,
        }
    }

    fn err(&self, msg: &str) -> LessError {
        LessError::Query(format!("cypher: {msg} at position {}", self.pos))
    }

    pub fn tokenize(mut self) -> Result<Vec<Tok>> {
        let mut out = vec![];
        loop {
            while self.chars.peek().is_some_and(|c| c.is_whitespace()) {
                self.next();
            }
            let Some(c) = self.peek() else {
                out.push(Tok::Eof);
                return Ok(out);
            };
            let tok = match c {
                '{' => {
                    self.next();
                    Tok::LBrace
                }
                '}' => {
                    self.next();
                    Tok::RBrace
                }
                '[' => {
                    self.next();
                    Tok::LBracket
                }
                ']' => {
                    self.next();
                    Tok::RBracket
                }
                '(' => {
                    self.next();
                    Tok::LParen
                }
                ')' => {
                    self.next();
                    Tok::RParen
                }
                ',' => {
                    self.next();
                    Tok::Comma
                }
                ':' => {
                    self.next();
                    Tok::Colon
                }
                '.' => {
                    self.next();
                    if self.peek() == Some('.') {
                        self.next();
                        // Second dot pushed first so the order is Dot, Dot.
                        out.push(Tok::Dot);
                    }
                    Tok::Dot
                }
                '=' => {
                    self.next();
                    Tok::Eq
                }
                '<' => {
                    self.next();
                    match self.peek() {
                        Some('=') => {
                            self.next();
                            Tok::Le
                        }
                        Some('-') => {
                            self.next();
                            Tok::ArrowL
                        }
                        Some('>') => {
                            self.next();
                            Tok::Neq
                        }
                        _ => Tok::Lt,
                    }
                }
                '>' => {
                    self.next();
                    if self.peek() == Some('=') {
                        self.next();
                        Tok::Ge
                    } else {
                        Tok::Gt
                    }
                }
                '-' => {
                    self.next();
                    if self.peek() == Some('>') {
                        self.next();
                        Tok::Arrow
                    } else if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                        self.number_with_sign(-1.0)?
                    } else {
                        Tok::Minus
                    }
                }
                '*' => {
                    self.next();
                    Tok::Star
                }
                '"' | '\'' => self.string(c)?,
                '`' => self.backtick_ident()?,
                c if c.is_ascii_digit() => self.number()?,
                c if c.is_ascii_alphabetic() || c == '_' => {
                    let ident = self.ident();
                    match keyword(&ident) {
                        Some(kw) => Tok::Kw(kw),
                        None => Tok::Ident(ident),
                    }
                }
                other => return Err(self.err(&format!("unexpected character '{other}'"))),
            };
            out.push(tok);
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }

    fn peek2(&mut self) -> Option<char> {
        let mut it = self.chars.clone();
        it.next();
        it.next()
    }

    fn next(&mut self) -> Option<char> {
        self.pos += 1;
        self.chars.next()
    }

    fn ident(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                s.push(c);
                self.next();
            } else {
                break;
            }
        }
        s
    }

    fn backtick_ident(&mut self) -> Result<Tok> {
        self.next(); // `
        let mut s = String::new();
        loop {
            match self.next() {
                Some('`') => return Ok(Tok::Ident(s)),
                Some(c) => s.push(c),
                None => return Err(self.err("unterminated backtick identifier")),
            }
        }
    }

    fn string(&mut self, quote: char) -> Result<Tok> {
        self.next();
        let mut s = String::new();
        loop {
            match self.next() {
                Some(c) if c == quote => return Ok(Tok::String(s)),
                Some('\\') => match self.next() {
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some('\\') => s.push('\\'),
                    Some('"') => s.push('"'),
                    Some('\'') => s.push('\''),
                    Some(c) => s.push(c),
                    None => return Err(self.err("unterminated string escape")),
                },
                Some(c) => s.push(c),
                None => return Err(self.err("unterminated string literal")),
            }
        }
    }

    fn number(&mut self) -> Result<Tok> {
        self.number_with_sign(1.0)
    }

    fn number_with_sign(&mut self, sign: f64) -> Result<Tok> {
        let mut s = String::new();
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            s.push(self.next().unwrap());
        }
        let mut is_float = false;
        if self.peek() == Some('.') && self.peek2().is_none_or(|c| c != '.') {
            is_float = true;
            s.push('.');
            self.next();
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                s.push(self.next().unwrap());
            }
        }
        if self.peek().is_some_and(|c| c == 'e' || c == 'E') {
            is_float = true;
            s.push(self.next().unwrap());
            if self.peek().is_some_and(|c| c == '+' || c == '-') {
                s.push(self.next().unwrap());
            }
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                s.push(self.next().unwrap());
            }
        }
        if is_float {
            let v: f64 = sign
                * s.parse::<f64>()
                    .map_err(|_| self.err(&format!("bad number '{s}'")))?;
            Ok(Tok::Number(v))
        } else {
            let v: i64 = (sign as i64)
                * s.parse::<i64>()
                    .map_err(|_| self.err(&format!("bad number '{s}'")))?;
            Ok(Tok::Int(v))
        }
    }
}

pub fn tokenize(src: &str) -> Result<Vec<Tok>> {
    Lexer::new(src).tokenize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens() {
        let toks = tokenize("MATCH (a:Person {name: 'Ann'})-[r:KNOWS*1..3]->(b) WHERE b.age > 30 RETURN b.name, count(*) LIMIT 10").unwrap();
        assert!(toks.contains(&Tok::Kw(Kw::Match)));
        assert!(toks.contains(&Tok::Ident("a".into())));
        assert!(toks.contains(&Tok::String("Ann".into())));
        assert!(
            toks.windows(3)
                .any(|w| w == [Tok::Int(1), Tok::Dot, Tok::Dot])
        );
        assert!(toks.contains(&Tok::Arrow));
        assert!(toks.contains(&Tok::Kw(Kw::Limit)));
        assert!(toks.contains(&Tok::Int(10)));
        assert!(toks.contains(&Tok::Eof));
    }

    #[test]
    fn strings_and_numbers() {
        let toks = tokenize(r#""hello\nworld" -3.5 42"#).unwrap();
        assert_eq!(toks[0], Tok::String("hello\nworld".into()));
        assert_eq!(toks[1], Tok::Number(-3.5));
        assert_eq!(toks[2], Tok::Int(42));
    }
}
