//! CREATE TABLE parsing.
//!
//! LessDB v1 DDL (case-insensitive):
//!
//! ```sql
//! CREATE TABLE [IF NOT EXISTS] name (
//!     col Type [NULL | NOT NULL], ...
//! )
//! [ENGINE = Firefly | FireflyCloud]
//! [ORDER BY (a, b)]          -- or PRIMARY KEY (a, b)
//! [UNIQUE (a, b)]            -- must be a prefix of the ORDER BY columns
//! [COMPRESSION = 'zstd']     -- zstd | lz4 | none
//! ```
//!
//! Types accept aliases (`String`, `Date`, `DateTime`,
//! `DateTime64(6)`) as well as SQL names (`Int64`, `BigInt`, ...).

use crate::{EngineKind, FieldSpec, TableDef, TypeSpec};
use less_common::{LessError, Result};

/// Parsed CREATE TABLE statement.
#[derive(Debug, Clone)]
pub struct ParsedCreate {
    pub if_not_exists: bool,
    pub table: String,
    pub fields: Vec<FieldSpec>,
    pub engine: EngineKind,
    pub sort_key: Vec<String>,
    pub unique: Vec<String>,
    pub compression: Option<String>,
    /// `TTL <col> INTERVAL <n> <unit>`: (column, seconds).
    pub ttl: Option<(String, u64)>,
}

impl ParsedCreate {
    /// Convert into a table definition.
    pub fn to_def(&self) -> TableDef {
        let mut def = TableDef::new(
            self.table.clone(),
            crate::SchemaSpec {
                fields: self.fields.clone(),
            },
            self.engine,
        );
        def.sort_key = self.sort_key.clone();
        def.unique = self.unique.clone();
        def.compression = self.compression.clone();
        if let Some((col, secs)) = &self.ttl {
            def.ttl_col = Some(col.clone());
            def.ttl_secs = Some(*secs);
        }
        def
    }
}

/// Keywords that can appear after the column list.
const CLAUSE_KEYWORDS: &[&str] = &[
    "compression",
    "unique",
    "primary key",
    "order by",
    "engine",
    "ttl",
];

pub fn parse_create(sql: &str) -> Result<ParsedCreate> {
    // Normalize whitespace and strip trailing semicolon.
    let s = sql.trim().trim_end_matches(';').trim();
    let mut chars = s.chars();

    fn eat_word(chars: &mut std::str::Chars<'_>, word: &str) -> bool {
        let mut peek = chars.clone();
        for wc in word.chars() {
            if peek.next() != Some(wc) {
                return false;
            }
        }
        *chars = peek;
        true
    }

    // CREATE TABLE
    if !eat_word(&mut chars, "CREATE") && !eat_word(&mut chars, "create") {
        return Err(LessError::Catalog("expected CREATE TABLE".into()));
    }
    skip_ws(&mut chars);
    let _ = eat_word(&mut chars, "TABLE") || eat_word(&mut chars, "table");

    // Optional IF NOT EXISTS
    skip_ws(&mut chars);
    let mut if_not_exists = false;
    {
        let mut peek = chars.clone();
        let mut consumed = String::new();
        if consume_ci(&mut peek, "IF NOT EXISTS ", &mut consumed) {
            if_not_exists = true;
            chars = peek;
        }
    }

    // Table name
    skip_ws(&mut chars);
    let table: String = chars
        .by_ref()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if table.is_empty() {
        return Err(LessError::Catalog(
            "missing table name in CREATE TABLE".into(),
        ));
    }

    // Column list
    skip_ws(&mut chars);
    let rest: String = chars.collect();
    let open = rest
        .find('(')
        .ok_or_else(|| LessError::Catalog("missing column list '(col Type, ...)'".into()))?;
    let close = matching_paren(&rest, open)?;
    let cols_src = &rest[open + 1..close];
    let after = rest[close + 1..].trim();

    let fields = parse_columns(cols_src)?;
    if fields.is_empty() {
        return Err(LessError::Catalog(
            "table must have at least one column".into(),
        ));
    }

    // Clause parsing after the column list.
    let mut engine = EngineKind::Firefly;
    let mut sort_key: Vec<String> = vec![];
    let mut primary_key: Vec<String> = vec![];
    let mut unique: Vec<String> = vec![];
    let mut compression: Option<String> = None;
    let mut ttl: Option<(String, u64)> = None;

    for (kw, raw) in split_clauses(after) {
        let value = raw.trim().trim_start_matches('=').trim();
        match kw.as_str() {
            "engine" => engine = EngineKind::parse(value)?,
            "order by" => sort_key = parse_id_list(value)?,
            "primary key" => primary_key = parse_id_list(value)?,
            "unique" => unique = parse_id_list(value)?,
            "compression" => {
                compression = Some(value.trim_matches('\'').trim_matches('"').to_string())
            }
            "ttl" => ttl = Some(parse_ttl(value)?),
            _ => {}
        }
    }
    if sort_key.is_empty() && !primary_key.is_empty() {
        sort_key = primary_key;
    }

    Ok(ParsedCreate {
        if_not_exists,
        table,
        fields,
        engine,
        sort_key,
        unique,
        compression,
        ttl,
    })
}

