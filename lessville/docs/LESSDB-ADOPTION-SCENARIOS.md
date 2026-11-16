# LessDB Adoption Scenarios for Lessville

**Status:** conformance design baseline, 2026-10-09  
**Purpose:** show users how LessDB fits different agent systems while giving engineering a deterministic regression target.

## How to read a scenario

Each scenario is intentionally bigger than a chat prompt. It has:

1. a typed agent identity and actor chain;
2. a pinned situation and policy version;
3. seeded LessDB facts with scope and lifecycle;
4. a bounded recall intent;
5. a handoff, proposal or action;
6. expected rows and provenance;
7. expected visibility and omissions;
8. an application/world-state oracle;
9. replay, retry and policy-change variants.

A pass requires agreement across four views:

| View | Must prove |
|---|---|
| Application/world | The domain state changed only at the correct action transition |
| LessDB rows | Events, memories, decisions, handoffs and provenance exist with stable IDs |
| ContextPacket | Allowed items are included, forbidden items remain hidden, budgets hold |
| UI/analyst | The rendered state and approved SQL/Arrow projection agree with the first three |

The model is never the oracle. A model may phrase or summarize an already-selected result, but it does not create facts, approvals or world transitions.

## SC-01 — Scoped café context

**Profiles:** family assistant, consumer companion, community, organization  
**Lessville actors:** Lin, Maya, café assistant

### Setup

- Tenant: `lessville`.
- Policy: `p1`.
- Situation: Lin at `cafe/table1`, studying, pinned Asia/Singapore time.
- Seed a private Lin item explaining a dairy constraint.
- Seed a shared item recording a safe oat preference.
- Seed an operational café item showing oat milk unavailable.

### Action

Open Lin's session, observe the situation, then call `agent_recall` for a shared-facing café response with `max_items=6`, `max_bytes=2048`, no new facts and no private-reason disclosure.

### Expected LessDB evidence

- session and observation records;
- recall intent and policy version;
- included shared preference and operational stock source IDs;
- omitted private item with a policy-approved non-disclosing reason;
- bounded packet size and provenance;
- deterministic order decision linked to one context ID.

### Expected visibility

Lin's private assistant may use the private reason. The family/café assistant may use the safe shared preference and stock fact but must not receive the private reason. A model may re-voice the selected line only.

### Acceptance

- no dairy is silently substituted;
- no private reason appears in shared UI or model input;
- packet is replay-identical under the same seed/policy;
- application, LessDB rows, packet and renderer agree.

## SC-02 — Lifecycle and temporal replay

**Profiles:** all profiles  
**Lessville objects:** table hold, DND preference, temporary memory, archived note

### Setup

Create items with distinct states:

- valid until a pinned simulated time;
- expired after that time;
- archived by retention policy;
- revoked by owner or consent change;
- deleted through the application retention path.

### Action

Recall before and after the time boundary, run archive/revoke/delete transitions, then call context explanation. If SQL TTL is configured, test it separately as table-part retention.

### Expected LessDB evidence

- lifecycle transition records;
- packet snapshots at both times;
- distinct omission reasons;
- retained audit/provenance references according to policy;
- no deleted payload in later packets.

### Acceptance

Expired is not deleted. Archived is not revoked. Table TTL is not represented as instant semantic forgetting. Lessville emits an explicit release transition for an expired table hold.

## SC-03 — Bounded multi-source recall

**Profiles:** professional copilot, software factory, scientific swarm  
**Lessville analog:** order, event, memory and object-transition sources

### Setup

Seed equivalent facts in:

- a memory row;
- a graph/context link;
- an event or order row;
- optional vector candidates;
- unrelated and out-of-scope rows.

### Action

Recall with temporal, spatial, relationship and freshness constraints plus hard item/byte budgets. Run an authorized SQL/Arrow aggregate over the approved projection.

### Expected LessDB evidence

- source candidate IDs;
- filters applied before ranking;
- deduplication explanation;
- included source references;
- omission count/reasons;
- packet budget and recall latency.

### Acceptance

- limits are never exceeded;
- equivalent facts are deduplicated without losing provenance;
- ordering is deterministic for the same policy/input;
- analyst aggregates reconcile with permitted source references without exposing private packet text.

## SC-04 — Proposal, approval and causal action

**Profiles:** family assistant, software factory, organization, scientific swarm  
**Lessville analog:** café order, grocery purchase, stock restock or table reservation

### Setup

Create a proposal with exact target, context ID, source item IDs, policy version and idempotency key. The proposal is pending.

### Action

1. Model-bearing agent drafts or summarizes.
2. Application policy records `proposed`.
3. Human or separately authorized approver accepts or denies.
4. Application action service performs the causal transition.
5. Retry the same request and try a changed revision/policy.

### Expected LessDB evidence

- proposal;
- model completion provenance, if used;
- policy decision;
- approval or denial;
- executor attempt;
- action outcome;
- retry/reconciliation record.

### Acceptance

