//! CLI command implementations.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::util::pretty::print_batches;
use less_common::{EngineConfig, LessError, Result};
use less_engine::LessEngine;
use less_query::LessSession;

use crate::load::{load_arrow, load_csv, load_jsonl, load_parquet_stream};
use less_catalog::ddl::parse_create;

/// Open an engine rooted at `dir`, honoring a persisted `config.json`
/// (written by `less init --shared <url>`).
/// Print the MIT license text (also at https://lessdb.dev/docs/license).
pub fn cmd_license() -> Result<()> {
    println!(
        "LessDB — MIT License\n\
         Copyright (c) 2025-2026 LessDB Project and LessDB contributors\n\n\
         Permission is hereby granted, free of charge, to any person obtaining a copy\n\
         of this software and associated documentation files (the \"Software\"), to deal\n\
         in the Software without restriction, including without limitation the rights\n\
         to use, copy, modify, merge, publish, distribute, sublicense, and/or sell\n\
         copies of the Software, and to permit persons to whom the Software is\n\
         furnished to do so, subject to the following conditions:\n\n\
         The above copyright notice and this permission notice shall be included in all\n\
         copies or substantial portions of the Software.\n\n\
         THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR\n\
         IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,\n\
         FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE\n\
         AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER\n\
         LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,\n\
         OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE\n\
         SOFTWARE.\n\n\
         Full text: https://lessdb.dev/docs/license"
    );
    Ok(())
}

pub fn open_engine(dir: &Path) -> Result<Arc<LessEngine>> {
    LessEngine::open(EngineConfig::load_or_default(dir))
}

/// Open an engine + session rooted at `dir` (async; for use inside runtimes).
pub async fn open_session_async(dir: &Path) -> Result<(Arc<LessEngine>, LessSession)> {
    let engine = open_engine(dir)?;
    let session = LessSession::new_async(engine.clone()).await?;
    Ok((engine, session))
}

/// Open an engine + session rooted at `dir` (sync; for embedded callers).
#[allow(dead_code)]
pub fn open_session(dir: &Path) -> Result<(Arc<LessEngine>, LessSession)> {
    let engine = open_engine(dir)?;
    let session = LessSession::new(engine.clone())?;
    Ok((engine, session))
}

/// Print result batches as a table.
pub fn print(batches: &[arrow::record_batch::RecordBatch]) -> Result<()> {
    if batches.is_empty() || batches.iter().all(|b| b.num_rows() == 0) {
        println!("(0 rows)");
        return Ok(());
    }
    print_batches(batches)?;
    Ok(())
}

/// Extra storage/durability knobs `less init` can persist into config.json.
pub struct InitOverrides {
    pub compression: String,
    pub zstd_level: i32,
    pub flush_rows: usize,
    pub max_merge_rows: usize,
    pub block_cache_bytes: usize,
    pub no_wal: bool,
}

pub fn cmd_init(
    dir: &Path,
    shared: Option<&str>,
    auth: Option<&str>,
    memory_limit: Option<usize>,
    overrides: InitOverrides,
) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut config = EngineConfig::load_or_default(dir);
    if let Some(url) = shared {
        config.shared_url = Some(url.to_string());
    }
    if let Some(limit) = memory_limit {
        config.memory_limit = limit;
    }
    config.compression = overrides.compression;
    config.zstd_level = overrides.zstd_level;
    config.flush_rows = overrides.flush_rows;
    config.max_merge_rows = overrides.max_merge_rows;
    config.block_cache_bytes = overrides.block_cache_bytes;
    if overrides.no_wal {
        config.wal_enabled = false;
    }
    config.validate()?;
    if let Some(auth_src) = auth {
        let json = if let Some(path) = auth_src.strip_prefix('@') {
            std::fs::read_to_string(path)?
        } else {
            auth_src.to_string()
        };
        config.auth = Some(
            serde_json::from_str(&json)
                .map_err(|e| LessError::Config(format!("invalid --auth JSON: {e}")))?,
        );
    }
    config.save()?;
    let engine = open_engine(dir)?;
    println!("LessDB initialized at {}", dir.display());
    match &config.shared_url {
        Some(url) => println!(
            "  shared storage: {url}  (FireflyCloud state is cloud-side; local disk = compute scratch)"
        ),
        None => println!(
            "  shared storage: {}/shared (local dev mode)",
            dir.display()
        ),
    }
    println!("  tables: {}", engine.tables()?.join(", "));
    if config.auth.as_ref().is_some_and(|a| a.is_enabled()) {
        println!("  auth: enabled (LDAP/AD or file)");
    }
    if config.memory_limit > 0 {
        println!(
            "  query memory limit: {} bytes ({:.1} MiB)",
            config.memory_limit,
            config.memory_limit as f64 / (1 << 20) as f64
        );
    }
    Ok(())
}

