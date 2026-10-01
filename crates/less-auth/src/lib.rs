//! LessDB authentication: LDAP / Active Directory integration plus a dev
//! file authenticator.
//!
//! The [`Authenticator`] trait is the pluggable seam. The LDAP
//! implementation (`ldap3`) follows the standard AD flow:
//!
//! 1. connect (`ldap://`/`ldaps://`, optional StartTLS);
//! 2. bind as the service account (or anonymous);
//! 3. search the user by filter (`sAMAccountName={user}` by default);
//! 4. **rebind as the user** with the supplied password — this is the
//!    actual password verification, done by the directory;
//! 5. resolve group memberships and map group names to roles
//!    (`role_mapping`, fail closed unless `default_role` is set).
//!
//! Fail-closed by default: an authenticated user in no mapped group is
//! denied unless `default_role` grants one.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use ldap3::{LdapConnAsync, LdapConnSettings, Scope, SearchEntry, drive};

use less_common::{AuthConfig, FileAuthConfig, FileUser, LdapConfig, LessError, Result};

pub mod tokens;
pub use tokens::{TOKEN_PREFIX, TokenRecord, TokenStore};

/// An authenticated caller.
#[derive(Debug, Clone)]
pub struct User {
    pub name: String,
    pub groups: Vec<String>,
    /// `admin` | `read` | `write`.
    pub role: String,
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.role == less_common::ROLE_ADMIN
    }
    pub fn can_write(&self) -> bool {
        self.role == less_common::ROLE_ADMIN || self.role == less_common::ROLE_WRITE
    }
}

/// Verifies credentials and resolves a role.
#[async_trait]
pub trait Authenticator: Send + Sync {
    async fn authenticate(&self, username: &str, password: &str) -> Result<User>;
}

/// Build the configured authenticator(s). When both LDAP and file config
/// are present, LDAP is tried first and the file store second (useful for
/// local service accounts next to a directory).
pub fn build(config: &AuthConfig) -> Result<Option<Arc<dyn Authenticator>>> {
    if !config.is_enabled() {
        return Ok(None);
    }
    let mut chain: Vec<Arc<dyn Authenticator>> = vec![];
    if let Some(ldap) = &config.ldap {
        chain.push(Arc::new(LdapAuthenticator::new(ldap.clone())?));
    }
    if let Some(file) = &config.file {
        chain.push(Arc::new(FileAuthenticator::load(file)?));
    }
    Ok(Some(Arc::new(ChainAuthenticator { chain })))
}

/// Tries each authenticator in order.
struct ChainAuthenticator {
    chain: Vec<Arc<dyn Authenticator>>,
}

#[async_trait]
impl Authenticator for ChainAuthenticator {
    async fn authenticate(&self, username: &str, password: &str) -> Result<User> {
        let mut last_err: Option<LessError> = None;
        for auth in &self.chain {
            match auth.authenticate(username, password).await {
                Ok(user) => return Ok(user),
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or_else(|| LessError::Config("no authenticator configured".into())))
    }
}

// ===========================================================================
// LDAP / Active Directory
// ===========================================================================

/// LDAP/AD authenticator with a pooled directory connection: one live
/// `Ldap` handle is reused across requests (a fresh connection per request
/// cost a full TCP + bind handshake each time). The pooled handle is
/// dropped on any error (stale connections) and when service-account bind
/// is not configured (the handle would be left bound as a user).
pub struct LdapAuthenticator {
    cfg: LdapConfig,
    conn: tokio::sync::Mutex<Option<ldap3::Ldap>>,
}

impl LdapAuthenticator {
    pub fn new(cfg: LdapConfig) -> Result<Self> {
        if cfg.url.is_empty() || cfg.base_dn.is_empty() {
            return Err(LessError::Config(
                "ldap auth requires 'url' and 'base_dn'".into(),
            ));
        }
        Ok(Self {
            cfg,
            conn: tokio::sync::Mutex::new(None),
        })
    }

    async fn connect(&self) -> Result<ldap3::Ldap> {
        let settings = LdapConnSettings::new()
            .set_starttls(self.cfg.start_tls)
            .set_no_tls_verify(self.cfg.no_verify_tls);
        let (conn, ldap) = LdapConnAsync::with_settings(settings, &self.cfg.url)
            .await
            .map_err(|e| LessError::Query(format!("ldap connect failed: {e}")))?;
        drive!(conn);
        Ok(ldap)
    }
}

/// Escape a value for safe interpolation into an LDAP search filter
/// (RFC 4515): prevents filter injection from usernames.
pub fn escape_ldap_filter(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '*' => out.push_str("\\2a"),
            '(' => out.push_str("\\28"),
            ')' => out.push_str("\\29"),
            '\\' => out.push_str("\\5c"),
            '\0' => out.push_str("\\00"),
            c => out.push(c),
        }
    }
    out
}

