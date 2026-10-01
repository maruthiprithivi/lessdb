//! Data-part I/O: writing immutable parts (parquet + metadata) and reading
//! them back.

use std::path::{Path, PathBuf};

use arrow::array::BooleanArray;
use arrow::compute::concat_batches;
use arrow::compute::{SortColumn, SortOptions, filter_record_batch, lexsort_to_indices, take};
use arrow::datatypes::{Schema, SchemaRef};
use arrow::record_batch::RecordBatch;
use arrow::row::{RowConverter, SortField};
use bytes::Bytes;
use parquet::arrow::ArrowWriter;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::file::properties::WriterProperties;
use std::sync::Arc;

use less_common::{LessError, Result};

use crate::bloom::Bloom;
use crate::codec::Compression;
use crate::part_meta::{ColumnStats, PartMeta};

/// Name of the data file inside a part directory.
pub const DATA_FILE: &str = "data.parquet";
/// Name of the metadata file inside a part directory.
pub const META_FILE: &str = "meta.json";

/// Options controlling how a part is written.
#[derive(Debug, Clone)]
pub struct WriteOptions {
    pub compression: Compression,
    pub zstd_level: i32,
    /// Sort-key columns; data is sorted by these before writing.
    pub sort_key: Vec<String>,
    /// Uniqueness columns; duplicates are collapsed keeping the last row.
    pub unique: Vec<String>,
    /// False-positive rate for bloom filters on uniqueness columns.
    pub bloom_fp_rate: f64,
    /// Merge level of the part (0 = flushed insert, N = merged N times).
    pub level: u32,
    /// Highest WAL LSN included in this part (crash-recovery filter).
    pub wal_lsn_max: Option<u64>,
}

impl WriteOptions {
    pub fn new(compression: Compression) -> Self {
        Self {
            compression,
            zstd_level: 3,
            sort_key: vec![],
            unique: vec![],
            bloom_fp_rate: 0.01,
            level: 0,
            wal_lsn_max: None,
        }
    }
}

fn column_index(schema: &Schema, name: &str) -> Result<usize> {
    schema
        .index_of(name)
        .map_err(|_| LessError::Engine(format!("column '{name}' not found in schema")))
}

/// Concatenate batches and sort by the sort key (stable).
/// Sort one batch by the sort key (used to prepare merge runs).
pub fn sort_batch(
    schema: &SchemaRef,
    batch: &RecordBatch,
    sort_key: &[String],
) -> Result<RecordBatch> {
    sort_batches(schema, std::slice::from_ref(batch), sort_key)
}

fn sort_batches(
    schema: &SchemaRef,
    batches: &[RecordBatch],
    sort_key: &[String],
) -> Result<RecordBatch> {
    let batch = concat_batches(schema, batches)?;
    let cols: Vec<_> = sort_key
        .iter()
        .map(|c| column_index(schema, c).map(|i| batch.column(i).clone()))
        .collect::<Result<_>>()?;
    if cols.is_empty() {
        return Ok(batch);
    }
    let sort_columns: Vec<SortColumn> = cols
        .iter()
        .map(|c| SortColumn {
            values: c.clone(),
            options: Some(SortOptions {
                descending: false,
                nulls_first: false,
            }),
        })
        .collect();
    let indices = lexsort_to_indices(&sort_columns, None)?;
    let taken: Vec<_> = batch
        .columns()
        .iter()
        .map(|c| take(c.as_ref(), &indices, None))
        .collect::<std::result::Result<_, _>>()?;
    Ok(RecordBatch::try_new(schema.clone(), taken)?)
}