pub fn cmd_create(dir: &Path, ddl: &str) -> Result<()> {
    let engine = open_engine(dir)?;
    let parsed = parse_create(ddl)?;
    let def = parsed.to_def();
    match engine.create_table(def.clone()) {
        Ok(()) => {
            println!("created table {}", def.name);
            for f in &def.schema.fields {
                println!(
                    "  {} {} {}{}",
                    f.name,
                    f.ty.name(),
                    if f.nullable { "" } else { "NOT NULL " },
                    if def.sort_key.contains(&f.name) {
                        "(key)"
                    } else {
                        ""
                    }
                );
            }
            if !def.unique.is_empty() {
                println!("  UNIQUE {}", def.unique.join(", "));
            }
            Ok(())
        }
        Err(e) => {
            if parsed.if_not_exists && e.to_string().contains("already exists") {
                println!("table {} already exists (IF NOT EXISTS)", def.name);
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

pub fn cmd_drop(dir: &Path, table: &str) -> Result<()> {
    let engine = open_engine(dir)?;
    engine.drop_table(table)?;
    println!("dropped table {table}");
    Ok(())
}

pub fn cmd_tables(dir: &Path) -> Result<()> {
    let engine = open_engine(dir)?;
    for t in engine.tables()? {
        println!("{t}");
    }
    Ok(())
}

pub fn cmd_describe(dir: &Path, table: &str) -> Result<()> {
    let engine = open_engine(dir)?;
    let def = engine.table(table)?;
    println!(
        "table {}  engine={}  compression={}",
        def.name,
        match def.engine {
            less_catalog::EngineKind::Firefly => "Firefly",
            less_catalog::EngineKind::FireflyCloud => "FireflyCloud",
        },
        def.effective_compression(&engine.config.compression)
    );
    if !def.sort_key.is_empty() {
        println!("sort key: {}", def.sort_key.join(", "));
    }
    if !def.unique.is_empty() {
        println!("unique:   {}", def.unique.join(", "));
    }
    let stats = engine.stats(table)?;
    println!(
        "rows: {}  parts: {}  disk: {:.2} MB",
        stats.rows,
        stats.part_count,
        stats.disk_bytes as f64 / 1e6
    );
    println!("columns:");
    for f in &def.schema.fields {
        println!("  {}  {}", f.name, f.ty.name());
    }
    Ok(())
}

pub fn cmd_parts(dir: &Path, table: &str) -> Result<()> {
    let engine = open_engine(dir)?;
    for part in engine.parts(table)? {
        println!("{}", part.summary());
    }
    Ok(())
}

pub fn cmd_optimize(dir: &Path, table: Option<&str>) -> Result<()> {
    let engine = open_engine(dir)?;
    match table {
        Some(t) => {
            let merged = engine.optimize(t)?;
            match merged {
                Some(m) => println!("merged into part {} ({} rows)", m.name, m.row_count),
                None => println!("table {t} has fewer than 2 parts; nothing to merge"),
            }
        }
        None => {
            for t in engine.tables()? {
                if engine.optimize(&t)?.is_some() {
                    println!("optimized {t}");
                }
            }
        }
    }
    Ok(())
}

pub fn cmd_insert(
    dir: &Path,
    table: &str,
    csv: Option<&PathBuf>,
    jsonl: Option<&PathBuf>,
    parquet: Option<&PathBuf>,
    arrow: Option<&PathBuf>,
    batch_rows: usize,
) -> Result<()> {
    let engine = open_engine(dir)?;
    let def = engine.table(table)?;
    let mut rows = 0usize;
    // Parquet streams row-group batches straight into the buffer (bounded
    // memory even for 100M-row files); the other formats load fully.
    if let (None, None, Some(path), None) = (csv, jsonl, parquet, arrow) {
        for batch in load_parquet_stream(&def, path, batch_rows)? {
            rows += engine.insert(table, batch?)?;
        }
    } else {
        let batches = match (csv, jsonl, parquet, arrow) {
            (Some(path), None, None, None) => load_csv(&def, path, batch_rows)?,
            (None, Some(path), None, None) => load_jsonl(&def, path, batch_rows)?,
            (None, None, Some(_path), None) => unreachable!("handled above"),
            (None, None, None, Some(path)) => load_arrow(&def, path)?,
            _ => {
                return Err(less_common::LessError::Config(
                    "provide exactly one of --csv, --jsonl, --parquet or --arrow".into(),
                ));
            }
        };
        for batch in batches {
            rows += engine.insert(table, batch)?;
        }
    }
    engine.flush(table)?;
    let stats = engine.stats(table)?;
    println!(
        "inserted {rows} rows into {table} (table now has {} rows, {} parts)",
        stats.rows, stats.part_count
    );
    Ok(())
}

pub async fn cmd_sql(dir: &Path, sql: &str, format: &str) -> Result<()> {
    let mode = OutputMode::parse(format).ok_or_else(|| {
        less_common::LessError::Config(format!(
            "unknown --format '{format}' (table | box | csv | json | line)"
        ))
    })?;
    let (_engine, session) = open_session_async(dir).await?;
    let batches = session.sql_batches(sql).await?;
    format_output(&batches, mode)
}

/// The published release triple for this machine (only the two we ship
/// have downloads; the others error with a build-from-source hint).
fn current_triple() -> Result<&'static str> {
    use std::env::consts::{ARCH, OS};
    match (OS, ARCH) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        _ => Err(less_common::LessError::Config(format!(
            "no prebuilt release for {OS}/{ARCH} — install from source: \
             cargo install --path crates/less-cli"
        ))),
    }
}

/// Self-upgrade: download the latest published release from lessdb.dev,
/// verify its SHA-256 against the served manifest, and atomically replace
/// the running binary. No brew/npm/curl needed.
pub async fn cmd_upgrade(
    check: bool,
    force: bool,
    install_dir: Option<&Path>,
    base: Option<&str>,
) -> Result<()> {
    let manifest_url = std::env::var("LESSDB_MANIFEST_URL")
        .unwrap_or_else(|_| "https://lessdb.dev/downloads/manifest.json".to_string());
    let latest_url = std::env::var("LESSDB_LATEST_URL")
        .unwrap_or_else(|_| "https://lessdb.dev/downloads/latest.json".to_string());
    let dl_base = base
        .map(|b| b.trim_end_matches('/').to_string())
        .unwrap_or_else(|| {
            std::env::var("LESSDB_DOWNLOAD_BASE")
                .unwrap_or_else(|_| "https://lessdb.dev/dl".to_string())
        });
    let triple = current_triple()?;

    let client = reqwest::Client::builder()
        .user_agent(format!("lessdb/{}-upgrader", less_common::VERSION))
        .build()
        .map_err(|e| less_common::LessError::Config(format!("http client: {e}")))?;

    let latest: serde_json::Value = client
        .get(&latest_url)
        .send()
        .await
        .map_err(|e| less_common::LessError::Query(format!("fetch {latest_url}: {e}")))?
        .error_for_status()
        .map_err(|e| less_common::LessError::Query(format!("{latest_url}: {e}")))?
        .json()
        .await
        .map_err(|e| less_common::LessError::Query(format!("parse latest.json: {e}")))?;

    let version = latest["version"]
        .as_str()
        .ok_or_else(|| less_common::LessError::Query("latest.json: missing version".into()))?;
    let file = latest["files"][triple].as_str().ok_or_else(|| {
        less_common::LessError::Query(format!(
            "no prebuilt release for {triple} (latest {version}) — build from source"
        ))
    })?;

    let manifest: serde_json::Value = client
        .get(&manifest_url)
        .send()
        .await
        .map_err(|e| less_common::LessError::Query(format!("fetch {manifest_url}: {e}")))?
        .error_for_status()
        .map_err(|e| less_common::LessError::Query(format!("{manifest_url}: {e}")))?
        .json()
        .await
        .map_err(|e| less_common::LessError::Query(format!("parse manifest.json: {e}")))?;
    let sha = manifest
        .as_array()
        .and_then(|a| a.iter().find(|e| e["file"] == file))
        .and_then(|e| e["sha256"].as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| less_common::LessError::Query(format!("no checksum for {file}")))?;

    let current = less_common::VERSION;
    if check {
        println!(
            "current: {current} · latest: {version}{}",
            if current == version {
                " (up to date)"
            } else {
                ""
            }
        );
        return Ok(());
    }
    if !force && current == version {
        println!("lessdb {current} is already the latest version.");
        return Ok(());
    }

    let url = format!("{dl_base}/{file}?v={}", &sha[..12]);
    println!("downloading {url}");
    let bytes = client
        .get(&url)
        .send()
        .await
        .map_err(|e| less_common::LessError::Query(format!("download {url}: {e}")))?
        .error_for_status()
        .map_err(|e| less_common::LessError::Query(format!("download {url}: {e}")))?
        .bytes()
        .await
        .map_err(|e| less_common::LessError::Query(format!("download {url}: {e}")))?;

    // SHA-256 verification (the manifest is the trusted channel).
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let got = hasher.finalize();
    let got_hex: String = got.iter().map(|b| format!("{b:02x}")).collect();
    if got_hex != sha {
        return Err(less_common::LessError::Query(format!(
            "sha256 mismatch:\n  got  {got_hex}\n  want {sha}"
        )));
    }

    // Unpack the tarball and locate the `lessdb` binary.
    let tmp = std::env::temp_dir().join(format!("lessdb-upgrade-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    let gz = flate2::read::GzDecoder::new(&bytes[..]);
    let mut archive = tar::Archive::new(gz);
    archive.unpack(&tmp).map_err(less_common::LessError::Io)?;
    let mut src = None;
    for top in std::fs::read_dir(&tmp)? {
        let top = top?.path();
        if top.is_dir() && top.join("lessdb").exists() {
            src = Some(top.join("lessdb"));
            break;
        }
    }
    let src = src.ok_or_else(|| {
        less_common::LessError::Query("downloaded archive has no lessdb binary".into())
    })?;

    // Target: explicit dir, else the running executable.
    let target = match install_dir {
        Some(d) => d.join("lessdb"),
        None => std::env::current_exe().map_err(less_common::LessError::Io)?,
    };
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Atomic replace: write next to the target, then rename over it.
    let staging = target.with_file_name(format!(
        ".lessdb-{}.new",
        target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("upgrade")
    ));
    std::fs::copy(&src, &staging)?;
    std::fs::rename(&staging, &target)?;
    // Best-effort cleanup of the temp unpack dir.
    let _ = std::fs::remove_dir_all(&tmp);

    println!("upgraded {current} -> {version} at {}", target.display());
    Ok(())
}

/// Output modes for query results (interactive `.mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Pretty tables (`+---+`) — the default display.
    Table,
    /// DuckDB box-drawing tables (`┌─┬─┐`).
    Box,
    Csv,
    Tsv,
    Json,
    JsonEachRow,
    /// Vertical: one `column: value` block per row (vertical display).
    Line,
}

impl OutputMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "table" | "pretty" => Some(Self::Table),
            "box" | "duckbox" | "prettycompact" => Some(Self::Box),
            "csv" => Some(Self::Csv),
            "tsv" | "tabs" => Some(Self::Tsv),
            "json" => Some(Self::Json),
            "jsoneachrow" | "ndjson" => Some(Self::JsonEachRow),
            "line" | "vertical" | "list" | "v" => Some(Self::Line),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::Box => "box",
            Self::Csv => "csv",
            Self::Tsv => "tsv",
            Self::Json => "json",
            Self::JsonEachRow => "jsoneachrow",
            Self::Line => "vertical",
        }
    }
}

