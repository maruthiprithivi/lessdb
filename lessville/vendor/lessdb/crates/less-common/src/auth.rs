//! Authentication configuration (LDAP / Active Directory / local file).
//!
//! The config types live here (dependency-free) so `EngineConfig` can carry
//! them; the client implementations live in `less-auth`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Well-known roles. Enforcement points may extend these; the strings are
/// the contract.
pub const ROLE_ADMIN: &str = "admin";
pub const ROLE_READ: &str = "read";
pub const ROLE_WRITE: &str = "write";

/// LDAP / Active Directory authentication settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LdapConfig {
    /// `ldap://host:389` or `ldaps://host:636`.
    pub url: String,
    /// Base DN for user lookups, e.g. `DC=corp,DC=example,DC=com`.
    pub base_dn: String,
    /// Service account DN for the search bind (anonymous when omitted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind_dn: Option<String>,
    /// Service account password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind_password: Option<String>,
    /// Search filter to locate the user; `{user}` is substituted with the
    /// escaped login name. Default: `(sAMAccountName={user})` (AD).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_filter: Option<String>,
    /// Group search filter; `{dn}` is substituted with the user's DN.
    /// Default: `(member={dn})` (AD groups).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_filter: Option<String>,
    /// Attribute carrying the group *name* used for role mapping.
    /// Default: `cn`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_attribute: Option<String>,
    /// Map group name -> role (`admin` | `read` | `write`).
    #[serde(default)]
    pub role_mapping: BTreeMap<String, String>,
    /// Role granted when the user authenticates but is in no mapped group.
    /// `None` denies access (default: fail closed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_role: Option<String>,
    /// Upgrade `ldap://` connections with StartTLS.
    #[serde(default)]
    pub start_tls: bool,
    /// Skip certificate verification (development only!).
    #[serde(default)]
    pub no_verify_tls: bool,
}

impl LdapConfig {
    pub fn effective_user_filter(&self) -> String {
        self.user_filter
            .clone()
            .unwrap_or_else(|| "(sAMAccountName={user})".to_string())
    }
    pub fn effective_group_filter(&self) -> String {
        self.group_filter
            .clone()
            .unwrap_or_else(|| "(member={dn})".to_string())
    }
    pub fn effective_group_attribute(&self) -> String {
        self.group_attribute
            .clone()
            .unwrap_or_else(|| "cn".to_string())
    }
}

/// One user in the dev-mode file authenticator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileUser {
    pub password: String,
    /// `admin` | `read` | `write`.
    pub role: String,
    #[serde(default)]
    pub groups: Vec<String>,
}

/// Dev/local authentication from a JSON file (no LDAP server needed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAuthConfig {
    /// Path to a JSON file mapping username -> FileUser. Relative paths
    /// resolve against the data directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// Inline users (alternative to `path`; dev only — plaintext).
    #[serde(default)]
    pub users: BTreeMap<String, FileUser>,
}

/// Authentication configuration attached to the engine config.
///
/// Example (`config.json`):
/// ```json
/// "auth": {
///   "ldap": {
///     "url": "ldaps://ad.corp.example.com:636",
///     "base_dn": "DC=corp,DC=example,DC=com",
///     "bind_dn": "CN=lessdb-svc,OU=Services,DC=corp,DC=example,DC=com",
///     "bind_password": "…",
///     "role_mapping": { "DB-Admins": "admin", "DB-Users": "read", "DB-Writers": "write" }
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ldap: Option<LdapConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<FileAuthConfig>,
}

impl AuthConfig {
    /// Is any authenticator configured?
    pub fn is_enabled(&self) -> bool {
        self.ldap.is_some() || self.file.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_fail_closed() {
        let cfg: LdapConfig = serde_json::from_str(
            r#"{"url": "ldap://x", "base_dn": "DC=x", "role_mapping": {"Admins": "admin"}}"#,
        )
        .unwrap();
        assert_eq!(cfg.effective_user_filter(), "(sAMAccountName={user})");
        assert_eq!(cfg.effective_group_filter(), "(member={dn})");
        assert_eq!(cfg.effective_group_attribute(), "cn");
        assert!(cfg.default_role.is_none());
        assert!(!cfg.start_tls && !cfg.no_verify_tls);
    }

    #[test]
    fn auth_config_parse() {
        let json = r#"{
          "ldap": {"url": "ldaps://ad", "base_dn": "DC=x", "role_mapping": {"DB-Admins": "admin"}},
          "file": {"users": {"alice": {"password": "pw", "role": "admin"}}}
        }"#;
        let cfg: AuthConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.is_enabled());
        assert_eq!(cfg.ldap.unwrap().role_mapping["DB-Admins"], "admin");
        assert_eq!(cfg.file.unwrap().users["alice"].role, "admin");
    }
}