/// Collapse runs of rows with equal uniqueness-column values, keeping the
/// *last* row of each run (replacing-merge semantics). Requires the input to be sorted by the sort key, of
/// which the unique columns are a prefix.
fn dedup_unique(batch: &RecordBatch, schema: &SchemaRef, unique: &[String]) -> Result<RecordBatch> {
    if unique.is_empty() || batch.num_rows() == 0 {
        return Ok(batch.clone());
    }
    let cols: Vec<_> = unique
        .iter()
        .map(|c| column_index(schema, c).map(|i| batch.column(i).clone()))
        .collect::<Result<_>>()?;
    let fields: Vec<SortField> = cols
        .iter()
        .map(|c| SortField::new(c.data_type().clone()))
        .collect();
    let converter = RowConverter::new(fields)?;
    let rows = converter.convert_columns(&cols)?;

    let n = batch.num_rows();
    let mut keep = vec![false; n];
    let mut i = 0usize;
    while i < n {
        let mut j = i + 1;
        while j < n && rows.row(j).as_ref() == rows.row(i).as_ref() {
            j += 1;
        }
        keep[j - 1] = true; // last of the run wins
        i = j;
    }
    Ok(filter_record_batch(batch, &BooleanArray::from(keep))?)
}

/// Rows per chunk in the streaming k-way merge (bounded working set).
const MERGE_CHUNK_ROWS: usize = 65_536;

/// Shared writer properties for data parts: zstd/lz4 compression, 1MiB data
/// pages, 1M-row row groups. The offset index is disabled: without it the
/// parquet reader never takes the sparse-column-chunk path, which is broken
/// for wide string columns in some DataFusion/parquet-rs combinations
/// (Mask selection trips with "Invalid offset in sparse column chunk data",
/// apache/datafusion#8092). Page indexes can be re-enabled once the upstream
/// reader is solid; row-group statistics skipping is unaffected.
fn writer_props(opts: &WriteOptions) -> WriterProperties {
    WriterProperties::builder()
        .set_compression(opts.compression.to_parquet(opts.zstd_level))
        .set_data_page_size_limit(1024 * 1024)
        .set_write_batch_size(1024)
        .set_max_row_group_row_count(Some(1 << 20))
        .set_offset_index_disabled(true)
        .build()
}