/// Format query results in the active mode.
pub fn format_output(batches: &[arrow::record_batch::RecordBatch], mode: OutputMode) -> Result<()> {
    if batches.is_empty() || batches.iter().all(|b| b.num_rows() == 0) {
        println!("(0 rows)");
        return Ok(());
    }
    match mode {
        OutputMode::Table => print_batches(batches)?,
        OutputMode::Box => print_batches_box(batches)?,
        OutputMode::Csv | OutputMode::Tsv => {
            for b in batches {
                let mut buf = Vec::new();
                {
                    let mut builder = arrow::csv::writer::WriterBuilder::new();
                    if mode == OutputMode::Tsv {
                        builder = builder.with_delimiter(b'\t');
                    }
                    let mut w = builder.build(&mut buf);
                    w.write(b).map_err(less_common::LessError::Arrow)?;
                }
                print!("{}", String::from_utf8_lossy(&buf));
            }
        }
        OutputMode::Json => {
            let mut w = arrow::json::ArrayWriter::new(Vec::new());
            for b in batches {
                w.write(b).map_err(less_common::LessError::Arrow)?;
            }
            w.finish().map_err(less_common::LessError::Arrow)?;
            let bytes = w.into_inner();
            println!("{}", String::from_utf8_lossy(&bytes));
        }
        OutputMode::JsonEachRow => {
            for b in batches {
                let mut w = arrow::json::LineDelimitedWriter::new(Vec::new());
                w.write(b).map_err(less_common::LessError::Arrow)?;
                w.finish().map_err(less_common::LessError::Arrow)?;
                let bytes = w.into_inner();
                print!("{}", String::from_utf8_lossy(&bytes));
            }
        }
        OutputMode::Line => {
            for b in batches {
                let schema = b.schema();
                let cols: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
                for r in 0..b.num_rows() {
                    println!("\x1b[1m* {}. row *\x1b[0m", r + 1);
                    for (i, c) in cols.iter().enumerate() {
                        let v = arrow::util::display::array_value_to_string(b.column(i), r)
                            .map_err(less_common::LessError::Arrow)?;
                        println!("{c}: {v}");
                    }
                    println!();
                }
            }
        }
    }
    Ok(())
}

