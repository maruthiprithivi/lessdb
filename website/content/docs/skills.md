# Skills — teach your coding agent LessDB in one file

A **skill** is a markdown file your coding agent loads on demand:
frontmatter (name + description) tells it *when* to use it, the body
tells it *how*. LessDB ships one — it turns any Claude Code / Codex /
Cursor-style agent from "can I have a database?" into "I already know
this database."

## Install

### Claude Code

```sh
# project-wide (checked into the repo — the whole team gets it)
mkdir -p .claude/skills/lessdb
curl -fsSL https://lessdb.dev/skills/lessdb/SKILL.md -o .claude/skills/lessdb/SKILL.md

# or just for you, every project
mkdir -p ~/.claude/skills/lessdb
curl -fsSL https://lessdb.dev/skills/lessdb/SKILL.md -o ~/.claude/skills/lessdb/SKILL.md
```

Then just talk: *"check the events table and find last week's top pages"*
— the agent loads the skill, runs `lessdb_schema`, `lessdb_explain`,
then the query.

### Codex / Cursor / any agent

The same file works anywhere agents read markdown skills — drop
`SKILL.md` into the agent's skills directory, or paste it into the
project's agent instructions (`AGENTS.md` / `.cursor/rules`) when your
agent has no skill loader.

## What the skill teaches

* **Explore → explain → query**: schema first, plan second, query third —
  the discipline that keeps agents from scanning 100M rows by accident.
* **Two memories, right tool**: durable `context_*` notes (survive
  restarts) vs `memory_*` RAM tables (O(1) lookups, die with the
  process).
* **SQL that fits the engine**: `ORDER BY` pruning, `UNIQUE` upserts,
  `TTL`, `read_parquet()`/`read_csv()`, `vector_search()` joins.
* **Safety**: read-first exploration, never drop/truncate without being
  asked, respect token-role denials, batch inserts.

## Why skills + MCP together

The skill is *knowledge*; MCP is *access*. With both:

1. The agent already knows the tool names and patterns — no flailing.
2. Its queries run through the audited MCP door (roles, tenants, audit).
3. You get the same engine your dashboards use, not a toy the agent
   invented.

The canonical public skill is served at
[`/skills/lessdb/SKILL.md`](https://lessdb.dev/skills/lessdb/SKILL.md). New
patterns you develop can be contributed through the project's source
workflow — one skill, shared by every LessDB user's agent.