/// Merge already-sorted runs (each sorted by the sort key) into a part via
/// a k-way merge that streams bounded chunks straight to the parquet file —
/// peak memory is the runs themselves plus one chunk, with none of the
/// concat-everything + global-sort blowup of [`write_part`]. Uniqueness
/// dedup runs across chunk boundaries (last row per unique key wins) and
/// stats/blooms accumulate incrementally.
pub fn write_part_sorted(
    parts_dir: &Path,
    table: &str,
    schema: &SchemaRef,
    runs: Vec<RecordBatch>,
    opts: &WriteOptions,
) -> Result<PartMeta> {
    if runs.is_empty() || runs.iter().all(|b| b.num_rows() == 0) {
        return Err(LessError::Engine("cannot write an empty part".into()));
    }
    let sort_idx: Vec<usize> = opts
        .sort_key
        .iter()
        .map(|c| column_index(schema, c))
        .collect::<Result<_>>()?;
    let unique_idx: Vec<usize> = opts
        .unique
        .iter()
        .map(|c| column_index(schema, c))
        .collect::<Result<_>>()?;

    // Row-format sort keys per run (the heap's ordering) and unique keys
    // (streaming dedup).
    let sort_conv = (!sort_idx.is_empty())
        .then(|| {
            RowConverter::new(
                sort_idx
                    .iter()
                    .map(|i| SortField::new(schema.field(*i).data_type().clone()))
                    .collect::<Vec<_>>(),
            )
        })
        .transpose()?;
    let uniq_conv = (!unique_idx.is_empty())
        .then(|| {
            RowConverter::new(
                unique_idx
                    .iter()
                    .map(|i| SortField::new(schema.field(*i).data_type().clone()))
                    .collect::<Vec<_>>(),
            )
        })
        .transpose()?;

    let sort_keys: Vec<Vec<arrow::row::OwnedRow>> = match &sort_conv {
        Some(conv) => runs
            .iter()
            .map(|b| {
                let cols: Vec<_> = sort_idx.iter().map(|i| b.column(*i).clone()).collect();
                conv.convert_columns(&cols)
                    .map(|rows| rows.iter().map(|r| r.owned()).collect())
                    .map_err(LessError::Arrow)
            })
            .collect::<Result<_>>()?,
        None => runs.iter().map(|_| Vec::new()).collect(),
    };

    // Heap merge: smallest sort key first. With no sort key the runs are
    // processed in arrival order (each run's rows seeded in order).
    /// Ordering over optional sort keys; `None` (no sort key) ranks above
    /// everything so those runs drain in an arbitrary but complete order.
    fn key_ord(
        a: &Option<arrow::row::OwnedRow>,
        b: &Option<arrow::row::OwnedRow>,
    ) -> std::cmp::Ordering {
        match (a, b) {
            (Some(x), Some(y)) => x.cmp(y),
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, None) => std::cmp::Ordering::Equal,
        }
    }
    #[derive(PartialEq, Eq)]
    struct Cursor {
        key: Option<arrow::row::OwnedRow>,
        run: usize,
        pos: usize,
    }
    impl PartialOrd for Cursor {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for Cursor {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            // Reverse: BinaryHeap pops the largest; we want the smallest.
            key_ord(&other.key, &self.key)
        }
    }
    let mut heap = std::collections::BinaryHeap::new();
    for (run, keys) in sort_keys.iter().enumerate() {
        if keys.is_empty() {
            for pos in 0..runs[run].num_rows() {
                heap.push(Cursor {
                    key: None,
                    run,
                    pos,
                });
            }
        } else if let Some(key) = keys.first() {
            heap.push(Cursor {
                key: Some(key.clone()),
                run,
                pos: 0,
            });
        }
    }

    // Stream merged chunks into a temp parquet file; rename into place at
    // the end (the part name embeds the row count, known only after the
    // merge pass).
    let tmp_path = parts_dir.join(format!(".tmp-{}.parquet", uuid::Uuid::new_v4()));
    let props = writer_props(opts);
    let mut writer = ArrowWriter::try_new(
        std::fs::File::create(&tmp_path)?,
        schema.clone(),
        Some(props),
    )?;

    let mut total_rows = 0u64;
    let mut merged_stats: Vec<Option<ColumnStats>> = vec![None; schema.fields().len()];
    let mut blooms: Vec<Option<crate::bloom::Bloom>> = unique_idx
        .iter()
        .map(|_| {
            Some(crate::bloom::Bloom::with_capacity(
                runs.iter().map(|b| b.num_rows()).sum::<usize>().max(1),
                opts.bloom_fp_rate,
            ))
        })
        .collect();
    let mut last_unique: Option<arrow::row::OwnedRow> = None;

    while !heap.is_empty() {
        // Pop one chunk of rows (global sorted order).
        let mut picks: Vec<(usize, usize)> = Vec::with_capacity(MERGE_CHUNK_ROWS);
        while picks.len() < MERGE_CHUNK_ROWS {
            let Some(cur) = heap.pop() else { break };
            picks.push((cur.run, cur.pos));
            let pos = cur.pos + 1;
            if pos < runs[cur.run].num_rows() {
                let key = sort_keys[cur.run].get(pos).cloned();
                heap.push(Cursor {
                    key,
                    run: cur.run,
                    pos,
                });
            }
        }

        // Per-run contiguous prefixes (each run contributes its smallest
        // remaining rows, in ascending order within the run).
        let mut start: Vec<usize> = vec![usize::MAX; runs.len()];
        let mut end: Vec<usize> = vec![0; runs.len()];
        for (run, pos) in &picks {
            start[*run] = start[*run].min(*pos);
            end[*run] = end[*run].max(*pos + 1);
        }

        // Concatenate per-run column slices, then restore the global order
        // with a take() permutation (no per-type value plumbing).
        let mut ordered_cols: Vec<arrow::array::ArrayRef> =
            Vec::with_capacity(schema.fields().len());
        for c in 0..schema.fields().len() {
            let mut parts: Vec<arrow::array::ArrayRef> = Vec::new();
            for run in 0..runs.len() {
                if start[run] == usize::MAX {
                    continue;
                }
                parts.push(runs[run].column(c).slice(start[run], end[run] - start[run]));
            }
            let concat = if parts.len() == 1 {
                parts.pop().unwrap()
            } else {
                arrow::compute::concat(&parts.iter().map(|a| a.as_ref()).collect::<Vec<_>>())?
            };
            ordered_cols.push(concat);
        }
        // Position within the concatenated chunk array = per-run prefix
        // offset + offset within the run's slice.
        let mut prefix = vec![0usize; runs.len()];
        for run in 1..runs.len() {
            prefix[run] = if start[run - 1] == usize::MAX {
                prefix[run - 1]
            } else {
                prefix[run - 1] + (end[run - 1] - start[run - 1])
            };
        }
        let indices = arrow::array::UInt32Array::from(
            picks
                .iter()
                .map(|(run, pos)| (prefix[*run] + (pos - start[*run])) as u32)
                .collect::<Vec<_>>(),
        );
        let perm: arrow::array::ArrayRef = Arc::new(indices);
        let mut final_cols: Vec<arrow::array::ArrayRef> = Vec::with_capacity(ordered_cols.len());
        for col in &ordered_cols {
            final_cols.push(take(col, &perm, None)?);
        }
        let chunk = RecordBatch::try_new(schema.clone(), final_cols)?;

        // Streaming uniqueness dedup: keep the last row of each unique-key
        // run, chained across chunks.
        let chunk = match &uniq_conv {
            Some(conv) => {
                let cols: Vec<_> = unique_idx
                    .iter()
                    .map(|i| chunk.column(*i).clone())
                    .collect();
                let keys = conv.convert_columns(&cols).map_err(LessError::Arrow)?;
                let mut keep = vec![true; chunk.num_rows()];
                let mut prev: Option<arrow::row::Row<'_>> = last_unique.as_ref().map(|r| r.row());
                for (k, key) in keys.iter().enumerate() {
                    if prev.as_ref().is_some_and(|p| *p == key) {
                        keep[k - 1] = false;
                    }
                    prev = Some(key);
                }
                last_unique = keys.iter().next_back().map(|r| r.owned());
                filter_record_batch(&chunk, &BooleanArray::from(keep))?
            }
            None => chunk,
        };

        total_rows += chunk.num_rows() as u64;
        for (i, arr) in chunk.columns().iter().enumerate() {
            let fresh = ColumnStats::compute(schema.field(i).name(), arr.as_ref());
            merged_stats[i] = Some(match merged_stats[i].take() {
                None => fresh,
                Some(prev) => merge_stats(prev, fresh),
            });
        }
        for (k, uniq) in unique_idx.iter().enumerate() {
            let arr = chunk.column(*uniq);
            for i in 0..arr.len() {
                if arr.is_null(i) {
                    continue;
                }
                let v = crate::part_meta::StatValue::from_array_value(arr.as_ref(), i);
                if let Some(bloom) = blooms[k].as_mut() {
                    bloom.insert(&v.to_bytes());
                }
            }
        }
        writer.write(&chunk)?;
    }
    writer.close()?;

    // Finalize: the part name embeds the row count, so rename the temp
    // file into the real part directory and write the metadata.
    let ts_ms = chrono::Utc::now().timestamp_millis();
    let uuid = uuid::Uuid::new_v4().to_string().replace('-', "");
    let name = PartMeta::new_part_name(ts_ms, &uuid[..8], total_rows, opts.level);
    let part_dir = parts_dir.join(&name);
    std::fs::create_dir_all(&part_dir)?;
    std::fs::rename(&tmp_path, part_dir.join(DATA_FILE))?;

    let mut columns: Vec<ColumnStats> = merged_stats
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            s.ok_or_else(|| {
                LessError::Engine(format!("no stats for column {}", schema.field(i).name()))
            })
        })
        .collect::<Result<_>>()?;
    for (k, uniq_name) in opts.unique.iter().enumerate() {
        if let Some(stats) = columns.iter_mut().find(|c| c.name == *uniq_name)
            && let Some(bloom) = blooms[k].take()
        {
            use base64::Engine;
            stats.bloom = Some(base64::engine::general_purpose::STANDARD.encode(bloom.to_bytes()));
        }
    }

    let meta = PartMeta {
        id: uuid,
        name,
        table: table.to_string(),
        row_count: total_rows,
        created_at: chrono::Utc::now().to_rfc3339(),
        compression: opts.compression.as_str().to_string(),
        sort_key: opts.sort_key.clone(),
        unique: opts.unique.clone(),
        columns,
        wal_lsn_max: opts.wal_lsn_max,
    };
    std::fs::write(
        part_dir.join(META_FILE),
        serde_json::to_string_pretty(&meta)?,
    )?;
    Ok(meta)
}

