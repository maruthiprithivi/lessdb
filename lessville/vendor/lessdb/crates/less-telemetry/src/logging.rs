//! Structured logging for the server/CLI: `tracing` with an optional
//! day-rotating file appender.
//!
//! * `LESSDB_LOG` env var sets the filter (default `info`, e.g.
//!   `lessdb=debug` or `debug`), overridden by the `--log-level` flag.
//! * `--log-dir <dir>` writes `lessdb.YYYY-MM-DD.log` files, rotated
//!   daily with 7 days retention; stdout stays human-readable regardless.

use std::path::Path;
use std::sync::OnceLock;

use less_common::Result;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

static _GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

/// Initialize the process logger. Call once, before doing any work.
/// `level` (e.g. "info", "debug") overrides the `LESSDB_LOG` env var;
/// when `log_dir` is Some, a daily-rotating file appender (7 days kept)
/// is added alongside stdout.
pub fn init_logging(level: &str, log_dir: Option<&Path>) -> Result<()> {
    if _GUARD.get().is_some() {
        return Ok(()); // already initialized (idempotent for tests/reloads)
    }
    let filter = EnvFilter::try_new(level)
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("info"));
    let filter_str = filter.to_string();

    let stdout_fmt = Layer::new()
        .with_writer(std::io::stdout)
        .with_target(false)
        .compact();

    if let Some(dir) = log_dir {
        std::fs::create_dir_all(dir)?;
        let file = tracing_appender::rolling::Builder::new()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix("lessdb")
            .filename_suffix("log")
            .max_log_files(7)
            .build(dir)
            .map_err(|e| less_common::LessError::Config(format!("log dir: {e}")))?;
        let (writer, guard) = tracing_appender::non_blocking(file);
        let _ = _GUARD.set(guard); // keep the writer flushing for process lifetime
        let file_fmt = Layer::new()
            .with_writer(writer)
            .with_ansi(false)
            .with_target(false)
            .compact();
        tracing_subscriber::registry()
            .with(filter)
            .with(stdout_fmt)
            .with(file_fmt)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter.clone())
            .with(stdout_fmt)
            .init();
    }
    tracing::info!(filter = %filter_str, "logging initialized");
    Ok(())
}
