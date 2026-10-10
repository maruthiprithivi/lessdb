# MCP — the agent door

LessDB speaks the Model Context Protocol natively. The same engine your
humans query with SQL, your agents drive through a typed tool interface —
with per-token roles, memory-tier tenant scoping, and an audit trail for MCP calls. No
separate MCP proxy, no second database: one binary, two doors.

## The two ways to run it

| | **stdio (local)** | **HTTP (hosted)** |
|---|---|---|
| Command | `lessdb mcp [--dir .less]` | `lessdb server --addr 0.0.0.0:7080` |
| Transport | stdio JSON-RPC, spawned by your agent | POST `/mcp` (streamable-HTTP flavoured, JSON or SSE) |
| Who hosts it | **You**, on the machine running the agent | **You or your team**, on a server/VPS/box |
| Data | Local `.less` dir, or any FireflyCloud shared dir | Server-side `.less` dir; put tables on FireflyCloud to share across nodes |
| Auth | Optional (`--require-auth`), open for local dev | **Always fail-closed** — agent tokens required |
| Best for | Personal agents, laptops, air-gapped work | Team agents, CI agents, a "database as a service" for your org |

Both doors expose the **same 28 tools** over the same engine; hosted HTTP is fail-closed while local stdio auth is optional for development: SQL
(`lessdb_query`, `lessdb_explain`, `lessdb_schema`, `lessdb_stats`,
`lessdb_tables`, `lessdb_optimize`), agent memory (`context_*`, `memory_*`),
vector search (`vector_*`), and graph (`lessdb_cypher`).

For new agent applications, place a small context broker above these tools.
The broker opens an identity-bearing session, observes the current situation,
recalls a bounded ContextPacket, records decisions and remembers the outcome.
The packet should carry identity, situation, intent, source references, scope,
validity, omissions and item/byte budget before any prompt is rendered. See
the [agent deployment blueprints](/docs/agent-blueprints) and
[adoption scenarios](/docs/agent-scenarios).

## 1. Local setup (stdio) — the 30-second path

Create a database, then register the server with your agent. The command
is the same everywhere; only the config file differs:

```json
// Claude Code:  claude mcp add lessdb -- lessdb mcp --dir /abs/path/to/mydb
// Codex:        ~/.codex/config.toml  →  [mcp_servers.lessdb]
// Cursor/Claude Desktop: settings JSON
{ "mcpServers": { "lessdb": {
    "command": "lessdb",
    "args": ["mcp", "--dir", "/abs/path/to/mydb"]
} } }
```

```sh
lessdb init --dir mydb                     # one-time
lessdb mcp --dir mydb                      # stdio server on mydb — 28 tools
```

**What you just got**: the agent can now run SQL over your tables,
create its own durable notes and links (`context_put`/`context_link`),
keep hot lookup state in RAM tables (`memory_*`), run vector search over
your embeddings, and query relationships with `lessdb_cypher` — and
every call lands in `lessdb audit`.

## 2. Hosted setup (HTTP) — one server, many agents

Run the door once, let every agent (and every teammate) point at it:

```sh
# on the server
lessdb init --dir /data/db
lessdb token create alice-agent --role read      # read-only agent
lessdb token create ops-bot    --role write      # may INSERT/UPDATE
lessdb token create root       --role admin      # may OPTIMIZE/DDL
lessdb server --addr 0.0.0.0:7080 \
       --tls-cert /etc/lessdb/cert.pem --tls-key /etc/lessdb/key.pem
```

Register the hosted server from any machine:

```sh
claude mcp add --transport http lessdb \
  https://your-host:7080/mcp \
  --header "Authorization: Bearer ldb_<token-from-create>"
```

- **HTTPS is on you**: `--tls-cert/--tls-key` serve TLS directly; behind
  nginx/Caddy, terminate TLS there and proxy `/mcp` and `/v1/*`.
- The `/mcp` door is **always fail-closed**: without a valid token,
  `tools/call` is denied (the `initialize` handshake still answers, so
  clients get a clean error instead of a timeout).
- Put tables on **FireflyCloud** (`ENGINE = FireflyCloud`, shared object
  storage) and every node — server or laptop — sees the same data.
- The SQL HTTP API (`/v1/sql`) uses its own Basic-auth users; agent
  tokens are for `/mcp`. Both are audited.

## 3. Auth, roles, tenants

```sh
lessdb token create <name> --role read|write|admin [--tenant <t>] [--ttl-days 30]
```

| Role | Can |
|---|---|
| `read` | `SELECT` via `lessdb_query`, schema/stats/explain, reads of context/memory/vector |
| `write` | everything read can, plus `INSERT`/`UPDATE`/`DELETE`, `context_*` writes, `memory_*` writes, vector writes |
| `admin` | everything, plus `lessdb_optimize`, DDL, token management |

* **Tokens are stored hashed** (SHA-256) in `.less/auth/tokens.json`; the
  plaintext prints once at creation.
* **Tenant scoping** — `lessdb mcp --tenant billing` (stdio) isolates the
  memory tier (context/memory/vector namespaces) per agent, so agents
  sharing one engine never see each other's notes.
* **Audit everything** — `lessdb audit` shows caller, tool, SQL, and
  outcome for every MCP call; stream it to your SIEM for compliance.

Tenant namespaces are not a universal row-level ACL for shared `less_*` SQL
tables. Scope, consent, temporal filtering, packet budgets, handoffs,
retention jobs and approval gates are application or broker responsibilities
unless a separate native feature is explicitly documented. A model completion
is not an approval or an action result.

## 4. The 28 tools

| group | tools | purpose |
|---|---|---|
| SQL | `lessdb_query` `lessdb_explain` `lessdb_schema` `lessdb_stats` `lessdb_tables` `lessdb_optimize` | analytics over Firefly/FireflyCloud tables |
| context | `context_put` `context_find` `context_get` `context_link` `context_unlink` `context_neighbors` `context_path` `context_delete` | durable linked notes with typed edges, BFS, shortest paths |
| memory | `memory_create` `memory_insert` `memory_sql` `memory_get` `memory_tables` `memory_compact` | RAM tables with O(1) latest-row point lookups |
| vector | `vector_*` | create spaces, add vectors, `vector_search` SQL function |
| graph | `lessdb_cypher` | openCypher `MATCH` with variable-length patterns |

## 5. The LessDB skill for coding agents

Want your coding agent to just *know* all of this? Install the skill
(see the [Skills guide](/docs/skills)):

```sh
mkdir -p .claude/skills/lessdb
curl -fsSL https://lessdb.dev/skills/lessdb/SKILL.md -o .claude/skills/lessdb/SKILL.md
```

The skill teaches the agent when to reach for LessDB, how to explore a
database safely (schema → explain → query), the context/memory/vector
patterns, and the role-safety rules — so `SELECT`-heavy exploration
doesn't burn `write` tokens.

## 6. Troubleshooting

| Symptom | Fix |
|---|---|
| `access denied: tool … requires role …` | Token's role is too low — mint a higher-role token and use it. |
| `authentication required` from a hosted server | You didn't send `Authorization: Bearer ldb_…` (or the token expired). |
| Client times out on `initialize` | The server answers initialize even without a token — if it hangs, check network/TLS. |
| `tenant 'x' does not exist` | The tenant namespace is created lazily; initialize with a token minted for that tenant. |
| Nothing in `lessdb audit` | Audit entries live under the **data dir** the door was started with — check `--dir`. |

**Value, in one line**: the moment you register LessDB with an agent,
every conversation gains a shared, durable, auditable memory and a real
analytics engine — instead of files the agent wrote and forgot.
