//! Part-level pruning: skip whole data parts without touching their data.
//!
//! A part can be pruned when its metadata *proves* it cannot contain rows
//! matching a filter:
//!
//! * **equality on a uniqueness column** — checked against the part's bloom
//!   filter (probabilistic, but never produces false negatives);
//! * **equality / range on any tracked column** — checked against the part's
//!   typed min/max statistics.
//!
//! All comparisons are typed ([`StatValue`]); when two values aren't
//! comparable the filter conservatively keeps the part. Pruning is therefore
//! *sound*: at worst we do unnecessary work, never miss rows.
//!
//! Row-group-level pruning inside a part is additionally performed by the
//! parquet reader itself.

use std::cmp::Ordering;

use datafusion::logical_expr::{BinaryExpr, Expr, Operator};
use datafusion::scalar::ScalarValue;

use less_storage::{Bloom, PartMeta, StatValue};

/// Convert a DataFusion literal into a [`StatValue`], if we track its type.
/// (In the arrow 59 ecosystem `ScalarValue` lives in datafusion-common, so
/// the conversion lives here in the query layer.)
fn stat_from_literal(v: &ScalarValue) -> Option<StatValue> {
    use ScalarValue::*;
    match v {
        Boolean(Some(b)) => Some(StatValue::Bool(*b)),
        Int8(Some(v)) => Some(StatValue::I64(*v as i64)),
        Int16(Some(v)) => Some(StatValue::I64(*v as i64)),
        Int32(Some(v)) => Some(StatValue::I64(*v as i64)),
        Int64(Some(v)) => Some(StatValue::I64(*v)),
        UInt8(Some(v)) => Some(StatValue::U64(*v as u64)),
        UInt16(Some(v)) => Some(StatValue::U64(*v as u64)),
        UInt32(Some(v)) => Some(StatValue::U64(*v as u64)),
        UInt64(Some(v)) => Some(StatValue::U64(*v)),
        Float32(Some(v)) => Some(StatValue::F64(*v as f64)),
        Float64(Some(v)) => Some(StatValue::F64(*v)),
        Utf8(Some(v)) | LargeUtf8(Some(v)) | Utf8View(Some(v)) => Some(StatValue::Str(v.clone())),
        Date32(Some(v)) => Some(StatValue::Date32(*v)),
        TimestampSecond(Some(v), _) => Some(StatValue::Timestamp {
            value: *v,
            unit: "s".into(),
        }),
        TimestampMillisecond(Some(v), _) => Some(StatValue::Timestamp {
            value: *v,
            unit: "ms".into(),
        }),
        TimestampMicrosecond(Some(v), _) => Some(StatValue::Timestamp {
            value: *v,
            unit: "us".into(),
        }),
        TimestampNanosecond(Some(v), _) => Some(StatValue::Timestamp {
            value: *v,
            unit: "ns".into(),
        }),
        _ => None,
    }
}

/// Returns `true` when the part *may* contain rows matching all filters
/// (i.e. it should be scanned).
pub fn prune_part(meta: &PartMeta, filters: &[Expr]) -> bool {
    filters.iter().all(|f| expr_may_match(meta, f))
}

fn expr_may_match(meta: &PartMeta, expr: &Expr) -> bool {
    match expr {
        Expr::Literal(ScalarValue::Boolean(Some(false)), _) => false,
        Expr::BinaryExpr(BinaryExpr { left, op, right }) => match op {
            // AND: prune when any conjunct provably cannot match.
            Operator::And => expr_may_match(meta, left) && expr_may_match(meta, right),
            // OR: prune only when *every* disjunct provably cannot match.
            Operator::Or => expr_may_match(meta, left) || expr_may_match(meta, right),
            _ => match (left.as_ref(), right.as_ref()) {
                (Expr::Column(c), Expr::Literal(v, _)) => col_lit_may_match(meta, &c.name, op, v),
                (Expr::Literal(v, _), Expr::Column(c)) => {
                    col_lit_may_match(meta, &c.name, &flip(op), v)
                }
                _ => true,
            },
        },
        // Nested/unsupported shapes: be conservative.
        _ => true,
    }
}

fn flip(op: &Operator) -> Operator {
    match op {
        Operator::Lt => Operator::Gt,
        Operator::LtEq => Operator::GtEq,
        Operator::Gt => Operator::Lt,
        Operator::GtEq => Operator::LtEq,
        other => *other,
    }
}

/// Bloom membership for equality pruning.
///
/// The bloom is keyed by [`StatValue::to_bytes`]. For almost every value that
/// encoding is injective w.r.t. `==`, but `-0.0` and `+0.0` compare equal
/// while encoding to different byte strings. Probing for one zero with the
/// other zero's bytes would therefore false-negative, so we treat the two
/// zero encodings as a single value when probing.
fn bloom_contains(bloom: &Bloom, v: &StatValue) -> bool {
    match v {
        StatValue::F64(x) if *x == 0.0 => {
            bloom.contains(&0.0f64.to_le_bytes()) || bloom.contains(&(-0.0f64).to_le_bytes())
        }
        _ => bloom.contains(&v.to_bytes()),
    }
}