/// DuckDB-style box-drawing table (`┌─┬─┐ │ ├─┼─┤ └─┴─┘`).
fn print_batches_box(batches: &[arrow::record_batch::RecordBatch]) -> Result<()> {
    use std::fmt::Write as _;
    for b in batches {
        let headers: Vec<String> = b
            .schema()
            .fields()
            .iter()
            .map(|f| f.name().to_string())
            .collect();
        let mut cells: Vec<Vec<String>> = vec![Vec::with_capacity(headers.len())];
        for (i, h) in headers.iter().enumerate() {
            cells[0].push(h.clone());
            let _ = i;
        }
        for r in 0..b.num_rows() {
            let mut row = Vec::with_capacity(headers.len());
            for c in 0..b.num_columns() {
                row.push(
                    arrow::util::display::array_value_to_string(b.column(c), r)
                        .map_err(less_common::LessError::Arrow)?,
                );
            }
            cells.push(row);
        }
        let widths: Vec<usize> = (0..headers.len())
            .map(|c| {
                cells
                    .iter()
                    .map(|row| row[c].chars().count())
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let mut out = String::new();
        let line = |out: &mut String, l: char, m: char, r: char| {
            let _ = write!(out, "{l}");
            for (i, w) in widths.iter().enumerate() {
                let _ = write!(out, "{}", "─".repeat(w + 2));
                if i + 1 < widths.len() {
                    let _ = write!(out, "{m}");
                }
            }
            let _ = writeln!(out, "{r}");
        };
        line(&mut out, '┌', '┬', '┐');
        for (ri, row) in cells.iter().enumerate() {
            let _ = write!(out, "│");
            for (i, cell) in row.iter().enumerate() {
                let pad = widths[i] - cell.chars().count();
                let _ = write!(out, " {cell}{}", " ".repeat(pad + 1));
                if i + 1 < row.len() {
                    let _ = write!(out, "│");
                }
            }
            let _ = writeln!(out, "│");
            if ri == 0 {
                line(&mut out, '├', '┼', '┤');
            }
        }
        line(&mut out, '└', '┴', '┘');
        print!("{out}");
    }
    Ok(())
}

/// Interactive shell with: a prompt, multi-line
/// statements terminated by `;` (or `\G` for vertical output), a long list
/// of dot/backslash commands, output formats, command history and line
/// editing — with LessDB's own character (Firefly/FireflyCloud, agents-first
/// MCP tools, LUMO).
///
/// `lessdb sql` (no query) opens this; the full-screen TUI stays available
/// as `lessdb tui`.
pub async fn cmd_repl(dir: &Path) -> Result<()> {
    let (_engine, session) = open_session_async(dir).await?;
    let mut mode = OutputMode::Table;
    let mut timer = false;

    println!(
        "lessdb {}  ·  db: {}  ·  Firefly/FireflyCloud\n\
         Agents: 28 MCP tools over `lessdb mcp` (see /docs/mcp). SQL ends with ';', \
         \\G for vertical. .help lists commands.",
        less_common::VERSION,
        dir.display()
    );

    if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        return repl_plain(&session, dir, &mut mode, &mut timer).await;
    }

    let hist = std::env::var("LESSDB_HISTORY").unwrap_or_else(|_| {
        std::env::var("HOME")
            .map(|h| format!("{h}/.lessdb_history"))
            .unwrap_or_else(|_| ".lessdb_history".to_string())
    });
    let mut rl = rustyline::DefaultEditor::new()
        .map_err(|e| less_common::LessError::Config(format!("readline: {e}")))?;
    let _ = rl.load_history(&hist);
    println!();

    let mut buffer = String::new();
    loop {
        let prompt = if buffer.trim().is_empty() {
            "lessdb :) "
        } else {
            ":-] "
        };
        let line = match rl.readline(prompt) {
            Ok(l) => l,
            Err(rustyline::error::ReadlineError::Interrupted) => continue,
            Err(rustyline::error::ReadlineError::Eof) => break,
            Err(e) => {
                eprintln!("readline: {e}");
                break;
            }
        };
        let trimmed = line.trim();
        if trimmed.is_empty() && buffer.trim().is_empty() {
            continue;
        }
        // A leading meta-command (only when not mid-statement).
        if buffer.trim().is_empty() && (trimmed.starts_with('.') || trimmed.starts_with('\\')) {
            if process_meta(&session, dir, trimmed, &mut mode, &mut timer)
                .await?
                .is_some()
            {
                let _ = rl.save_history(&hist);
                break;
            }
            continue;
        }
        if buffer.trim().is_empty()
            && matches!(trimmed.to_ascii_lowercase().as_str(), "exit" | "quit")
        {
            let _ = rl.save_history(&hist);
            break;
        }
        buffer.push_str(&line);
        buffer.push('\n');
        // Terminated by `;`, by `\G` (run vertical), or by `\g` (go).
        let content = buffer
            .trim_end()
            .trim_end_matches('\n')
            .trim_end()
            .to_string();
        let run_vertical = content.ends_with("\\G");
        let go = content.ends_with("\\g");
        let terminated = content.ends_with(';') || run_vertical || go;
        if !terminated {
            continue;
        }
        let mut sql: &str = content.trim();
        if sql.ends_with(';') {
            sql = &sql[..sql.len() - 1];
        } else if sql.ends_with("\\G") || sql.ends_with("\\g") {
            sql = &sql[..sql.len() - 2];
        }
        let sql = sql.trim().to_string();
        let _ = rl.add_history_entry(sql.clone());
        buffer.clear();
        if sql.is_empty() {
            continue;
        }
        run_query(
            &session,
            sql,
            if run_vertical { OutputMode::Line } else { mode },
            timer,
        )
        .await;
    }
    let _ = rl.save_history(&hist);
    Ok(())
}

/// Non-tty fallback: plain line loop (piped input, scripts, CI).
async fn repl_plain(
    session: &LessSession,
    dir: &Path,
    mode: &mut OutputMode,
    timer: &mut bool,
) -> Result<()> {
    let mut buffer = String::new();
    loop {
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim().trim_end_matches(';').trim();
        if buffer.is_empty() && (trimmed.starts_with('.') || trimmed.starts_with('\\')) {
            if process_meta(session, dir, trimmed, mode, timer)
                .await?
                .is_some()
            {
                return Ok(());
            }
            continue;
        }
        if buffer.is_empty() && matches!(trimmed.to_ascii_lowercase().as_str(), "exit" | "quit") {
            break;
        }
        buffer.push_str(&line);
        buffer.push('\n');
        let content = buffer
            .trim_end()
            .trim_end_matches('\n')
            .trim_end()
            .to_string();
        let run_vertical = content.ends_with("\\G");
        let go = content.ends_with("\\g");
        if !(content.ends_with(';') || run_vertical || go) {
            continue;
        }
        let mut sql: &str = content.trim();
        if sql.ends_with(';') {
            sql = &sql[..sql.len() - 1];
        } else if sql.ends_with("\\G") || sql.ends_with("\\g") {
            sql = &sql[..sql.len() - 2];
        }
        let sql = sql.trim().to_string();
        buffer.clear();
        if !sql.is_empty() {
            let out = if run_vertical {
                OutputMode::Line
            } else {
                *mode
            };
            run_query(session, sql, out, *timer).await;
        }
    }
    Ok(())
}

/// Execute a finished statement, print the result and (optionally) timing.
async fn run_query(session: &LessSession, sql: String, mode: OutputMode, timer: bool) {
    let start = std::time::Instant::now();
    match session.sql_batches(&sql).await {
        Ok(batches) => {
            let rows: usize = batches.iter().map(|b| b.num_rows()).sum();
            if let Err(e) = format_output(&batches, mode) {
                eprintln!("print error: {e}");
            }
            if timer {
                println!(
                    "{} rows in set. Elapsed: {:.3} s.",
                    rows,
                    start.elapsed().as_secs_f64()
                );
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            if let Ok(tables) = session.engine().tables()
                && let Some(hint) = suggest_table(&e.to_string(), &tables)
            {
                eprintln!("hint: {hint}");
            }
        }
    }
}

/// Handle a dot- or backslash-command line. Returns `Ok(Some(quit))` when
/// the command quit the shell, `Ok(None)` to continue.
async fn process_meta(
    session: &LessSession,
    dir: &Path,
    line: &str,
    mode: &mut OutputMode,
    timer: &mut bool,
) -> Result<Option<bool>> {
    match repl_command(session, dir, line, mode, timer).await? {
        true => Ok(Some(true)),
        false => Ok(None),
    }
}

/// Handle one dot- or backslash-command line. Returns `true` to quit.
async fn repl_command(
    session: &LessSession,
    dir: &Path,
    line: &str,
    mode: &mut OutputMode,
    timer: &mut bool,
) -> Result<bool> {
    let parts: Vec<&str> = line.splitn(2, char::is_whitespace).collect();
    let cmd = parts[0].trim();
    let arg = parts
        .get(1)
        .map(|a| a.trim().to_string())
        .unwrap_or_default();
    match cmd {
        ".help" | ".commands" | "\\h" | "\\help" => {
            println!(
                "Statements: end with ';' (\\G runs as vertical, \\g sends now)\n\
                 Formats:    .mode table|pretty|box|csv|tsv|json|jsoneachrow|vertical\n\
                 .tables [pattern]         list tables   .schema [table]  show CREATE TABLE\n\
                 .mode <fmt>               set output    .timer on|off    query timing\n\
                 .import file [table]      load csv/parquet/json\n\
                 .read file.sql            run a script  .demo            seed + showcase\n\
                 .license                  MIT license   .mcp             how to open the agent door\n\
                 .quit / .exit / exit      leave\n\
                 \\t tables · \\d [t] describe · \\p [t] parts · \\o [t] optimize · \\c clear"
            );
        }
        ".mcp" => {
            println!(
                "Open the agent door (MCP):\n  lessdb mcp [--dir {}] [--require-auth]   # stdio, 28 tools\n  lessdb server --addr 0.0.0.0:7080      # hosted: POST /mcp, tokens required\nThen register with your agent, or install the one-file skill:\n  curl -fsSL https://lessdb.dev/skills/lessdb/SKILL.md -o ~/.claude/skills/lessdb/SKILL.md",
                dir.display()
            );
        }
        ".import" => {
            if arg.is_empty() {
                eprintln!("usage: .import file.csv [table]");
                return Ok(false);
            }
            let mut it = arg.split_whitespace();
            let path = it.next().unwrap_or_default();
            let table = match it.next() {
                Some(t) => t.trim_matches(['\'', '"', ';']).to_string(),
                None => {
                    let stem = std::path::Path::new(path)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    stem.chars()
                        .map(|c| {
                            if c.is_alphanumeric() || c == '_' {
                                c
                            } else {
                                '_'
                            }
                        })
                        .collect()
                }
            };
            if table.is_empty() {
                eprintln!(
                    "could not derive a table name from '{path}' — pass one: .import {path} mytable"
                );
                return Ok(false);
            }
            let ext = std::path::Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let read_fn = match ext.as_str() {
                "csv" => "read_csv",
                "parquet" => "read_parquet",
                "json" | "jsonl" | "ndjson" => "read_json",
                other => {
                    eprintln!(
                        "unsupported file type '{other}' (.import supports csv, parquet, json)"
                    );
                    return Ok(false);
                }
            };
            let escaped = path.replace('\'', "''");
            match session
                .sql_batches(&format!(
                    "CREATE TABLE {table} AS SELECT * FROM {read_fn}('{escaped}')"
                ))
                .await
            {
                Ok(_) => println!("imported '{path}' into table '{table}'"),
                Err(e) => eprintln!("import error: {e}"),
            }
        }
        ".tables" | "\\t" => {
            let sql = if arg.is_empty() {
                "SHOW TABLES".to_string()
            } else {
                format!("SHOW TABLES LIKE '{}'", arg.trim_matches(['\'', '"']))
            };
            let batches = session.sql_batches(&sql).await?;
            format_output(&batches, *mode)?;
        }
        ".schema" | "\\d" => {
            if arg.is_empty() {
                for t in session.engine().tables()? {
                    let batches = session
                        .sql_batches(&format!("SHOW CREATE TABLE {t}"))
                        .await?;
                    format_output(&batches, OutputMode::Line)?;
                }
            } else {
                let batches = session
                    .sql_batches(&format!("SHOW CREATE TABLE {}", arg))
                    .await?;
                format_output(&batches, OutputMode::Line)?;
            }
        }
        ".mode" => match OutputMode::parse(&arg) {
            Some(m) => {
                *mode = m;
                println!("mode: {}", m.name());
            }
            None => eprintln!(
                "unknown mode '{arg}' (table|pretty|box|csv|tsv|json|jsoneachrow|vertical)"
            ),
        },
        ".timer" => {
            *timer = match arg.as_str() {
                "on" | "true" | "1" => {
                    println!("timer on");
                    true
                }
                "off" | "false" | "0" => {
                    println!("timer off");
                    false
                }
                _ => {
                    eprintln!("usage: .timer on|off");
                    return Ok(false);
                }
            };
        }
        ".read" => {
            let contents = std::fs::read_to_string(&arg)?;
            for stmt in contents
                .split(';')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                match session.sql_batches(stmt).await {
                    Ok(batches) => {
                        if let Err(e) = format_output(&batches, *mode) {
                            eprintln!("print error: {e}");
                        }
                    }
                    Err(e) => eprintln!("error ({stmt}): {e}"),
                }
            }
        }
        ".license" => {
            cmd_license()?;
        }
        ".demo" => {
            Box::pin(cmd_demo(dir, 50_000)).await?;
        }
        "\\p" => {
            let table = if arg.is_empty() {
                session.engine().tables()?.join(" ")
            } else {
                arg.clone()
            };
            for t in table.split_whitespace() {
                for p in session.engine().parts(t)? {
                    println!(
                        "{}  rows={}  level={}",
                        p.meta.name,
                        p.meta.row_count,
                        p.level()
                    );
                }
            }
        }
        "\\o" => {
            let tables = if arg.is_empty() {
                session.engine().tables()?
            } else {
                vec![arg.clone()]
            };
            for t in tables {
                match session.engine().optimize(&t) {
                    Ok(Some(m)) => println!("{t}: merged into {} ({} rows)", m.name, m.row_count),
                    Ok(None) => println!("{t}: nothing to merge"),
                    Err(e) => eprintln!("{t}: {e}"),
                }
            }
        }
        ".quit" | ".exit" => return Ok(true),
        other => {
            // did-you-mean for near-miss dot-commands
            const CMDS: [&str; 14] = [
                ".help", ".tables", ".schema", ".mode", ".timer", ".read", ".license", ".demo",
                ".quit", ".exit", "\\t", "\\d", "\\p", "\\o",
            ];
            let best = CMDS
                .iter()
                .map(|c| (c, levenshtein(&other.to_ascii_lowercase(), c)))
                .filter(|(_, d)| *d <= 2)
                .min_by_key(|(_, d)| *d);
            match best {
                Some((c, _)) => eprintln!("unknown command '{other}' — did you mean '{c}'?"),
                None => eprintln!("unknown command '{other}' — .help lists commands"),
            }
        }
    }
    Ok(false)
}

/// Levenshtein edit distance (for did-you-mean suggestions).
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j + 1] + 1)
                .min(cur[j] + 1)
                .min(prev[j] + usize::from(ca != cb));
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Near-miss table suggestion from an error message: when the error says
/// something was "not found", look for identifier-like words that don't
/// name an existing table and find the closest real one.
fn suggest_table(err: &str, tables: &[String]) -> Option<String> {
    if !err.to_ascii_lowercase().contains("not found") {
        return None;
    }
    for word in err
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() >= 3)
    {
        if tables.iter().any(|t| t.eq_ignore_ascii_case(word)) {
            continue; // it exists — the miss is elsewhere
        }
        let mut best: Option<(&String, usize)> = None;
        for t in tables {
            let d = levenshtein(&word.to_ascii_lowercase(), &t.to_ascii_lowercase());
            if d <= 2 && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((t, d));
            }
        }
        if let Some((t, _)) = best {
            return Some(format!("did you mean '{t}'?"));
        }
    }
    None
}