/// Resolve group names to a role. Fail closed unless `default_role` set.
fn resolve_role(cfg: &LdapConfig, groups: &[String]) -> Option<String> {
    for group in groups {
        if let Some(role) = cfg.role_mapping.get(group) {
            return Some(role.clone());
        }
    }
    cfg.default_role.clone()
}

#[async_trait]
impl Authenticator for LdapAuthenticator {
    async fn authenticate(&self, username: &str, password: &str) -> Result<User> {
        let mut ldap = match self.conn.lock().await.take() {
            Some(ldap) => ldap,
            None => self.connect().await?,
        };
        let result = self.authenticate_on(&mut ldap, username, password).await;
        // Keep the handle only when the flow ended user-bound (service
        // account configured); otherwise close it.
        if result.is_ok() && self.cfg.bind_dn.is_some() {
            *self.conn.lock().await = Some(ldap);
        } else {
            let _ = ldap.unbind().await;
        }
        result
    }
}

impl LdapAuthenticator {
    async fn authenticate_on(
        &self,
        ldap: &mut ldap3::Ldap,
        username: &str,
        password: &str,
    ) -> Result<User> {
        let cfg = &self.cfg;

        // 1. Service-account (or anonymous) bind for the search.
        if let (Some(dn), Some(pw)) = (&cfg.bind_dn, &cfg.bind_password) {
            ldap.simple_bind(dn, pw)
                .await
                .map_err(|e| LessError::Query(format!("ldap bind failed: {e}")))?
                .success()
                .map_err(|e| LessError::Query(format!("ldap service bind rejected: {e}")))?;
        }

        // 2. Locate the user's DN.
        let filter = cfg
            .effective_user_filter()
            .replace("{user}", &escape_ldap_filter(username));
        let (entries, _res) = ldap
            .search(&cfg.base_dn, Scope::Subtree, &filter, vec!["dn"])
            .await
            .map_err(|e| LessError::Query(format!("ldap search failed: {e}")))?
            .success()
            .map_err(|e| LessError::Query(format!("ldap search rejected: {e}")))?;
        let entry = entries.into_iter().next();
        let dn = match entry {
            Some(entry) => SearchEntry::construct(entry).dn,
            None => {
                // Username does not exist; return the generic credential
                // error (don't leak which part failed).
                return Err(invalid_credentials());
            }
        };

        // 3. Verify the password: rebind as the user.
        match ldap.simple_bind(&dn, password).await {
            Ok(res) => {
                res.success().map_err(|_| invalid_credentials())?;
            }
            Err(_) => return Err(invalid_credentials()),
        }

        // 4. Resolve groups (search the directory for groups whose filter
        //    matches this user's DN).
        let group_filter = cfg
            .effective_group_filter()
            .replace("{dn}", &escape_ldap_filter(&dn));
        let group_attr = cfg.effective_group_attribute();
        let (group_entries, _res) = ldap
            .search(
                &cfg.base_dn,
                Scope::Subtree,
                &group_filter,
                vec![&group_attr],
            )
            .await
            .map_err(|e| LessError::Query(format!("ldap group search failed: {e}")))?
            .success()
            .map_err(|e| LessError::Query(format!("ldap group search rejected: {e}")))?;
        let mut groups: Vec<String> = group_entries
            .into_iter()
            .filter_map(|entry| {
                let entry = SearchEntry::construct(entry);
                entry
                    .attrs
                    .get(&group_attr)
                    .and_then(|v| v.first())
                    .cloned()
            })
            .collect();
        groups.sort();
        groups.dedup();

        let role = resolve_role(cfg, &groups).ok_or_else(|| {
            LessError::Query(format!("user '{username}' is not in any mapped group"))
        })?;
        Ok(User {
            name: username.to_string(),
            groups,
            role,
        })
    }
}

fn invalid_credentials() -> LessError {
    LessError::Query("invalid username or password".into())
}

// ===========================================================================
// File authenticator (dev / local accounts)
// ===========================================================================

/// Plain-text users file — development and tests only.
pub struct FileAuthenticator {
    users: BTreeMap<String, (FileUser, Vec<String>)>,
}

