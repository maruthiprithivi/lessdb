//! `less token` — mint and list agent credentials for the MCP door.
//!
//! The plaintext token is printed exactly once at creation; only its
//! SHA-256 hash is stored on disk (see `less-auth`'s token store).

use std::path::PathBuf;

use clap::Subcommand;
use less_common::Result;

#[derive(Subcommand)]
pub enum TokenCmd {
    /// Mint a token for an agent (prints the plaintext once)
    Create {
        /// Token name — becomes the agent's identity in the audit trail.
        name: String,
        /// Role: admin | read | write (default read).
        #[arg(long, default_value = "read")]
        role: String,
        /// Tenant this token may act in ("" = any tenant).
        #[arg(long, default_value = "")]
        tenant: String,
        /// Days until the token expires (omit for no expiry).
        #[arg(long)]
        expires_days: Option<u64>,
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
    },
    /// List token identities (hashes only — plaintext is never shown)
    List {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
    },
}

pub fn run_token(cmd: &TokenCmd) -> Result<()> {
    match cmd {
        TokenCmd::Create {
            name,
            role,
            tenant,
            expires_days,
            dir,
        } => {
            let mut store = less_auth::TokenStore::open(dir)?;
            let token = store.create(name, role, tenant, *expires_days)?;
            println!(
                "created token for '{name}' (role={role}, tenant={}) — copy it now, \
                 it is shown only once:\n\n{token}\n",
                if tenant.is_empty() { "*" } else { tenant },
            );
            Ok(())
        }
        TokenCmd::List { dir } => {
            let store = less_auth::TokenStore::open(dir)?;
            if store.is_empty() {
                println!("(no agent tokens)");
                return Ok(());
            }
            println!("name\trole\ttenant\texpires");
            for t in store.list() {
                let exp = t
                    .expires_at_ms
                    .map(|ms| {
                        let secs = ms / 1000;
                        format!("{secs}")
                    })
                    .unwrap_or_else(|| "never".to_string());
                let tenant = if t.tenant.is_empty() { "*" } else { &t.tenant };
                println!("{}\t{}\t{}\t{exp}", t.name, t.role, tenant);
            }
            Ok(())
        }
    }
}
