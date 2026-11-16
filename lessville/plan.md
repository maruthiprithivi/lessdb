# LessVille character growth and lesson loop

## Objective

Use high-level inspiration from identity continuity, social learning, moral choice and consequence-driven growth to make LessVille characters feel like they develop over time. The implementation must remain original, fictional and bounded: no copied characters, plot, dialogue or protected setting; no claim of consciousness or unrestricted autonomous self-learning.

## Product behavior

- Persist structured `lessdb-learning-lesson/v1` records beside ordinary memories, messages and decisions.
- Distinguish candidate lessons from accepted lessons, with source references, topic, trigger, confidence, policy version and provenance.
- Let recurring deterministic experiences promote a bounded lesson candidate into an accepted application-layer lesson after repeated evidence.
- Allow only versioned deterministic rules to consult accepted lessons. Model reflections may create candidate lessons but may not approve, apply, grant scopes or actuate anything.
- Show each selected character's growth stage, lesson counts, lesson statements, evidence references and the later decision influenced by a lesson.
- Preserve replayability, idempotent lesson IDs, existing public observer-only behavior and LessDB boundary language.

## Design direction

The visual and interaction language remains the established polished pixel-town interface. Growth is shown as a small timeline of evidence rather than a level-up number: an observation becomes a candidate, repeated experience makes a lesson eligible, and a deterministic rule may use an accepted lesson in a later situation. The UI should make uncertainty and provenance visible instead of hiding them behind an anthropomorphic “AI soul” label.

## Implementation shape

- `lessdb-wal/repo/crates/less-agent/src/lib.rs`: add the reusable bounded lesson contract and validation tests.
- `backend/agentd/src/sim.rs`: add `lv_lessons`, idempotent lesson insertion, recurring bus/trail lesson evidence and accepted-lesson-aware rules.
- `backend/agentd/src/llm.rs`: record model reflections as candidate lesson rows in addition to the existing private reflection memory.
- `backend/agentd/src/main.rs`: include lessons and a derived growth summary in `/api/agent/{id}`.
- `scripts/main.gd`: show the growth stage and lesson provenance in the existing Mind panel.
- `backend/agentd/src/scenarios.rs`: add an executable lesson promotion and future-decision replay scenario.
- `docs/LESSVILLE-GROWTH-LOOP.md` and the roadmap: document gaps, boundaries and evidence.

## Boundaries

This is application-owned behavior above embedded LessDB. It is not native ACL, native semantic forgetting, formal verification, consciousness, general intelligence, or physical authorization. `auto_apply` remains false for typed lessons. Public mode remains read-only; only private test/staging controls may advance or reset the world.

## Phase 2 research direction — bounded emergence and gap discovery

LessVille becomes a replayable pressure test for LessDB's agent-native meta-plane. Agents receive bounded novelty opportunities generated from a world seed, current situation and prior evidence. A novelty opportunity may change a local routine, relationship or context request, but it cannot execute arbitrary code, widen a scope, mutate shared schema or actuate a robot.

Every opportunity produces three linked outcomes: a world event, an agent-visible memory or decision, and a research-frontier signal when the existing application contract cannot naturally represent the situation. Frontier signals may create candidate evolution proposals for a view, memory lens, context recipe or additive projection. Proposals remain evidence-backed and inactive until separately reviewed.

Novelty is pseudo-random but deterministic: the world seed, generation, day and agent identity reproduce the same branch. Different seeds produce different bounded trajectories. This is intentional experimental variation, not an unrestricted self-modifying loop. The observer UI shows the opportunity, outcome, gap signal, proposal state and replay seed.

The first slice is implemented in `backend/agentd/src/sim.rs`, persisted through `lv_novelty_events`, `lv_frontier_signals` and `lv_evolution_proposals`, exposed in the read-only state snapshot and DB panel, and covered by a scenario proving same-seed replay, bounded variation, proposal-only evolution and no scope widening.