pub async fn cmd_server(
    dir: &Path,
    addr: &str,
    tls_cert: Option<PathBuf>,
    tls_key: Option<PathBuf>,
) -> Result<()> {
    let (engine, session) = open_session_async(dir).await?;
    tracing::info!(%addr, tables = engine.tables_async().await?.len(), "lessdb server starting");
    // TLS: explicit flags win, then config.json, and both files must be set.
    let cert = tls_cert
        .or_else(|| engine.config.tls_cert.clone())
        .filter(|_| engine.config.tls_key.is_some() || tls_key.is_some());
    let key = tls_key.or_else(|| engine.config.tls_key.clone());
    let tls = match (cert, key) {
        (Some(cert), Some(key)) => Some(less_server::TlsPaths { cert, key }),
        _ => None,
    };
    let session = Arc::new(session);
    // The MCP door is always on for the hosted server and always
    // fail-closed: agent tokens are required for every tools/call.
    // (A hosted door must never default to open.)
    let tokens = less_auth::TokenStore::open(dir)?;
    if tokens.is_empty() {
        eprintln!(
            "warning: no agent tokens exist yet — POST /mcp is up but every tools/call \
             will be denied.\nmint one with:  lessdb token create <name> --role read"
        );
    }
    let mcp = less_mcp::state_from_tenant_with_auth(session.clone(), "default", tokens, true)?;
    less_server::serve_with_tls(session, addr, tls.as_ref(), Some(mcp)).await
}

