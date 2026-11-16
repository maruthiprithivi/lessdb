# LessVille growth-loop outcomes

## Structured lesson contract and persistence

- Add a versioned `lessdb-learning-lesson/v1` contract to the LessDB incubator with agent identity, topic, trigger, bounded statement, source references, confidence, state, policy version, timestamps and an explicit `auto_apply=false` rule.
- Add `lv_lessons` to the embedded LessVille schema and persist idempotent candidate and accepted lesson records with source provenance.
- Keep model-generated reflections as candidate evidence only; a model response must not approve a lesson, change a rule, widen a scope or invoke an actuator.

## Repeatable character growth

- Record recurring bus and trail experiences as lesson candidates.
- Promote a candidate only after deterministic repeated evidence and record the promotion as a separate provenance event.
- Make at least one later route decision consult an accepted lesson and show the lesson reference in the decision explanation.
- Keep all transitions deterministic, replayable and bounded by the existing simulated clock.

## Observer experience and validation

- Add growth stage, candidate/accepted counts and lesson evidence to the selected character's Mind panel.
- Add an executable scenario proving candidate creation, repeated-evidence promotion, future decision influence and no cross-agent leakage.
- Run LessDB contract tests, LessVille backend tests, adoption scenarios, project checks, Godot parser checks and privacy/diff audits before delivery.
- Document the gap analysis and the distinction between app-layer learning evidence and native LessDB guarantees.

## Consent-bound relationships and repair — complete locally, not deployed

- [x] Add a versioned `lessdb-relationship/v1` incubation contract with distinct agents, relationship context, consent state, reciprocity evidence, source references, confidence, lifecycle state and `auto_apply=false`.
- [x] Persist relationship edges in `lv_relationships` only when the simulation has an explicit consent or revocation event; preserve prior versions so observers can distinguish granted from revoked context.
- [x] Add a versioned `lessdb-lesson-challenge/v1` incubation contract and `lv_lesson_challenges` ledger for contradiction, correction and repair evidence; preserve challenged lessons rather than deleting them.
- [x] Make later lesson recall exclude only lessons explicitly superseded or repaired, then use the repaired replacement with a new policy version; do not automatically change permissions, policies or actuators.
- [x] Expose relationship and challenge provenance in the read-only Mind panel and prove the full consent/revocation and lesson-repair replay with an executable scenario.

## Teaching and replay — complete locally, not deployed

- [x] Add a versioned `lessdb-teaching-grant/v1` contract with teacher, recipient, lesson, purpose, explicit scope, validity window, source evidence, revocation state and `auto_apply=false`.
- [x] Persist `lv_teaching_grants` without copying the teacher's lesson into the recipient's lesson ledger; recall is allowed only through an active, unexpired grant and is suppressed by a later explicit revocation.
- [x] Add a versioned `lessdb-counterfactual/v1` contract and `lv_counterfactuals` ledger comparing baseline and lesson-aware choices with source references and `not_applied=true`.
- [x] Expose teaching and replay provenance in the read-only Mind panel and prove active grant, no implicit copy, revocation and non-executing replay in an executable scenario.

## Still open

- Add counterfactual replay, identity-thread reset/fork records and explicit cross-agent teaching grants with expiry and revocation.
- Add identity-thread reset/fork records and richer relationship evidence without collapsing it into a single global trust score.

## Bounded emergence and LessDB gap discovery — complete locally, not deployed

- [x] Generate bounded novelty opportunities from the persisted world seed, day, agent identity and current situation; different seeds may produce different trajectories, while the same seed reproduces the same branch.
- [x] Let an opportunity change only a local routine, social/context request or observer-visible proposal; it must not execute arbitrary code, widen scopes, mutate shared schema automatically or invoke physical controls.
- [x] Persist linked novelty event, agent memory/decision and research-frontier evidence so observers can follow what happened, what the agent lacked and what LessDB contract might address it.
- [x] Create candidate evolution proposals for views, memory lenses, context recipes or additive projections with evidence, affected scope, compatibility, replay seed and `auto_apply=false`; never activate them automatically.
- [x] Expose the active opportunity, frontier signals, candidate proposal state and replay seed in the read-only observer UI.
- [x] Add a deterministic scenario proving same-seed replay, bounded different-seed variation, no scope widening and proposal-only evolution; keep production unchanged until a separate review.


## Agent-session adapter — complete locally, not deployed

