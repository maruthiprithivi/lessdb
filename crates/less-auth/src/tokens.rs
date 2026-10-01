//! Agent credentials: a small, file-backed token store for the MCP door.
//!
//! Tokens are minted by a human (`less token create`) and verified by the
//! MCP server. Only the SHA-256 hash is stored on disk, so a leaked data
//! dir does not leak credentials. Shape follows SEP-1046's client-credentials
//! idea (per-agent credentials, revocable) without depending on an external
//! authorization server yet.

use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use less_common::{LessError, ROLE_ADMIN, ROLE_READ, ROLE_WRITE, Result};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Plaintext token prefix.
pub const TOKEN_PREFIX: &str = "ldb_";

/// A stored agent credential (hash only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRecord {
    pub name: String,
    /// `admin` | `read` | `write`.
    pub role: String,
    /// Tenant namespace the token may act in ("" = all tenants).
    pub tenant: String,
    /// Epoch milliseconds; `None` = no expiry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<u64>,
    /// Hex-encoded SHA-256 of the plaintext token.
    pub hash: String,
}

impl TokenRecord {
    pub fn is_expired(&self, now_ms: u64) -> bool {
        self.expires_at_ms.is_some_and(|exp| now_ms >= exp)
    }
}

/// File-backed token store at `<data_dir>/auth/tokens.json`.
#[derive(Debug)]
pub struct TokenStore {
    path: PathBuf,
    tokens: Vec<TokenRecord>,
}

impl TokenStore {
    /// Load the store, creating an empty one when no file exists yet.
    pub fn open(data_dir: &Path) -> Result<Self> {
        let dir = data_dir.join("auth");
        fs::create_dir_all(&dir).map_err(LessError::Io)?;
        let path = dir.join("tokens.json");
        let tokens = if path.exists() {
            let text = fs::read_to_string(&path).map_err(LessError::Io)?;
            if text.trim().is_empty() {
                Vec::new()
            } else {
                serde_json::from_str(&text).map_err(LessError::Json)?
            }
        } else {
            Vec::new()
        };
        Ok(Self { path, tokens })
    }

    /// Mint a token. Returns the plaintext once (never stored).
    pub fn create(
        &mut self,
        name: &str,
        role: &str,
        tenant: &str,
        ttl_days: Option<u64>,
    ) -> Result<String> {
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        {
            return Err(LessError::Config(format!(
                "invalid token name '{name}' (expected [A-Za-z0-9_.-]+)"
            )));
        }
        if !matches!(role, ROLE_ADMIN | ROLE_READ | ROLE_WRITE) {
            return Err(LessError::Config(format!(
                "invalid role '{role}' (expected admin|read|write)"
            )));
        }
        if self.tokens.iter().any(|t| t.name == name) {
            return Err(LessError::Config(format!("token '{name}' already exists")));
        }

        let mut bytes = [0u8; 32];
        rand::thread_rng().fill(&mut bytes);
        let plaintext = format!("{TOKEN_PREFIX}{}", hex(&bytes));
        let expires_at_ms = ttl_days.map(|d| now_ms().saturating_add(d.saturating_mul(86_400_000)));

        self.tokens.push(TokenRecord {
            name: name.to_string(),
            role: role.to_string(),
            tenant: tenant.to_string(),
            expires_at_ms,
            hash: hex(&Sha256::digest(bytes)),
        });
        self.persist()?;
        Ok(plaintext)
    }

    /// Verify a plaintext token against the store (constant-time compare,
    /// expiry checked). Returns the matching record.
    pub fn verify(&self, token: &str) -> Option<&TokenRecord> {
        let hexpart = token.strip_prefix(TOKEN_PREFIX)?;
        let bytes = decode_hex(hexpart)?;
        let hash = hex(&Sha256::digest(&bytes));
        let now = now_ms();
        self.tokens.iter().find(|t| {
            if t.is_expired(now) {
                return false;
            }
            let Some(stored) = decode_hex(&t.hash) else {
                return false;
            };
            let Some(computed) = decode_hex(&hash) else {
                return false;
            };
            ct_eq(&stored, &computed)
        })
    }

    pub fn list(&self) -> &[TokenRecord] {
        &self.tokens
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    fn persist(&self) -> Result<()> {
        let tmp = self.path.with_extension("json.tmp");
        let mut f = fs::File::create(&tmp).map_err(LessError::Io)?;
        f.write_all(serde_json::to_string_pretty(&self.tokens)?.as_bytes())
            .map_err(LessError::Io)?;
        f.flush().map_err(LessError::Io)?;
        fs::rename(&tmp, &self.path).map_err(LessError::Io)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(s, "{b:02x}").unwrap();
    }
    s
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

/// Constant-time byte comparison.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static DIR_SEQ: AtomicUsize = AtomicUsize::new(0);

    fn temp_store() -> (TokenStore, PathBuf) {
        let n = DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("lessdb-token-test-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let store = TokenStore::open(&dir).unwrap();
        (store, dir)
    }

    #[test]
    fn create_and_verify_roundtrip() {
        let (mut store, dir) = temp_store();
        let token = store.create("claude", "read", "alice", None).unwrap();
        assert!(token.starts_with(TOKEN_PREFIX));
        let rec = store.verify(&token).expect("token must verify");
        assert_eq!(rec.name, "claude");
        assert_eq!(rec.role, "read");
        assert_eq!(rec.tenant, "alice");
        assert!(store.verify("ldb_deadbeef").is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn tokens_persist_and_expire() {
        let (mut store, dir) = temp_store();
        let token = store.create("exp", "write", "", Some(1)).unwrap();
        // Reload from disk: the token still verifies (not yet expired).
        let reloaded = TokenStore::open(&dir).unwrap();
        assert!(reloaded.verify(&token).is_some());
        // A 0-day TTL is expired immediately (now >= now + 0).
        let token2 = store.create("exp0", "write", "", Some(0)).unwrap();
        assert!(store.verify(&token2).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_bad_names_roles_dupes() {
        let (mut store, dir) = temp_store();
        assert!(store.create("bad name", "read", "", None).is_err());
        assert!(store.create("ok", "superuser", "", None).is_err());
        store.create("dup", "read", "", None).unwrap();
        assert!(store.create("dup", "read", "", None).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