pub async fn cmd_mcp(dir: &Path, tenant: &str, require_auth: bool) -> Result<()> {
    let (_engine, session) = open_session_async(dir).await?;
    let state = if require_auth {
        let tokens = less_auth::TokenStore::open(dir)?;
        if tokens.is_empty() {
            eprintln!(
                "warning: --require-auth is on but no agent tokens exist yet.\n\
                 mint one with:  less token create <name> --role read --tenant {tenant}"
            );
        }
        less_mcp::state_from_tenant_with_auth(Arc::new(session), tenant, tokens, true)?
    } else {
        less_mcp::state_from_tenant(Arc::new(session), tenant)?
    };
    less_mcp::run_stdio(state).await
}

/// Show the audit trail (newest first), optionally filtered.
pub fn cmd_audit(
    dir: &Path,
    since: &str,
    caller: Option<&str>,
    outcome: Option<&str>,
) -> Result<()> {
    let audit_dir = dir.join(less_telemetry::audit::AUDIT_DIR);
    if !audit_dir.exists() {
        println!("(no audit entries yet)");
        return Ok(());
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let since_ms = less_telemetry::audit::parse_since(since, now)?;
    let lines = less_telemetry::audit::query(&audit_dir, Some(since_ms))?;
    let mut shown = 0;
    for line in lines {
        let e = &line.entry;
        if let Some(c) = caller
            && e.caller != c
        {
            continue;
        }
        if let Some(o) = outcome
            && e.outcome != o
        {
            continue;
        }
        shown += 1;
        let action = match &e.tool {
            Some(t) => format!("{} {t}", e.action),
            None => e.action.clone(),
        };
        let mut out = format!(
            "{}  {}:{}  {}  {}  {}",
            e.ts, e.caller_kind, e.caller, e.door, action, e.outcome
        );
        if !e.tenant.is_empty() {
            out.push_str(&format!("  tenant:{}", e.tenant));
        }
        if let Some(d) = &e.detail {
            let d = if d.len() > 72 { &d[..72] } else { d };
            out.push_str(&format!("  {d}"));
        }
        println!("{out}");
    }
    if shown == 0 {
        println!("(no matching audit entries)");
    }
    Ok(())
}

pub async fn cmd_bench(dir: &Path, rows: usize) -> Result<()> {
    crate::bench::bench(dir, rows).await
}

/// Print the effective engine configuration for a directory.
/// The 60-second tour: seed a demo database and run showcase queries so a
/// first-time user sees real analytics with real timings immediately.
/// `lessdb demo` is the recommended second command after installing.
pub async fn cmd_demo(dir: &Path, rows: usize) -> Result<()> {
    use arrow::array::{Float64Array, Int64Array, StringArray, TimestampMillisecondArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use less_catalog::{EngineKind, FieldSpec, SchemaSpec, TableDef, TypeSpec};
    use std::sync::Arc;
    use std::time::Instant;

    println!("◈ LessDB demo — seeding {rows} rows of event data…");
    let t = Instant::now();
    std::fs::create_dir_all(dir)?;
    let engine = open_engine(dir)?;
    let def = TableDef::new(
        "events",
        SchemaSpec {
            fields: vec![
                FieldSpec::new("event_id", TypeSpec::Int64),
                FieldSpec::new("user_id", TypeSpec::Int64),
                FieldSpec::new("kind", TypeSpec::Utf8),
                FieldSpec::new("url", TypeSpec::Utf8),
                FieldSpec::new("amount", TypeSpec::Float64),
                FieldSpec::new("ts", TypeSpec::TimestampMs),
            ],
        },
        EngineKind::Firefly,
    );
    let mut d = def;
    d.sort_key = vec!["kind".into(), "ts".into()];
    if engine.table("events").is_ok() {
        engine.drop_table("events")?;
    }
    engine.create_table(d)?;

    // Deterministic pseudo-random seed data (LCG — no external deps).
    let schema = Arc::new(Schema::new(vec![
        Field::new("event_id", DataType::Int64, false),
        Field::new("user_id", DataType::Int64, false),
        Field::new("kind", DataType::Utf8, false),
        Field::new("url", DataType::Utf8, false),
        Field::new("amount", DataType::Float64, false),
        Field::new(
            "ts",
            DataType::Timestamp(arrow::datatypes::TimeUnit::Millisecond, None),
            false,
        ),
    ]));
    const KINDS: [&str; 4] = ["view", "click", "purchase", "share"];
    const URLS: [&str; 5] = ["/", "/pricing", "/docs", "/blog", "/downloads"];
    const BATCH: usize = 262_144;
    let base_ts = 1_700_000_000_000i64; // 2023-11-14
    let mut state: u64 = 42;
    let mut next = |m: u64| {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 33) % m
    };
    let mut loaded = 0usize;
    while loaded < rows {
        let n = BATCH.min(rows - loaded);
        let mut ids = Vec::with_capacity(n);
        let mut users = Vec::with_capacity(n);
        let mut kinds = Vec::with_capacity(n);
        let mut urls = Vec::with_capacity(n);
        let mut amounts = Vec::with_capacity(n);
        let mut tss = Vec::with_capacity(n);
        for i in 0..n {
            ids.push((loaded + i) as i64);
            users.push(next(100_000) as i64);
            kinds.push(KINDS[next(KINDS.len() as u64) as usize]);
            urls.push(URLS[next(URLS.len() as u64) as usize]);
            amounts.push((next(10_000) as f64) / 100.0);
            tss.push(base_ts + (next(7_776_000) as i64) * 1000); // ~90 days
        }
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(Int64Array::from(ids)),
                Arc::new(Int64Array::from(users)),
                Arc::new(StringArray::from(kinds)),
                Arc::new(StringArray::from(urls)),
                Arc::new(Float64Array::from(amounts)),
                Arc::new(TimestampMillisecondArray::from(tss)),
            ],
        )?;
        engine.insert("events", batch)?;
        loaded += n;
    }
    engine.flush("events")?;
    engine.optimize("events")?;
    println!("seeded + optimized in {:.1}s", t.elapsed().as_secs_f64());

    let session = LessSession::new_async(engine).await?;
    let session_ref = &session;
    let show = |label: String, sql: String| async move {
        println!();
        println!("── {label} ──");
        println!("  {sql}");
        let t = Instant::now();
        let batches = session_ref.sql_batches(&sql).await?;
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        for b in batches {
            let b = b;
            let pretty = arrow::util::pretty::pretty_format_batches(&[b])
                .map_err(less_common::LessError::Arrow)?;
            for line in pretty.to_string().lines() {
                println!("  {line}");
            }
        }
        println!("  ── {ms:.1} ms");
        Ok::<(), less_common::LessError>(())
    };

    show(
        "revenue by event kind".to_string(),
        "SELECT kind, count(*) AS events, round(sum(amount), 2) AS revenue FROM events GROUP BY kind ORDER BY revenue DESC".to_string(),
    ).await?;
    show(
        "daily active users (last 30 days)".to_string(),
        "SELECT to_date(to_timestamp_millis(ts)) AS day, count(DISTINCT user_id) AS dau FROM events GROUP BY day ORDER BY day DESC LIMIT 7".to_string(),
    ).await?;
    show(
        "top pages by views".to_string(),
        "SELECT url, count(*) AS views FROM events WHERE kind = 'view' GROUP BY url ORDER BY views DESC".to_string(),
    ).await?;

    println!();
    println!("That is LessDB in 60 seconds: one engine for SQL, vectors, and agent memory.");
    println!("  lessdb sql                    # interactive REPL (this database)");
    println!("  lessdb mcp                    # open it to AI agents (28 MCP tools)");
    println!("  lessdb server                 # serve it over HTTP + Prometheus");
    println!("  https://lessdb.dev/docs/getting-started");
    println!("  MIT License — https://lessdb.dev/docs/license");
    Ok(())
}