- [x] Add a transport-neutral `agent_session` adapter above the existing LessDB application layer with `catalog`, `compose`, `explain`, `propose` and `simulate` operations; activation is not included in this slice.
- [x] Make `catalog` report typed contract versions, bounded budgets, supported logical operations and explicit non-guarantees instead of presenting application predicates as native ACL.
- [x] Make `compose` compile a bounded situation-shaped context packet from identity, task, lens, scope and budget inputs, returning included items and explicit omission reasons while preserving source provenance.
- [x] Make `explain` expose request-scoped packet/recipe, frontier signal and candidate-proposal provenance with scope/temporal/budget boundary language.
- [x] Make `propose` accept only declarative, evidence-backed evolution candidates with affected scope, compatibility, migration reference, rollback reference and `auto_apply=false`; reject executable SQL, missing evidence and scope widening.
- [x] Make `simulate` replay a candidate against bounded evidence and return a counterfactual result with `not_applied=true`, unchanged source rows and no permission, policy or actuator mutation.
- [x] Keep public mode observer-only: read-only catalog/explanation routes are visible, while compose/propose/simulate mutation-like requests remain private-development routes and return 403 publicly.
- [x] Add executable private replay coverage, parser/project checks, backend tests and documentation; leave production unchanged until a separate release review.


## Competencies and reciprocal character exchanges — locally verified, release pending

- [x] Add the versioned `lessdb-competency/v1` incubation vocabulary with bounded level, evidence count, source references, policy version and `auto_apply=false`.
- [x] Add the versioned `lessdb-social-interaction/v1` incubation vocabulary with distinct participants, action → response, reciprocity evidence, outcome and source references.
- [x] Persist `lv_competencies` and `lv_interactions`; connect successful café handoffs to the real service path and update competency evidence for both residents.
- [x] Expose competency and reciprocal-interaction provenance in the selected character Mind panel and read-only agent detail API.
- [x] Prove repeated evidence, source preservation and non-automatic semantics in an executable scenario; verify the real private HTTP service path.
- [ ] Build and deploy the optimized character-development release after the release marker, live schema and observer-only checks are complete.

## Identity continuity and competency-state transitions — locally verified, release pending

- [x] Add `lessdb-identity-thread/v1` with root/fork/reset lineage, parent references, source evidence and an explicit non-consciousness boundary.
- [x] Add `lessdb-competency-state/v1` with bounded support, contradiction, decay and transfer event kinds, prior/next levels and `auto_apply=false`.
- [x] Persist `lv_identity_threads` and `lv_competency_events`; add idempotent roots on fresh/resumed worlds and emit support evidence from successful experiences.
- [x] Record a real mixed café outcome as contradiction evidence and expose bounded state changes in the read-only agent detail API and Mind panel.
- [x] Prove root → fork → reset lineage, source preservation, support → contradiction → decay and level-zero clamping in an executable replay plus fresh private/public HTTP checks.
- [x] Build and deploy the optimized identity/competency-state release, refresh the content-addressed Godot PCK, and verify the live observer-only catalog, agent detail, state and mutation guards.

## Next character gaps

- [ ] Add explicit competency transfer between agents or domains with a source grant and replay evidence.
- [ ] Add multi-step conversations and mixed outcomes beyond the café handoff path.
## Situated presence (phones, leashes, carried items, umbrellas)
- [x] Derive hands-free character frames from the approved art (`tools/derive_handsfree_sprites.py`); originals stay as prop-holding variants.
- [x] Add `deterministic:presence/v1` (`backend/agentd/src/presence.rs`): attention, hands, item, companion, umbrella, setting, nearby, surroundings, reason and `since`, derived from clock, place, travel, messages, carried items, weather and co-location.
- [x] Phone only in a 4-minute window after a remote (not face-to-face) message; deferred while talking or typing/cooking/working out/studying; never for café staff at the counter.
- [x] Mochi and the leash only on Ama's outdoor walk to or from the park/trail; never indoors, on the bus or for anyone else. Mochi rests at home otherwise.
- [x] Item lifecycles: shopping put away at home, a take-away drink finished after 30 minutes, a backpack set down while indoors; finished items leave the authoritative carried state.
- [x] Umbrellas only outdoors while it is raining; no claims about off-map "away" places. Nudged showers now update the visible weather label.
- [x] Persist transitions in `lv_presence_events` (bounded by retention) and expose presence in `/api/state` plus the Mind panel with the rule and policy label.
- [x] Executable full-day replay against embedded LessDB: zero invariant violations, identical trace on replay, hands free most of the day.
## Next presence gaps
- [ ] Re-implement the climate, traffic.v2 and foliage/lamp detail lost in the November sandbox reset, with the same deterministic and replay discipline.
- [ ] Pedestrian awareness at crossings and kerbs (waiting for signals, giving way) and passengers visible in the bus.
- [ ] Promote a situated-presence vocabulary into the LessDB typed contracts only once a second simulation needs it.