/// Parse `col INTERVAL 3 DAY` into (column, seconds).
fn parse_ttl(value: &str) -> Result<(String, u64)> {
    let tokens: Vec<&str> = value.split_whitespace().collect();
    if tokens.len() != 4 || !tokens[1].eq_ignore_ascii_case("interval") {
        return Err(LessError::Catalog(format!(
            "bad TTL clause '{value}' (expected: TTL <col> INTERVAL <n> DAY|HOUR|MONTH)"
        )));
    }
    let col = tokens[0].to_string();
    let n: u64 = tokens[2]
        .parse()
        .map_err(|_| LessError::Catalog(format!("bad TTL interval number '{}'", tokens[2])))?;
    let unit = tokens[3].trim_end_matches('s').to_ascii_lowercase();
    let secs = match unit.as_str() {
        "day" => n * 86_400,
        "hour" => n * 3_600,
        "month" => n * 30 * 86_400,
        other => {
            return Err(LessError::Catalog(format!(
                "bad TTL unit '{other}' (expected DAY, HOUR or MONTH)"
            )));
        }
    };
    Ok((col, secs))
}

fn skip_ws(chars: &mut std::str::Chars<'_>) {
    while chars.clone().next().is_some_and(|c| c.is_whitespace()) {
        chars.next();
    }
}

fn consume_ci(chars: &mut std::str::Chars<'_>, word: &str, _out: &mut String) -> bool {
    let mut peek = chars.clone();
    for wc in word.chars() {
        match peek.next() {
            Some(c) if c.eq_ignore_ascii_case(&wc) => {}
            _ => return false,
        }
    }
    *chars = peek;
    true
}

/// Find the index of the paren matching the one at `open`.
fn matching_paren(s: &str, open: usize) -> Result<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    for (i, b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(i);
                }
            }
            _ => {}
        }
    }
    Err(LessError::Catalog(
        "unbalanced parentheses in CREATE TABLE".into(),
    ))
}

/// Split `a Int64, b String NULL, ...` into field specs (top-level commas).
fn parse_columns(src: &str) -> Result<Vec<FieldSpec>> {
    let mut fields = vec![];
    for col in split_top_level(src, ',') {
        let col = col.trim();
        if col.is_empty() {
            continue;
        }
        let tokens: Vec<&str> = col.split_whitespace().collect();
        if tokens.len() < 2 {
            return Err(LessError::Catalog(format!("bad column definition '{col}'")));
        }
        let name = tokens[0].to_string();
        // Type tokens run until an optional NULL / NOT NULL annotation.
        let mut type_tokens: Vec<&str> = vec![];
        let mut nullable = true;
        for t in &tokens[1..] {
            if t.eq_ignore_ascii_case("NOT") {
                nullable = false;
                break;
            }
            if t.eq_ignore_ascii_case("NULL") {
                break;
            }
            type_tokens.push(t);
        }
        let ty_src = type_tokens.join("");
        let ty = normalize_type(&ty_src)?;
        let mut field = FieldSpec::new(name, ty);
        field.nullable = nullable;
        fields.push(field);
    }
    Ok(fields)
}

/// Normalize a SQL type name into a TypeSpec.
fn normalize_type(t: &str) -> Result<TypeSpec> {
    let lower = t.trim().to_ascii_lowercase();
    // DateTime64(3/6/9) precision mapping.
    if let Some(p) = lower
        .strip_prefix("datetime64(")
        .and_then(|r| r.strip_suffix(')'))
    {
        return match p.trim() {
            "3" => Ok(TypeSpec::TimestampMs),
            "6" => Ok(TypeSpec::TimestampUs),
            "9" => Ok(TypeSpec::TimestampNs),
            _ => Err(LessError::Catalog(format!(
                "unsupported DateTime64 precision '{p}' (use 3, 6 or 9)"
            ))),
        };
    }
    TypeSpec::parse(&lower)
}