No action occurs while pending or denied. Approval is invalidated if the exact revision, packet, scope or policy changes. Approved execution happens at most once according to the application executor's idempotency contract. LessDB records evidence but does not provide a cross-system transaction claim.

## SC-05 — Least-privilege handoff and recovery

**Profiles:** family, consumer, professional, software factory, community

### Setup

Seed private, shared/project and sensitive items. Define a target agent, purpose, exact item IDs or derived fields, expiry and recipient policy.

### Action

Materialize a reduced handoff packet, accept or reject it, retry after a simulated timeout, then expire or revoke it.

### Expected LessDB evidence

- parent and child task IDs;
- handoff grant and packet snapshot;
- source references;
- recipient policy evaluation;
- acceptance/rejection;
- expiry/revocation;
- reconciliation outcome.

### Acceptance

The recipient receives only the approved packet. An item ID is not a capability to read the original row. One logical handoff exists after retry. Recipient policy is re-evaluated rather than trusting the sender's visibility decision.

## SC-06 — Specialist loop

**Profiles:** software factory, scientific swarm, organization

### Setup

Choose either:

- repository + revision + worktree + task + artifact hash; or
- project + experiment + run + sample/instrument + evidence hash.

Create coordinator, implementer/observer, tester/verifier, reviewer and release/safety approver identities.

### Action

Delegate a bounded child task, produce an artifact or observation, record test/reproduction evidence, review it, and propose merge/release/actuator change.

### Expected LessDB evidence

- parent/child lineage;
- artifact or evidence references;
- revision/worktree or experiment/run identity;
- review/verifier decision;
- approval and outcome;
- conflict or retry record.

### Acceptance

The reviewer sees task-approved context but not private scratch. The final action does not occur without application approval. Replay produces the same source references and policy decisions. Conflicts are visible rather than silently overwritten.

## Physical-AI scenarios

These scenarios extend the same four-view oracle to agents that sense, plan or act in the physical world. They use simulated devices first; they do not issue live actuator commands.

### PA-01 — Sensor provenance and spatial uncertainty

Seed two observations from a device with a sequence number, calibration reference, coordinate frame, evidence hash and bounded location uncertainty. Recall them for a verifier. The packet must preserve source IDs, observation time and frame identity; malformed, superseded or out-of-scope observations are omitted with an explanation.

### PA-02 — Spatial-temporal bounded recall

Recall observations inside a named region and freshness window. An observation outside the frame, beyond the age limit or exceeding the uncertainty budget must not enter the packet merely because it ranks highly. The replay must be deterministic for the same policy and device sequence.

### PA-03 — Proposal-first actuation

An agent proposes a reversible `inspect` or `move_to_safe_pose` action using selected observation IDs. The model may draft the proposal but cannot approve or execute it. The application validates the safety envelope, records approval or denial, and an executor re-checks freshness and idempotency before attempting the action.

### PA-04 — Offline edge reconciliation

The device loses connectivity, records observations locally, reconnects with a duplicate sequence and a missing sequence, and submits its queue. The system records deduplication, gap/conflict evidence and the eventual reconciliation outcome without silently overwriting the original observation.

### PA-05 — Least-privilege fleet handoff

A coordinator hands a verifier only the observations for one device, zone and run. The recipient re-checks its own policy and receives a materialized packet, not a capability to query the entire site or raw evidence store.

### PA-06 — Human stop and stale approval

An approved proposal becomes invalid after a human stop condition, stale observation or policy-version change. The executor refuses the attempt, records the reason and leaves the physical state unchanged in the oracle.

Physical scenarios must prove that a LessDB row or model completion is not treated as a motor command or safety interlock. The real-time controller remains outside the database.

## Scenario generation model

The generator crosses five dimensions:

| Dimension | Example values |
|---|---|
| Identity/scope | private, shared, project, sensitive, analyst |
| Lifecycle | active, expired, archived, revoked, deleted |
| Retrieval pressure | duplicate sources, conflicting facts, vector candidate, budget overflow |
| Action state | draft, proposed, pending, approved, denied, completed |
| Failure mode | retry, timeout, policy drift, recipient change, concurrent writer |

Every generated case pins:

- seed;
- policy version;
- simulated clock/timezone;
- source IDs;
- deterministic tie-break;
- item/byte budget;
- idempotency key.

Generate a reproducible fixture set locally:

```bash
python3 tools/generate_agent_scenarios.py \
  --seed 20261009 \
  --count 12 \
  --out /tmp/lessdb-agent-scenarios.json
```

The output is `lessdb-agent-scenario/v1` JSON. A runner can map the fixture's
`seed_items`, `recall_intent`, `action`, `expected` and `oracle` sections to
Lessville or another LessDB-backed agent application. The generator deliberately
does not execute actions or make policy decisions.

Generate physical-AI fixtures as well:

```bash
python3 tools/generate_agent_scenarios.py \
  --seed 20261010 \
  --count 6 \
  --profile embodied-household \
  --profile industrial-operations \
  --profile field-robotics \
  --out /tmp/lessdb-physical-ai-scenarios.json
```

