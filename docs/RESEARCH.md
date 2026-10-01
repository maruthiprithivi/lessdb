# LessDB — Market Research (compiled)

Status: **evidence base for [POSITIONING.md](./POSITIONING.md)** · Updated: 2025

Four research streams (market landscape, agent data challenges, governance,
positioning playbooks), each with sources. Statistics from third-party
trackers/vendors are directional, not audited.

---

## 1. Landscape — databases & data stores for AI agents

### How agents connect today
Three paths: **MCP servers** (dominant new pattern), **native SDKs/APIs**,
**plain SQL** (often through text-to-SQL layers).

### Who is wired in
| Store | Positioning today | Agent connection |
|---|---|---|
| SQLite | Local default for single-agent tooling | Official MCP server (known path-traversal bug Anthropic declined to fix — The Register, June 2025) |
| DuckDB | "In-process SQL OLAP" | Third-party MCP servers; DIY memory projects |
| Postgres + pgvector | Enterprise safe default | Community MCP servers; vector as an extension |
| LanceDB | "Developer-friendly database for AI" (OSS, embedded, Rust/columnar) | SDK/Python; community MCP server |
| Chroma | "AI-native" embedding database | Official chroma-mcp |
| Pinecone/Qdrant/Weaviate/Milvus | Hosted vector DBs | MCP servers + SDKs; Pinecone reframing as "knowledge layer" |
| Neo4j | Graph/KG agent memory, "graph-native agents" | Official Neo4j MCP |
| Redis | Agent-memory story (vector + JSON/array) | SDKs |
| Databricks/Snowflake/BigQuery | Warehouses bolting on agent access | MCP servers + proprietary agent surfaces |

### MCP ecosystem facts
- ~5,800–10,000 MCP servers reported by trackers (self-reported registry
  counts, not audited).
- Database MCP servers are plentiful but **mostly community-built**, with
  coarse permissions.
- The consistent user complaint: **security and permissions**. Reports of
  ~200,000 exposed MCP instances; MCP called "the protocol that trusted
  everyone". Specific issues: uncontrolled writes, prompt/tool poisoning,
  missing auth, command injection. Users demand read-only/allowlist defaults
  and fine-grained write controls — exactly the surface our 27-tool MCP
  server touches.

### Category claims today
- "AI-native database" is already a common (diluted) claim: Weaviate,
  Chroma, Infoworld's "dawn of the AI-native database".
