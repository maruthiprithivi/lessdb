//! Property-based soundness tests for part pruning.
//!
//! [`less_query::prune_part`] decides whether a data part *may* contain rows
//! matching a set of filters, using only the part's metadata (typed min/max
//! stats and bloom filters on uniqueness columns). Pruning must be
//! **conservative**: whenever pruning says "prune" (returns `false`), no row
//! in the part may actually match the filters.
//!
//! This test generates random typed columns, writes a *real* part through
//! [`less_storage::write_part`], reads the actual rows back with
//! [`less_storage::read_part`], generates random filter expressions, and then
//! checks the soundness direction:
//!
//! > if **any** actual row matches the predicate (evaluated by DataFusion on
//! > the rows themselves), then `prune_part` MUST return `true` for that part.
//!
//! The ground-truth evaluation uses DataFusion's own expression evaluator (via
//! a memtable) so the "does a row match" semantics exactly match what the
//! query engine would compute at scan time.

use std::sync::Arc;

use arrow::array::{
    Array, BooleanArray, Float64Array, Int32Array, Int64Array, StringArray,
    TimestampMillisecondArray,
};
use arrow::compute::concat_batches;
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;

use datafusion::logical_expr::Expr;
use datafusion::prelude::{SessionContext, col, lit};
use datafusion::scalar::ScalarValue;

use less_query::prune_part;
use less_storage::{
    ColumnStats, Compression, PartMeta, StatValue, WriteOptions, read_part, write_part,
};

use proptest::prelude::*;
use proptest::test_runner::{Config, TestCaseError, TestRunner};

/// The column types we generate. "Double" is the same Arrow type as Float64.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ty {
    I32,
    I64,
    F64,
    Str,
    Bool,
    Ts,
}

impl Ty {
    fn data_type(self) -> DataType {
        match self {
            Ty::I32 => DataType::Int32,
            Ty::I64 => DataType::Int64,
            Ty::F64 => DataType::Float64,
            Ty::Str => DataType::Utf8,
            Ty::Bool => DataType::Boolean,
            Ty::Ts => DataType::Timestamp(TimeUnit::Millisecond, None),
        }
    }
}

/// A generated scalar value, kept type-tagged so literals always match the
/// column type they are compared against.
#[derive(Debug, Clone, PartialEq)]
enum Val {
    I32(i32),
    I64(i64),
    F64(f64),
    Str(String),
    Bool(bool),
    Ts(i64),
}

#[derive(Debug, Clone)]
struct GenColumn {
    ty: Ty,
    /// One cell per row; `None` means SQL NULL.
    values: Vec<Option<Val>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone)]
enum GExpr {
    Pred { col: usize, op: Op, lit: Val },
    IsNull { col: usize },
    IsNotNull { col: usize },
    And(Box<GExpr>, Box<GExpr>),
    Or(Box<GExpr>, Box<GExpr>),
}

/// A fully generated test case.
#[derive(Debug, Clone)]
struct Case {
    columns: Vec<GenColumn>,
    /// Sort-key column indices (a prefix of the columns).
    sort_key: Vec<usize>,
    /// Uniqueness-column indices (a prefix of `sort_key`).
    unique: Vec<usize>,
    /// Top-level filters, AND-ed together by `prune_part`.
    filters: Vec<GExpr>,
}

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

fn any_ty() -> impl Strategy<Value = Ty> {
    prop_oneof![
        Just(Ty::I32),
        Just(Ty::I64),
        Just(Ty::F64),
        Just(Ty::Str),
        Just(Ty::Bool),
        Just(Ty::Ts),
    ]
}

