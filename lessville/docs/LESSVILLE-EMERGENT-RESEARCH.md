# LessVille bounded-emergence research phase

**Status:** implemented and locally verified on 18 October 2026; not deployed.

## Why this phase exists

LessVille is now used as a pressure test for LessDB's agent-native direction. The goal is not to make characters secretly rewrite code or schemas. The goal is to let bounded variation create situations that are awkward to express with a conventional storage interface, then record those awkward situations as concrete LessDB requirements.

This creates two linked workstreams:

1. **LessVille evolves:** residents encounter novelty, make small local deviations, request context, form evidence and produce different but replayable trajectories.
2. **LessDB evolves:** recurring friction becomes a typed frontier signal and an inactive evolution proposal for a view, memory lens, context recipe, grant or additive projection.

A proposal is evidence about a missing capability. It is not an activation, migration, permission change, model-policy change or actuator command.

## Bounded novelty model

Every world has a persisted `world_seed`. At bounded clock buckets, each foreground resident receives a deterministic opportunity selected from a small vocabulary:

- `social_detour` — a low-risk social-context possibility;
- `weather_route` — a request to combine weather, route, temporal and spatial evidence;
- `shared_context_request` — a possible one-fact lesson exchange that would require an explicit expiring grant;
- `skill_experiment` — a small practice variation that must not silently change occupation or identity.

The branch is selected from the world seed, day, clock bucket and agent identity. The same seed reproduces the same opportunity; different seeds can produce different options. This is controlled pseudo-random variation, not unrestricted autonomous self-modification.

Each opportunity is bounded to the agent's own scope, expires after 45 simulated minutes, and produces linked evidence:

```text
novelty event
  -> agent event and temporary memory
  -> deterministic decision
  -> frontier gap signal
  -> inactive evolution proposal
```

## LessDB gap signals

The first frontier capabilities are deliberately expressed as logical contracts rather than arbitrary DDL:

| Frontier capability | What the town exposes | Candidate LessDB direction |
| --- | --- | --- |
| `memory-lens.social-consent` | A social possibility is not yet a relationship or permission | Consent-bound social memory lens with reciprocity evidence |
| `context-recipe.temporal-spatial` | A route decision needs weather, freshness and frame together | Situation-shaped context recipe |
| `shared-context.expiring-grant` | One useful fact should be shared without copying a private row | Recipient-specific, expiring teaching grant |
| `derived-view.skill-evidence` | Repeated practice should form evidence without one global score | Derived skill-evidence view |

LessVille persists these records in application-owned tables:

- `lv_novelty_events`;
- `lv_frontier_signals`;
- `lv_evolution_proposals`.

The read-only state snapshot and DB panel expose the replay key, frontier capability, recurrence, proposal ID and inactive state.

## Safety and governance boundary

The emergent slice must not:

- execute arbitrary code or arbitrary DDL;
- widen a scope or create a scope membership;
- copy another agent's private lesson implicitly;
- activate a schema, lens, grant or policy proposal;
- issue a robot, daemon, actuator or safety-controller command;
- delete contradictory evidence;
- claim consciousness, general intelligence, native ACL, formal verification or physical safety.

The world can produce a candidate and LessDB can eventually simulate it. Activation remains a separate owner/controller operation requiring compatibility, evidence, migration and rollback references.

## Blocker discovered during implementation

The first run attempted to persist the full unsigned 64-bit replay seed in an `Int64` column. LessDB correctly rejected values above the signed range. The fix preserves the complete seed in the human-readable replay key while storing a bounded positive diagnostic projection in the table. This is a useful example of the pressure-test loop: replay identity and storage representation need an explicit typed contract rather than an implicit cast.

## Local evidence

- LessVille backend: 25 tests passed, 1 simulator-file test ignored because it requires an external generated JSONL file.
- Bounded-emergence scenario: passed; same-seed replay, different-seed variation, frontier capture, inactive proposal state and no scope widening were verified.
- Godot parser: passed.
- Native project check: passed; existing visual-coverage metadata remains incomplete and is not being represented as complete.
- Physical adapter regression: passed.
- Traffic chaos: passed at x1/x4/x8 with 15 samples, six traffic actors and eight background actors.
- World stability: passed at x4/x8 with seeded fallback weather.
- Python tooling compilation: passed.
- Reusable `security-trust-simulation` skill validation: passed.

This evidence is local. The production LessVille runtime remains on the prior release until a separate release build, schema review, deployment and live observer-only verification.

## Next research slices

1. Add a versioned `agent_session` adapter that can catalog primitives, compose a `ContextRecipe`, explain omissions and create a typed `EvolutionProposal`.
2. Add replay comparison for candidate context recipes: included items, omissions, scope effects, freshness effects and budget effects.
3. Add identity-thread fork/reset records so a resident can change direction without erasing its prior evidence.
4. Add a human-approved proposal fixture that activates only an additive private lens, then test rollback and restart persistence.
5. Use the resulting conformance failures to decide which semantics belong in LessDB core and which remain application-owned.
