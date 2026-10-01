# Agent memory & knowledge graphs (MCP)

**What**: Give your coding agents a *durable* memory — linked notes,
hot lookup tables, vector recall, and a graph — plus real SQL analytics,
through one MCP server. LessDB replaces the "context vault + graph DB +
vector store + analytics DB" stack with one process.

**Why LessDB**:

- Agents forget between sessions; LessDB doesn't — notes and links are
  snapshotted atomically to disk, crash-safe by construction.
- The agent's memory lives in the **same engine** as your analytics, so
  an agent's findings are instantly queryable by humans — and vice
  versa. No export step, no "what did the agent see?" mystery.
- Tenant scoping isolates one agent's memory from another's on a shared
  engine, and every call is audited.

**How, step by step**:

```sh
# 1. one-time setup: a database + a token so the door isn't wide open
lessdb init --dir agentdb
lessdb token create dev-agent --role write
lessdb mcp --dir agentdb --require-auth

# 2. register with your agent (Claude Code / Codex / Cursor)
claude mcp add lessdb -- lessdb mcp --dir "$PWD/agentdb" --require-auth
# (or hosted: lessdb server … + claude mcp add --transport http …)
```

```text
# 3. the agent works — and remembers
agent: lessdb_query   → SELECT count(*) FROM incidents WHERE status='open'
agent: context_put    → key "incident/42", title "DB failover", tags ["incident","postmortem"]
agent: context_link   → "incident/42" -[follows_up]-> "incident/39"
agent: context_path   → "incident/39" → "incident/42"          (shortest path)
agent: memory_create  → sessions(id Int64, user String, ts Timestamp)
agent: memory_get     → key: sessions/id = 7                    (O(1), latest row wins)
agent: lessdb_cypher  → MATCH (a:incident)-[:depends_on*1..3]->(b) RETURN a.id, b.id
```

```sh
# 4. humans see the same world the agent does
lessdb sql --dir agentdb "SELECT count(*) FROM incidents WHERE status='open'"
lessdb audit                 # every agent call: caller, tool, SQL, outcome
```

**Want your agent to know all this without prompting?** Install the
[LessDB skill](/docs/skills) — one markdown file that teaches the
tool set, the patterns, and the safety rules.

**Value**: your agents stop being amnesiac contractors and become
long-lived teammates whose memory your whole team can query.