/// Generate a non-null value of the given type from a small, overlapping
/// domain (so literals frequently collide with the actual data, exercising
/// boundary and equality pruning).
fn value(ty: Ty) -> BoxedStrategy<Val> {
    match ty {
        Ty::I32 => (-2i32..=6).prop_map(Val::I32).boxed(),
        Ty::I64 => (-2i64..=6).prop_map(Val::I64).boxed(),
        Ty::F64 => prop_oneof![
            Just(0.0f64),
            Just(-0.0f64),
            Just(0.5f64),
            Just(1.0f64),
            Just(1.5f64),
            Just(2.0f64),
            Just(3.0f64),
            Just(7.5f64),
            Just(-1.0f64),
            Just(f64::NAN),
        ]
        .prop_map(Val::F64)
        .boxed(),
        Ty::Str => prop_oneof![
            Just("a"),
            Just("aa"),
            Just("ab"),
            Just("b"),
            Just("ba"),
            Just("m"),
            Just("z"),
        ]
        .prop_map(|s| Val::Str(s.to_string()))
        .boxed(),
        Ty::Bool => prop::bool::ANY.prop_map(Val::Bool).boxed(),
        Ty::Ts => (0i64..=8).prop_map(Val::Ts).boxed(),
    }
}

/// Generate one cell (value or NULL) of the given type.
fn cell(ty: Ty) -> BoxedStrategy<Option<Val>> {
    prop::option::weighted(0.8f64, value(ty)).boxed()
}

/// Generate one column specification: its type plus `rows` cells.
fn column_spec(rows: usize) -> BoxedStrategy<(Ty, Vec<Option<Val>>)> {
    any_ty()
        .prop_flat_map(move |ty| {
            prop::collection::vec(cell(ty), rows).prop_map(move |values| (ty, values))
        })
        .boxed()
}

/// Generate a single predicate on an existing column (comparison operators are
/// restricted to types DataFusion supports them on).
fn leaf(columns: Vec<GenColumn>) -> BoxedStrategy<GExpr> {
    let n = columns.len();
    (0usize..n)
        .prop_flat_map(move |ci| {
            let ty = columns[ci].ty;
            let cmp: BoxedStrategy<Op> = match ty {
                Ty::Bool => prop_oneof![Just(Op::Eq), Just(Op::Ne)].boxed(),
                _ => prop_oneof![
                    Just(Op::Eq),
                    Just(Op::Ne),
                    Just(Op::Lt),
                    Just(Op::Le),
                    Just(Op::Gt),
                    Just(Op::Ge),
                ]
                .boxed(),
            };
            let pred = (cmp, value(ty)).prop_map(move |(op, lit)| GExpr::Pred { col: ci, op, lit });
            let nulls = prop_oneof![
                Just(GExpr::IsNull { col: ci }),
                Just(GExpr::IsNotNull { col: ci }),
            ];
            prop_oneof![pred, nulls].boxed()
        })
        .boxed()
}

/// Generate a small boolean expression tree (leaves plus AND/OR combinations,
/// up to `depth` levels of nesting).
fn gexpr(columns: Vec<GenColumn>, depth: u8) -> BoxedStrategy<GExpr> {
    if depth == 0 {
        return leaf(columns);
    }
    let leaf_s = leaf(columns.clone());
    let and_s = (
        gexpr(columns.clone(), depth - 1),
        gexpr(columns.clone(), depth - 1),
    )
        .prop_map(|(a, b)| GExpr::And(Box::new(a), Box::new(b)));
    let or_s = (
        gexpr(columns.clone(), depth - 1),
        gexpr(columns.clone(), depth - 1),
    )
        .prop_map(|(a, b)| GExpr::Or(Box::new(a), Box::new(b)));
    prop_oneof![leaf_s, and_s, or_s].boxed()
}