- Couchbase is most aggressive on agent framing ("AI Data Plane — serves and
  remembers data for AI agents", "persistent agent memory to the edge").
- Category names circulating: agent memory, agent storage, AI data platform,
  memory layer, "Agent-Native".
- **No player owns an agent-native database category** — it's contested by
  vector, graph, OLAP, and incumbent vendors.

### Gaps found
1. **Multi-model gap** — agents juggle vectors + relational + graph; the
   market forces an either/or choice (vector DB vs graph DB debates). A
   store with SQL + vectors + graph + RAM tables in one engine fills a real
   hole.
2. **Embedded beats external for agents** — "The Case Against External
   Vector DBs for Agent Memory" (Vectorize/Hindsight) argues in-process
   stores win for agent workloads.
3. **No standard permission/schema model for agent access** — most DB MCP
   servers expose raw SQL with coarse permissions; explain/schema/stats
   surfaces are usually missing.
4. **Context-window pressure** — memory must be queryable on demand, not
   stuffed into context (the driver behind DIY two-tier memory projects).

### Agent-memory category: crowded and commoditizing
Mem0 ($24M raised, "memory layer for AI apps"), Letta/MemGPT, Zep/Graphiti
(20k stars, temporal KG), LangMem, Cognee — plus analyses arguing pure
memory frameworks are a dead end **because the underlying store is the
moat**. Conclusion: don't position as "agent memory" — position as the
database underneath.

### Sources
- The Register — Anthropic won't fix SQLite MCP server bug: https://www.theregister.com/software/2025/06/25/anthropic-wont-fix-a-bug-in-its-sqlite-mcp-server/
- Infoworld — dawn of the AI-native database: https://www.infoworld.com/article/4080483/the-dawn-of-the-ai-native-database.html
- Couchbase AI Data Plane: https://www.blocksandfiles.com/ai-ml/2026/06/30/couchbase-wants-to-serve-and-remember-data-for-ai-agents/ · https://venturebeat.com/data/ai-agents-need-context-everywhere-they-run-even-where-the-cloud-cant-follow
- MCP scale/security: https://agentmarketcap.ai/blog/2026/04/06/mcp-18-months-5800-servers-security-debt-enterprise-adoption · https://lyrie.ai/research/research/mcp-security-crisis-architecture-flaws-200k-exposed-supply-chain · https://www.dreamfactory.com/hub/model-context-protocol-security-statistics/ · https://safeguard.sh/resources/blog/mcp-server-permissions-a-practical-checklist-for-reducing-blast-radius
- Embedded vs external vector DBs: https://hindsight.vectorize.io/blog/2026/05/12/case-against-external-vector-dbs-agent-memory
- Vector-vs-graph either/or: https://atlan.com/know/vector-store-vs-graph-database-agent-memory/ · https://machinelearningmastery.com/vector-databases-vs-graph-rag-for-agent-memory-when-to-use-which/ · https://next.growth.devrev.ai/blog/knowledge-graph-vs-vector-database
- Memory startups: https://finance.yahoo.com/news/mem0-raises-24m-yc-peak-151413348.html · https://blog.getzep.com/graphiti-hits-20k-stars-mcp-server-1-0/ · https://www.langchain.com/blog/langmem-sdk-launch · https://www.cognee.ai/open-source-memory-frameworks-llm-agents · https://agentmarketcap.ai/blog/2026/04/12/ai-agent-tool-category-shakeout-2026
- Database MCP servers: https://mcp.directory/blog/best-database-mcp-servers-2026 · https://chatforest.com/reviews/vector-database-embedding-mcp-servers/
- LanceDB: https://docs.lancedb.com/faq/faq-oss · Chroma MCP: https://github.com/chroma-core/chroma-mcp · Pinecone pricing: https://www.pinecone.io/pricing/pods/ · https://agentset.ai/vector-databases

---

## 2. Agent data challenges — failure modes & memory architecture

_Full report: [research/agent-data-challenges-2025.md](./research/agent-data-challenges-2025.md)_

### What breaks when agents touch databases
- **The viral incident:** a Claude/Cursor coding agent wiped a startup's
  database *and its backups* in 9 seconds (~30-hour recovery). Second
  documented case: Replit Agent deleted a production database.
- Recurring root causes: no row/table/rate caps, hallucinated SQL on real
  tables, **agents inheriting the human's full DB role**, no prod/staging
  separation, no pre-write audit.
- Schema drift is now a named product category (PingCAP TiNE — DB branching
  for agents).

### Memory architecture consensus
- "Memory engineering" is a named discipline (MongoDB, Inngest durable
  agents).
- **Vector-only memory is called insufficient** — FalkorDB: the real problem
  is retrieval/graph architecture, not embeddings. Validates LessDB's
  vector + knowledge-graph hybrid.
- Missing in the market: versioned/undoable memory, permission-aware
  recall, entity provenance, eviction policy.

### The "full control" checklist enterprise buyers articulate
1. per-agent identity
2. row/column-level query-time permissions
3. immutable agent→action audit trail
4. human approval on writes

### Sources
- 9-second DB wipe: https://www.techspot.com/news/112207-ai-coding-agent-running-claude-wiped-startup-database.html · https://indianexpress.com/article/technology/artificial-intelligence/cursor-ai-agent-wipes-out-startup-database-in-9-seconds-10544837/
- Replit deletion: https://safeguard.sh/resources/blog/replit-agent-database-deletion-vibe-coding-2025
- TiNE branching: https://www.pingcap.com/blog/database-branching-ai-agents-tine/
- FalkorDB retrieval architecture: https://www.falkordb.com/blog/ai-agent-memory-retrieval-architecture/
- Memory frameworks: https://atlan.com/know/best-ai-agent-memory-frameworks-2026/
- Gartner 40% cancellation: https://www.computerworld.com/article/4016206/nearly-half-of-agentic-ai-projects-will-be-killed-by-27-due-to-hype-costs-and-risks.html
- Microsoft per-agent identity: https://learn.microsoft.com/en-us/security/zero-trust/sfi/least-privilege-for-ai-agents

---

## 3. Governance & control for agent data

_Full report: [research/agent-data-governance-2025.md](./research/agent-data-governance-2025.md)_

### Enterprise governance primitives (the bar)
authN (LDAP/AD/OIDC/SCIM) · authZ (RBAC/ABAC) · **row-level security +
column masking** (the de-facto "real governance" test) · immutable audit
logs · column-level lineage (OpenLineage) · policy-as-code (OPA/Cedar) ·
approval/change workflows. **RLS and lineage are the two primitives
enterprise buyers check first.**

### Agent governance: nascent, being productized
- **Agent identity is still an open proposal** (MCP Discussion #2404 — no
  ratified "agent as principal"). Per-agent credentials exist via
  **SEP-1046 (OAuth client-credentials)**.
- Tool-level permission *hints* exist (`readOnlyHint`, `destructiveHint`…)
  but are **hints, not enforcement** — enforcement lives in gateways
  (Kong, Tyk, Gravitee, TrueFoundry, Britive).
- MCP authorization = OAuth 2.1 + PKCE + RFC 7591 (DCR) + RFC 8707
  (resource indicators); latest spec 2025-11-25. MCP + A2A consolidating
  under the Linux Foundation's Agentic AI Foundation.
- DB-native agent controls shipping: Oracle Deep Data Security, Databricks
  Unity Catalog, Snowflake Cortex Agents (RBAC + managed MCP).

### "Shared system of record for humans and agents" — a real, emerging category
- **Bytebase: "database governance built for humans and agents"** — one
  change-review/approval queue + audit trail for both. Closest direct
  analogue.
- Databricks Unity Catalog: single control point; Redpanda Agentic Data
  Plane ("governing AI agents for enterprise data").
- Shared pattern everywhere: **one identity plane + one audit log + one
  policy engine, treating an agent as a first-class principal** — exactly
  LessDB's "one control plane, both front doors" design.

### Gap analysis vs LessDB
- **Already strong:** LDAP/AD RBAC, fail-closed auth, 27-tool MCP surface,
  per-agent tenancy, single embedded store = unified record.
- **Must add to credibly claim agent governance:** per-agent credentials
  (SEP-1046 style), tool-level permissions keyed to MCP annotations,
  append-only audit log, policy layer (Cedar/Rego), then RLS + lineage.

### Sources
- MCP authorization spec: https://modelcontextprotocol.org/specification/2025-06-18/basic/authorization · changelog: https://modelcontextprotocol.io/specification/2025-11-25/changelog.md
- SEP-1046: https://modelcontextprotocol.org/seps/1046-support-oauth-client-credentials-flow-in-authoriza
- Agent identity discussion: https://github.com/modelcontextprotocol/modelcontextprotocol/discussions/2404
- Bytebase: https://www.bytebase.com/database-governance-ai-agents/
- Databricks Unity Catalog agents: https://www.databricks.com/blog/governing-ai-agents-scale-unity-catalog
- Redpanda ADP: https://www.redpanda.com/press/redpanda-acquires-oxla-launches-new-agentic-data-plane-for-enterprise-data
- Gateways: https://konghq.com/blog/product-releases/securing-observing-governing-mcp-servers-with-ai-gateway · https://www.gravitee.io/blog/mcp-authorization-how-to-manage-permissions-for-ai-agents-services
- OPA agent policy: https://github.com/open-policy-agent/opa/issues/8851 · MSFT toolkit: https://github.com/microsoft/agent-governance-toolkit
- Access governance term: https://securityboulevard.com/2026/02/what-is-access-governance-for-ai-agents/

---

## 4. Positioning playbooks

_Full report: [research/positioning-playbooks-2025.md](./research/positioning-playbooks-2025.md)_

### The dual-audience pattern
One crisp architectural core promise + audience-specific **doorways** — not
two products. Cleanest template: DuckDB ("in-process SQL OLAP" core; analyst
doorway "run SQL on files, no server"; developer doorway "embed analytics,
no infra"). Stripe/Postman/Terraform show the same shape (dev wedge →
governance/ops buyer).

### Category creation vs adoption
- Pinecone deliberately created "vector database" ("long-term memory for
  AI") — it stuck because embeddings were a genuinely new primitive, then
  got absorbed as pgvector/everyone added vectors. Lesson: categories built
  on a feature get absorbed into feature races.
- "AI-native database" claims are now **low-signal** (TiDB, Google Cloud,
  Oracle); they only land when anchored to a real architectural change
  (agent as first-class query user, MCP-native), not a rebrand.
- Adoption-with-a-twist wins when you can't own a new word.

### Language audit (agent era)
- **Under-used whitespace:** "shared workspace / same tool your team uses",
  "system of record" (a16z, AxonIQ), protocol framing ("new standard").
- **Commoditized/strip-mined:** "memory layer for AI agents" (Mem0, Zep,
  Letta, Cognee, Epitome), "long-term memory", "AI-native", "single source
  of truth".

### What not to do
SEC "AI-washing" enforcement (Delphia/Global Predictions, 2024) makes
unsubstantiated AI claims a legal risk. Community backlash warning signs:
AI bolted on without behavior change, unshipped claims, renaming existing
features as "agent" features, abandoning human users to chase agents.

### GTM/pricing for OSS databases
Open-core, hosted cloud and BYOC support paid tiers. **Enterprises pay
for governance** — SSO/LDAP/OIDC, RBAC, audit, SOC 2/HIPAA/FedRAMP, data
residency, encryption, Prometheus, SLA — and governance is now being sold
as *agent governance*, which maps directly onto LessDB's LDAP/RBAC/audit +
MCP surface.

### Recommended position (synthesis)
> **"The analytical database your agents and your analysts share"** — one
> governed SQL/columnar system of record, MCP-native for agents but built
> for analysts, with identical governance on both sides. Avoid "AI-native"
> and "memory layer" as the lead.

### Sources
- Pinecone category creation: https://www.pinecone.io/ (see report §2 for attributed snippets)
- AI-native dilution: https://www.infoworld.com/article/4080483/the-dawn-of-the-ai-native-database.html
- SEC AI-washing: https://www.sec.gov/news/press-release/2024-36
- System-of-record whitespace: https://a16z.com/ (see report §3)
- Governance as the paid tier: https://konghq.com/blog/product-releases/securing-observing-governing-mcp-servers-with-ai-gateway