/// Merge two per-column stat sets (min of mins, max of maxes, summed
/// counts).
fn merge_stats(mut a: ColumnStats, b: ColumnStats) -> ColumnStats {
    a.min = match (a.min.take(), b.min) {
        (Some(x), Some(y)) => Some(match x.partial_cmp(&y) {
            Some(std::cmp::Ordering::Greater) => y,
            _ => x,
        }),
        (x, y) => x.or(y),
    };
    a.max = match (a.max.take(), b.max) {
        (Some(x), Some(y)) => Some(match x.partial_cmp(&y) {
            Some(std::cmp::Ordering::Less) => y,
            _ => x,
        }),
        (x, y) => x.or(y),
    };
    a.null_count += b.null_count;
    a.has_nan |= b.has_nan;
    a
}

/// Write an immutable data part into `parts_dir/{part_name}/`.
///
/// Returns the part metadata. The part directory contains `data.parquet`
/// and `meta.json`.
pub fn write_part(
    parts_dir: &Path,
    table: &str,
    schema: &SchemaRef,
    batches: Vec<RecordBatch>,
    opts: &WriteOptions,
) -> Result<PartMeta> {
    if batches.is_empty() || batches.iter().all(|b| b.num_rows() == 0) {
        return Err(LessError::Engine("cannot write an empty part".into()));
    }

    // Sort by sort key, then collapse uniqueness duplicates (keep last).
    let sorted = sort_batches(schema, &batches, &opts.sort_key)?;
    let deduped = dedup_unique(&sorted, schema, &opts.unique)?;
    let row_count = deduped.num_rows() as u64;

    // Part identity.
    let ts_ms = chrono::Utc::now().timestamp_millis();
    let uuid = uuid::Uuid::new_v4().to_string().replace('-', "");
    let name = PartMeta::new_part_name(ts_ms, &uuid[..8], row_count, opts.level);
    let part_dir = parts_dir.join(&name);
    std::fs::create_dir_all(&part_dir)?;

    // Per-column statistics + blooms on uniqueness columns.
    let mut columns: Vec<ColumnStats> = deduped
        .columns()
        .iter()
        .enumerate()
        .map(|(i, arr)| ColumnStats::compute(schema.field(i).name(), arr.as_ref()))
        .collect();
    for uniq in &opts.unique {
        let idx = column_index(schema, uniq)?;
        let arr = deduped.column(idx);
        let mut bloom = Bloom::with_capacity(row_count as usize, opts.bloom_fp_rate);
        for i in 0..arr.len() {
            if arr.is_null(i) {
                continue;
            }
            let v = crate::part_meta::StatValue::from_array_value(arr.as_ref(), i);
            bloom.insert(&v.to_bytes());
        }
        if let Some(stats) = columns.iter_mut().find(|c| c.name == *uniq) {
            use base64::Engine;
            stats.bloom = Some(base64::engine::general_purpose::STANDARD.encode(bloom.to_bytes()));
        }
    }

    // Write data.parquet with the configured compression.
    let file = std::fs::File::create(part_dir.join(DATA_FILE))?;
    let props = writer_props(opts);
    let mut writer = ArrowWriter::try_new(file, schema.clone(), Some(props))?;
    writer.write(&deduped)?;
    writer.close()?;

    let meta = PartMeta {
        id: uuid,
        name,
        table: table.to_string(),
        row_count,
        created_at: chrono::Utc::now().to_rfc3339(),
        compression: opts.compression.as_str().to_string(),
        sort_key: opts.sort_key.clone(),
        unique: opts.unique.clone(),
        columns,
        wal_lsn_max: opts.wal_lsn_max,
    };
    std::fs::write(
        part_dir.join(META_FILE),
        serde_json::to_string_pretty(&meta)?,
    )?;
    Ok(meta)
}

