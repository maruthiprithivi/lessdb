# LessVille character growth and lesson loop

**Status:** local implementation tested on 18 October 2026; not deployed. Deterministic application behavior, not autonomous learning.

## Inspiration handled safely

The requested reference is used only for high-level design themes: identity continuity, relationships that shape later choices, learning through consequences, and moral or practical growth inside a persistent world. LessVille does not reuse protected characters, dialogue, plot events, names or setting details. The characters remain fictional research-simulation residents.

## Gaps found

LessVille already persists memories, messages, decisions, occupation changes and model-written nightly reflections. The important gaps were:

1. A reflection was only a free-text memory, so the system could not distinguish a candidate lesson from an accepted behavioral rule.
2. Repeated experiences were not aggregated into a typed, inspectable progression record.
3. The UI showed what an agent remembered but not how repeated evidence changed its future behavior.
4. There was no reusable LessDB incubation type for a bounded lesson with evidence, confidence, state and an explicit non-automatic boundary.
5. The existing deterministic rules already changed routes from memories, but the provenance chain did not say “this later decision used a learned lesson.”

## New lesson model

A lesson is an application-owned, versioned evidence record:

- `candidate`: a possible generalization from one bounded experience or one nightly reflection;
- `accepted`: a deterministic application policy has enough repeated evidence to use it in a later rule;
- `superseded` or `revoked`: the lesson is no longer eligible, while the evidence remains explainable;
- `auto_apply=false`: the record is never an automatic deployment, permission grant, model-policy change or physical command.

Each lesson carries an agent, topic, trigger, short statement, source references, confidence, policy version and logical timestamps. Source references point back to LessDB event or memory rows. The lesson ledger is separate from ordinary memories so a user can tell the difference between an observation and a generalized habit.

## Example progression

Lin's commute provides the first replayable example:

1. A named bus arrives late.
2. LessDB stores the event and Lin's private temporal memory.
3. The lesson ledger records `commute.bus` as a candidate with the event and memory references.
4. A repeated late-bus observation creates enough deterministic evidence for an accepted lesson.
5. On a later morning, the commute rule consults the accepted lesson and chooses an earlier bus.
6. The decision explanation includes the lesson ID and source references, so the observer can inspect what changed and why.

The same pattern is used for trail conditions: repeated rain and muddy-trail observations may form a bounded route lesson. The rule changes a route choice, not the character's identity, permissions or safety envelope.

## Model-call boundary

Nightly model reflection remains a single concise task over at most six of the agent's own memories. Its output is validated and stored as a private reflection memory plus a `candidate` lesson. Model output cannot approve or apply a lesson, modify a deterministic policy, widen a shared scope or control a robot. If the model is unavailable or over budget, deterministic rules continue unchanged.

## Product interpretation

The character experience should feel continuous because the world retains evidence, relationships and consequences. It should not pretend that a trust score or lesson row proves consciousness, subjective experience or general intelligence. The Mind panel therefore shows:

- growth stage: observing, forming, adapting or integrating;
- candidate and accepted lesson counts;
- the lesson statement and confidence;
- evidence references and policy version; and
- the later decision that used an accepted lesson.

## LessDB boundary

`lv_lessons` and the typed `lessdb-learning-lesson/v1` value are an application/incubation contract above LessDB storage. They do not claim native row-level ACL, native semantic forgetting, full ACID multi-row commits, distributed consistency, model awareness, formal verification or physical actuation safety.

## Current evidence

The local slice adds idempotent `lv_lessons` persistence, candidate-only nightly reflection lessons, repeated late-bus evidence promotion, a later commute decision that cites the accepted lesson, consent-bound relationship edges, explicit lesson challenge/repair history, expiring teaching grants and non-executing counterfactual comparisons. The focused LessDB contract suite passed 19 tests; the LessVille backend suite passed 23 tests with one simulator-file integration test ignored because it requires an external generated JSONL file; the executable adoption suite now includes seven passing scenarios; the physical adapter test passed; traffic chaos previously passed at x1/x4/x8 with 15 samples; Godot parsing passed; and the native project check passed with the existing visual-coverage metadata still marked incomplete. This evidence is local and has not been promoted to the public runtime.

## Next gaps after this slice

- Relationship edges with explicit reciprocity, trust context and consent provenance.
- Skills and competencies that grow from multiple lesson topics without collapsing into a single score.
- Lesson expiry, contradiction and supersession policies.
- Human-approved configuration changes that can accept a policy proposal while preserving a replay record.
- Identity-thread reset/fork records and richer relationship evidence without collapsing it into a single global trust score.

The relationship and contestation slice is now implemented locally: `lessdb-relationship/v1` records distinct agents, consent, reciprocity, trust context and evidence; `lessdb-lesson-challenge/v1` records contradiction/correction/repair; `lv_relationships` preserves granted and revoked versions; `lv_lesson_challenges` preserves the challenge; and later lesson recall excludes only explicitly repaired or superseded lessons. The read-only Mind panel shows both ledgers. The executable replay proves that the old lesson remains evidence, the repaired replacement is eligible, relationship consent can be revoked, and neither transition auto-applies a permission, policy or actuator command.

