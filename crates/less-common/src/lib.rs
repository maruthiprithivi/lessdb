//! LessDB shared foundations: errors and engine configuration.

pub mod auth;
pub mod config;
pub mod error;

pub use auth::{
    AuthConfig, FileAuthConfig, FileUser, LdapConfig, ROLE_ADMIN, ROLE_READ, ROLE_WRITE,
};
pub use config::EngineConfig;
pub use error::{LessError, Result};

/// LessDB semantic version, kept in sync with the workspace manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