## Phase 3 — agent-session adapter proving ground

Implement a small transport adapter above the existing LessDB-backed context and frontier ledgers. The adapter exposes five proposal-safe operations:

- `catalog`: discover the typed context, memory, grant, frontier and evolution capabilities, limits and explicit unsupported native guarantees;
- `compose`: compile a bounded `ContextRecipe` from an agent identity, current situation, task, allowed lens names, scope and budget; return included and omitted items with reasons;
- `explain`: return the provenance and omission explanation for a composed packet or frontier signal;
- `propose`: persist a declarative, evidence-backed `EvolutionProposal` candidate with affected scope, compatibility, migration/rollback references and `auto_apply=false`;
- `simulate`: replay the candidate against bounded evidence and return a counterfactual comparison without mutating shared state.

Keep `catalog` and read-only explanations available to observers, while composition/proposal/simulation request paths remain private-development operations and are rejected in public mode. Do not add activation in this slice. Reuse the existing `RecallIntent`, `ContextPacket`, `FrontierSignal` and `EvolutionProposal` contracts; use application-layer scope predicates honestly and preserve all source rows and omission reasons.

The adapter lives in `backend/agentd/src/agent_session.rs`, with route wiring in `main.rs`, executable private replay coverage in `scenarios.rs`, and a read-only observer projection in the existing DB/Mind surface only after the backend response shape is stable. The LessVille pressure test remains the feedback loop for deciding which semantics should later move into LessDB core.


## Phase 4 — character competencies and reciprocal exchanges

Make character development visible through evidence rather than level-up fiction. A completed café/service exchange is the first bounded social unit: one resident states a need or preference, another responds, and LessDB stores the context, outcome and source references. Repeated successful exchanges update a per-domain competency projection with an evidence count, bounded level, status and policy version.

The competency and interaction projections remain application-owned incubator contracts above LessDB. They must not become a global trust score, native permission grant, automatic policy mutation or physical action authority. The Mind panel exposes action → response, reciprocity, evidence, status and boundary language. The scenario suite must prove idempotent interaction IDs, repeated competency evidence, source provenance and `auto_apply=0`; a private HTTP replay must reach the real order path before any release.

The next slice after this one is identity-thread fork/reset history and competency decay/transfer, followed by richer multi-step dialogue and mixed-outcome repair. Each must be a separate replayable contract and must be released only after backend, UI, privacy, scenario, traffic, physical-adapter and production observer-only checks.


## Phase 5 — identity continuity and competency state transitions

Add a small, append-only continuity layer to make character progression more natural without claiming inner consciousness. Each resident receives a root identity thread, while explicit research branches may record `fork` or `reset` events that cite a parent thread and preserve all prior evidence. The current branch is application metadata, not a native identity primitive.

Add `lessdb-identity-thread/v1` and `lessdb-competency-state/v1` to the incubating typed vocabulary. Persist `lv_identity_threads` and `lv_competency_events`; successful exchanges emit `support` events, mixed café outcomes emit `contradiction` events, and bounded quiet-period experiments can emit `decay` events. The current competency projection remains bounded to level 0–5 and keeps evidence counts and source references.

Expose both ledgers through the read-only `/api/agent/{id}` response and Mind panel. Add an executable replay proving fork/reset lineage, support → contradiction → decay transitions, source preservation and `auto_apply=0`. Keep transfer, multi-step dialogue and release deployment as separate follow-up slices.
## Phase 6 — situated presence
Props were baked into base frames, so characters looked like they always held a leash, phone or cup. Phase 6 makes every visible prop a consequence of authoritative state: a deterministic `presence/v1` policy derives attention, hands, companion, umbrella, setting, nearby characters and surroundings each minute, the renderer chooses a prop-holding or hands-free frame from that state, and every change is a bounded LessDB row. It is labelled as rules, not awareness, and the schema stays application-level (`lessville-situated-presence/v1`) until a second simulation needs it in the typed vocabulary.
