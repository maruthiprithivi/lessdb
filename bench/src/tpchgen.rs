//! Deterministic TPC-H-inspired data generator (dbgen-lite).
//!
//! Follows TPC-H cardinalities and key distributions at any scale factor:
//! lineitem = 6,000,000·SF, orders = 1,500,000·SF, part = 200,000·SF,
//! partsupp = 800,000·SF, supplier = 10,000·SF, customer = 150,000·SF,
//! nation = 25, region = 5. Dates are stored as ISO strings (string
//! comparison == chronological comparison), so queries stay portable.

use std::sync::Arc;

use arrow::array::{Float64Builder, Int64Builder, StringBuilder};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow::record_batch::RecordBatch;

/// Deterministic xorshift RNG.
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// Uniform in [0, 1).
    pub fn f(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next_u64() % (hi - lo + 1) as u64) as i64
    }
    pub fn pick<'a>(&mut self, pool: &'a [&'a str]) -> &'a str {
        pool[(self.next_u64() as usize) % pool.len()]
    }
}

const NATION_NAMES: [&str; 25] = [
    "ALGERIA",
    "ARGENTINA",
    "BRAZIL",
    "CANADA",
    "EGYPT",
    "ETHIOPIA",
    "FRANCE",
    "GERMANY",
    "INDIA",
    "INDONESIA",
    "IRAN",
    "IRAQ",
    "JAPAN",
    "JORDAN",
    "KENYA",
    "MOROCCO",
    "MOZAMBIQUE",
    "PERU",
    "CHINA",
    "ROMANIA",
    "SAUDI ARABIA",
    "VIETNAM",
    "RUSSIA",
    "UNITED KINGDOM",
    "UNITED STATES",
];
const REGION_NAMES: [&str; 5] = ["AFRICA", "AMERICA", "ASIA", "EUROPE", "MIDDLE EAST"];
const SEGMENTS: [&str; 5] = [
    "AUTOMOBILE",
    "BUILDING",
    "FURNITURE",
    "HOUSEHOLD",
    "MACHINERY",
];
const PRIORITIES: [&str; 5] = ["1-URGENT", "2-HIGH", "3-MEDIUM", "4-NOT SPECIFIED", "5-LOW"];
const SHIPMODES: [&str; 7] = ["MAIL", "SHIP", "TRUCK", "AIR", "REG AIR", "FOB", "RAIL"];
const PART_TYPES: [&str; 8] = [
    "PROMO BURNISHED COPPER",
    "LARGE ANODIZED NICKEL",
    "MEDIUM POLISHED BRASS",
    "SMALL PLATED STEEL",
    "PROMO PLATED TIN",
    "ECONOMY BRUSHED STEEL",
    "STANDARD POLISHED COPPER",
    "LARGE PLATED BRASS",
];
const WORD_POOL: [&str; 24] = [
    "azure", "blush", "coral", "dusk", "ember", "frost", "golden", "honey", "ivory", "jade",
    "khaki", "linen", "misty", "navy", "olive", "peach", "quartz", "rose", "sandy", "teal",
    "umber", "violet", "wheat", "zinc",
];

