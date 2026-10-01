# Agent Governance — one control plane, both front doors

Status: **proposal (v0.3/v0.7+)** · Owner: product

LessDB is one database with two front doors — MCP for agents, HTTP/SDK/CLI
for humans — and a third, always-on property: **everything that happens is
recorded and attributable**. This document specifies the control plane that
makes the shared-database story safe, and the concrete features that get us
there from what exists today.

## 1. Today (v0.1 reality, verified in code)

| Capability | Status |
|---|---|
| LDAP/AD + file auth with roles (`admin`/`read`/`write`) | ✅ HTTP server only (`less-auth`) |
| Fail-closed group→role mapping, injection-safe filters, auth-failure counters | ✅ HTTP server |
| MCP server, 27 tools (`less_*` shared engine, `context_*`/`memory_*`/`vector_*` tenant-scoped) | ✅ |
| **Append-only audit log** (`<data_dir>/audit/`, NDJSON, `lessdb audit`) | ✅ **implemented** — every MCP tool call recorded (caller, tool, role, outcome, SQL detail, duration) |
| **Agent tokens** (`lessdb token create/list`, SHA-256-hashed store at `auth/tokens.json`) | ✅ **implemented** |
| **MCP-door auth + tool→permission map** (`lessdb mcp --require-auth`, role ladder admin>write>read, fail-closed) | ✅ **implemented** |
| Per-agent tenancy (`lessdb mcp --tenant <name>`) | ✅ namespaced under `<data_dir>/tenants/` |
| Prometheus metrics incl. auth failures | ✅ |
| JWT/bearer tokens and mTLS for service-to-service access | ❌ roadmap (v0.7) |
| Table-level grants, RLS + masking, approvals, quotas, lineage | ❌ roadmap (P0/P1/P2 below) |

**The gap:** a human connecting over HTTP is authenticated and role-checked,
but an agent spawning `lessdb mcp --dir <data>` gets unrestricted access to the
shared engine — every table, every SQL statement, every optimize. That is the
single biggest trust blocker for the "agents and humans share one database"
positioning, and it is our natural wedge: **most agent data stores have no
audit story at all; most governed databases have no agent story at all.**

## 2. Principles

1. **One identity, both doors.** An agent principal and a human principal are
   the same object — a name, a role, a set of grants. No parallel worlds.
2. **Fail-closed everywhere.** The MCP door inherits the HTTP door's
   fail-closed philosophy: unknown caller → deny.
3. **Attributable by default.** Every authenticated action lands in the audit
   log with caller, tool/SQL, role, outcome, and cost (rows scanned). Agents
   are accountable exactly like humans — because they are just another
   caller.
4. **Humans hold the veto.** Grants, schema changes, and "dangerous" tool
   classes default to human approval. Agents propose; humans dispose.
5. **Boring primitives.** RBAC + audit + tenancy + approval — no ML, no
   "AI governance" magic. Deterministic, reviewable, compliance-shaped.

## 3. Feature spec

### 3.1 Agent identity (MCP auth) — new, P0

- `lessdb mcp --require-auth` (default once enabled): MCP `initialize` must
  present a bearer token (`Authorization` header in stdio/streamable
  transports).
- **Standards-aligned:** implement the SEP-1046 shape (OAuth 2.1
  client-credentials, RFC 7591 dynamic client registration, RFC 8707
  resource indicators) so agents are first-class principals with
  per-agent revocability — the same pattern the MCP spec is standardizing.
  Phase 1 may ship simpler HS256 tokens minted by
  `lessdb token create <name> --role read|write|admin [--tenant <t>] [--expires 30d]`
  (hashed in `<data_dir>/auth/tokens.json`, reusing `less-auth`), with the
  full OAuth resource-server flow as the follow-up.
- Ambient default for local dev: `--no-auth` flag, loudly logged, refused
  when a token store exists.

### 3.2 Tool → permission map — new, P0

Every MCP tool gets a permission class; the role check happens **before** the
tool executes:

| Tools | Permission |
|---|---|
| `less_query`, `less_explain`, `less_schema`, `less_stats`, `less_tables`, `vector_search`, `vector_list`, `context_get/find/neighbors/path` | `read` |
| `less_insert` (future), `less_optimize`, `context_put/link/unlink`, `memory_insert/update`, `vector_put/add` | `write` |
| DDL (`less_create`, `less_drop` when added), token management, `memory_create/drop` | `admin` |

- `read` role → read tools on shared engine + its own tenant tier.
- `write` role → read + write tools.
- `admin` role → everything, including schema changes.
- Denied calls return a structured error; the refusal is audited.
- **Standards hook:** each tool's permission class maps onto MCP tool
  annotations (`readOnlyHint`/`destructiveHint`) so clients can *display*
  intent, while LessDB *enforces* it — annotations are hints, our map is
  the enforcement.