/// Read a part's metadata.
pub fn read_part_meta(dir: &Path) -> Result<PartMeta> {
    let bytes = std::fs::read(dir.join(META_FILE))?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Read all rows of a part, optionally projecting columns.
pub fn read_part(dir: &Path, projection: Option<&[usize]>) -> Result<Vec<RecordBatch>> {
    let bytes = std::fs::read(dir.join(DATA_FILE))?;
    read_part_from_bytes(Bytes::from(bytes), projection)
}

/// Read all rows of a part from raw parquet bytes (used for object-store
/// parts, which never touch the local filesystem).
pub fn read_part_from_bytes(
    bytes: Bytes,
    projection: Option<&[usize]>,
) -> Result<Vec<RecordBatch>> {
    let builder = ParquetRecordBatchReaderBuilder::try_new(bytes)?;
    let reader = builder.build()?;
    let mut batches: Vec<RecordBatch> = reader.collect::<std::result::Result<_, _>>()?;
    if let Some(proj) = projection {
        for batch in &mut batches {
            let projected = batch.project(proj)?;
            *batch = projected;
        }
    }
    Ok(batches)
}

/// List all parts under a parts directory, ordered oldest-first.
/// A part directory commits only when its `meta.json` is written (it is
/// written last), so dirs without one are torn writes from a crash
/// mid-`write_part` and are skipped — never visible as parts.
pub fn list_part_metas(parts_dir: &Path) -> Result<Vec<(PathBuf, PartMeta)>> {
    let mut out = vec![];
    if !parts_dir.exists() {
        return Ok(out);
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(parts_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    entries.sort();
    for dir in entries {
        if !dir.join(META_FILE).exists() {
            continue; // uncommitted (torn) part — never became visible
        }
        out.push((dir.clone(), read_part_meta(&dir)?));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Float64Array, Int64Array, StringArray};
    use std::sync::Arc;

    fn schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            arrow::datatypes::Field::new("id", arrow::datatypes::DataType::Int64, false),
            arrow::datatypes::Field::new("city", arrow::datatypes::DataType::Utf8, false),
            arrow::datatypes::Field::new("amount", arrow::datatypes::DataType::Float64, false),
        ]))
    }

    #[test]
    fn part_roundtrip_with_sort_dedup_and_bloom() {
        let schema = schema();
        let b1 = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(Int64Array::from(vec![3, 1, 2])),
                Arc::new(StringArray::from(vec!["berlin", "paris", "berlin"])),
                Arc::new(Float64Array::from(vec![30.0, 10.0, 20.0])),
            ],
        )
        .unwrap();
        let b2 = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(Int64Array::from(vec![2, 1])),
                Arc::new(StringArray::from(vec!["berlin", "london"])),
                Arc::new(Float64Array::from(vec![22.0, 11.0])),
            ],
        )
        .unwrap();

        let dir = std::env::temp_dir().join(format!("less-test-{}", uuid::Uuid::new_v4()));
        let parts = dir.join("parts");
        std::fs::create_dir_all(&parts).unwrap();

        let opts = WriteOptions {
            compression: Compression::Zstd,
            zstd_level: 3,
            sort_key: vec!["city".into()],
            unique: vec!["city".into()],
            bloom_fp_rate: 0.01,
            level: 0,
            wal_lsn_max: None,
        };
        let meta = write_part(&parts, "t", &schema, vec![b1, b2], &opts).unwrap();
        assert_eq!(meta.row_count, 3); // berlin twice -> last wins; paris; london
        let blooms = meta.column("city").unwrap().bloom_bytes().unwrap();
        let bloom = Bloom::from_bytes(&blooms);
        assert!(bloom.contains(b"berlin"));
        assert!(bloom.contains(b"london"));
        assert!(!bloom.contains(b"tokyo"));

        let read = read_part(&parts.join(&meta.name), None).unwrap();
        let total: usize = read.iter().map(|b| b.num_rows()).sum();
        assert_eq!(total, 3);
        // Sorted by city: berlin, london, paris.
        let cities = read[0]
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(cities.value(0), "berlin");
        assert_eq!(cities.value(1), "london");
        assert_eq!(cities.value(2), "paris");
        // berlin keeps the last amount seen (22.0).
        let amounts = read[0]
            .column(2)
            .as_any()
            .downcast_ref::<Float64Array>()
            .unwrap();
        assert_eq!(amounts.value(0), 22.0);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn list_parts_orders_oldest_first() {
        let dir = std::env::temp_dir().join(format!("less-list-{}", uuid::Uuid::new_v4()));
        let parts = dir.join("parts");
        std::fs::create_dir_all(&parts).unwrap();
        let schema = schema();
        let mk = |rows: Vec<i64>| {
            let b = RecordBatch::try_new(
                schema.clone(),
                vec![
                    Arc::new(Int64Array::from(rows)),
                    Arc::new(StringArray::from(vec!["x", "y"])),
                    Arc::new(Float64Array::from(vec![0.0, 0.0])),
                ],
            )
            .unwrap();
            write_part(
                &parts,
                "t",
                &schema,
                vec![b],
                &WriteOptions::new(Compression::Lz4),
            )
            .unwrap()
        };
        let a = mk(vec![1, 2]);
        // Distinct timestamps so "oldest first" has a well-defined order:
        // part names encode ms timestamps, and same-ms ties break
        // arbitrarily (caught by CI on a fast machine).
        std::thread::sleep(std::time::Duration::from_millis(5));
        let b = mk(vec![3, 4]);
        let metas = list_part_metas(&parts).unwrap();
        assert_eq!(metas.len(), 2);
        assert_eq!(metas[0].1.name, a.name);
        assert_eq!(metas[1].1.name, b.name);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Parts must be written without a parquet offset index: the sparse
    /// column-chunk path it enables is broken for wide string columns in
    /// some DataFusion/parquet-rs combinations ("Invalid offset in sparse
    /// column chunk data", apache/datafusion#8092). Row-group statistics
    /// skipping is unaffected.
    #[test]
    fn parts_have_no_offset_index() {
        let dir = std::env::temp_dir().join(format!("less-test-{}", uuid::Uuid::new_v4()));
        let parts = dir.join("parts");
        std::fs::create_dir_all(&parts).unwrap();
        let opts = WriteOptions::new(Compression::Zstd);
        let batch = RecordBatch::try_new(
            schema(),
            vec![
                Arc::new(Int64Array::from(vec![1, 2, 3])),
                Arc::new(StringArray::from(vec!["a", "b", "c"])),
                Arc::new(Float64Array::from(vec![1.0, 2.0, 3.0])),
            ],
        )
        .unwrap();

        let meta = write_part(&parts, "t", &schema(), vec![batch], &opts).unwrap();
        let file = std::fs::File::open(parts.join(&meta.name).join(DATA_FILE)).unwrap();
        let reader = parquet::file::reader::SerializedFileReader::new(file).unwrap();
        use parquet::file::reader::FileReader;
        let md = reader.metadata();
        for rg in md.row_groups() {
            for cc in rg.columns() {
                assert!(
                    cc.offset_index_offset().is_none(),
                    "column {} must not carry an offset index",
                    cc.column_path()
                );
            }
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
