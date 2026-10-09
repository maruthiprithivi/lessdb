# Multi-tenant agent platform

**What**: You're building (or running) a product where many agents —
or many customers' agents — share one database. Each needs private
memory, controlled access, and a single engine to operate on.

For the broader progression from one assistant to a governed organization,
see the [agent deployment blueprints](/docs/agent-blueprints) and the
[adoption scenarios](/docs/agent-scenarios).

**Why LessDB**:

- **Tenant scoping is built in**: `lessdb mcp --tenant acme` isolates
  each agent's context notes, RAM tables, and vector spaces under
  `<data_dir>/tenants/acme/` — while SQL tools stay on the shared
  engine. Row-level isolation, consent, handoffs and application action
  policy are yours to design; namespace isolation is the native boundary.
- **Tokens with roles**: mint `read`/`write`/`admin` tokens per agent
  or per tenant; every call is attributable and audited.
- **One hosted door**: `lessdb server` serves authenticated MCP over
  HTTP (`/mcp`) plus the SQL API — the whole platform is one process.
- **FireflyCloud** puts the shared tables on object storage so the
  platform scales compute horizontally without moving data.

**How, step by step**:

```sh
# 1. platform database on shared storage (tables every tenant's agents query)
lessdb init --dir platform
lessdb create --dir platform "CREATE TABLE events (...)
    ENGINE = FireflyCloud ORDER BY (tenant_id, ts)"

# 2. one token per tenant, scoped roles
lessdb token create acme-reader  --role read  --tenant acme
lessdb token create acme-writer  --role write --tenant acme
lessdb token create acme-admin   --role admin --tenant acme

# 3. one hosted door for everyone (TLS in front, or --tls-cert/--tls-key)
lessdb server --dir platform --addr 0.0.0.0:7080

# 4. each customer's agent connects with its own token
claude mcp add --transport http lessdb-acme https://platform:7080/mcp \
  --header "Authorization: Bearer ldb_<acme-token>"
```

```sh
# 5. the platform operator sees everything
lessdb audit --dir platform          # caller, tenant, tool, SQL, outcome
```

**Value**: the "database for agents" becomes a real product surface —
per-tenant privacy, per-token permission, complete auditability — from
one binary, with no vendor lock-in on the storage underneath.
