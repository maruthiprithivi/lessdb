//! Write-ahead log for buffered inserts.
//!
//! Records are `[len u64][lsn u64][arrow-ipc batch]`. Recovery validates every
//! record, including those covered by existing parts, before accepting writes.
//! Invalid framing fails startup closed with the original WAL unchanged.
//! No automatic tail repair or checksum format is provided. File fsync remains
//! configurable; parent-directory/part publication barriers and multipart
//! atomicity are separate unresolved durability requirements on this baseline.
//! Process-restart tests do not establish physical power-loss safety.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use arrow::ipc::reader::StreamReader;
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;

use less_common::{LessError, Result};

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

    /// Replay records above the committed part cutoff; reject malformed tails.
    pub fn replay(&self, table: &str, min_lsn: u64) -> Result<Vec<RecordBatch>> {
        let path = self.path_for(table);
        if !path.exists() {
            return Ok(vec![]);
        }
        let mut file = File::open(&path)?;
        let file_len = file.metadata()?.len();
        let mut out = vec![];
        while file.stream_position()? < file_len {
            let offset = file.stream_position()?;
            let invalid = |reason: &str| {
                LessError::Engine(format!(
                    "invalid WAL for table {table} at byte {offset}: {reason}; preserve the log for audited recovery"
                ))
            };
            let mut len_buf = [0u8; 8];
            match file.read_exact(&mut len_buf) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Err(invalid("incomplete length header"));
                }
                Err(e) => return Err(e.into()),
            }
            let len = u64::from_le_bytes(len_buf);
            let mut lsn_buf = [0u8; 8];
            file.read_exact(&mut lsn_buf)
                .map_err(|_| invalid("incomplete sequence header"))?;
            let lsn = u64::from_le_bytes(lsn_buf);
            // A length prefix that cannot fit in the bytes physically left in
            // the file is a torn/garbage tail (crash mid-append, or a bogus
            // prefix). Reject it before attempting a payload allocation.
            let pos = file.stream_position()?;
            if len > file_len.saturating_sub(pos) {
                return Err(invalid("payload length exceeds remaining file bytes"));
            }
            let len = usize::try_from(len).map_err(|_| invalid("payload length is unsupported"))?;
            let mut payload = vec![0u8; len];
            file.read_exact(&mut payload)
                .map_err(|_| invalid("incomplete payload"))?;
            // Without a validated boundary/checksum we cannot safely infer
            // that an invalid record was unacknowledged. In particular, a
            // writable reopen must never append behind a malformed tail.
            let batches =
                ipc_to_batches(&payload).map_err(|_| invalid("invalid Arrow IPC payload"))?;
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
    // Our StreamWriter always finishes with the modern IPC EOS marker. Arrow
    // also accepts EOF without EOS, which is too permissive for WAL recovery.
    const EOS: [u8; 8] = [255, 255, 255, 255, 0, 0, 0, 0];
    if !bytes.ends_with(&EOS) {
        return Err(LessError::Engine(
            "WAL IPC stream is missing its end marker".into(),
        ));
    }
    let mut cursor = std::io::Cursor::new(bytes);
    let batches = {
        let reader = StreamReader::try_new(&mut cursor, None)?;
        reader.collect::<std::result::Result<Vec<_>, _>>()?
    };
    // Arrow stops at EOS. Reject extra bytes that an expanded outer length
    // could otherwise use to swallow a following acknowledged WAL record.
    if cursor.position() != bytes.len() as u64 {
        return Err(LessError::Engine(
            "WAL IPC stream has trailing bytes".into(),
        ));
    }
    Ok(batches)
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
    fn truncated_tail_record_is_rejected_without_modifying_the_log() {
        let dir = std::env::temp_dir().join(format!("less-walt-{}", uuid::Uuid::new_v4()));
        let wal = Wal::open(&dir, false).unwrap();
        wal.append("t", 1, &batch(vec![1, 2])).unwrap();
        wal.append("t", 2, &batch(vec![3])).unwrap();
        truncate_tail(&wal.path_for("t"));
        let before = std::fs::read(wal.path_for("t")).unwrap();
        assert!(wal.replay("t", 0).is_err());
        assert_eq!(std::fs::read(wal.path_for("t")).unwrap(), before);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ipc_missing_end_marker_is_rejected() {
        let payload = batch_to_ipc(&batch(vec![1])).unwrap();
        assert!(ipc_to_batches(&payload).is_ok());
        assert!(ipc_to_batches(&payload[..payload.len() - 8]).is_err());
    }

    #[test]
    fn expanded_record_cannot_swallow_the_next_record() {
        use std::io::SeekFrom;
        let dir = std::env::temp_dir().join(format!("less-wal-expanded-{}", uuid::Uuid::new_v4()));
        let wal = Wal::open(&dir, true).unwrap();
        wal.append("t", 1, &batch(vec![1])).unwrap();
        wal.append("t", 2, &batch(vec![2])).unwrap();
        let mut file = OpenOptions::new()
            .write(true)
            .open(wal.path_for("t"))
            .unwrap();
        let expanded_len = file.metadata().unwrap().len() - 16;
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(&expanded_len.to_le_bytes()).unwrap();
        file.sync_all().unwrap();
        drop(file);
        let before = std::fs::read(wal.path_for("t")).unwrap();
        assert!(wal.replay("t", 0).is_err());
        assert_eq!(std::fs::read(wal.path_for("t")).unwrap(), before);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_tail_after_covered_records_is_rejected() {
        for tail in [
            vec![1],
            [8u64.to_le_bytes().as_slice(), &[0; 4]].concat(),
            [
                100u64.to_le_bytes().as_slice(),
                2u64.to_le_bytes().as_slice(),
            ]
            .concat(),
            [
                3u64.to_le_bytes().as_slice(),
                2u64.to_le_bytes().as_slice(),
                b"bad",
            ]
            .concat(),
        ] {
            let dir =
                std::env::temp_dir().join(format!("less-wal-invalid-{}", uuid::Uuid::new_v4()));
            let wal = Wal::open(&dir, false).unwrap();
            wal.append("t", 1, &batch(vec![1])).unwrap();
            let mut file = OpenOptions::new()
                .append(true)
                .open(wal.path_for("t"))
                .unwrap();
            file.write_all(&tail).unwrap();
            let before = std::fs::read(wal.path_for("t")).unwrap();
            assert!(wal.replay("t", 1).is_err());
            assert_eq!(std::fs::read(wal.path_for("t")).unwrap(), before);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