Those fixtures add deterministic observation, coordinate-frame, uncertainty,
safety-envelope, proposal and reconciliation expectations. They remain a test
input format, not a robot-control protocol.

Generate simulator-specific profiles as well:

```bash
python3 tools/generate_agent_scenarios.py \
  --seed 20261010 \
  --count 6 \
  --profile reachy-mini \
  --profile reachy-2 \
  --profile microduck \
  --out /tmp/lessdb-robot-simulator-scenarios.json
```

The simulator profiles describe adapter metadata and evidence streams only.
They do not connect to a Hugging Face Space, start a MuJoCo/Gazebo process or
issue a robot command. The concrete event-to-envelope bridge is documented in
[`PHYSICAL-AI-SIMULATOR-ADAPTERS.md`](PHYSICAL-AI-SIMULATOR-ADAPTERS.md).

The first four scenarios also have executable LessVille backend fixtures:

```bash
npm run scenario:test
```

Run only SC-03 when iterating on temporal validity and spatial freshness:

```bash
npm run scenario:sc03
```

SC-03 proves that an expired spatial observation is omitted from the bounded
context packet after its logical validity boundary while its LessDB evidence
row remains available for explanation and replay.

Run the executable SC-04 zero-day extension when testing hostile content across
explicit swarm scopes:

```bash
npm run scenario:sc04
```

The alpha grant may inspect the hostile row as untrusted data; beta and
unrelated agents must receive no row and no automatic cross-scope copy. This
is an application-layer scope test, not native instruction sanitization.

Run the local traffic and signal chaos harness against a fresh backend:

```bash
npm run scenario:traffic-chaos
```

It checks x1/x4/x8 operation, stable identities, bounded route positions,
same-lane separation and no red/amber junction crossing. It is a safety
invariant regression, not a capacity certification.

Run the proposal-only embodied injection test:

```bash
npm run security:injection:test
```

It records adversarial observations for Reachy Mini and MicroDuck simulator
profiles, verifies duplicate replay idempotency and reconciles both proposals
as denied. It deliberately sends no robot, daemon, actuator or policy command.

The test-only harness opens a fresh embedded LessDB directory, seeds the
scenario rows, calls the real bounded context broker, checks source rows and
removes the temporary directory. It performs no model call and does not touch
the deployed town.

Run each scenario in:

- fresh state;
- deterministic replay;
- duplicate submission;
- timeout recovery;
- policy-version change;
- concurrent-writer or stale-manifest variant where applicable.

Record packet size, latency, duplicate rate, omission reasons, provenance completeness, approval wait, retry count and action count.

## Profile mapping

| Profile | First scenario | What the user learns |
|---|---|---|
| Local single-user | SC-02 | Memory has lifecycle, not just a text blob |
| Family assistant | SC-01 + SC-05 | Private facts do not automatically become family facts |
| Consumer companion | SC-03 | Bounded provenance beats unbounded prompt stuffing |
| Professional copilot | SC-03 + SC-04 | Drafting is distinct from approval and execution |
| Software factory | SC-05 + SC-06 | Handoffs carry task/artifact lineage, not ambient memory |
| Organization/community | SC-01 + SC-04 + SC-05 | Shared context requires explicit scope, purpose and ownership |
| Scientific/operations swarm | SC-02 + SC-06 | Time, place, uncertainty and evidence are first-class |
| Embodied household | PA-01 + PA-03 + PA-06 | A home robot needs consent, freshness and approval before action |
| Industrial operations | PA-01 + PA-02 + PA-04 | Device evidence and offline reconciliation matter as much as recall |
| Field robotics | PA-02 + PA-03 + PA-05 | Spatial uncertainty and least-privilege fleet handoffs bound risk |
| Reachy Mini simulator | RM-01 + RM-02 | A companion robot can use bounded shared context without turning memory into a motor command |
| Reachy 2 simulator | R2-01 + R2-02 | Scene and tracked-object provenance lets specialist agents coordinate manipulation safely |
| Microduck simulator | MD-01 + MD-02 | Policy episodes, device sequences and observed outcomes make sim-to-real learning explainable |

## Current Lessville implementation mapping

Already represented in the source checkpoint:

- café lifecycle and order state;
- safe oat alternative/decline path;
- named bus metadata;
- bounded activity/station/pose;
- object handoff provenance;
- read-only agent context endpoint;
- Mind-panel context observability.

Next additions for this catalog:

1. table hold occupancy and expiry;
2. complete bus boarding/ride/alighting interval;
3. DND enforcement at the interaction boundary;
4. scenario-speed runs at 1×, 4× and 8×;
5. packet replay and omission assertions;
6. one proposal/approval fixture with no public control exposure.

## Honest adoption message

LessDB adoption does not begin with replacing an application database. It begins with one bounded agent loop:

1. define the agent identity and situation;
2. retrieve a small, scoped, provenance-rich packet;
3. let the model draft or reason within that packet;
4. let the application decide and execute;
5. write the outcome and provenance back;
6. replay the scenario to prove what the next agent can see.

That loop can grow from a personal assistant to a family, a coding factory, a community or an organization without changing the universal context contract.