/// Row counts, part counts and on-disk size (all tables, or one).
pub fn cmd_stats(dir: &Path, table: Option<&str>) -> Result<()> {
    let engine = open_engine(dir)?;
    let names = match table {
        Some(t) => vec![t.to_string()],
        None => engine.tables()?,
    };
    for name in names {
        let s = engine.stats(&name)?;
        println!(
            "{}  rows={}  parts={}  buffered={}  disk={}",
            s.table,
            s.rows,
            s.part_count,
            s.buffered_rows,
            human_bytes(s.disk_bytes)
        );
    }
    Ok(())
}

fn human_bytes(b: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = b as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

pub fn cmd_config(dir: &Path) -> Result<()> {
    let config = EngineConfig::load_or_default(dir);
    println!("{}", serde_json::to_string_pretty(&config)?);
    Ok(())
}

/// Print shared-storage block-cache statistics.
pub fn cmd_cache(dir: &Path) -> Result<()> {
    let engine = open_engine(dir)?;
    match engine.cache_stats() {
        Some(s) => {
            println!("block cache: enabled");
            println!("  memory entries: {}", s.entries);
            println!("  memory bytes:   {}", s.total_bytes);
            println!("  hits:           {}", s.hits);
            println!("  disk hits:      {}", s.disk_hits);
            println!("  misses:         {}", s.misses);
        }
        None => println!("block cache: disabled (block_cache_bytes = 0)"),
    }
    Ok(())
}

/// Run an openCypher query against the context graph
/// (`<data_dir>/memory`).
pub fn cmd_cypher(dir: &Path, query: &str) -> Result<()> {
    let mut store = less_graph::ContextStore::open(Some(&dir.join("memory")))?;
    let json = less_cypher::run_json(store.graph_mut(), query)?;
    println!("{json}");
    store.flush()?;
    Ok(())
}

/// Fan a query out across HTTP nodes and print the merged result.
pub async fn cmd_fanout(nodes: &str, sql: &str) -> Result<()> {
    let nodes: Vec<String> = nodes
        .split(',')
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let batches = less_fanout::fanout(&nodes, sql).await?;
    print(&batches)
}

/// Print Prometheus metrics for this process (gauges refreshed from the
/// engine state when a database directory is present).
pub fn cmd_metrics(dir: &Path) -> Result<()> {
    if dir.exists() {
        let engine = open_engine(dir)?;
        let metrics = less_telemetry::global();
        metrics.tables.set(engine.tables()?.len() as i64);
        metrics.buffered_rows.set(engine.buffered_rows()? as i64);
    }
    print!("{}", less_telemetry::global().render());
    Ok(())
}

#[cfg(feature = "gpu")]
pub async fn cmd_gpu_bench(rows: usize) -> Result<()> {
    less_gpu::benchmark(rows).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levenshtein_distances() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("tables", "tables"), 0);
        assert_eq!(levenshtein("tabls", "tables"), 1);
        assert_eq!(levenshtein("", "abc"), 3);
    }

    #[test]
    fn suggest_table_finds_near_misses_only_on_not_found() {
        let tables = vec!["events".to_string(), "pageviews".to_string()];
        // near miss in a "not found" error
        let hint = suggest_table("table 'eventz' not found", &tables).unwrap();
        assert_eq!(hint, "did you mean 'events'?");
        // exact table is not re-suggested
        assert!(suggest_table("table 'events' not found", &tables).is_none());
        // no hint on unrelated errors
        assert!(suggest_table("syntax error at end of input", &tables).is_none());
        // too-far words get no hint
        assert!(suggest_table("table 'completelydifferent' not found", &tables).is_none());
    }
}
