# LessDB agent-session adapter

**Status:** implemented and privately verified locally; not deployed.

The first adapter slice gives LessVille an agent-shaped interface above embedded LessDB. It is intentionally a proving ground for the agent-native meta-plane, not a claim that the storage engine now provides native row ACL, autonomous schema evolution or physical authorization.

## Operations

| Operation | Route | Mode | Behavior |
| --- | --- | --- | --- |
| `catalog` | `GET /api/agent/session/catalog` | read-only | Reports contract versions, bounds, supported operations and explicit non-guarantees. |
| `compose` | `POST /api/agent/session/compose` | private | Compiles a bounded `ContextRecipe` and current context packet from agent, task, purpose, source kinds, scopes and budgets. |
| `explain` | `GET /api/agent/session/explain/{id}` | read-only | Explains request-scoped context/recipe identifiers, frontier signals and candidate evolution records. |
| `propose` | `POST /api/agent/session/propose` | private | Validates and persists a declarative candidate `EvolutionProposal`. |
| `simulate` | `POST /api/agent/session/simulate` | private | Replays a candidate against bounded evidence and returns a `CounterfactualDecision` with `not_applied=true`. |

Private operations are accepted only in local/private mode. Public mode exposes the read-only catalog and explanation routes but returns HTTP 403 for compose, propose and simulate, preserving LessVille's observer-only contract.

## Compose behavior

`compose` reuses the current LessVille context broker and its explicit application-layer owner/membership predicate. It does not create a native grant. The response contains:

- a validated `lessdb-context-recipe/v1` recipe;
- the situation-shaped packet and source memory IDs;
- the selected item and byte budgets;
- scope, expiration and budget omission explanations;
- the boundary statement that native LessDB ACL is not implied.

A requested scope must already be a membership of the requesting agent. The adapter cannot use a recipe request to widen visibility. A bounded packet keeps source rows in place and returns omission reasons rather than silently copying or deleting evidence.

## Proposal behavior

Candidate proposals must use `lessdb-evolution-proposal/v1`, include evidence, affected scopes, a compatibility class, migration and rollback references, and set `auto_apply=false`. The adapter rejects:

- non-candidate states;
- missing evidence or compatibility;
- missing migration or rollback references;
- affected scopes outside the owner/proposing-agent namespace;
- executable SQL or DDL-like patch text.

The persisted application row is `lv_evolution_proposals`. It remains `candidate`; no schema, scope, policy or actuator behavior changes.

## Simulation behavior

`simulate` loads a candidate and compares the current bounded visible-memory baseline with a candidate-only branch. The result records the proposal source reference and explicitly reports:

```text
not_applied = true
source_rows_changed = false
scope_changed = false
policy_changed = false
actuator_called = false
```

This is an explanatory replay, not formal verification, a migration dry-run with engine guarantees, or an approval.

## Local evidence

The executable private replay covers catalog, scoped compose, proposal persistence, explanation, simulation, unchanged source-memory and scope counts, and candidate-only state. The HTTP replay against fresh private/public backends verified:

```text
private: catalog 200, compose 200, propose 200, explain 200, simulate 200
public:  catalog 200, compose 403, propose 403, simulate 403
```

The complete LessVille backend suite passed 28 tests with one external simulator-file test intentionally ignored. Godot parsing and the native project check passed. The route is not part of the deployed public release yet.

## Next boundary

The next slice should add a persisted session/task lineage and a replay manifest so a composed packet can be reproduced across restart with explicit policy, time, scope, frame and budget inputs. Activation remains out of scope until proposal review, compatibility, migration and rollback evidence exist.
