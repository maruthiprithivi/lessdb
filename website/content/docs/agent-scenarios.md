# Agent adoption scenarios

LessDB adoption starts with one bounded loop and grows through evidence. The same scenarios can be run by a personal assistant, a family assistant, a coding-agent swarm, a community service or a scientific workflow.

A scenario passes only when four views agree:

1. application or world state;
2. LessDB source, decision and provenance rows;
3. ContextPacket visibility, omissions and budget;
4. rendered UI or authorized SQL/Arrow output.

Model wording is never the oracle.

## SC-01 — Scoped context

Seed one shared operational fact, one private reason and one bounded situation with the same keywords. Recall for a shared-facing response.

**Expected:** the shared fact is included with stable source references; the private reason is omitted with a permitted non-disclosing reason; the packet stays inside its item/byte budget; no model-generated new fact is accepted.

## SC-02 — Lifecycle replay

Create active, expired, archived, revoked and deleted records. Recall before and after a pinned time boundary and inspect the explanation.

**Expected:** the states remain distinct. Expired is not deleted, archived is not revoked, and a storage TTL is not presented as immediate semantic forgetting. Retained audit/provenance follows the application retention policy.

## SC-03 — Bounded multi-source recall

Seed equivalent facts in memory, graph/context, an event table and optional vector candidates, plus unrelated rows. Apply scope, time, relationship and freshness constraints before ranking.

**Expected:** duplicate facts collapse without losing provenance; hard item/byte limits hold; ordering is deterministic for the same policy; an authorized analyst can reconcile approved source references without receiving private packet text.

## SC-04 — Proposal, approval and causal action

Create a proposal for a message, purchase, deployment, merge or other consequential action. Allow a model to draft, but keep policy, approval and execution separate.

**Expected:** no action occurs while pending or denied; changed revision, packet or policy invalidates approval; the application executor handles idempotency and reconciliation; LessDB records evidence but does not claim a cross-system transaction.

## SC-05 — Least-privilege handoff

Materialize a handoff with a target, purpose, exact item IDs or derived fields, expiry and recipient policy. Retry it after a simulated timeout, then expire or revoke it.

**Expected:** the recipient receives only the reduced packet; an item ID is not a capability to read the source row; one logical handoff remains after retry; recipient visibility is re-evaluated.

## SC-06 — Specialist loop

Use a coordinator, implementer/observer, tester/verifier, reviewer and release/safety approver. Bind the work to a repository revision/worktree or experiment/run/sample identity.

**Expected:** parent/child lineage, artifact/evidence references and review decisions are durable; private scratch is not exposed to the reviewer; no merge, release or actuator action occurs without application approval.

## Generate reproducible fixtures

A runner can generate scenario JSON with a fixed seed:

```sh
python3 tools/generate_agent_scenarios.py \
  --seed 20261009 \
  --count 12 \
  --out /tmp/lessdb-agent-scenarios.json
```

The fixture schema is `lessdb-agent-scenario/v1`. It contains actors, situation, recall intent, seeded items, action state, expected visibility and a four-view oracle. The generator does not execute actions or decide policy.

## LessVille reference implementation

[LessVille](https://ville.lessdb.dev) demonstrates these ideas with a fictional research simulation. Its café order, named bus, object-transition, activity and context-inspection flows are application-owned causal state. LessDB stores the memories, messages, decisions, events, packet provenance and observability needed to inspect what agents can see and why.

The public simulation is observer-only. Its deterministic policies are labelled as deterministic, and its current application-level visibility broker is not presented as native LessDB ACL.

## Adoption rule

Start with one agent, one tenant, one workflow and a small packet. Add sharing, handoffs, approvals and specialist agents only after replay, hidden-row, lifecycle, provenance, budget and retry behavior is measurable.
