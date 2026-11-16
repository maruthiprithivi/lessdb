# Agent simulation catalog for LessDB

**Status:** next-wave scenario design, 2026-10-10

LessVille should become more than one town. It should be a visible laboratory for a different data model: agents begin with identity, situation and purpose; they receive bounded context; applications decide what may happen; the resulting evidence becomes part of the next situation.

The simulations below are deliberately different. They test whether the same LessDB contract remains useful as the number of agents, people, scopes, devices, time windows and action risks changes.

## Simulation 1: Home and family memory

A household has private assistants, a shared household assistant and a visitor-facing assistant. One agent knows a private reason, while another may receive only a safe derived preference. Memory expires, is revoked or is promoted to a shared scope through an explicit handoff.

**LessDB must show:** identity separation, purpose-bound handoff, consent, lifecycle, omission explanations and source provenance.

**LessVille presentation:** the café order, DND preference and family message become one inspectable chain. The UI should show what each assistant could see, not merely what the final message said.

## Simulation 2: Consumer companion and personal knowledge

A notes-and-chat product has imported notes, session scratch, durable memories and user corrections. Several notes conflict. The assistant retrieves a small packet, explains the selected and omitted sources, and updates one item only after the user confirms the correction.

**LessDB must show:** source identity, temporal validity, duplicate handling, bounded recall and correction lineage.

**Why it matters:** this is the smallest adoption path. It proves LessDB is useful without requiring a swarm, a robot or an organization.

## Simulation 3: Software factory

A coordinator delegates a bounded task to an implementer, tester, reviewer and release approver. Each agent sees the task-approved packet rather than the repository’s entire ambient history. Artifacts, revisions, test evidence, review decisions and release approvals are linked.

**LessDB must show:** parent/child lineage, worktree or revision identity, artifact hashes, reviewer scope, retry behavior and proposal-first release actions.

**LessVille presentation:** a small “digital workshop” district can render agents moving between task board, build room and review desk. The observer can open one task and follow its context packet through the full specialist loop.

## Simulation 4: Community coordination

A community receives an incident or resource request. Local agents collect reports, a verifier checks evidence, a coordinator prepares a public summary and an approver decides whether to publish or dispatch help. Sensitive personal details remain restricted.

**LessDB must show:** public versus private projections, evidence agreement/disagreement, relationship scope, moderation decisions and reversible publication proposals.

**Why it matters:** it demonstrates that shared memory is not the same thing as making every row visible to every agent.

## Simulation 5: Scientific and environmental swarm

Observers collect measurements from different instruments and locations. A verifier receives only the relevant time window and spatial region. The system distinguishes measured data, inference, forecast and proposed intervention. Offline observations arrive later with duplicates or gaps.

**LessDB must show:** temporal and spatial recall, uncertainty, calibration references, sequence reconciliation, conflicting observations and replay.

**LessVille presentation:** a research outpost can show a live map, instrument status and a context inspector that explains why a measurement was excluded.

## Simulation 6: Reachy companion and manipulation lab

Reachy Mini tests expressive companion behavior in MuJoCo. Reachy 2 tests tracked-object inspection and manipulation scenes in Gazebo or MuJoCo. Their simulator adapters emit observations into the same LessDB envelope.

**LessDB must show:** episode memory, consented preferences, scene/object provenance, freshness limits, proposal state, approval and observed result.

**Important boundary:** LessDB stores bounded observations and coordination evidence. Reachy’s SDK, simulator, controller and safety mechanisms remain the owners of movement and real-time execution.

## Simulation 7: Microduck policy-learning lab

Microduck episodes compare walking, standing, grabbing and recovery behaviors. The policy version, device sequence, sensor evidence and outcome are persisted as episode summaries. A later agent can ask why a policy was retried or which conditions surrounded a failed recovery.

**LessDB must show:** policy-version lineage, episode-level context, duplicate/missing sequence handling, sim-to-real comparison and reconciliation after partial outcomes.

**Important boundary:** LessDB does not replace the MuJoCo policy training loop, ONNX runtime, daemon IPC or servo loop.

## Simulation 8: Logistics and facilities

Agents coordinate a delivery or inspection across a building. A planner creates a route proposal from fresh observations. A facility agent checks access and a safety service re-checks the proposal at execution time. Arrival, delay, cancellation or partial completion is reconciled.

**LessDB must show:** vehicle/person/facility scopes, stale position rejection, idempotent execution, route revision and partial-success evidence.

## Simulation 9: Security, verification and trust lab

Named personas move through four knowledge quadrants while untrusted content, private scopes, tool grants and outbound proposals combine into safe, held, blocked or dangerous paths. Trust scores and open questions evolve after verification and learning events.

**LessDB must show:** event provenance, decision provenance, guardrail policy version, evidence references, compound-risk explanation, trust changes and a versioned restartable scenario snapshot.

**Important boundary:** this is a deterministic application-owned research simulation. It does not provide native authorization, formal verification, model-introspection guarantees or a security certification.

## Common simulation contract

Every simulation should use the same six-stage loop:

1. **Open a session** with agent identity, tenant, role, scope and policy version.
2. **Observe a situation** with present time, place, activity, commitments and world conditions.
3. **Recall bounded context** under purpose, scope, lifecycle, temporal, spatial, relationship, freshness and byte limits.
4. **Draft a result or proposal** while keeping model output separate from facts and authorization.
5. **Apply an application decision** and execute only through the owning action service or simulator/controller.
6. **Reconcile the outcome** with stable IDs, evidence references, errors, retries and the next situation.

A simulation passes only when the application state, LessDB rows, ContextPacket and UI/analyst projection agree. The model is never the oracle.

## What the simulations teach users

The central lesson is not that every application needs a swarm or robot. It is that agent data has a shape that ordinary chat history and raw database connections do not capture well. The important units are situation, evidence, scope, lifecycle, task lineage, decision, proposal, approval and observed outcome.

LessDB should make those units inspectable and bounded. An application can start with one private assistant, then add a household, a coding factory, a community, a scientific swarm or a physical device without changing the core context contract. What changes is the deployment blueprint: identity topology, scopes, retention, policy, evidence ownership and action risk.

## Implementation order

1. Finish executable SC-03 through SC-06 and the PA suite.
2. Add the Reachy Mini, Reachy 2 and Microduck adapter fixtures to the replay report.
3. Add one community scenario and one software-factory scenario to the LessVille observer UI.
4. Add packet fingerprints and reconciliation rows to the `less-agent` contract.
5. Add scenario scorecards for visibility, provenance completeness, replay determinism, omission quality, duplicate handling and action safety.
6. Publish only scenarios that have passed the four-view oracle and have an explicit guarantee matrix.
