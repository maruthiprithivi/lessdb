//! Append-only NDJSON audit log — the shared trail behind both front doors.
//!
//! Every authenticated action (human or agent) is recorded before it
//! returns: caller, door, action, tool, role, outcome, and cost. One file
//! per day (`audit-YYYY-MM-DD.ndjson`) under `<data_dir>/audit/`; the file
//! is flushed per record so a crash never loses an accepted action.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use less_common::{LessError, Result};
use serde::{Deserialize, Serialize};

/// Directory name for the audit trail under the data dir.
pub const AUDIT_DIR: &str = "audit";

/// One recorded action. Serialized as a single NDJSON line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// RFC 3339 timestamp (UTC).
    pub ts: String,
    /// `agent` | `human` | `system`.
    pub caller_kind: String,
    /// Caller name (agent token name, LDAP user, …).
    pub caller: String,
    /// Tenant namespace, when applicable.
    pub tenant: String,
    /// Which surface the action came through: `mcp` | `http` | `cli` | `sdk`.
    pub door: String,
    /// e.g. `tools/call`, `sql`, `token/create`.
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// `ok` | `denied` | `error`.
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows_scanned: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dur_ms: Option<f64>,
}

impl AuditEntry {
    pub fn new(caller_kind: &str, caller: &str, tenant: &str, door: &str, action: &str) -> Self {
        Self {
            ts: Utc::now().to_rfc3339(),
            caller_kind: caller_kind.to_string(),
            caller: caller.to_string(),
            tenant: tenant.to_string(),
            door: door.to_string(),
            action: action.to_string(),
            tool: None,
            role: None,
            outcome: "ok".to_string(),
            detail: None,
            rows_scanned: None,
            dur_ms: None,
        }
    }
    pub fn tool(mut self, tool: &str) -> Self {
        self.tool = Some(tool.to_string());
        self
    }
    pub fn role(mut self, role: &str) -> Self {
        self.role = Some(role.to_string());
        self
    }
    pub fn outcome(mut self, outcome: &str) -> Self {
        self.outcome = outcome.to_string();
        self
    }
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
    pub fn rows_scanned(mut self, n: u64) -> Self {
        self.rows_scanned = Some(n);
        self
    }
    pub fn dur_ms(mut self, ms: f64) -> Self {
        self.dur_ms = Some(ms);
        self
    }
}

/// Day-rotating NDJSON writer. Cheap to clone-arc and share across threads.
pub struct AuditLog {
    dir: PathBuf,
    file: Mutex<Option<(String, File)>>,
}

impl AuditLog {
    /// Open (creating) the audit directory. A record is written synchronously
    /// before the caller's action returns; denials are never lost.
    pub fn open(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir).map_err(LessError::Io)?;
        Ok(Self {
            dir: dir.to_path_buf(),
            file: Mutex::new(None),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn record(&self, entry: &AuditEntry) -> Result<()> {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let mut guard = self.file.lock().unwrap();
        let rotate = match &*guard {
            Some((day, _)) => day != &today,
            None => true,
        };
        if rotate {
            let path = self.dir.join(format!("audit-{today}.ndjson"));
            let f = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(LessError::Io)?;
            *guard = Some((today, f));
        }
        let (_, f) = guard.as_mut().expect("file opened above");
        writeln!(f, "{}", serde_json::to_string(entry)?).map_err(LessError::Io)?;
        f.flush().map_err(LessError::Io)
    }
}

/// A parsed audit line for display.
#[derive(Debug)]
pub struct AuditLine {
    pub file: String,
    pub ts: String,
    pub entry: AuditEntry,
}

/// Read audit lines, newest first, optionally filtered to entries at or
/// after `since` (epoch milliseconds).
pub fn query(dir: &Path, since_ms: Option<u64>) -> Result<Vec<AuditLine>> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(LessError::Io)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("audit-") && n.ends_with(".ndjson"))
        })
        .collect();
    files.sort();
    files.reverse(); // newest day first

    let mut out = Vec::new();
    for path in files {
        let text = fs::read_to_string(&path).map_err(LessError::Io)?;
        let file = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        for line in text.lines().rev() {
            if line.trim().is_empty() {
                continue;
            }
            let entry: AuditEntry = serde_json::from_str(line).map_err(LessError::Json)?;
            if let Some(since) = since_ms {
                let keep = DateTime::parse_from_rfc3339(&entry.ts)
                    .map(|t| t.timestamp_millis() >= since as i64)
                    .unwrap_or(true); // unparsable ts: keep (visible, not dropped)
                if !keep {
                    continue;
                }
            }
            out.push(AuditLine {
                file: file.clone(),
                ts: entry.ts.clone(),
                entry,
            });
        }
    }
    Ok(out)
}

/// Parse a CLI-style duration (`30s`, `15m`, `2h`, `7d`) into epoch ms of
/// `now - duration`.
pub fn parse_since(spec: &str, now_ms: u64) -> Result<u64> {
    let (num, unit) = spec.split_at(spec.len().saturating_sub(1));
    let n: u64 = num
        .parse()
        .map_err(|_| LessError::Config(format!("invalid duration '{spec}'")))?;
    let ms_per = match unit {
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        _ => {
            return Err(LessError::Config(format!(
                "invalid duration unit in '{spec}' (use s/m/h/d)"
            )));
        }
    };
    Ok(now_ms.saturating_sub(n.saturating_mul(ms_per)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_query_roundtrip() {
        let dir = std::env::temp_dir().join(format!("lessdb-audit-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let log = AuditLog::open(&dir).unwrap();
        let entry = AuditEntry::new("agent", "claude", "alice", "mcp", "tools/call")
            .tool("lessdb_query")
            .role("read")
            .detail("SELECT 1");
        log.record(&entry).unwrap();

        let lines = query(&dir, None).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].entry.caller, "claude");
        assert_eq!(lines[0].entry.outcome, "ok");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn since_filter_drops_old_entries() {
        let dir = std::env::temp_dir().join(format!("lessdb-audit-since-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let log = AuditLog::open(&dir).unwrap();
        let old = AuditEntry::new("human", "alex", "", "cli", "sql");
        log.record(&old).unwrap();
        let now = Utc::now().timestamp_millis() as u64;
        let lines = query(&dir, Some(now + 5_000)).unwrap(); // only "future" entries
        assert!(lines.is_empty());
        let lines = query(&dir, Some(now.saturating_sub(5_000))).unwrap();
        assert_eq!(lines.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_since_units() {
        assert_eq!(parse_since("30s", 100_000).unwrap(), 100_000 - 30_000);
        assert_eq!(parse_since("2h", 7_200_000).unwrap(), 0);
        assert!(parse_since("5x", 0).is_err());
    }
}
