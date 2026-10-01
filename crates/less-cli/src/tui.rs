//! The LessDB interactive TUI — a rich, full-screen SQL console.
//!
//! Layout: table sidebar on the left, scrollable results in the middle,
//! a status bar, and a query editor at the bottom. Keys:
//!
//! * type SQL, `Enter` runs (hold `Shift` for a newline);
//! * `Up`/`Down` = history in the editor, scroll elsewhere;
//! * `Tab` cycles focus: editor → results → tables;
//! * `PgUp`/`PgDn`/`Home`/`End` scroll the results;
//! * `Enter` in the tables pane describes the selected table;
//! * `\t` tables, `\d [t]` describe, `\p [t]` parts, `\o [t]` optimize,
//!   `\h` help, `\c` clear, `\q` / `Ctrl+D` quit.
//!
//! Non-interactive contexts (piped stdout, no TTY) fall back to the
//! plain line-based shell.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::record_batch::RecordBatch;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use less_common::{LessError, Result};
use less_engine::LessEngine;
use less_query::LessSession;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Row, Table};
use tokio::sync::mpsc;

/// Maximum rows rendered in the results grid.
const MAX_RESULT_ROWS: usize = 500;
/// Maximum width of one column in the grid.
const MAX_COL_WIDTH: usize = 30;

#[derive(Clone, Copy, PartialEq)]
enum Focus {
    Editor,
    Results,
    Tables,
}

enum QueryOutcome {
    Rows(Vec<Vec<String>>, Vec<String>, Vec<String>, usize),
    Text(Vec<String>),
    Err(String),
}

pub async fn run(dir: &Path) -> Result<()> {
    if !std::io::stdout().is_terminal() {
        return super::commands::cmd_repl(dir).await;
    }
    let (engine, session) = super::commands::open_session_async(dir).await?;
    let mut app = App::new(dir, Arc::new(session), engine).await?;
    app.event_loop().await
}

struct App {
    dir: PathBuf,
    session: Arc<LessSession>,
    engine: Arc<LessEngine>,
    tables: Vec<String>,
    table_selected: usize,
    input: String,
    history: Vec<String>,
    history_idx: Option<usize>,
    columns: Vec<String>,
    types: Vec<String>,
    rows: Vec<Vec<String>>,
    shown_rows: usize,
    total_rows: usize,
    text_lines: Vec<String>,
    last_error: Option<String>,
    scroll: usize,
    hscroll: usize,
    focus: Focus,
    running: bool,
    last_ms: Option<f64>,
    help_open: bool,
    quitting: bool,
    spinner: u8,
    tx: mpsc::UnboundedSender<QueryOutcome>,
    rx: mpsc::UnboundedReceiver<QueryOutcome>,
    query_started: Option<std::time::Instant>,
}

impl App {
    async fn new(dir: &Path, session: Arc<LessSession>, engine: Arc<LessEngine>) -> Result<Self> {
        let tables = engine.tables_async().await?;
        let (tx, rx) = mpsc::unbounded_channel();
        let app = Self {
            dir: dir.to_path_buf(),
            session,
            engine,
            tables,
            table_selected: 0,
            input: String::new(),
            history: Vec::new(),
            history_idx: None,
            columns: Vec::new(),
            types: Vec::new(),
            rows: Vec::new(),
            shown_rows: 0,
            total_rows: 0,
            text_lines: vec![
                "Welcome to LessDB. Type SQL and press Enter —".into(),
                "  \\t tables · \\d [table] describe · \\p [table] parts · \\o [table] optimize"
                    .into(),
                "  \\h help · \\c clear · \\q quit · Tab switches panes · Shift+Enter newline"
                    .into(),
            ],
            last_error: None,
            scroll: 0,
            hscroll: 0,
            focus: Focus::Editor,
            running: false,
            last_ms: None,
            help_open: false,
            quitting: false,
            spinner: 0,
            tx,
            rx,
            query_started: None,
        };
        Ok(app)
    }