impl FileAuthenticator {
    pub fn load(cfg: &FileAuthConfig) -> Result<Self> {
        let mut users = cfg.users.clone();
        if let Some(path) = &cfg.path {
            let bytes = std::fs::read(path)?;
            let loaded: BTreeMap<String, FileUser> = serde_json::from_slice(&bytes)?;
            users.extend(loaded);
        }
        let users = users
            .into_iter()
            .map(|(name, u)| (name, (u.clone(), u.groups.clone())))
            .collect();
        Ok(Self { users })
    }

    /// Resolve a relative users-file path against the data directory.
    pub fn resolve_path(cfg: &mut FileAuthConfig, data_dir: &std::path::Path) {
        if let Some(path) = &cfg.path
            && path.is_relative()
        {
            cfg.path = Some(data_dir.join(path));
        }
    }
}

#[async_trait]
impl Authenticator for FileAuthenticator {
    async fn authenticate(&self, username: &str, password: &str) -> Result<User> {
        match self.users.get(username) {
            Some((user, groups)) if user.password == password => Ok(User {
                name: username.to_string(),
                groups: groups.clone(),
                role: user.role.clone(),
            }),
            _ => Err(invalid_credentials()),
        }
    }
}

/// Resolve `~` and relative paths for file auth config against the data dir.
pub fn normalize_config(config: &mut less_common::EngineConfig) {
    if let Some(auth) = &mut config.auth
        && let Some(file) = &mut auth.file
    {
        FileAuthenticator::resolve_path(file, &config.data_dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ldap_filter_escaping() {
        assert_eq!(escape_ldap_filter("simple"), "simple");
        assert_eq!(escape_ldap_filter("a*b"), "a\\2ab");
        assert_eq!(escape_ldap_filter("(x)"), "\\28x\\29");
        assert_eq!(escape_ldap_filter("a\\b"), "a\\5cb");
        assert_eq!(escape_ldap_filter("a\0b"), "a\\00b");
    }

    #[test]
    fn role_resolution_fails_closed() {
        let cfg: LdapConfig = serde_json::from_str(
            r#"{"url":"ldap://x","base_dn":"DC=x","role_mapping":{"Admins":"admin"}}"#,
        )
        .unwrap();
        assert_eq!(resolve_role(&cfg, &["Admins".into()]), Some("admin".into()));
        assert_eq!(resolve_role(&cfg, &["Users".into()]), None);

        let cfg: LdapConfig = serde_json::from_str(
            r#"{"url":"ldap://x","base_dn":"DC=x","role_mapping":{"Admins":"admin"},"default_role":"read"}"#,
        )
        .unwrap();
        assert_eq!(resolve_role(&cfg, &["Users".into()]), Some("read".into()));
    }

    #[tokio::test]
    async fn file_authenticator_verifies_and_roles() {
        let cfg: AuthConfig = serde_json::from_str(
            r#"{"file":{"users":{
                "alice": {"password": "pw1", "role": "admin", "groups": ["DB-Admins"]},
                "bob":   {"password": "pw2", "role": "read"}
            }}}"#,
        )
        .unwrap();
        let auth = build(&cfg).unwrap().unwrap();
        let user = auth.authenticate("alice", "pw1").await.unwrap();
        assert!(user.is_admin());
        assert!(user.can_write());
        assert_eq!(user.groups, vec!["DB-Admins".to_string()]);

        let user = auth.authenticate("bob", "pw2").await.unwrap();
        assert_eq!(user.role, "read");
        assert!(!user.can_write());

        assert!(auth.authenticate("alice", "wrong").await.is_err());
        assert!(auth.authenticate("carol", "x").await.is_err());
    }

    #[tokio::test]
    async fn file_auth_from_path() {
        let dir = std::env::temp_dir().join(format!("less-auth-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let users_path = dir.join("users.json");
        std::fs::write(
            &users_path,
            r#"{"svc": {"password": "s3cret", "role": "admin"}}"#,
        )
        .unwrap();
        let cfg: AuthConfig = serde_json::from_str(&format!(
            r#"{{"file":{{"path":"{}"}}}}"#,
            users_path.display()
        ))
        .unwrap();
        let auth = build(&cfg).unwrap().unwrap();
        assert!(auth.authenticate("svc", "s3cret").await.is_ok());
        assert!(auth.authenticate("svc", "nope").await.is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn no_auth_config_disables() {
        assert!(build(&AuthConfig::default()).unwrap().is_none());
    }
}
