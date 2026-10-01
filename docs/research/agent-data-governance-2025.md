# Governing Human & Agent Access to Data Stores (Late 2025)

Research for LessDB positioning on "full human and agent control and governance."

## 1. How enterprise data platforms govern human access today

The standard primitives a database must expose to be considered "enterprise governable":

- **Authentication (authN)** — LDAP/AD, SAML, OIDC/SSO, SCIM provisioning. LessDB's LDAP/AD integration maps here.
- **Authorization (authZ)** — RBAC roles (admin/read/write), plus ABAC (attribute-based) for finer control. Postgres/Snowflake/Databricks all layer this on.
- **Row-level security (RLS) / column masking** — policy functions that filter rows and mask columns per role (Postgres RLS, Snowflake masking policies, Databricks Unity Catalog row/column filters). This is the de-facto test for "real" governance vs. table-level grants only.
- **Audit logs** — immutable, queryable records of *who did what, when, from where*. SOC 2/enterprise deals routinely die on a missing audit log ([OpenClaw audit-log blog](https://www.clawctl.com/blog/enterprise-prospect-soc2-openclaw-audit-log)).
- **Data lineage** — column-level lineage via the open [OpenLineage](https://openlineage.io/) standard and native integrations (e.g. [Debezium + OpenLineage](https://debezium.io/blog/2025/06/13/openlineage-integration/)); Snowflake and Databricks ship lineage natively.
- **Policy engines / policy-as-code** — [Open Policy Agent](https://www.openpolicyagent.org/) (Rego) and AWS Cedar; decouples policy from the DB engine.
- **Change management / approval workflows** — PR-style review and approvals for schema and data changes (Bytebase, Liquibase, Flyway). [Bytebase](https://github.com/bytebase/bytebase) now positions itself as "database governance built for humans and agents."

Vendor framing: [Dremio Data Governance](https://www.dremio.com/platform/unified-analytics/data-governance/), [ThoughtSpot Governance](https://www.thoughtspot.com/product/governance), and Microsoft Dataverse all converge on the same list — authN, authZ, lineage, audit, classification/tagging, retention.

## 2. Governing AGENT access specifically

Agent governance is nascent but concretely productized:

- **Agent identity & delegation** — MCP discussion [#2404 "Agent Identity and Delegation for MCP Tool Calls"](https://github.com/modelcontextprotocol/modelcontextprotocol/discussions/2404) is an open proposal; no ratified spec for "the agent as a distinct principal" yet. *This is the single biggest gap.*
- **Per-agent credentials** — [SEP-1046 (OAuth client-credentials flow for MCP authorization)](https://modelcontextprotocol.org/seps/1046-support-oauth-client-credentials-flow-in-authoriza) gives each agent/server its own client credentials so audit is per-agent, not per-human.
- **Tool-level permissions** — MCP **tool annotations** (`readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint`) let gateways write policy against tool behavior; they are *hints, not enforcement* (see [MCP auth spec](https://modelcontextprotocol.org/specification/2025-06-18/basic/authorization)). A separate discussion ([#2315 "Securing MCP Tools with Metadata"](https://github.com/modelcontextprotocol/modelcontextprotocol/discussions/2315)) proposes richer policy metadata.
- **Approval flows / human-in-the-loop** — gateway products gate destructive tools behind approval. [Tyk MCP gateway RBAC](https://www.tyk.io/docs/nightly/ai-management/mcp-gateway/how-to-mcp-rbac), [Kong AI Gateway](https://konghq.com/blog/product-releases/securing-observing-governing-mcp-servers-with-ai-gateway), [TrueFoundry Agent Gateway](https://www.01net.it/truefoundry-launches-agent-gateway-to-close-the-enterprise-ai-governance-gap/), [Gravitee](https://www.gravitee.io/blog/mcp-authorization-how-to-manage-permissions-for-ai-agents-services), and [Britive](https://www.britive.com/resource/blog/secure-mcp-workflows-agentic-ai-access-control/) all advertise MCP authN/authZ, rate limiting, and per-tool policy.
- **Policy-as-code for agents** — [Stacklok Toolhive uses Cedar policies](https://github.com/stacklok/docs-website/blob/main/docs/toolhive/concepts/cedar-policies.mdx) for tool access; OSS [policyaware](https://github.com/ktirupati/policyaware) and [bedrock-policy](https://github.com/codecr/bedrock-policy) target LLM/agent control planes.
- **Sandboxing** — not standardized; enterprise answer is usually gateway + ephemeral containers/OS sandbox, not a DB-level primitive. *(LessDB's write role + optional approval is a competitive simplification.)*
- **Agent audit products** — [LangSmith](https://www.langchain.com/resources/langsmith-vs-langfuse) / [Langfuse](https://www.langchain.com/resources/langsmith-vs-langfuse) for LLM tracing; [WitnessAI Agentic Control](https://www.livethreat.ai/intelligence/witnessai-agentic-control-secures-ai-agents-tools-and-mcp-server-access-32469) for agent/tool/MCP governance; [Rubrik Agent Cloud](https://virtualizationreview.com/articles/2025/10/22/rubrik-debuts-agent-cloud-to-monitor-and-govern-ai-agent-actions.aspx) monitors agent actions; [MLflow tracing](https://mlflow.org/articles/best-llm-tracing-tools-for-multi-agent-systems-in-2026/).
- **Platform-native governance** — [Databricks Unity Catalog "Governing AI agents at scale"](https://www.databricks.com/blog/governing-ai-agents-scale-unity-catalog) (credential vending, lineage, MCP tools); Snowflake Cortex Agents inherit RBAC/RLS and [Cortex Agent Guardrails](https://snowflake.discourse.group/t/cortex-agent-guardrails/19430/3).

## 3. Emerging standards & specs

- **MCP spec latest version is 2025-11-25** ([changelog](https://modelcontextprotocol.io/specification/2025-11-25/changelog.md)). Authorization was formalized in the 2025-06-18 spec ([authorization](https://modelcontextprotocol.org/specification/2025-06-18/basic/authorization)) as an **OAuth 2.1-based** model with **authorization server + resource server** roles, **PKCE**, **Dynamic Client Registration (RFC 7591)**, and **resource indicators (RFC 8707)**. See [CSA's MCP OAuth 2.1 breakdown](https://cloudsecurityalliance.org/blog/2025/05/28/mcp-oauth-2-1-pkce-and-the-future-of-ai-authorization) and [safeguard.sh defender's guide](https://safeguard.sh/resources/blog/mcp-spec-2025-06-18-oauth-resource-server).
- **MCP & A2A governance body** — MCP and the **Agent2Agent (A2A)** protocol are being consolidated under the Linux Foundation's Agentic AI Foundation ([Yahoo/Tech coverage](https://tech.yahoo.com/ai/articles/agent2agent-joins-agentic-ai-foundation-130048369.html), [01net](https://www.01net.it/a2a-agentic-ai-foundation-interoperabilita-agenti-ai/)). A2A matters for *agent-to-agent delegation* governance; MCP for *agent-to-tool*.
- **AGENTS.md** — the de-facto context-file convention; being pulled into AAIF compliance ecosystems ([nexus issue #1731](https://github.com/nexi-lab/nexus/issues/1731)).
- **OpenAI plugin/app auth** ([OpenAI auth docs](https://developers.openai.com/plugins/build/auth)) and [ChatGPT Enterprise MCP connectors with RBAC](https://help.openai.com/ko-kr/articles/10128477-chatgpt-enterprise-edu-release-notes) are the model-vendor equivalents of per-tool/per-connector admin control.

## 4. "Shared system of record for humans and agents"

This is emerging as an explicit category:

- **[Bytebase — "database governance built for humans and agents"](https://www.bytebase.com/database-governance-ai-agents/)** is the clearest direct example: one change-review/approval pipeline where human DBA review and agent-initiated changes flow through the same queue and audit trail.
- **[Databricks Unity Catalog](https://www.databricks.com/blog/governing-ai-agents-scale-unity-catalog)** positions the catalog as the single control point where human BI users and agents share the same lineage, permissions, and audit.
- **[Redpanda Agentic Data Plane](https://main--redpanda-documentation.netlify.app/agentic-data-plane/get-started/adp-overview/)** (with Oxla SQL engine) markets "governing AI agents for enterprise data" ([press](https://www.redpanda.com/press/redpanda-acquires-oxla-launches-new-agentic-data-plane-for-enterprise-data)).
- **Starburst** markets an "AI-ready platform for secure, governed data use" ([SecurityBrief](https://securitybrief.co.uk/story/starburst-unveils-ai-ready-platform-for-secure-governed-data-use)).
- **"Agentic data governance" / "access governance for AI agents"** now appears as its own term ([Security Boulevard](https://securityboulevard.com/2026/02/what-is-access-governance-for-ai-agents/)); the shared implementation pattern is **one identity plane + one audit log + one policy engine** that treats an agent as a principal identical to a human.

The architectural takeaway for LessDB: a single embedded store that is both the query engine and the audit/authority source *is* the "shared system of record" — the differentiators are per-principal credentials (human via LDAP/AD, agent via OAuth client credentials) and a unified audit log.

## 5. What an OSS embedded DB needs to compete on governance

- **Determinism & auditability over files** — Oracle's dev blog argues databases beat filesystems for agent memory on ACID transactions, concurrency, and structured query vs. grep ([Oracle blog](https://blogs.oracle.com/developers/comparing-file-systems-and-databases-for-effective-ai-agent-memory-management); [dev.to guide](https://dev.to/oracledevs/a-practical-guide-to-choosing-the-right-memory-substrate-for-your-ai-agents-33hj)). The "filesystem problem agents face" is a recurring community theme ([Fireside Fedi podcast](https://www.ivoox.com/en/the-file-system-problem-agents-face-audios-mp3_rf_173239221_1.html)).
- **Single-file embeddability is a trust feature, not a toy** — [HN "Remembrane — agent memory in one SQLite file, zero dependencies"](https://news.ycombinator.com/item?id=49207194) shows the community values portable, inspectable, versioned state for agents. *(LessDB's Rust single-binary story should lean into this: an embeddable store that is also the authority for audit is the governance pitch.)*
- **Expert sentiment (paraphrased, not direct quotes)** — across HN/Reddit/Lobsters and vendor blogs, the recurring governance asks for agent data access are: (1) **immutable, tamper-evident audit** of every tool call/read/write; (2) **schema + constraints** so agents can't corrupt state; (3) **deterministic query results** for reproducible reasoning; (4) **per-agent credentials**, so a compromised agent is revocable without killing a human's access; (5) **approval gates** on destructive operations. *These are consolidated from multiple secondary sources above rather than a single named expert quote.*

### Gap analysis for LessDB
- **Strong already:** LDAP/AD RBAC (human authZ), MCP server with 27 tools (surface for tool-level policy), single embedded store = unified record.
- **Must add to credibly claim "agent governance":** per-agent OAuth 2.1 client credentials (SEP-1046), tool-level permissions keyed to MCP annotations (`readOnlyHint`/`destructiveHint`), an immutable/append-only audit log, and a policy layer (Cedar or Rego). Row-level security and data lineage are the two primitives most enterprise buyers will check first.

## Key sources

- MCP Authorization spec (2025-06-18): https://modelcontextprotocol.org/specification/2025-06-18/basic/authorization
- MCP spec changelog 2025-11-25: https://modelcontextprotocol.io/specification/2025-11-25/changelog.md
- MCP SEP-1046 (OAuth client credentials): https://modelcontextprotocol.org/seps/1046-support-oauth-client-credentials-flow-in-authoriza
- MCP Discussion #2404 (Agent Identity & Delegation): https://github.com/modelcontextprotocol/modelcontextprotocol/discussions/2404
- MCP Discussion #2315 (Securing tools w/ metadata): https://github.com/modelcontextprotocol/modelcontextprotocol/discussions/2315
- CSA — MCP OAuth 2.1, PKCE, AI authorization: https://cloudsecurityalliance.org/blog/2025/05/28/mcp-oauth-2-1-pkce-and-the-future-of-ai-authorization
- safeguard.sh — MCP 2025-06-18 OAuth for defenders: https://safeguard.sh/resources/blog/mcp-spec-2025-06-18-oauth-resource-server
- Bytebase — Database governance for AI agents: https://www.bytebase.com/database-governance-ai-agents/ and repo: https://github.com/bytebase/bytebase
- Databricks — Governing AI agents at scale with Unity Catalog: https://www.databricks.com/blog/governing-ai-agents-scale-unity-catalog
- Redpanda — Agentic Data Plane / Oxla acquisition: https://www.redpanda.com/press/redpanda-acquires-oxla-launches-new-agentic-data-plane-for-enterprise-data
- Tyk — MCP gateway RBAC: https://www.tyk.io/docs/nightly/ai-management/mcp-gateway/how-to-mcp-rbac
- Kong — Governing MCP servers with AI Gateway: https://konghq.com/blog/product-releases/securing-observing-governing-mcp-servers-with-ai-gateway
- Gravitee — MCP authorization: https://www.gravitee.io/blog/mcp-authorization-how-to-manage-permissions-for-ai-agents-services
- Stacklok Toolhive Cedar policies: https://github.com/stacklok/docs-website/blob/main/docs/toolhive/concepts/cedar-policies.mdx
- WitnessAI Agentic Control: https://www.livethreat.ai/intelligence/witnessai-agentic-control-secures-ai-agents-tools-and-mcp-server-access-32469
- Rubrik Agent Cloud: https://virtualizationreview.com/articles/2025/10/22/rubrik-debuts-agent-cloud-to-monitor-and-govern-ai-agent-actions.aspx
- OpenAI — ChatGPT Enterprise MCP connectors/RBAC: https://help.openai.com/ko-kr/articles/10128477-chatgpt-enterprise-edu-release-notes
- Anthropic — Claude admin controls: https://www.anthropic.com/news/claude-code-on-team-and-enterprise
- A2A + Agentic AI Foundation: https://tech.yahoo.com/ai/articles/agent2agent-joins-agentic-ai-foundation-130048369.html
- OpenLineage: https://openlineage.io/ and Debezium integration: https://debezium.io/blog/2025/06/13/openlineage-integration/
- Oracle — file systems vs databases for agent memory: https://blogs.oracle.com/developers/comparing-file-systems-and-databases-for-effective-ai-agent-memory-management
- HN — Remembrane (agent memory in one SQLite file): https://news.ycombinator.com/item?id=49207194
- Security Boulevard — Access governance for AI agents: https://securityboulevard.com/2026/02/what-is-access-governance-for-ai-agents/