    fn run_sql(&mut self, sql: &str) {
        if self.running {
            return;
        }
        let sql = sql.to_string();
        let session = self.session.clone();
        let engine = self.engine.clone();
        let tx = self.tx.clone();
        self.running = true;
        self.last_error = None;
        self.query_started = Some(std::time::Instant::now());
        tokio::spawn(async move {
            let outcome = execute(&session, &engine, &sql).await;
            let _ = tx.send(outcome);
        });
    }

    async fn event_loop(&mut self) -> Result<()> {
        let mut terminal = ratatui::init();
        let mut events = crossterm::event::EventStream::new();
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(150));
        let result: Result<()> = loop {
            tokio::select! {
                maybe = events.next() => {
                    match maybe {
                        Some(Ok(ev)) => self.on_event(ev)?,
                        Some(Err(e)) => return Err(LessError::Engine(format!("terminal: {e}"))),
                        None => break Ok(()),
                    }
                }
                msg = self.rx.recv() => {
                    if let Some(msg) = msg { self.on_outcome(msg); }
                }
                _ = tick.tick() => { self.spinner = self.spinner.wrapping_add(1); }
            }
            if self.quitting {
                break Ok(());
            }
            let _ = terminal.draw(|f| self.render(f));
        };
        ratatui::restore();
        result
    }

    fn on_event(&mut self, ev: Event) -> Result<()> {
        match ev {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key(key),
            _ => {}
        }
        Ok(())
    }

    fn on_key(&mut self, key: KeyEvent) {
        // Global keys.
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.quitting = true;
            return;
        }
        if key.code == KeyCode::Char('d') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if self.input.is_empty() {
                self.quitting = true;
            }
            return;
        }
        if key.code == KeyCode::Esc {
            self.help_open = false;
            return;
        }
        if self.help_open {
            self.help_open = false;
            return;
        }
        if key.code == KeyCode::Char('?') {
            self.help_open = !self.help_open;
            return;
        }
        if key.code == KeyCode::Tab {
            self.focus = match self.focus {
                Focus::Editor => Focus::Results,
                Focus::Results => Focus::Tables,
                Focus::Tables => Focus::Editor,
            };
            return;
        }

        match self.focus {
            Focus::Editor => self.editor_key(key),
            Focus::Results => self.results_key(key),
            Focus::Tables => self.tables_key(key),
        }
    }

    fn editor_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.input.push('\n');
                    return;
                }
                let sql = self.input.trim().to_string();
                if sql.is_empty() {
                    return;
                }
                self.history.push(self.input.clone());
                self.history_idx = None;
                self.input.clear();
                self.scroll = 0;
                self.hscroll = 0;
                if sql.starts_with('\\') {
                    self.meta(&sql);
                } else {
                    self.run_sql(&sql);
                }
            }
            KeyCode::Char(c) => self.input.push(c),
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Up => {
                if self.history.is_empty() {
                    return;
                }
                let idx = match self.history_idx {
                    None => self.history.len().saturating_sub(1),
                    Some(0) => 0,
                    Some(i) => i - 1,
                };
                self.history_idx = Some(idx);
                self.input = self.history[idx].clone();
            }
            KeyCode::Down => match self.history_idx {
                Some(i) if i + 1 < self.history.len() => {
                    self.history_idx = Some(i + 1);
                    self.input = self.history[i + 1].clone();
                }
                Some(_) => {
                    self.history_idx = None;
                    self.input.clear();
                }
                None => {}
            },
            _ => {}
        }
    }

    fn results_key(&mut self, key: KeyEvent) {
        let page = 10usize;
        match key.code {
            KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down => self.scroll = (self.scroll + 1).min(self.shown_rows.saturating_sub(1)),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(page),
            KeyCode::PageDown => {
                self.scroll = (self.scroll + page).min(self.shown_rows.saturating_sub(1))
            }
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => self.scroll = self.shown_rows.saturating_sub(1),
            KeyCode::Left => self.hscroll = self.hscroll.saturating_sub(1),
            KeyCode::Right => self.hscroll += 1,
            _ => {}
        }
    }

    fn tables_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.table_selected = self.table_selected.saturating_sub(1),
            KeyCode::Down => {
                self.table_selected =
                    (self.table_selected + 1).min(self.tables.len().saturating_sub(1))
            }
            KeyCode::Enter => {
                if let Some(t) = self.tables.get(self.table_selected).cloned() {
                    self.describe(&t);
                }
            }
            _ => {}
        }
    }

    fn meta(&mut self, cmd: &str) {
        let mut parts = cmd.split_whitespace();
        match parts.next() {
            Some("\\q") | Some("\\quit") => self.quitting = true,
            Some("\\h") | Some("\\help") => self.help_open = true,
            Some("\\c") => {
                self.input.clear();
                self.rows.clear();
                self.columns.clear();
                self.types.clear();
                self.last_error = None;
                self.text_lines = vec!["cleared".into()];
            }
            Some("\\t") | Some("\\tables") => {
                let mut lines = self.tables.clone();
                if lines.is_empty() {
                    lines = vec!["(no tables — CREATE TABLE … to get started)".into()];
                }
                self.show_text(lines);
            }
            Some("\\d") | Some("\\describe") => {
                let t = parts
                    .next()
                    .map(str::to_string)
                    .or_else(|| self.tables.get(self.table_selected).cloned());
                match t {
                    Some(t) => self.describe(&t),
                    None => self.show_text(vec!["no table selected".into()]),
                }
            }
            Some("\\p") | Some("\\parts") => {
                let t = parts
                    .next()
                    .map(str::to_string)
                    .or_else(|| self.tables.get(self.table_selected).cloned());
                match t {
                    Some(t) => self.parts(&t),
                    None => self.show_text(vec!["no table selected".into()]),
                }
            }
            Some("\\o") | Some("\\optimize") => {
                let t = parts
                    .next()
                    .map(str::to_string)
                    .or_else(|| self.tables.get(self.table_selected).cloned());
                match t {
                    Some(t) => {
                        let r = self.engine.optimize(&t);
                        self.show_text(match r {
                            Ok(Some(m)) => {
                                vec![format!("merged {} rows into part {}", m.row_count, m.name)]
                            }
                            Ok(None) => vec!["fewer than two parts; nothing to merge".into()],
                            Err(e) => vec![format!("error: {e}")],
                        });
                        if let Ok(tables) = self.engine.tables() {
                            self.tables = tables;
                        }
                    }
                    None => self.show_text(vec!["no table selected".into()]),
                }
            }
            other => self.show_text(vec![format!(
                "unknown meta command '{}' (\\t \\d \\p \\o \\h \\c \\q)",
                other.unwrap_or("")
            )]),
        }
    }

    fn describe(&mut self, t: &str) {
        match self.engine.table(t) {
            Ok(def) => {
                let mut lines = vec![format!("table {t}  engine={:?}", def.engine)];
                for f in &def.schema.fields {
                    lines.push(format!(
                        "  {} {}{}",
                        f.name,
                        f.ty.name(),
                        if f.nullable { "" } else { " NOT NULL" }
                    ));
                }
                lines.push(format!("  order by: {}", def.sort_key.join(", ")));
                if !def.unique.is_empty() {
                    lines.push(format!("  unique: {}", def.unique.join(", ")));
                }
                if let (Some(c), Some(s)) = (&def.ttl_col, def.ttl_secs) {
                    lines.push(format!("  ttl: {c} every {s}s"));
                }
                self.show_text(lines);
            }
            Err(e) => self.show_text(vec![format!("error: {e}")]),
        }
    }

    fn parts(&mut self, t: &str) {
        match self.engine.parts(t) {
            Ok(parts) if parts.is_empty() => {
                self.show_text(vec!["(no parts — insert rows to flush a part)".into()]);
            }
            Ok(parts) => {
                let mut lines = Vec::with_capacity(parts.len() + 1);
                lines.push(format!("{} part(s):", parts.len()));
                for p in parts {
                    lines.push(format!("  {}", p.summary()));
                }
                self.show_text(lines);
            }
            Err(e) => self.show_text(vec![format!("error: {e}")]),
        }
    }

    fn show_text(&mut self, lines: Vec<String>) {
        self.text_lines = lines;
        self.columns.clear();
        self.types.clear();
        self.rows.clear();
        self.shown_rows = 0;
        self.total_rows = 0;
        self.scroll = 0;
        self.last_error = None;
    }

    fn on_outcome(&mut self, outcome: QueryOutcome) {
        self.running = false;
        if let Some(started) = self.query_started.take() {
            self.last_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
        }
        match outcome {
            QueryOutcome::Rows(rows, columns, types, total) => {
                self.columns = columns;
                self.types = types;
                self.total_rows = total;
                self.shown_rows = rows.len();
                self.rows = rows;
                self.text_lines.clear();
                self.last_error = None;
            }
            QueryOutcome::Text(lines) => self.show_text(lines),
            QueryOutcome::Err(e) => {
                self.text_lines = vec![format!("error: {e}")];
                self.last_error = Some(e);
                self.columns.clear();
                self.types.clear();
                self.rows.clear();
                self.shown_rows = 0;
                self.total_rows = 0;
            }
        }
    }

    fn render(&mut self, f: &mut Frame) {
        let area = f.area();
        let chunks = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(2),
            Constraint::Length(3),
        ])
        .split(area);

        // Header
        let header = Line::from(vec![
            Span::styled(
                format!(" LessDB {}", less_common::VERSION),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {} · {} table(s)", self.dir.display(), self.tables.len()),
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        f.render_widget(
            Paragraph::new(header).style(Style::default().bg(Color::Black)),
            chunks[0],
        );

        // Main: sidebar | results
        let main =
            Layout::horizontal([Constraint::Length(28), Constraint::Min(20)]).split(chunks[1]);
        self.render_tables(f, main[0]);
        self.render_results(f, main[1]);

        // Status bar
        let (left, style) = if self.running {
            (
                format!("{} running…", spinner(self.spinner)),
                Style::default().fg(Color::Yellow),
            )
        } else if let Some(e) = &self.last_error {
            (shorten(e, 60), Style::default().fg(Color::Red))
        } else if let Some(ms) = self.last_ms {
            (
                format!("✓ {ms:.1} ms · {} row(s)", self.total_rows),
                Style::default().fg(Color::Green),
            )
        } else {
            (
                "ready — type SQL, Enter runs".to_string(),
                Style::default().fg(Color::DarkGray),
            )
        };
        let right = format!(
            "{} · Tab switches · \\h help · \\q quit",
            match self.focus {
                Focus::Editor => "editor",
                Focus::Results => "results",
                Focus::Tables => "tables",
            }
        );
        let status = Line::from(vec![
            Span::styled(format!(" {left:<60}"), style),
            Span::styled(format!("{right:>50}"), Style::default().fg(Color::DarkGray)),
        ]);
        f.render_widget(Paragraph::new(status), chunks[2]);

        // Editor
        let cursor = if self.focus == Focus::Editor {
            "█"
        } else {
            " "
        };
        let prompt = Span::styled(
            "sql> ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
        let text = if self.input.is_empty() {
            vec![Line::from(vec![
                prompt,
                Span::styled(cursor, Style::default().fg(Color::Cyan)),
            ])]
        } else {
            let mut lines: Vec<Line> = Vec::new();
            let parts: Vec<&str> = self.input.split('\n').collect();
            for (i, p) in parts.iter().enumerate() {
                let mut spans = vec![if i == 0 {
                    prompt.clone()
                } else {
                    Span::raw("     ")
                }];
                spans.push(Span::raw(*p));
                if i == parts.len() - 1 {
                    spans.push(Span::styled(cursor, Style::default().fg(Color::Cyan)));
                }
                lines.push(Line::from(spans));
            }
            lines
        };
        let editor = Paragraph::new(text).block(
            Block::default()
                .borders(Borders::TOP)
                .border_type(BorderType::Plain)
                .title(Span::styled(
                    " SQL — Enter runs, Shift+Enter newline, ↑↓ history ",
                    Style::default().fg(Color::DarkGray),
                )),
        );
        f.render_widget(editor, chunks[3]);

        if self.help_open {
            self.render_help(f, area);
        }
    }

    fn render_tables(&self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .tables
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let style = if i == self.table_selected && self.focus == Focus::Tables {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else if i == self.table_selected {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default()
                };
                ListItem::new(format!(" {t}")).style(style)
            })
            .collect();
        let list = List::new(items).block(
            Block::default()
                .borders(Borders::RIGHT)
                .border_type(BorderType::Plain)
                .title(Span::styled(
                    " tables ",
                    Style::default().fg(Color::DarkGray),
                )),
        );
        f.render_widget(list, area);
    }

    fn render_results(&self, f: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::NONE).title(Span::styled(
            " results ",
            Style::default().fg(Color::DarkGray),
        ));
        let inner = block.inner(area);
        f.render_widget(block, area);

        if !self.text_lines.is_empty() {
            let lines: Vec<Line> = self
                .text_lines
                .iter()
                .map(|l| Line::raw(l.as_str()))
                .collect();
            f.render_widget(Paragraph::new(lines), inner);
            return;
        }
        if self.columns.is_empty() {
            let welcome = if self.tables.is_empty() {
                vec![
                    Line::from(Span::styled(
                        " ◈ LessDB — one database for agents and humans",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )),
                    Line::raw(""),
                    Line::raw("  No tables yet. Get value in 60 seconds:"),
                    Line::raw("    lessdb demo            seeded data + showcase queries"),
                    Line::raw("    lessdb sql             interactive REPL"),
                    Line::raw("    lessdb mcp             open it to AI agents (27 tools)"),
                    Line::raw(""),
                    Line::raw("  MIT License · lessdb.dev/docs/license"),
                ]
            } else {
                vec![Line::raw("(no results yet)")]
            };
            f.render_widget(Paragraph::new(welcome), inner);
            return;
        }

        // Column widths from headers, type names and values (capped).
        let mut widths: Vec<usize> = self
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let mut w = c.len();
                if let Some(t) = self.types.get(i) {
                    w = w.max(t.len());
                }
                w + 2
            })
            .collect();
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if let Some(w) = widths.get_mut(i) {
                    *w = (*w).max(cell.len().min(MAX_COL_WIDTH) + 2);
                }
            }
        }
        for w in &mut widths {
            *w = (*w).min(MAX_COL_WIDTH + 4);
        }

        let header = Row::new(self.columns.iter().map(|c| {
            Span::styled(
                c.as_str(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
        }));
        let type_row = Row::new(
            self.types
                .iter()
                .map(|t| Span::styled(t.as_str(), Style::default().fg(Color::DarkGray))),
        );
        let mut table_rows = vec![header, type_row];
        for row in &self.rows {
            let mut cells = Vec::new();
            for (i, cell) in row.iter().enumerate() {
                let text: String = cell
                    .chars()
                    .skip(self.hscroll)
                    .take(MAX_COL_WIDTH)
                    .collect();
                let _ = i;
                cells.push(Span::raw(text));
            }
            table_rows.push(Row::new(cells));
        }

        let constraints: Vec<Constraint> = widths
            .iter()
            .map(|w| Constraint::Length(*w as u16))
            .collect();
        let table = Table::new(table_rows, constraints)
            .column_spacing(1)
            .block(Block::default().borders(Borders::NONE));
        f.render_widget(table, inner);

        // Row count footer (rendered on the last line of the results area).
        let note = if self.shown_rows < self.total_rows {
            format!(
                "showing {} of {} rows — scroll with ↑↓ PgUp PgDn",
                self.shown_rows, self.total_rows
            )
        } else {
            format!("{} row(s)", self.total_rows)
        };
        let footer_area = Rect {
            x: inner.x,
            y: inner.y + inner.height.saturating_sub(1),
            width: inner.width,
            height: 1,
        };
        f.render_widget(
            Paragraph::new(Span::styled(note, Style::default().fg(Color::DarkGray))),
            footer_area,
        );
    }

    fn render_help(&self, f: &mut Frame, area: Rect) {
        const LUMO: &str = "       ●     ●\n\
       ▓     ▓\n\
      ▓       ▓\n\
     ▓▓▓▓▓▓▓▓▓▓▓\n\
    ▓███████████▓\n\
    ▓███◉███◉███▓\n\
    ▓███████████▓\n\
     ▓▒████████▒▓\n\
      ▓▓▓▓▓▓▓▓▓\n\
       ▄▄▄▄▄▄▄\n\
      ▄▄▄▄▄▄▄▄▄\n\
 ░░░  ▓▓▓▓▓▓▓▓▓  ░░░\n\
░░░░ ░▓▓▓▓▓▓▓▓▓░ ░░░░\n\
░░░░░ █████████ ░░░░░\n\
 ░░░░ █████████ ░░░░\n\
  ░░░ █████████ ░░░\n\
      █████████\n\
     ██▒▒▒▒▒▒▒██\n\
     █▒●▒▒▒▒▒●▒█\n\
     █▒▒●▒▒▒●▒▒█\n\
      ▒▒▒▒▒▒▒▒▒\n\
        ●●●";
        let mut lines: Vec<Line> = vec![
            Line::from(Span::styled(
                " LessDB keys",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
            Line::raw("  Enter         run the query (Shift+Enter = newline)"),
            Line::raw("  ↑ / ↓         history (editor), scroll (results/tables)"),
            Line::raw("  PgUp/PgDn     page the results · Home/End jump"),
            Line::raw("  ← / →         horizontal scroll of results"),
            Line::raw("  Tab           cycle editor → results → tables"),
            Line::raw("  Enter         in tables pane: describe the table"),
            Line::raw(""),
            Line::raw("  \\t  tables    \\d [t] describe   \\p [t] parts"),
            Line::raw("  \\o [t] optimize   \\h help   \\c clear   \\q quit"),
            Line::raw("  Ctrl+D quit (empty editor) · Ctrl+C quit · Esc closes"),
            Line::raw(""),
            Line::raw("  MIT License — lessdb.dev/docs/license  ·  `lessdb license`"),
        ];
        lines.extend(LUMO.lines().map(Line::raw));
        let width = 60;
        let height = lines.len() as u16 + 2;
        let popup = Rect {
            x: area.x + area.width.saturating_sub(width + 2) / 2,
            y: area.y + area.height.saturating_sub(height + 2) / 2,
            width: width.min(area.width),
            height: height.min(area.height),
        };
        f.render_widget(Clear, popup);
        f.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" help "),
            ),
            popup,
        );
    }
}