fn case_strategy() -> BoxedStrategy<Case> {
    let num_cols = 1usize..=3usize;
    let num_rows = 1usize..=20usize;
    (num_cols, num_rows)
        .prop_flat_map(|(num_cols, num_rows)| {
            let cols = prop::collection::vec(column_spec(num_rows), num_cols);
            let sort_len = 0usize..=num_cols;
            let has_unique = prop::bool::weighted(0.6f64);
            (cols, sort_len, has_unique).prop_flat_map(move |(specs, sort_len, has_unique)| {
                let columns: Vec<GenColumn> = specs
                    .into_iter()
                    .map(|(ty, values)| GenColumn { ty, values })
                    .collect();
                let sort_key: Vec<usize> = (0..sort_len).collect();
                let unique: Vec<usize> = if !sort_key.is_empty() && has_unique {
                    vec![sort_key[0]]
                } else {
                    vec![]
                };
                let filters = prop::collection::vec(gexpr(columns.clone(), 2), 1..=4);
                filters.prop_map(move |filters| Case {
                    columns: columns.clone(),
                    sort_key: sort_key.clone(),
                    unique: unique.clone(),
                    filters,
                })
            })
        })
        .boxed()
}

// ---------------------------------------------------------------------------
// Conversion to Arrow / DataFusion
// ---------------------------------------------------------------------------

fn build_array(col: &GenColumn) -> Arc<dyn Array> {
    match col.ty {
        Ty::I32 => {
            let vals: Vec<Option<i32>> = col
                .values
                .iter()
                .map(|v| {
                    v.as_ref().map(|x| match x {
                        Val::I32(n) => *n,
                        _ => unreachable!("type mismatch"),
                    })
                })
                .collect();
            Arc::new(Int32Array::from(vals))
        }
        Ty::I64 => {
            let vals: Vec<Option<i64>> = col
                .values
                .iter()
                .map(|v| {
                    v.as_ref().map(|x| match x {
                        Val::I64(n) => *n,
                        _ => unreachable!("type mismatch"),
                    })
                })
                .collect();
            Arc::new(Int64Array::from(vals))
        }
        Ty::F64 => {
            let vals: Vec<Option<f64>> = col
                .values
                .iter()
                .map(|v| {
                    v.as_ref().map(|x| match x {
                        Val::F64(n) => *n,
                        _ => unreachable!("type mismatch"),
                    })
                })
                .collect();
            Arc::new(Float64Array::from(vals))
        }
        Ty::Str => {
            let vals: Vec<Option<&str>> = col
                .values
                .iter()
                .map(|v| {
                    v.as_ref().map(|x| match x {
                        Val::Str(s) => s.as_str(),
                        _ => unreachable!("type mismatch"),
                    })
                })
                .collect();
            Arc::new(StringArray::from(vals))
        }
        Ty::Bool => {
            let vals: Vec<Option<bool>> = col
                .values
                .iter()
                .map(|v| {
                    v.as_ref().map(|x| match x {
                        Val::Bool(b) => *b,
                        _ => unreachable!("type mismatch"),
                    })
                })
                .collect();
            Arc::new(BooleanArray::from(vals))
        }
        Ty::Ts => {
            let vals: Vec<Option<i64>> = col
                .values
                .iter()
                .map(|v| {
                    v.as_ref().map(|x| match x {
                        Val::Ts(t) => *t,
                        _ => unreachable!("type mismatch"),
                    })
                })
                .collect();
            Arc::new(TimestampMillisecondArray::from(vals))
        }
    }
}

fn build_batch(case: &Case) -> RecordBatch {
    let fields: Vec<Field> = case
        .columns
        .iter()
        .enumerate()
        .map(|(i, c)| Field::new(format!("c{i}"), c.ty.data_type(), true))
        .collect();
    let schema = Arc::new(Schema::new(fields));
    let arrays: Vec<Arc<dyn Array>> = case.columns.iter().map(build_array).collect();
    RecordBatch::try_new(schema, arrays).expect("valid record batch")
}

fn val_to_scalar(v: &Val) -> ScalarValue {
    match v {
        Val::I32(n) => ScalarValue::Int32(Some(*n)),
        Val::I64(n) => ScalarValue::Int64(Some(*n)),
        Val::F64(n) => ScalarValue::Float64(Some(*n)),
        Val::Str(s) => ScalarValue::Utf8(Some(s.clone())),
        Val::Bool(b) => ScalarValue::Boolean(Some(*b)),
        Val::Ts(t) => ScalarValue::TimestampMillisecond(Some(*t), None),
    }
}

