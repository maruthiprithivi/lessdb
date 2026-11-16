//! Durable checkpoints of the LessDB data directory in Cloudflare R2.
//!
//! The container disk is ephemeral. To keep the town's long-term memory:
//! * On boot, before LessDB opens, the latest checkpoint archive is restored
//!   into the empty data directory. A network or extraction error aborts
//!   startup (the platform retries) instead of seeding a fresh world that
//!   would later overwrite the good checkpoint. Only a definite 404 means
//!   "no checkpoint yet".
//! * Periodically, with the simulation lock held, the world is persisted, every
//!   table is flushed (empties the WAL into immutable parts) and the directory
//!   is archived. The archive is uploaded as one object (S3 PutObject is
//!   atomic), plus an hourly copy for rollback.
//!
//! Transport: the Worker's key-protected `/internal/ckpt` route (native R2 binding).
//! The key goes through a 0600 curl config file, never argv.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone)]
pub struct R2 {
    base: String,
    key: String,
    label: String,
}

#[derive(Default, Clone, serde::Serialize)]
pub struct Status {
    pub enabled: bool,
    pub restored: Option<String>,
    pub last_ok_unix: Option<u64>,
    pub last_bytes: u64,
    pub uploads: u64,
    pub failures: u64,
    pub last_error: Option<String>,
    pub interval_secs: u64,
}

/// Checkpoints go through the Worker's `/internal/ckpt/<object>` route, which writes to
/// the `lessville-data` R2 bucket via its native binding. The container holds no R2 key;
/// it authenticates to the Worker with the same internal key used for model calls.
pub fn from_env() -> Option<R2> {
    let get = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
    Some(R2 {
        base: get("LESSVILLE_CKPT_URL")?.trim_end_matches('/').to_string(),
        key: get("LESSVILLE_INTERNAL_KEY").filter(|k| k.len() >= 16)?,
        label: get("LESSVILLE_CKPT_LABEL")
            .unwrap_or_else(|| "r2://lessville-data/lessville/v1".into()),
    })
}

impl R2 {
    fn url(&self, object: &str) -> String {
        format!("{}/{}", self.base, object)
    }

    /// Run curl with the internal key from a private config file; return the HTTP status.
    fn curl(&self, extra: &str) -> Result<u16, String> {
        let dir = std::env::temp_dir().join(format!("lv-ckpt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let cfg_path = dir.join("ckpt.cfg");
        let cfg = format!(
            "header = \"x-lessville-key: {}\"\nsilent\nshow-error\nmax-time = 120\nretry = 2\nwrite-out = \"%{{http_code}}\"\n{extra}",
            self.key
        );
        {
            let mut f = std::fs::File::create(&cfg_path).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = f.set_permissions(std::fs::Permissions::from_mode(0o600));
            }
            f.write_all(cfg.as_bytes()).map_err(|e| e.to_string())?;
        }
        let out = Command::new("curl")
            .arg("-K")
            .arg(&cfg_path)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("curl: {e}"));
        let _ = std::fs::remove_file(&cfg_path);
        let out = out?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let code: u16 = s
            .get(s.len().saturating_sub(3)..)
            .unwrap_or("")
            .parse()
            .unwrap_or(0);
        if code == 0 {
            return Err(format!(
                "checkpoint request failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(code)
    }

    /// Restore the latest checkpoint into an empty data dir. Ok(Some(desc)) if restored,
    /// Ok(None) if there is no checkpoint yet or the directory already holds data.
    pub fn restore(&self, data_dir: &Path) -> Result<Option<String>, String> {
        let has_data = std::fs::read_dir(data_dir.join("catalog"))
            .map(|mut d| d.next().is_some())
            .unwrap_or(false);
        if has_data {
            return Ok(None);
        }
        let tmp = std::env::temp_dir().join("lessville-restore.tar.gz");
        let _ = std::fs::remove_file(&tmp);
        let code = self.curl(&format!(
            "url = \"{}\"\noutput = \"{}\"\n",
            self.url("latest.tar.gz"),
            tmp.display()
        ))?;
        match code {
            200 => {}
            404 => return Ok(None),
            c => {
                return Err(format!(
                    "checkpoint GET latest.tar.gz returned HTTP {c}; refusing to start a fresh world over an unreadable checkpoint"
                ));
            }
        }
        let bytes = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
        let st = Command::new("tar")
            .arg("-xzf")
            .arg(&tmp)
            .arg("-C")
            .arg(data_dir)
            .status()
            .map_err(|e| e.to_string())?;
        if !st.success() {
            return Err("checkpoint archive failed to extract; refusing to start".into());
        }
        let _ = std::fs::remove_file(&tmp);
        Ok(Some(format!(
            "restored {bytes} bytes from {}/latest.tar.gz",
            self.label
        )))
    }

    /// Archive the (already flushed) data dir to a local file. Call with writers paused.
    pub fn archive(data_dir: &Path) -> Result<PathBuf, String> {
        let out = std::env::temp_dir().join("lessville-checkpoint.tar.gz");
        let st = Command::new("tar")
            .arg("--exclude=./.lessdb.lock")
            .arg("-czf")
            .arg(&out)
            .arg("-C")
            .arg(data_dir)
            .arg(".")
            .status()
            .map_err(|e| e.to_string())?;
        if !st.success() {
            return Err("tar failed".into());
        }
        Ok(out)
    }

    pub fn upload(&self, file: &Path, object: &str) -> Result<(), String> {
        let code = self.curl(&format!(
            "url = \"{}\"\nupload-file = \"{}\"\nrequest = \"PUT\"\nheader = \"content-type: application/gzip\"\n",
            self.url(object),
            file.display()
        ))?;
        if code == 200 || code == 201 {
            Ok(())
        } else {
            Err(format!("checkpoint PUT {object} returned HTTP {code}"))
        }
    }
}
