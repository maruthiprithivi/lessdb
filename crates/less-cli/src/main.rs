//! LessDB command-line interface.
//!
//! ```text
//! lessdb init                          initialize a database directory
//! less create "CREATE TABLE ..."      create a table
//! lessdb insert t --csv data.csv        bulk load
//! less sql "SELECT ..."               one-shot query (or interactive REPL)
//! lessdb server                         HTTP server (JSON + Arrow IPC SQL API)
//! lessdb mcp                            MCP server over stdio (for AI agents)
//! lessdb mcp --require-auth             MCP door, tokens required (control plane)
//! lessdb token create <name>            mint an agent token
//! lessdb audit                          the shared audit trail, newest first
//! lessdb bench                          built-in benchmark
//! lessdb context / less graph           in-memory context & graph storage
//! ```
//!
//! Synchronous commands (init/create/insert/optimize/...) run on the plain
//! main thread — the engine's embedded io-runtime does its own blocking —
//! while async commands (sql/server/mcp/bench) run inside an explicit tokio
//! runtime created here.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod bench;
mod commands;
mod context;
mod load;
mod tokens;
mod tui;
mod vectors;

#[derive(Parser)]
#[command(
    name = "lessdb",
    version,
    about = "LessDB — a fast, resource-efficient analytical database in Rust",
    long_about = "LessDB is a SQL-first analytical database in Rust: columnar storage with \
                  immutable-part engines (local and shared over object storage), a full SQL engine, \
                  vector search, in-memory context/graph storage for agents, and native MCP \
                  support — one binary.",
    after_help = "Run without a subcommand for the interactive TUI. \
License: MIT — https://lessdb.dev/docs/license (or `lessdb license`)."
)]
struct Cli {
    /// No subcommand opens the interactive TUI (defaults to `./.less`).
    #[command(subcommand)]
    cmd: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize a database directory
    Init {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        /// Shared storage URL for FireflyCloud tables (s3://bucket/prefix,
        /// gcs://..., az://..., file:///path). Compute/storage separation:
        /// all durable shared-table state lives there, local disk stays
        /// ephemeral compute scratch.
        #[arg(long, env = "LESSDB_SHARED")]
        shared: Option<String>,
        /// Authentication config as JSON: {"ldap": {...}} and/or
        /// {"file": {"path"/"users": {...}}}. Prefix with @ to read from a
        /// file, e.g. --auth @auth.json.
        #[arg(long)]
        auth: Option<String>,
        /// Query memory limit in bytes (DataFusion memory pool cap).
        #[arg(long, env = "LESSDB_MEMORY_LIMIT")]
        memory_limit: Option<usize>,
        /// Default column compression: zstd | lz4 | none.
        #[arg(long, env = "LESSDB_COMPRESSION", default_value = "zstd")]
        compression: String,
        /// zstd level (0 = library default, 1..=22).
        #[arg(long, env = "LESSDB_ZSTD_LEVEL", default_value_t = 3)]
        zstd_level: i32,
        /// Rows buffered in memory before a part is flushed.
        #[arg(long, env = "LESSDB_FLUSH_ROWS", default_value_t = 262_144)]
        flush_rows: usize,
        /// Cap on input rows per merge pass (bounds merge memory).
        #[arg(long, env = "LESSDB_MAX_MERGE_ROWS", default_value_t = 4_194_304)]
        max_merge_rows: usize,
        /// Shared-part block cache size in bytes.
        #[arg(long, env = "LESSDB_BLOCK_CACHE_BYTES", default_value_t = 268_435_456)]
        block_cache_bytes: usize,
        /// Disable the write-ahead log (insert durability).
        #[arg(long, env = "LESSDB_NO_WAL")]
        no_wal: bool,
    },
    /// Create a table from DDL
    Create {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        /// e.g. "CREATE TABLE t (a Int64, s String) ENGINE=Firefly ORDER BY (s)"
        ddl: String,
    },
    /// Drop a table
    Drop {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        table: String,
    },
    /// Insert rows from a CSV or JSONL file
    Insert {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        table: String,
        #[arg(long)]
        csv: Option<PathBuf>,
        #[arg(long)]
        jsonl: Option<PathBuf>,
        /// Insert from a Parquet file
        #[arg(long)]
        parquet: Option<PathBuf>,
        /// Insert from an Arrow IPC stream file
        #[arg(long)]
        arrow: Option<PathBuf>,
        #[arg(long, default_value_t = 262_144)]
        batch_rows: usize,
    },
    /// Run one SQL query (omit the query for an interactive shell)
    Sql {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        /// Output format: table|pretty|box|csv|tsv|json|jsoneachrow|vertical.
        #[arg(long, default_value = "table")]
        format: String,
        sql: Option<String>,
    },
    /// Full-screen TUI (panes, mouse, SQL editor) — also `lessdb sql`'s
    /// default in the ratatui preview is `lessdb tui`.
    Tui {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
    },
    /// List tables
    Tables {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
    },
    /// Row counts, part counts and on-disk size for one or all tables
    Stats {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        /// Table to report; all tables when omitted
        table: Option<String>,
    },
    /// Describe a table (schema, keys, parts, sizes)
    Describe {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        table: String,
    },
    /// List a table's data parts
    Parts {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        table: String,
    },
    /// Merge parts and enforce UNIQUE constraints (OPTIMIZE)
    Optimize {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        /// Table to optimize; all tables when omitted
        table: Option<String>,
    },
    /// Seed a demo database and run showcase queries (60-second tour)
    Demo {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        #[arg(long, default_value_t = 50_000)]
        rows: usize,
    },
    /// Run the built-in insert/query benchmark
    Bench {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        #[arg(long, default_value_t = 5_000_000)]
        rows: usize,
    },
    /// Self-upgrade to the latest published release (no brew/npm needed)
    Upgrade {
        /// Only report the current vs latest version, download nothing.
        #[arg(long)]
        check: bool,
        /// Reinstall even when already on the latest version.
        #[arg(long)]
        force: bool,
        /// Override the binary to replace (default: the running executable).
        #[arg(long)]
        install_dir: Option<PathBuf>,
        /// Override the download base (default: https://lessdb.dev/dl).
        #[arg(long, env = "LESSDB_DOWNLOAD_BASE")]
        base: Option<String>,
    },
    /// Start the HTTP server (JSON + Arrow IPC SQL API)
    Server {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        #[arg(long, default_value = "127.0.0.1:7080")]
        addr: String,
        /// PEM certificate chain for HTTPS (overrides config.json).
        #[arg(long)]
        tls_cert: Option<PathBuf>,
        /// PEM private key for HTTPS (overrides config.json).
        #[arg(long)]
        tls_key: Option<PathBuf>,
        /// Log filter (off|error|warn|info|debug|trace); overrides LESSDB_LOG.
        #[arg(long, env = "LESSDB_LOG", default_value = "info")]
        log_level: String,
        /// Write day-rotating logs to <dir>/lessdb.YYYY-MM-DD.log (7 kept).
        #[arg(long)]
        log_dir: Option<PathBuf>,
    },
    /// Run the MCP server over stdio (for AI agents)
    Mcp {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        /// Agent-memory tenant namespace (context/memory/vector state).
        #[arg(long, default_value = "default")]
        tenant: String,
        /// Fail-closed: require a valid agent token at `initialize` and
        /// enforce role-based tool permissions (admin/read/write).
        #[arg(long)]
        require_auth: bool,
    },
    /// Show the audit trail (newest first)
    Audit {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        /// Only entries at/after this age (e.g. 30m, 2h, 7d).
        #[arg(long, default_value = "24h")]
        since: String,
        /// Only entries from this caller.
        #[arg(long)]
        caller: Option<String>,
        /// Only entries with this outcome (ok | denied | error).
        #[arg(long)]
        outcome: Option<String>,
    },
    /// Manage agent tokens for the MCP door
    Token {
        #[command(subcommand)]
        cmd: tokens::TokenCmd,
    },
    /// Print the effective engine configuration
    Config {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
    },
    /// Block-cache statistics for shared storage
    Cache {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
    },
    /// Print Prometheus metrics for this process (useful after bench, or
    /// for debugging telemetry)
    Metrics {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
    },
    /// Manage in-memory context entities and graph links
    Context {
        #[command(subcommand)]
        cmd: context::ContextCmd,
    },
    /// In-memory SQL tables (point lookups, SQL, persistence)
    Memory {
        #[command(subcommand)]
        cmd: context::MemoryCmd,
    },
    /// Native vector search: spaces, k-NN, embeddings
    Vector {
        #[command(subcommand)]
        cmd: vectors::VectorCmd,
    },
    /// Print the MIT license text
    License,
    /// Query the context graph with openCypher
    Cypher {
        #[arg(long, env = "LESSDB_DIR", default_value = ".less")]
        dir: PathBuf,
        query: String,
    },
    /// Fan a query out across compute nodes (distributed execution v1:
    /// single-table count/sum/min/max queries sharded by part hash)
    Fanout {
        /// Comma-separated node base URLs (e.g. http://host1:7080,http://host2:7080).
        #[arg(long)]
        nodes: String,
        /// SQL query (fan-out v1 subset).
        sql: String,
    },
    /// GPU kernel benchmark (requires the `gpu` feature)
    #[cfg(feature = "gpu")]
    Gpu {
        #[arg(long, default_value_t = 4_000_000)]
        rows: usize,
    },
    /// Print version information
    Version,
}

