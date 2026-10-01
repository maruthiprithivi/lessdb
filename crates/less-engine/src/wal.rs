//! Write-ahead log for crash-safe inserts.
//!
//! Inserts are appended to `<data_dir>/wal/<table>.wal` as length-prefixed
//! records `[len u64][lsn u64][arrow-ipc batch]`, optionally fsynced, so
//! accepted rows survive a crash even before they flush into a data part.
//!
//! **Idempotent replay**: every data part records `wal_lsn_max` — the
//! highest WAL sequence number its rows came from (persisted in the part's
//! `meta.json`). On recovery, replay skips every record with
//! `lsn <= max(parts.wal_lsn_max)` and re-buffers the rest, then flushes
//! them. The flush sequence (write part with `wal_lsn_max` → truncate the
//! WAL) makes recovery exactly-once:
//!
//! * crash before the part is visible → nothing was flushed → all records
//!   replay;
//! * crash after the part is visible → its `wal_lsn_max` filters the
//!   replayed records → no duplicates;
//! * crash after the WAL truncate → nothing to replay.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use arrow::ipc::reader::StreamReader;
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;

use less_common::Result;

/// The WAL directory.
pub struct Wal {
    dir: PathBuf,
    fsync: bool,
}

impl Wal {
    pub fn open(dir: &Path, fsync: bool) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        Ok(Self {
            dir: dir.to_path_buf(),
            fsync,
        })
    }

    pub fn path_for(&self, table: &str) -> PathBuf {
        self.dir.join(format!("{table}.wal"))
    }

    /// Append one record (lsn-stamped batch) and fsync when configured.
    pub fn append(&self, table: &str, lsn: u64, batch: &RecordBatch) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path_for(table))?;
        let bytes = batch_to_ipc(batch)?;
        file.write_all(&(bytes.len() as u64).to_le_bytes())?;
        file.write_all(&lsn.to_le_bytes())?;
        file.write_all(&bytes)?;
        if self.fsync {
            file.sync_data()?;
        }
        Ok(())
    }

    /// Read records with `lsn > min_lsn`. A truncated tail record (crash
    /// mid-append) stops the scan — those rows were never acknowledged.
    pub fn replay(&self, table: &str, min_lsn: u64) -> Result<Vec<RecordBatch>> {
        let path = self.path_for(table);
        if !path.exists() {
            return Ok(vec![]);
        }
        let mut file = File::open(&path)?;
        let file_len = file.metadata()?.len();
        let mut out = vec![];
        loop {
            let mut len_buf = [0u8; 8];
            match file.read_exact(&mut len_buf) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }
            let len = u64::from_le_bytes(len_buf);
            let mut lsn_buf = [0u8; 8];
            if file.read_exact(&mut lsn_buf).is_err() {
                break; // truncated record header
            }
            let lsn = u64::from_le_bytes(lsn_buf);
            // A length prefix that cannot fit in the bytes physically left in
            // the file is a torn/garbage tail (crash mid-append, or a bogus
            // prefix). Stop the scan here — honoring it would allocate an
            // absurd buffer and panic on `vec![0u8; len]`.
            let pos = file.stream_position()?;
            if len > file_len.saturating_sub(pos) {
                break;
            }
            let mut payload = vec![0u8; len as usize];
            if file.read_exact(&mut payload).is_err() {
                break; // truncated payload
            }
            // A record whose payload is not a valid Arrow IPC stream is a
            // corrupt tail; stop scanning rather than failing the whole
            // recovery (good records before it are still replayed).
            let Ok(batches) = ipc_to_batches(&payload) else {
                break;
            };
            for batch in batches {
                if lsn > min_lsn {
                    out.push(batch);
                }
            }
        }
        Ok(out)
    }

    /// Reset the WAL (after a flush made its records durable in a part).
    pub fn truncate(&self, table: &str) -> Result<()> {
        let file = OpenOptions::new().write(true).open(self.path_for(table))?;
        file.set_len(0)?;
        if self.fsync {
            file.sync_data()?;
        }
        Ok(())
    }

    pub fn remove_table(&self, table: &str) -> Result<()> {
        let path = self.path_for(table);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    /// All tables with WAL files (for open-time recovery).
    pub fn tables_with_wal(&self) -> Result<Vec<String>> {
        let mut out = vec![];
        for entry in std::fs::read_dir(&self.dir)? {
            let name = entry?.file_name().to_string_lossy().to_string();
            if let Some(table) = name.strip_suffix(".wal") {
                out.push(table.to_string());
            }
        }
        Ok(out)
    }
}

fn batch_to_ipc(batch: &RecordBatch) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    {
        let mut writer = StreamWriter::try_new(&mut buf, &batch.schema())?;
        writer.write(batch)?;
        writer.finish()?;
    }
    Ok(buf)
}

fn ipc_to_batches(bytes: &[u8]) -> Result<Vec<RecordBatch>> {
    let reader = StreamReader::try_new(std::io::Cursor::new(bytes), None)?;
    Ok(reader.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// Simulate a crash mid-append (test helper): chop the last bytes off the
/// WAL file so the tail record is incomplete.
#[cfg(test)]
pub fn truncate_tail(path: &Path) {
    let len = std::fs::metadata(path).unwrap().len();
    if len > 4 {
        let file = OpenOptions::new().write(true).open(path).unwrap();
        file.set_len(len - 4).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;

    fn batch(values: Vec<i64>) -> RecordBatch {
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("x", DataType::Int64, false)])),
            vec![Arc::new(Int64Array::from(values))],
        )
        .unwrap()
    }

    #[test]
    fn append_replay_roundtrip_and_lsn_filter() {
        let dir = std::env::temp_dir().join(format!("less-wal-{}", uuid::Uuid::new_v4()));
        let wal = Wal::open(&dir, false).unwrap();
        wal.append("t", 1, &batch(vec![1, 2])).unwrap();
        wal.append("t", 2, &batch(vec![3])).unwrap();
        wal.append("t", 3, &batch(vec![4, 5, 6])).unwrap();

        let all = wal.replay("t", 0).unwrap();
        assert_eq!(all.iter().map(|b| b.num_rows()).sum::<usize>(), 6);

        // lsn filter skips records already durable in a part.
        let rest = wal.replay("t", 2).unwrap();
        assert_eq!(rest.iter().map(|b| b.num_rows()).sum::<usize>(), 3);

        wal.truncate("t").unwrap();
        assert!(wal.replay("t", 0).unwrap().is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn truncated_tail_record_is_skipped() {
        let dir = std::env::temp_dir().join(format!("less-walt-{}", uuid::Uuid::new_v4()));
        let wal = Wal::open(&dir, false).unwrap();
        wal.append("t", 1, &batch(vec![1, 2])).unwrap();
        wal.append("t", 2, &batch(vec![3])).unwrap();
        truncate_tail(&wal.path_for("t"));
        let replay = wal.replay("t", 0).unwrap();
        // Only the intact first record survives.
        assert_eq!(replay.iter().map(|b| b.num_rows()).sum::<usize>(), 2);
        std::fs::remove_dir_all(&dir).ok();
    }
}