fn gexpr_to_expr(e: &GExpr) -> Expr {
    match e {
        GExpr::Pred {
            col: ci,
            op,
            lit: lit_val,
        } => {
            let lhs = col(format!("c{ci}"));
            let scalar = val_to_scalar(lit_val);
            match op {
                Op::Eq => lhs.eq(lit(scalar)),
                Op::Ne => lhs.not_eq(lit(scalar)),
                Op::Lt => lhs.lt(lit(scalar)),
                Op::Le => lhs.lt_eq(lit(scalar)),
                Op::Gt => lhs.gt(lit(scalar)),
                Op::Ge => lhs.gt_eq(lit(scalar)),
            }
        }
        GExpr::IsNull { col: ci } => col(format!("c{ci}")).is_null(),
        GExpr::IsNotNull { col: ci } => col(format!("c{ci}")).is_not_null(),
        GExpr::And(a, b) => gexpr_to_expr(a).and(gexpr_to_expr(b)),
        GExpr::Or(a, b) => gexpr_to_expr(a).or(gexpr_to_expr(b)),
    }
}

// ---------------------------------------------------------------------------
// Ground truth & soundness check
// ---------------------------------------------------------------------------

fn new_temp_dir() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("less-prune-proptest-{}", uuid::Uuid::new_v4()))
}

/// Evaluate `filters` (AND-ed) against `batch` using DataFusion on a memtable
/// and report whether at least one row matches.
fn any_row_matches(
    ctx: &SessionContext,
    rt: &tokio::runtime::Runtime,
    batch: &RecordBatch,
    filters: &[Expr],
) -> bool {
    let df = ctx.read_batch(batch.clone()).expect("read_batch");
    let pred = filters
        .iter()
        .cloned()
        .reduce(|a, b| a.and(b))
        .unwrap_or_else(|| lit(true));
    let df = df.filter(pred).expect("filter");
    rt.block_on(df.count()).expect("count") > 0
}

