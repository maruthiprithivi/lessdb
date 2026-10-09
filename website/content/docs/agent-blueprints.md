# Agent deployment blueprints

LessDB is designed to grow with the agent system around it. The durable core is the same in every deployment:

> Agents arrive with identity and situation. LessDB returns bounded, provenance-rich context. The application decides and executes. LessDB records what happened so the next agent can understand it.

## The universal context loop

A safe agent interaction is a typed loop, not an unbounded prompt assembled from arbitrary rows:

1. **AgentIdentity** — principal, tenant, role, scopes and policy version.
2. **Situation** — current time, timezone, location, activity, nearby entities, commitments, world conditions and allowed actions.
3. **RecallIntent** — purpose, task ID, memory kinds, temporal/spatial/relationship constraints, freshness and hard item/byte budgets.
4. **ContextPacket** — included and omitted items, visibility boundary, provenance, constraints and budget usage.
5. **Application decision** — policy, approval and execution are separate from model output.
6. **Remember** — the resulting fact, event, decision or handoff is stored with stable IDs and source references.

The packet is the contract presented to an agent. A prompt or model completion is only a projection of that packet.

## Choose a deployment profile

| Profile | Default boundary | First useful workflow |
|---|---|---|
| Local single-user | One private tenant and one assistant | Remember, recall, explain and correct one durable note |
| Family assistant | Private person scopes plus explicit household sharing | Share a safe fact without exposing a private reason |
| Consumer companion | Private notes, session scratch and consented workspace scopes | Bounded recall over duplicate or conflicting notes |
| Professional copilot | Private work by default; explicit project promotion | Draft a status update while excluding sensitive notes |
| Software factory | Repository, revision, worktree, task and artifact scopes | Implement, test, review and approve one exact revision |
| Organization/community | Team, project, public, shared, private and sensitive scopes | Publish and hand off an approved operational fact |
| Scientific/operations swarm | Project, experiment, run, sample and instrument scopes | Verify a time/spatial observation before an action |

The profile changes identity, scopes, retention, handoffs and approvals. It does not require a different storage engine.

## The six task-oriented operations

A reference broker can expose these operations above LessDB's existing SQL, Arrow, MCP, graph, memory and vector surfaces:

- `agent_session_open`
- `agent_observe`
- `agent_recall`
- `agent_remember`
- `agent_decide`
- `agent_context_explain`

The broker filters identity, scope, lifecycle, time, space, relationships and freshness before ranking or prompt rendering. It enforces maximum items and bytes, records source references and explains omissions where the caller is allowed to know the reason.

## What LessDB guarantees, and what the application must provide

| Layer | Responsibility |
|---|---|
| Native LessDB | SQL/DataFusion/Arrow, Parquet parts, query pruning, graph/context, memory, vector surfaces, configured SQL TTL behavior, MCP namespaces and documented telemetry |
| Broker/application | Scope and consent policy, temporal eligibility, deduplication, ranking policy, packet budgets, handoffs, retention jobs, proposals, approvals, idempotency, reconciliation and action execution |
| Deployment/control plane | Authentication mapping, gateway isolation for shared SQL, TLS, encryption at rest, key ownership, backup/restore, object-store permissions and tamper-evident audit |

The MCP tenant namespace scopes configured context, memory and vector paths. It is not a universal row-level ACL for shared `less_*` SQL tables. Raw SQL and analyst access need their own authorization and projections.

Do not treat a model completion as a fact, approval, identity or action result. External messages, purchases, deployments, destructive writes and cross-scope sharing should be proposal-first in the application.

## Recommended evolution

1. Start with one tenant, one agent and deterministic bounded recall.
2. Add packet inspection, source provenance, omission reasons and replay.
3. Add explicit handoffs, retention states and approval-gated actions.
4. Add specialist agents only after hidden-row, lifecycle, budget, provenance and retry tests pass.
5. Add organization or shared-object-storage scale only with a documented recovery model for broker metadata.
6. Consider native grants, valid-time governance, immutable context-access audit or approval primitives only after the application behavior has conformance evidence.

See the [agent memory and MCP use case](/use-cases/agent-memory-mcp), the [adoption scenarios](/docs/agent-scenarios), and the [MCP guide](/docs/mcp) for concrete paths.