fn main() {
    let cli = Cli::parse();
    let result = run(cli);
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> less_common::Result<()> {
    use Command::*;
    // Async commands get their own runtime; sync commands run on the plain
    // main thread so the engine's embedded io-runtime can block freely.
    let make_rt = || {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(8),
            )
            .thread_name("less-main")
            .enable_all()
            .build()
            .map_err(|e| less_common::LessError::Engine(format!("tokio: {e}")))
    };
    let Some(cmd) = cli.cmd else {
        return make_rt()?.block_on(commands::cmd_repl(std::path::Path::new(".less")));
    };
    match cmd {
        Version => {
            println!("lessdb {}", less_common::VERSION);
            Ok(())
        }
        Init {
            dir,
            shared,
            auth,
            memory_limit,
            compression,
            zstd_level,
            flush_rows,
            max_merge_rows,
            block_cache_bytes,
            no_wal,
        } => commands::cmd_init(
            &dir,
            shared.as_deref(),
            auth.as_deref(),
            memory_limit,
            commands::InitOverrides {
                compression,
                zstd_level,
                flush_rows,
                max_merge_rows,
                block_cache_bytes,
                no_wal,
            },
        ),
        Create { dir, ddl } => commands::cmd_create(&dir, &ddl),
        Drop { dir, table } => commands::cmd_drop(&dir, &table),
        Insert {
            dir,
            table,
            csv,
            jsonl,
            parquet,
            arrow,
            batch_rows,
        } => commands::cmd_insert(
            &dir,
            &table,
            csv.as_ref(),
            jsonl.as_ref(),
            parquet.as_ref(),
            arrow.as_ref(),
            batch_rows,
        ),
        Sql { dir, format, sql } => {
            let rt = make_rt()?;
            match sql {
                Some(sql) => rt.block_on(commands::cmd_sql(&dir, &sql, &format)),
                None => rt.block_on(commands::cmd_repl(&dir)),
            }
        }
        Tui { dir } => make_rt()?.block_on(tui::run(&dir)),
        Tables { dir } => commands::cmd_tables(&dir),
        Stats { dir, table } => commands::cmd_stats(&dir, table.as_deref()),
        Describe { dir, table } => commands::cmd_describe(&dir, &table),
        Parts { dir, table } => commands::cmd_parts(&dir, &table),
        Optimize { dir, table } => commands::cmd_optimize(&dir, table.as_deref()),
        Demo { dir, rows } => make_rt()?.block_on(commands::cmd_demo(&dir, rows)),
        Bench { dir, rows } => make_rt()?.block_on(commands::cmd_bench(&dir, rows)),
        Upgrade {
            check,
            force,
            install_dir,
            base,
        } => make_rt()?.block_on(commands::cmd_upgrade(
            check,
            force,
            install_dir.as_deref(),
            base.as_deref(),
        )),
        Server {
            dir,
            addr,
            tls_cert,
            tls_key,
            log_level,
            log_dir,
        } => {
            less_telemetry::logging::init_logging(&log_level, log_dir.as_deref())?;
            make_rt()?.block_on(commands::cmd_server(&dir, &addr, tls_cert, tls_key))
        }
        Mcp {
            dir,
            tenant,
            require_auth,
        } => make_rt()?.block_on(commands::cmd_mcp(&dir, &tenant, require_auth)),
        Audit {
            dir,
            since,
            caller,
            outcome,
        } => commands::cmd_audit(&dir, &since, caller.as_deref(), outcome.as_deref()),
        Token { cmd } => tokens::run_token(&cmd),
        Config { dir } => commands::cmd_config(&dir),
        Cache { dir } => commands::cmd_cache(&dir),
        Metrics { dir } => commands::cmd_metrics(&dir),
        Context { cmd } => context::run_context(&cmd),
        Memory { cmd } => context::run_memory(&cmd),
        Vector { cmd } => vectors::run_vector(&cmd),
        License => commands::cmd_license(),
        Cypher { dir, query } => commands::cmd_cypher(&dir, &query),
        Fanout { nodes, sql } => make_rt()?.block_on(commands::cmd_fanout(&nodes, &sql)),
        #[cfg(feature = "gpu")]
        Gpu { rows } => make_rt()?.block_on(commands::cmd_gpu_bench(rows)),
    }
}