fn check_case(
    case: &Case,
    ctx: &SessionContext,
    rt: &tokio::runtime::Runtime,
) -> Result<(), TestCaseError> {
    let batch = build_batch(case);
    let schema = batch.schema();

    let dir = new_temp_dir();
    let parts_dir = dir.join("parts");
    std::fs::create_dir_all(&parts_dir).expect("create parts dir");

    let opts = WriteOptions {
        compression: Compression::Zstd,
        zstd_level: 3,
        sort_key: case.sort_key.iter().map(|&i| format!("c{i}")).collect(),
        unique: case.unique.iter().map(|&i| format!("c{i}")).collect(),
        bloom_fp_rate: 0.01,
        level: 0,
        wal_lsn_max: None,
    };
    let meta =
        write_part(&parts_dir, "t", &schema, vec![batch.clone()], &opts).expect("write_part");

    // Read the actual (sorted, deduped) part rows back — this is the source of
    // truth for "what the part really contains".
    let rows = read_part(&parts_dir.join(&meta.name), None).expect("read_part");
    let actual: RecordBatch = if rows.len() == 1 {
        rows.into_iter().next().unwrap()
    } else {
        concat_batches(&rows[0].schema(), &rows).expect("concat read batches")
    };

    let exprs: Vec<Expr> = case.filters.iter().map(gexpr_to_expr).collect();

    let any_match = any_row_matches(ctx, rt, &actual, &exprs);

    let _ = std::fs::remove_dir_all(&dir);

    if any_match {
        let kept = prune_part(&meta, &exprs);
        prop_assert!(
            kept,
            "UNSOUND pruning: a part containing matching rows was pruned.\n\
             filters: {:#?}\npart meta: {:#?}\ncase: {:#?}",
            exprs,
            meta,
            case
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn pruning_is_sound() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let ctx = SessionContext::new();

    let mut config = Config::with_cases(400);
    config.failure_persistence = None;

    let mut runner = TestRunner::new(config);
    runner
        .run(&case_strategy(), |case| check_case(&case, &ctx, &rt))
        .unwrap();
}

/// Deterministic regression test for the `-0.0 == +0.0` bloom-encoding bug.
///
/// `StatValue::to_bytes` encodes `-0.0` and `+0.0` to different byte strings
/// even though they compare equal, so probing the bloom for one zero with the
/// other zero's bytes used to false-negative and unsoundly prune the part.
#[test]
fn negative_zero_bloom_equality_is_sound() {
    let dir = new_temp_dir();
    let parts_dir = dir.join("parts");
    std::fs::create_dir_all(&parts_dir).expect("create parts dir");

    let schema = Arc::new(Schema::new(vec![Field::new("f", DataType::Float64, true)]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![Arc::new(Float64Array::from(vec![Some(-0.0f64)])) as Arc<dyn Array>],
    )
    .expect("record batch");

    let opts = WriteOptions {
        compression: Compression::Zstd,
        zstd_level: 3,
        sort_key: vec!["f".into()],
        unique: vec!["f".into()],
        bloom_fp_rate: 0.01,
        level: 0,
        wal_lsn_max: None,
    };
    let meta = write_part(&parts_dir, "t", &schema, vec![batch], &opts).expect("write_part");

    // `f = 0.0` and `f = -0.0` both match the single row (-0.0), so the part
    // must be kept for both.
    assert!(prune_part(&meta, &[col("f").eq(lit(0.0f64))]));
    assert!(prune_part(&meta, &[col("f").eq(lit(-0.0f64))]));
    // A genuinely absent value is still pruned.
    assert!(!prune_part(&meta, &[col("f").eq(lit(5.0f64))]));

    let _ = std::fs::remove_dir_all(&dir);
}

/// A minimal hand-built metadata helper for the AND/OR / flip determinism tests.
fn i64_meta(min: i64, max: i64) -> PartMeta {
    PartMeta {
        id: "id".into(),
        name: "p".into(),
        table: "t".into(),
        row_count: 100,
        created_at: "now".into(),
        compression: "zstd".into(),
        sort_key: vec!["id".into()],
        unique: vec!["id".into()],
        columns: vec![ColumnStats {
            name: "id".into(),
            ty: "Int64".into(),
            null_count: 0,
            min: Some(StatValue::I64(min)),
            max: Some(StatValue::I64(max)),
            bloom: None,
            has_nan: false,
        }],
        wal_lsn_max: None,
    }
}

#[test]
fn and_or_and_literal_on_left_are_conservative() {
    let m = i64_meta(10, 90);

    // AND: one conjunct provably impossible -> prune the whole conjunction.
    assert!(!prune_part(
        &m,
        &[col("id").eq(lit(5i64)).and(col("id").eq(lit(50i64)))]
    ));
    // OR: one disjunct possibly matches -> keep.
    assert!(prune_part(
        &m,
        &[col("id").eq(lit(5i64)).or(col("id").eq(lit(50i64)))]
    ));
    // Literal-on-left comparisons are flipped: `100 < id` == `id > 100` -> prune.
    assert!(!prune_part(&m, &[lit(100i64).lt(col("id"))]));
    // `5 > id` == `id < 5` -> prune.
    assert!(!prune_part(&m, &[lit(5i64).gt(col("id"))]));
    // `50 <= id` == `id >= 50` -> keep.
    assert!(prune_part(&m, &[lit(50i64).lt_eq(col("id"))]));
    // `90 >= id` == `id <= 90` -> keep.
    assert!(prune_part(&m, &[lit(90i64).gt_eq(col("id"))]));
}
