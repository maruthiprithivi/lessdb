# LessDB — Product Positioning

Status: **adopted** · Owner: product · Updated: 2025

How LessDB positions itself — a fit that comes from what the product
*already is*, validated against market research. Evidence base:
[RESEARCH.md](./RESEARCH.md) (four compiled streams in
[docs/research/](./research/)). Product consequences:
[AGENT-GOVERNANCE.md](./AGENT-GOVERNANCE.md).

---

## 1. The position (one sentence)

> **LessDB is the analytical database your agents and your analysts share.**
> One governed SQL system of record — MCP-native for agents, built for
> analysts — with identical governance on both sides.

Supporting line (the two-doorway frame):

> **One engine, two front doors.** Agents work it over 27 MCP tools;
> humans work it over SQL, the REPL, and the SDKs. Same engine, same data,
> same control, one audit trail.

## 2. Why this is a natural fit, not a forced one

The product already *is* this in three verifiable ways:

1. **Two doors already exist.** `lessdb sql` (human) and `lessdb mcp`
   (agent) are two commands on one binary over the same tables. No
   narrative contortion required — the demo shows it.
2. **The multi-model store already exists.** SQL tables, vector spaces,
   a linked knowledge graph, and RAM tables share one data dir. Research
   shows the market forces an either/or choice (vector DB *or* graph DB
   *or* SQL store) while agents actually juggle all three — we remove the
   forced choice (RESEARCH §1, gaps).
3. **The governance gap is the market's loudest pain.** ~200k exposed MCP
   instances, "the protocol that trusted everyone", agents inheriting full
   human DB roles, and a viral incident where a coding agent wiped a
   startup's database and backups in 9 seconds (RESEARCH §1–§3). Our
   answer — one role model, per-agent identity, one audit trail — is the
   feature set enterprise buyers already articulate as their "full control"
   checklist: per-agent identity, row/column permissions, immutable audit,
   write-approval gates (RESEARCH §2).

## 3. What we are NOT (the discipline)

- **Not an "agent memory" product.** The memory category is crowded and
  commoditizing (Mem0, Letta, Zep, LangMem, Cognee); analyses argue memory
  wrappers are a dead end because the underlying store is the moat. Memory
  is one of our features, never our category (RESEARCH §1).
- **Not "AI-native" marketing.** The claim is low-signal and now carries
  SEC "AI-washing" risk. Our claims are boring and verifiable: SQL, roles,
  audit, tenancy.
- **No competitor names in marketing copy.** The story stands on its own;
  naming others borrows their frame instead of owning ours.
- **Not "single source of truth" / "memory layer" language** — strip-mined
  terms; we use "shared system of record", which research flags as
  under-used whitespace (RESEARCH §4).
- **Not abandoning humans to chase agents.** Both audiences are first-class;
  that is the product's entire point.

## 4. Positioning pillars (messaging hierarchy)

1. **One database, two front doors.** Agents over MCP, humans over SQL —
   same engine, same data, no sync, no exports, no drift.
2. **Full control and governance for both.** One identity model, one role
   model, one audit trail. An agent is just another caller — with a
   credential, a role, and a record of everything it did. (The wedge:
   agent data stores lack audit; governed databases lack agents.)
3. **More with less.** Columnar storage with zstd, bloom pruning, 42 MB
   RSS, GPU kernels, stateless compute — the resource-efficiency story the
   name promises.
4. **Boring, open, embeddable.** Rust, MIT, one binary, Arrow-native.
   Works with the tools both audiences already use.

## 5. Competitive frame (from research)

| Neighbor | Owns | Doesn't own | Our wedge |
|---|---|---|---|
| Vector DBs (hosted/embedded) | Embedding search | SQL, governance, graph | The store vectors graduate into when they must meet tables and audits |
| Embedded analytics engines | Human SQL, no server | First-class agent door with permissions | Same power plus a governed, MCP-native agent door |
| Memory frameworks | Session state as a library/app | The store underneath (their moat problem) | We are the store underneath — not the app on top |
| Warehouses / lakehouse platforms | Enterprise governance + scale | Embeddability, open single-binary, agent-native tooling | The embedded, open-source version of that trust |
| MCP gateways (Kong/Tyk/Gravitee…) | Enforcing policy in front of *other* databases | Being the database | Governance built into the store itself — one less hop, one audit trail |

## 6. Audiences (the two doorways)

| | Agents | Humans |
|---|---|---|
| **Door** | MCP (stdio), 27 tools | CLI/REPL, Python/Node SDKs, HTTP API |
| **Work** | Query, search, link, persist context across sessions | SQL analytics, ingestion, dashboards, pipelines |
| **Needs** | Tool-shaped access, cheap calls, persistence, *permission to act* | Full SQL, ecosystem interop (Arrow/pandas), *visibility + control* |
| **Shared** | The same tables, vectors, graph — and the same governance | |

The doorway pattern follows the DuckDB model (one architectural truth,
audience-specific entrances) — not two products (RESEARCH §4).

## 7. GTM shape

- **Land:** one-command curl install → `lessdb init` → first SQL query → MCP
  connection. The website's quickstart is exactly this path; the demo shows
  both doors side by side.
- **Expand:** Python/Node embedding in existing data stacks; MCP tools in
  agent workflows; per-agent tenancy for multi-agent teams.
- **Monetize (open-core):** the governance tier. Research is unambiguous:
  enterprises pay for SSO/LDAP/OIDC, RBAC, audit, compliance, observability
  — and governance is now being sold as *agent governance*, which maps
  directly onto our pillar #2. Paid tier: audit retention + signed policy
  bundles + approval workflows + table-grant management + OTel export.
- **Proof assets:** the deterministic bundled benchmark harness + the audit
  log as a demo-able feature ("denied · role: read" sells itself).

## 8. Website changes implied

- Hero: "One database for agents and humans" + shared-system-of-record
  sub-line + "One audit trail" in the fine print.
- New **Control & governance** section: same roles on every door, per-agent
  identity/tenancy, one audit trail, human approval gates — with an audit
  terminal demo (including a denied agent call).
- Quickstart leads with the shared path: install → ingest → query → agent
  door. LUMO the firefly stays as the mascot across the site.

## 9. Product changes implied (priority order)

See [AGENT-GOVERNANCE.md](./AGENT-GOVERNANCE.md) for the full spec.

1. **P0 — close the trust gap:** MCP auth (SEP-1046-style client
   credentials), tool→permission map, append-only audit log.
2. **P0 — table-level grants** in the catalog (cheap, unblocks tenancy
   completion).
3. **P1 — human approval gates** (propose→approve→commit for agent writes)
   + quotas + schema-governance flags.
4. **P1 — standards alignment:** MCP tool annotations surfaced as
   permissions; OAuth 2.1/PKCE on the MCP door; OPA/Cedar policy hook.
5. **P2 — the enterprise test:** row-level security + column masking, then
   data lineage (OpenLineage) — the two primitives buyers check first.