The teaching/replay slice is now implemented locally: `lessdb-teaching-grant/v1` records who may present which lesson, for what purpose, in which scope and during what validity window; `lv_teaching_grants` preserves active and revoked grant history; recipient recall returns the teacher's source row without copying it; and `lessdb-counterfactual/v1` / `lv_counterfactuals` compare baseline and lesson-aware branches with `not_applied=true`. The read-only Mind panel exposes grant and replay evidence. An executable scenario proves active access, no implicit lesson copy, later revocation and non-executing replay.

The bounded-emergence slice is now implemented locally: a persisted world seed selects replayable novelty opportunities for residents at bounded clock buckets; each opportunity produces a world event, temporary memory, deterministic decision, frontier gap signal and inactive evolution proposal. The read-only DB panel exposes the replay key, capability gap, recurrence and proposal ID. Same-seed replay, different-seed variation, no scope widening and `auto_apply=false` proposal state are covered by an executable scenario. This is controlled pseudo-random variation, not unrestricted self-modification.

The first implementation pressure-tested the storage boundary: an unsigned 64-bit replay seed exceeded LessDB's signed `Int64` column range. The full seed now remains in the replay key while a bounded diagnostic projection is stored in the application table. This is recorded as a LessDB design gap rather than hidden behind an implicit cast. Full details are in `docs/LESSVILLE-EMERGENT-RESEARCH.md`.

The remaining work should add identity-thread reset/fork records, richer relationship evidence and an approved additive-lens activation/rollback fixture as separate conformance-tested slices rather than introducing an unrestricted self-modifying loop.

## Reference handling

The named fictional reference informed only the high-level themes above. The implementation remains an original LessVille design. Public-facing claims continue to describe a deterministic research simulation with LessDB-backed provenance.


The first `agent_session` adapter slice is now implemented locally and remains undeployed. `catalog` reports the incubating typed capabilities and non-guarantees; `compose` validates a situation-shaped `ContextRecipe`, applies bounded owner/membership recall and returns explicit scope, temporal and budget omissions; `explain` reports request-scoped context/recipe, frontier and candidate-proposal provenance; `propose` persists declarative candidate evolution only; and `simulate` returns a `not_applied=true` counterfactual without changing source rows, scopes, policies or actuators. Private HTTP replay passed all five operations, while public mode exposed catalog but returned 403 for compose, propose and simulate. The next gap is a persisted session/task replay manifest before any activation workflow.


### Competency and reciprocity slice — 2026-10-18

LessVille now records completed café handoffs as `lessdb-social-interaction/v1`-shaped application evidence in `lv_interactions`. Each exchange preserves the initiator, recipient, context/order, action, response, reciprocity statement, outcome and source references. Successful exchanges also update bounded `lessdb-competency/v1`-shaped records in `lv_competencies`; repeated evidence moves a skill from `emerging` to `practising` and later `reliable` without turning the resident into one global score or granting permissions.

The selected character's Mind panel now shows competency level, evidence count, status, policy version, interaction direction and source references. A private HTTP replay reached the real service path and produced a persisted handoff interaction plus Sam's `clear preference communication` competency by simulated minute 536. The executable scenario proves two reciprocal exchanges accumulate Maya's `context-aware service` evidence to `practising`, preserve source references and keep `auto_apply=0`. LessDB typed-contract tests passed 24 tests; LessVille backend tests passed 29 with one external-simulator-file test ignored; all ten adoption scenarios, physical adapter, traffic chaos, x4/x8 world stability, Godot parser/project checks and Python tooling gates passed against the exact character binary.

This is application/incubation behavior above LessDB storage. It does not prove consciousness, subjective learning, global trust, native permissions, automatic policy change or physical authorization. The next character gaps are identity-thread fork/reset history, competency decay and transfer rules, richer multi-step conversations, and explicit conflict handling when a social outcome is mixed rather than a clean handoff.


### Identity continuity and competency-state slice — 2026-10-18

The next character slice adds two append-only evidence ledgers. `lessdb-identity-thread/v1` records a resident's root thread and any explicit fork/reset branch with parent thread, reason, continuity statement and source references. A branch does not delete or rewrite the earlier thread and does not assert a human-like self. `lessdb-competency-state/v1` records `support`, `contradiction`, `decay` or `transfer` transitions with prior/next levels, evidence and policy version; it cannot auto-apply permissions, policies or actions.

Successful café handoffs now emit support state events. A real declined-alternative café outcome emits a contradiction event for the staff competency when a prior projection exists. The read-only agent detail API and Mind panel expose identity history and state transitions, including the distinction between a mixed outcome and a clean success. A local executable replay proves root → fork → reset lineage, two support events, contradiction then decay, preserved source references, bounded level 0–5 and `auto_apply=0`.

This remains application/incubation behavior above LessDB storage. Competency transfer, richer multi-step dialogue, automatic decay scheduling and production rollout remain separate reviewable slices.

## Situated presence (deterministic:presence/v1)
Characters now carry things only when the situation justifies it. A phone appears for a short reply window after a message from someone who is elsewhere, and is put away when that window ends or when the character is talking or busy. Mochi and the leash appear only while Ama is outdoors on a walk to or from the park or trail. Shopping is put away at home, drinks are finished, backpacks are set down indoors and umbrellas open only outdoors in rain. Each change records the rule that produced it in `lv_presence_events`, so an observer can see why a prop appeared. This is a deterministic policy over simulation state; it is not model reasoning and makes no claim of awareness.