/// Civil date helpers (proleptic Gregorian, days since 1970-01-01).
pub fn date_to_days(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}
pub fn days_to_date(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
pub fn fmt_date(days: i64) -> String {
    let (y, m, d) = days_to_date(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// One generated table with a raw-size estimate (for compression ratios).
pub struct GeneratedTable {
    pub name: String,
    pub schema: SchemaRef,
    pub batches: Vec<RecordBatch>,
    pub raw_bytes: u64,
}

pub struct TpchData {
    pub tables: Vec<GeneratedTable>,
}

fn batch_from_builders(
    name: &str,
    fields: Vec<Field>,
    cols: Vec<Box<dyn arrow::array::ArrayBuilder>>,
) -> GeneratedTable {
    let schema = Arc::new(Schema::new(fields));
    let mut arrays: Vec<Arc<dyn arrow::array::Array>> = Vec::with_capacity(cols.len());
    for mut b in cols {
        arrays.push(b.finish());
    }
    let batch = RecordBatch::try_new(schema.clone(), arrays).expect("batch");
    GeneratedTable {
        name: name.to_string(),
        schema,
        batches: vec![batch],
        raw_bytes: 0,
    }
}

fn i64_col(name: &str) -> (Field, Int64Builder) {
    (
        Field::new(name, DataType::Int64, false),
        Int64Builder::new(),
    )
}
fn f64_col(name: &str) -> (Field, Float64Builder) {
    (
        Field::new(name, DataType::Float64, false),
        Float64Builder::new(),
    )
}
fn str_col(name: &str) -> (Field, StringBuilder) {
    (
        Field::new(name, DataType::Utf8, false),
        StringBuilder::new(),
    )
}

fn raw_estimate(batch: &RecordBatch) -> u64 {
    let mut bytes = 0u64;
    for col in batch.columns() {
        use arrow::datatypes::DataType as T;
        match col.data_type() {
            T::Int64 => bytes += 8 * col.len() as u64,
            T::Float64 => bytes += 8 * col.len() as u64,
            T::Utf8 => {
                let arr = col
                    .as_any()
                    .downcast_ref::<arrow::array::StringArray>()
                    .unwrap();
                bytes += arr
                    .iter()
                    .map(|s| s.map(|v| v.len() as u64).unwrap_or(0))
                    .sum::<u64>();
            }
            _ => {}
        }
    }
    bytes
}

/// Generate the full TPC-H-like dataset at the given scale factor.
pub fn generate(sf: f64) -> TpchData {
    let mut rng = Rng::new(0x7A11_5CAF);
    let li = (6_000_000.0 * sf) as usize;
    let o = (1_500_000.0 * sf) as usize;
    let p = (200_000.0 * sf) as usize;
    let s = (10_000.0 * sf) as usize;
    let c = (150_000.0 * sf) as usize;
    let ps = (800_000.0 * sf) as usize;
    let o = o.max(1);
    let p = p.max(1);
    let s = s.max(1);
    let c = c.max(1);
    let ps = ps.max(1);
    let li = li.max(4);

    // ---- nation / region ----
    let (f0, mut b0) = i64_col("n_nationkey");
    let (f1, mut b1) = str_col("n_name");
    let (f2, mut b2) = i64_col("n_regionkey");
    for i in 0..25i64 {
        b0.append_value(i);
        b1.append_value(NATION_NAMES[i as usize]);
        b2.append_value(i / 5);
    }
    let nation = batch_from_builders(
        "nation",
        vec![f0, f1, f2],
        vec![Box::new(b0), Box::new(b1), Box::new(b2)],
    );

    let (f0, mut b0) = i64_col("r_regionkey");
    let (f1, mut b1) = str_col("r_name");
    for i in 0..5i64 {
        b0.append_value(i);
        b1.append_value(REGION_NAMES[i as usize]);
    }
    let region = batch_from_builders("region", vec![f0, f1], vec![Box::new(b0), Box::new(b1)]);

    // ---- part ----
    let (f0, mut b0) = i64_col("p_partkey");
    let (f1, mut b1) = str_col("p_name");
    let (f2, mut b2) = str_col("p_type");
    let (f3, mut b3) = i64_col("p_size");
    let (f4, mut b4) = f64_col("p_retailprice");
    let mut part_prices = vec![0.0f64; p + 1];
    for i in 1..=p as i64 {
        b0.append_value(i);
        b1.append_value(format!("{} {}", rng.pick(&WORD_POOL), rng.pick(&WORD_POOL)));
        b2.append_value(rng.pick(&PART_TYPES));
        b3.append_value(rng.range(1, 50));
        let price = (900.0 + rng.f() * 1000.0).round() / 10.0;
        b4.append_value(price);
        part_prices[i as usize] = price;
    }
    let part = batch_from_builders(
        "part",
        vec![f0, f1, f2, f3, f4],
        vec![
            Box::new(b0),
            Box::new(b1),
            Box::new(b2),
            Box::new(b3),
            Box::new(b4),
        ],
    );

    // ---- supplier ----
    let (f0, mut b0) = i64_col("s_suppkey");
    let (f1, mut b1) = str_col("s_name");
    let (f2, mut b2) = i64_col("s_nationkey");
    let (f3, mut b3) = f64_col("s_acctbal");
    for i in 1..=s as i64 {
        b0.append_value(i);
        b1.append_value(format!("Supplier#{i:09}"));
        b2.append_value(rng.range(0, 24));
        b3.append_value(((rng.f() - 0.5) * 10000.0).round() / 100.0);
    }
    let supplier = batch_from_builders(
        "supplier",
        vec![f0, f1, f2, f3],
        vec![Box::new(b0), Box::new(b1), Box::new(b2), Box::new(b3)],
    );

    // ---- partsupp ----
    let (f0, mut b0) = i64_col("ps_partkey");
    let (f1, mut b1) = i64_col("ps_suppkey");
    let (f2, mut b2) = i64_col("ps_availqty");
    let (f3, mut b3) = f64_col("ps_supplycost");
    for i in 0..ps as i64 {
        let partkey = (i % p as i64) + 1;
        let suppkey = (partkey + (i / p as i64) % 4 * (s as i64 / 4).max(1)) % s as i64 + 1;
        b0.append_value(partkey);
        b1.append_value(suppkey);
        b2.append_value(rng.range(1, 9999));
        b3.append_value((rng.f() * 100.0).round() / 100.0);
    }
    let partsupp = batch_from_builders(
        "partsupp",
        vec![f0, f1, f2, f3],
        vec![Box::new(b0), Box::new(b1), Box::new(b2), Box::new(b3)],
    );

    // ---- customer ----
    let (f0, mut b0) = i64_col("c_custkey");
    let (f1, mut b1) = str_col("c_name");
    let (f2, mut b2) = i64_col("c_nationkey");
    let (f3, mut b3) = f64_col("c_acctbal");
    let (f4, mut b4) = str_col("c_mktsegment");
    for i in 1..=c as i64 {
        b0.append_value(i);
        b1.append_value(format!("Customer#{i:09}"));
        b2.append_value(rng.range(0, 24));
        b3.append_value(((rng.f() - 0.5) * 10000.0).round() / 100.0);
        b4.append_value(rng.pick(&SEGMENTS));
    }
    let customer = batch_from_builders(
        "customer",
        vec![f0, f1, f2, f3, f4],
        vec![
            Box::new(b0),
            Box::new(b1),
            Box::new(b2),
            Box::new(b3),
            Box::new(b4),
        ],
    );

    // ---- orders ----
    let (f0, mut b0) = i64_col("o_orderkey");
    let (f1, mut b1) = i64_col("o_custkey");
    let (f2, mut b2) = str_col("o_orderdate");
    let (f3, mut b3) = str_col("o_orderpriority");
    let (f4, mut b4) = i64_col("o_shippriority");
    let (f5, mut b5) = f64_col("o_totalprice");
    let d0 = date_to_days(1992, 1, 1);
    let d1 = date_to_days(1998, 8, 2);
    let mut order_dates = vec![0i64; o + 1];
    for i in 1..=o as i64 {
        let days = rng.range(d0, d1);
        order_dates[i as usize] = days;
        b0.append_value(i);
        b1.append_value(rng.range(1, c as i64));
        b2.append_value(fmt_date(days));
        b3.append_value(rng.pick(&PRIORITIES));
        b4.append_value(0);
        b5.append_value(((rng.f() * 500000.0) + 10000.0).round() / 100.0);
    }
    let orders = batch_from_builders(
        "orders",
        vec![f0, f1, f2, f3, f4, f5],
        vec![
            Box::new(b0),
            Box::new(b1),
            Box::new(b2),
            Box::new(b3),
            Box::new(b4),
            Box::new(b5),
        ],
    );

    // ---- lineitem ----
    let (f0, mut b0) = i64_col("l_orderkey");
    let (f1, mut b1) = i64_col("l_partkey");
    let (f2, mut b2) = i64_col("l_suppkey");
    let (f3, mut b3) = i64_col("l_linenumber");
    let (f4, mut b4) = f64_col("l_quantity");
    let (f5, mut b5) = f64_col("l_extendedprice");
    let (f6, mut b6) = f64_col("l_discount");
    let (f7, mut b7) = f64_col("l_tax");
    let (f8, mut b8) = str_col("l_shipdate");
    let (f9, mut b9) = str_col("l_returnflag");
    let (f10, mut b10) = str_col("l_linestatus");
    let (f11, mut b11) = str_col("l_shipmode");
    let retflags = ["N", "N", "N", "N", "N", "N", "N", "N", "R", "A"];
    for i in 0..li as i64 {
        let orderkey = (i / 4) % o as i64 + 1;
        let linenumber = i % 4 + 1;
        let partkey = rng.range(1, p as i64);
        let suppkey = (partkey + (linenumber - 1) * (s as i64 / 4).max(1)) % s as i64 + 1;
        let quantity = rng.range(1, 50) as f64;
        let price = part_prices[partkey as usize];
        b0.append_value(orderkey);
        b1.append_value(partkey);
        b2.append_value(suppkey);
        b3.append_value(linenumber);
        b4.append_value(quantity);
        b5.append_value((quantity * price).round() / 100.0);
        b6.append_value((rng.f() * 10.0).round() / 100.0);
        b7.append_value((rng.f() * 8.0).round() / 100.0);
        let ship = order_dates[orderkey as usize] + rng.range(1, 121);
        b8.append_value(fmt_date(ship));
        b9.append_value(rng.pick(&retflags));
        b10.append_value(if rng.f() < 0.5 { "F" } else { "O" });
        b11.append_value(rng.pick(&SHIPMODES));
    }
    let lineitem = batch_from_builders(
        "lineitem",
        vec![f0, f1, f2, f3, f4, f5, f6, f7, f8, f9, f10, f11],
        vec![
            Box::new(b0),
            Box::new(b1),
            Box::new(b2),
            Box::new(b3),
            Box::new(b4),
            Box::new(b5),
            Box::new(b6),
            Box::new(b7),
            Box::new(b8),
            Box::new(b9),
            Box::new(b10),
            Box::new(b11),
        ],
    );

    let mut tables = vec![
        nation, region, part, supplier, partsupp, customer, orders, lineitem,
    ];
    for t in &mut tables {
        t.raw_bytes = t.batches.iter().map(raw_estimate).sum();
    }
    TpchData { tables }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_roundtrip() {
        let d = date_to_days(2025, 1, 15);
        assert_eq!(days_to_date(d), (2025, 1, 15));
        assert_eq!(fmt_date(d), "2025-01-15");
        let d = date_to_days(1992, 1, 1);
        assert_eq!(fmt_date(d), "1992-01-01");
    }

    #[test]
    fn cardinalities() {
        let data = generate(0.001);
        let get =
            |name: &str| data.tables.iter().find(|t| t.name == name).unwrap().batches[0].num_rows();
        assert_eq!(get("nation"), 25);
        assert_eq!(get("region"), 5);
        assert_eq!(get("lineitem"), 6000);
        assert_eq!(get("orders"), 1500);
        assert_eq!(get("part"), 200);
        assert_eq!(get("supplier"), 10);
        assert_eq!(get("customer"), 150);
        assert_eq!(get("partsupp"), 800);
    }
}