fn col_lit_may_match(meta: &PartMeta, col: &str, op: &Operator, lit: &ScalarValue) -> bool {
    let stats = match meta.column(col) {
        Some(s) => s,
        None => return true,
    };
    match op {
        Operator::Eq => {
            // 1) Bloom filter on uniqueness columns: provably absent values
            //    prune the part outright.
            if meta.unique.iter().any(|u| u == col)
                && let Some(bytes) = stats.bloom_bytes()
            {
                let bloom = Bloom::from_bytes(&bytes);
                if let Some(v) = stat_from_literal(lit)
                    && !bloom_contains(&bloom, &v)
                {
                    return false;
                }
            }
            // 2) Min/max containment.
            if let (Some(l), Some(min), Some(max)) =
                (stat_from_literal(lit), &stats.min, &stats.max)
                && (l.partial_cmp(min) == Some(Ordering::Less)
                    || l.partial_cmp(max) == Some(Ordering::Greater))
            {
                return false;
            }
            true
        }
        Operator::NotEq => true, // almost everything matches; never prune
        // Prune ONLY on provable outcomes; `partial_cmp` returning `None`
        // (e.g. a NaN literal — the engine orders every finite value below
        // NaN) must stay conservative (keep).
        // col <  lit  prunes iff min >= lit provably.
        Operator::Lt => match (stat_from_literal(lit), &stats.min) {
            (Some(l), Some(min)) => !matches!(
                min.partial_cmp(&l),
                Some(Ordering::Greater) | Some(Ordering::Equal)
            ),
            _ => true,
        },
        // col <= lit  prunes iff min > lit provably.
        Operator::LtEq => match (stat_from_literal(lit), &stats.min) {
            (Some(l), Some(min)) => !matches!(min.partial_cmp(&l), Some(Ordering::Greater)),
            _ => true,
        },
        // col >  lit  prunes iff max <= lit provably — except when the part
        // holds NaN, which the engine's comparison semantics order above
        // every finite literal (min/max skip NaN, so `has_nan` is the only
        // signal; NaN satisfies `>` and `>=`).
        Operator::Gt => {
            if stats.has_nan {
                return true;
            }
            match (stat_from_literal(lit), &stats.max) {
                (Some(l), Some(max)) => !matches!(
                    max.partial_cmp(&l),
                    Some(Ordering::Less) | Some(Ordering::Equal)
                ),
                _ => true,
            }
        }
        // col >= lit  prunes iff max < lit provably (NaN: see above).
        Operator::GtEq => {
            if stats.has_nan {
                return true;
            }
            match (stat_from_literal(lit), &stats.max) {
                (Some(l), Some(max)) => !matches!(max.partial_cmp(&l), Some(Ordering::Less)),
                _ => true,
            }
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::logical_expr::col;
    use datafusion::prelude::lit;
    use less_storage::ColumnStats;

    fn meta() -> PartMeta {
        PartMeta {
            id: "id".into(),
            name: "p".into(),
            table: "t".into(),
            row_count: 100,
            created_at: "now".into(),
            compression: "zstd".into(),
            sort_key: vec!["id".into()],
            unique: vec!["id".into()],
            columns: vec![
                ColumnStats {
                    name: "id".into(),
                    ty: "Int64".into(),
                    null_count: 0,
                    min: Some(StatValue::I64(10)),
                    max: Some(StatValue::I64(90)),
                    bloom: None,
                    has_nan: false,
                },
                ColumnStats {
                    name: "city".into(),
                    ty: "Utf8".into(),
                    null_count: 0,
                    min: Some(StatValue::Str("berlin".into())),
                    max: Some(StatValue::Str("paris".into())),
                    bloom: None,
                    has_nan: false,
                },
            ],
            wal_lsn_max: None,
        }
    }

    #[test]
    fn range_pruning_is_typed_and_sound() {
        let m = meta();
        // id between 10 and 90 -> keep
        assert!(prune_part(&m, &[col("id").gt(lit(5i64))]));
        assert!(prune_part(&m, &[col("id").lt(lit(100i64))]));
        assert!(prune_part(&m, &[col("id").eq(lit(50i64))]));
        // provably outside -> prune
        assert!(!prune_part(&m, &[col("id").lt(lit(10i64))]));
        assert!(!prune_part(&m, &[col("id").lt_eq(lit(9i64))]));
        assert!(!prune_part(&m, &[col("id").gt(lit(90i64))]));
        assert!(!prune_part(&m, &[col("id").gt_eq(lit(91i64))]));
        assert!(!prune_part(&m, &[col("id").eq(lit(5i64))]));
        assert!(!prune_part(&m, &[col("id").eq(lit(1000i64))]));
        // string ranges stay lexicographic *for strings only*
        assert!(!prune_part(&m, &[col("city").lt(lit("aaa"))]));
        assert!(prune_part(&m, &[col("city").lt(lit("zzz"))]));
        // mixed type comparison must not prune
        assert!(prune_part(&m, &[col("id").eq(lit(50.0f64))]));
        // unknown column: keep
        assert!(prune_part(&m, &[col("nope").eq(lit(1i64))]));
    }

    #[test]
    fn not_eq_and_or_are_conservative() {
        let m = meta();
        assert!(prune_part(&m, &[col("id").not_eq(lit(5i64))]));
        assert!(prune_part(
            &m,
            &[col("id").eq(lit(5i64)).or(col("id").eq(lit(50i64)))]
        ));
        assert!(!prune_part(
            &m,
            &[col("id").eq(lit(5i64)).and(col("id").eq(lit(50i64)))]
        ));
    }
}