/// Split on a delimiter, ignoring occurrences inside parentheses.
fn split_top_level(s: &str, delim: char) -> Vec<&str> {
    let mut out = vec![];
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            c if c == delim && depth == 0 => {
                out.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

/// Split `rest` into (keyword, value) pairs by scanning for clause keywords.
fn split_clauses(rest: &str) -> Vec<(String, String)> {
    let normalized: String = rest.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = normalized.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let norm_bytes = normalized.as_bytes();

    let find_kw = |from: usize| -> Option<(usize, &'static str)> {
        let mut best: Option<(usize, &'static str)> = None;
        for kw in CLAUSE_KEYWORDS {
            let mut start = from;
            while let Some(rel) = lower[start..].find(kw) {
                let abs = start + rel;
                let before_ok = abs == 0 || {
                    let b = bytes[abs - 1];
                    !b.is_ascii_alphanumeric() && b != b'_'
                };
                let after = abs + kw.len();
                let after_ok = after >= bytes.len() || {
                    let b = bytes[after];
                    !b.is_ascii_alphanumeric() && b != b'_'
                };
                if before_ok && after_ok {
                    if best.is_none_or(|(b, _)| abs < b) {
                        best = Some((abs, kw));
                    }
                    break;
                }
                start = abs + 1;
            }
        }
        best
    };

    let mut out: Vec<(String, String)> = vec![];
    let mut pos = 0usize;
    while let Some((kw_pos, kw)) = find_kw(pos) {
        let value_start = kw_pos + kw.len();
        let value_end = find_kw(value_start)
            .map(|(p, _)| p)
            .unwrap_or(norm_bytes.len());
        let value = String::from_utf8_lossy(&norm_bytes[value_start..value_end])
            .trim()
            .to_string();
        out.push((kw.to_string(), value));
        pos = value_end;
    }
    out
}

/// Parse `(a, b)` or `a, b` into an identifier list.
fn parse_id_list(value: &str) -> Result<Vec<String>> {
    let v = value.trim();
    let inner = v
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(v);
    let ids: Vec<String> = inner
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if ids.is_empty() {
        return Err(LessError::Catalog("empty key column list".into()));
    }
    for id in &ids {
        if !crate::schema::is_identifier(id) {
            return Err(LessError::Catalog(format!("invalid identifier '{id}'")));
        }
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_ddl() {
        let p = parse_create(
            "CREATE TABLE IF NOT EXISTS events (
                id Int64 NOT NULL,
                kind String,
                amount Float64,
                ts DateTime64(6)
             ) ENGINE = FireflyCloud ORDER BY (kind, id) UNIQUE (kind) COMPRESSION = 'lz4';",
        )
        .unwrap();
        assert!(p.if_not_exists);
        assert_eq!(p.table, "events");
        assert_eq!(p.fields.len(), 4);
        assert_eq!(p.fields[0].name, "id");
        assert_eq!(p.fields[0].ty, TypeSpec::Int64);
        assert!(!p.fields[0].nullable);
        assert_eq!(p.fields[3].ty, TypeSpec::TimestampUs);
        assert_eq!(p.engine, EngineKind::FireflyCloud);
        assert_eq!(p.sort_key, vec!["kind", "id"]);
        assert_eq!(p.unique, vec!["kind"]);
        assert_eq!(p.compression.as_deref(), Some("lz4"));
    }

    #[test]
    fn parses_minimal_ddl() {
        let p = parse_create("create table t (a int, s string)").unwrap();
        assert_eq!(p.table, "t");
        assert_eq!(p.fields.len(), 2);
        assert_eq!(p.engine, EngineKind::Firefly);
        assert!(p.sort_key.is_empty());
        assert!(p.unique.is_empty());
    }

    #[test]
    fn primary_key_as_sort_key() {
        let p = parse_create("CREATE TABLE t (a Int64, b String) PRIMARY KEY (a, b)").unwrap();
        assert_eq!(p.sort_key, vec!["a", "b"]);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_create("SELECT 1").is_err());
        assert!(parse_create("CREATE TABLE t ()").is_err());
        assert!(parse_create("CREATE TABLE t (a Nope)").is_err());
    }
}