fn spinner(i: u8) -> char {
    const FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧'];
    FRAMES[(i as usize) % FRAMES.len()]
}

fn shorten(s: &str, max: usize) -> String {
    let s = s.lines().next().unwrap_or("");
    let mut out: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        out.push('…');
    }
    out
}

/// Execute SQL (plus LessDB DDL) against the session and render the outcome.
async fn execute(session: &LessSession, engine: &LessEngine, sql: &str) -> QueryOutcome {
    let upper = sql.trim().to_ascii_uppercase();
    if upper.starts_with("CREATE TABLE") {
        return match less_catalog::ddl::parse_create(sql) {
            Ok(parsed) => {
                let def = parsed.to_def();
                match engine.create_table(def.clone()) {
                    Ok(()) => {
                        let _ = session.refresh_async().await;
                        QueryOutcome::Text(vec![format!("created table {}", def.name)])
                    }
                    Err(e) => QueryOutcome::Err(e.to_string()),
                }
            }
            Err(e) => QueryOutcome::Err(e.to_string()),
        };
    }
    if upper.starts_with("DROP TABLE") {
        let name = sql["DROP TABLE".len()..]
            .trim()
            .trim_end_matches(';')
            .trim();
        return match engine.drop_table(name) {
            Ok(()) => {
                let _ = session.refresh_async().await;
                QueryOutcome::Text(vec![format!("dropped table {name}")])
            }
            Err(e) => QueryOutcome::Err(e.to_string()),
        };
    }
    if upper.starts_with("OPTIMIZE TABLE") {
        let name = sql["OPTIMIZE TABLE".len()..]
            .trim()
            .trim_end_matches(';')
            .trim();
        return match engine.optimize(name) {
            Ok(Some(m)) => QueryOutcome::Text(vec![format!(
                "merged {} rows into part {}",
                m.row_count, m.name
            )]),
            Ok(None) => QueryOutcome::Text(vec!["fewer than two parts; nothing to merge".into()]),
            Err(e) => QueryOutcome::Err(e.to_string()),
        };
    }

    let started = std::time::Instant::now();
    match session.sql_batches(sql).await {
        Ok(batches) => {
            let ms = started.elapsed().as_secs_f64() * 1000.0;
            let _ = ms;
            format_batches(&batches)
        }
        Err(e) => QueryOutcome::Err(format!("{sql}\n{e}")),
    }
}

