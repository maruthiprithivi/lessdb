---
name: less-context
description: Use LessDB's in-memory context graph and memory tables to store durable, linked agent memory (notes, decisions, tasks, project state) via the MCP context_* and memory_* tools or the less CLI. Use whenever the user asks to remember something, organize knowledge, or work with persisted agent context.
---

# LessDB Context Skill

LessDB ships an in-memory **context graph** + **memory tables** tier for
agent memory — a structured replacement for Obsidian-style vaults and
standalone graph databases. Everything persists under `<data_dir>/memory/`
(the CLI layout). Through the MCP server, state is additionally scoped to
an agent tenant: `less mcp --tenant <name>` stores that agent's contexts,
memory tables and vector spaces under `<data_dir>/tenants/<name>/`, so
agents sharing a server do not see each other's memory.

## Access

- **MCP tools** (preferred): `context_put`, `context_get`, `context_find`,
  `context_link`, `context_unlink`, `context_neighbors`, `context_path`,
  `context_delete`, `context_stats`, plus `memory_*` for tabular memory.
- **CLI**: `less context {put,get,find,link,unlink,neighbors,path,stats,delete}`
  and `less memory {create,insert,get,sql,compact,tables}`.

## Patterns

- **Durable facts & decisions**: `context_put` with a stable, readable key
  (`proj/<name>`, `decision/<slug>`, `task/<id>`, `person/<handle>`).
  Include tags; body text is searchable.
- **Recall**: `context_find "query"` — ranked matches over keys, titles,
  tags and text. Search before answering "what do we know about X".
- **Structure**: `context_link` with meaningful edge kinds
  (`depends_on`, `part_of`, `mentions`, `blocked_by`, `authored_by`).
  Undirected links are the default; use directed for one-way relations.
- **Explore**: `context_neighbors` (BFS, depth-limited) to see the
  neighborhood of a node; `context_path a b` for shortest paths.
- **Tabular memory**: `memory_create` (fields + optional pk), then
  `memory_insert` rows; `memory_get` reads the latest row for a pk;
  `memory_sql` runs joins/aggregations over all memory tables;
  `memory_compact` dedups by pk keeping the last row.
- **Update semantics**: `context_put` on an existing key merges properties
  and replaces title/text/tags; `context_delete` cascades to edges;
  `memory_insert` appends (duplicates visible in SQL until `memory_compact`,
  while `memory_get` already returns the latest).

## Workflow for "remember this"

1. `context_find` — check whether something related exists.
2. `context_put` the new note with a good key and tags.
3. `context_link` it to related notes (auto-creates missing endpoints).
4. Optionally `context_stats` to confirm counts.