### 3.3 Audit log — elevate existing v0.7 TODO to P0

Append-only NDJSON log, one per data dir:
`<data_dir>/audit/audit-<date>.ndjson` (+ object store for shared dirs).

```json
{"ts":"2025-…","caller":{"kind":"agent|human","name":"claude","tenant":"alice"},"door":"mcp|http|sdk","action":"tools/call","tool":"less_query","sql":"SELECT …","role":"read","outcome":"ok|denied|error","rows_scanned":4102559,"dur_ms":7.9}
```

- CLI: `lessdb audit [--since 1h] [--caller claude] [--outcome denied]`
- HTTP: `GET /v1/admin/audit` (admin), streamed to Prometheus counters.
- Bounded retention + `lessdb audit --rotate`; audit file is written
  synchronously before the action returns (denials must never be lost).

### 3.4 Human approval gates — new, P1

- `lessdb mcp --approval admin` mode: `write`/`admin`-class agent calls are
  **proposed**, not executed — emitted as a pending approval with id, shown
  via `less approvals list`, accepted/rejected via
  `less approvals accept <id> | reject <id>` (or HTTP
  `/v1/admin/approvals`). Approvals expire (default 10 min).
- `--approval off` restores direct execution for trusted agents; every
  approval decision is audited with the human's identity.
- This is the "humans hold the veto" primitive — the feature that answers
  "what if my agent writes something it shouldn't?" deterministically.

### 3.5 Guardrails for the shared engine — new, P1

- **Schema governance:** DDL is `admin`-class by default (3.2). Config flag
  `allow_agent_ddl = false` keeps agents out of schema entirely.
- **Cost limits:** per-caller quotas (`max_rows_scanned_per_query`,
  `max_concurrent_queries`) enforced in `less-query`; violation → denied +
  audited (pairs with the existing `memory_limit` work).
- **Table-level grants:** `GRANT read ON events TO agent:claude` stored in
  the catalog (`less grant/revoke`), checked at pruning time — cheap because
  the planner already resolves tables per query.
- **Data classification:** optional column/table tags (`PII`, `internal`) in
  `CREATE TABLE` options; `deny_pii_for role:read` policy evaluated before
  execution. (Matches the deterministic-policy pattern buyers already know.)

### 3.6 Tenancy completion — extend existing P2 work

- Tenancy exists for the memory tier; extend `--tenant` to scope
  `less_*` table visibility via table-level grants (3.5) so multi-agent
  deployments can't cross-read.
- Tenant listing + admin tooling: `less tenants list`.

### 3.7 Row-level security + column masking — new, P2

Enterprise buyers check RLS and masking first — they are the de-facto test
of "real" governance (RESEARCH §3). Design:

- `CREATE TABLE … ROW POLICY <name> AS (<SQL predicate>)` — predicate
  evaluated per query session against the caller's role/attributes; merged
  into the scan (`WHERE` pushdown) so pruned parts keep it cheap.
- `MASK <col> [TO role] AS <expr>` for column-level masking of PII.
- Policies are declarative, stored in the catalog, applied uniformly on
  **all three doors** (MCP, HTTP, SDK) — one policy plane, not three.

### 3.8 Data lineage — new, P2

- Column-level lineage via OpenLineage events emitted from query execution
  (inputs/outputs of each query and merge), consumed by Marquez/whatever
  the org already runs. Pairs with the audit log: audit = who did what;
  lineage = what data produced what.

## 4. Where this lands in the code

| Feature | Crates touched |
|---|---|
| Token store, role model reuse | `less-auth` (new `TokenStore`, `Authenticator` impl) |
| MCP auth + permission map | `less-mcp` (initialize/authz middleware, tool metadata `permission` field) |
| Audit writer + `lessdb audit` | `less-telemetry` (new `AuditLog`), `less-cli`, `less-server`, `less-mcp` |
| Approvals | `less-mcp` (pending store), `less-cli`/`less-server` admin surface |
| Quotas | `less-query` (execution guards) |
| Table grants + policy eval | `less-catalog` (grants in table manifest), `less-query` (pre-exec check) |

## 5. Sequencing (revised roadmap)

1. **P0 — audit log + MCP tokens + tool→permission map** (weeks): closes the
   trust gap, unblocks the "shared database" story end-to-end.
2. **P0 — table grants** on top of the catalog (small: manifest field +
   check).
3. **P1 — approvals + quotas + schema governance flags.**
4. **P1 — OAuth 2.1/PKCE resource-server flow on the MCP door** +
   JWT/mTLS for service-to-service (already in v0.7), TLS for MCP
   streamable transports.
5. **P2 — RLS + column masking + OpenLineage**, signed policy bundles
   (Cedar/Rego hook), OpenTelemetry traces — the enterprise compliance
   tier.

Each stage ships with CLI, HTTP, and MCP surfaces simultaneously — the
control plane is a feature of *the database*, not of one integration.