fn format_batches(batches: &[RecordBatch]) -> QueryOutcome {
    if batches.is_empty() {
        return QueryOutcome::Rows(vec![], vec![], vec![], 0);
    }
    let columns: Vec<String> = batches[0]
        .schema()
        .fields()
        .iter()
        .map(|f| f.name().clone())
        .collect();
    let types: Vec<String> = batches[0]
        .schema()
        .fields()
        .iter()
        .map(|f| format!("{:?}", f.data_type()))
        .collect();
    let mut rows: Vec<Vec<String>> = Vec::new();
    for b in batches {
        for r in 0..b.num_rows() {
            let mut row = Vec::with_capacity(b.num_columns());
            for c in 0..b.num_columns() {
                let col = b.column(c);
                row.push(if col.is_null(r) {
                    "NULL".to_string()
                } else {
                    arrow::util::display::array_value_to_string(col, r)
                        .unwrap_or_else(|_| "?".to_string())
                });
            }
            rows.push(row);
        }
    }
    let total = rows.len();
    rows.truncate(MAX_RESULT_ROWS);
    QueryOutcome::Rows(rows, columns, types, total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use less_common::EngineConfig;

    #[tokio::test]
    async fn render_smoke_and_meta_commands() {
        let dir = std::env::temp_dir().join(format!("less-tui-{}", uuid::Uuid::new_v4()));
        let engine = LessEngine::open(EngineConfig::with_data_dir(&dir)).unwrap();
        let session = LessSession::new_async(engine.clone()).await.unwrap();
        let mut app = App::new(&dir, Arc::new(session), engine).await.unwrap();

        // Simulate a query outcome and render two frames (results + help).
        app.on_outcome(QueryOutcome::Rows(
            vec![
                vec!["1".into(), "1.5".into()],
                vec!["2".into(), "2.5".into()],
            ],
            vec!["id".into(), "v".into()],
            vec!["Int64".into(), "Float64".into()],
            2,
        ));
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        app.help_open = true;
        terminal.draw(|f| app.render(f)).unwrap();
        app.help_open = false;

        // Meta commands.
        app.meta("\\t");
        assert!(!app.text_lines.is_empty());
        app.meta("\\q");
        assert!(app.quitting);
        app.quitting = false;
        app.meta("\\unknown");
        assert!(app.text_lines[0].contains("unknown meta command"));

        // History navigation state machine.
        app.input.clear();
        app.history = vec!["SELECT 1".into(), "SELECT 2".into()];
        app.history_idx = None;
        app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.input, "SELECT 2");
        app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.input, "SELECT 1");
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.input, "SELECT 2");

        std::fs::remove_dir_all(&dir).ok();
    }
}
