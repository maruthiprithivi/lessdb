# Security and traffic verification report

**Date:** 2026-10-12
**Status:** local/private staging evidence; not a security certification

## Scope

This report records bounded, replayable tests for LessVille and its application-owned
LessDB adapters. Hostile strings remain data in the test store. No model prompt,
robot motion API, actuator, policy-control endpoint or production write route was
used by these tests.

## Results

| Check | Result | Evidence |
| --- | --- | --- |
| SC-04 zero-day payload scope isolation | **PASS** | `npm run scenario:sc04`; the granted alpha scope can inspect an `adversarial_payload`, while beta and unrelated agents cannot recall it and no automatic beta copy is created. |
| Full scenario suite | **PASS** | `npm run scenario:test`; SC-01 through SC-04: 4 passed. |
| Backend regression | **PASS** | `cargo test --offline`; 20 passed, 1 ignored simulator-file integration test. |
| Traffic and signal chaos | **PASS** | Optimized release x1/x4/x8 run: 15 samples, six traffic actors and eight background residents; identities/routes/signals/separation/red-amber invariants held. |
| World stability | **PASS** | x4 and x8 run: monotonic logical time, stable background population and seeded weather fallback. |
| Physical adapter unit test | **PASS** | `npm run physical:test`; duplicate-safe envelope/reconciliation fixture passed. |
| Reachy Mini staging health | **PASS** | `/` and `/health` returned HTTP 200. The Reachy visual source intentionally does not expose the MicroDuck-style `/state.json` route. |
| MicroDuck staging health | **PASS** | `/`, `/health` and `/state.json` returned HTTP 200. |
| Embodied hostile injection | **PASS** | Two profiles, duplicate replay, held proposals and denied reconciliations; actuator flags remained false. Live private staging was used, not production. |
| Bounded security observation | **PASS WITH LIMIT** | Observer capture showed deterministic quadrant/trust projections and a blocked compound-path verdict. A short window did not increase event count, so no new learned pattern is claimed. |
| Typed policy-proposal replay | **PASS** | Fresh private optimized backend: 11 deterministic steps produced one `lessdb-policy-proposal/v1` candidate backed by two assessments; approval was required, `auto_apply=false`, and all guardrails remained unchanged. |
| Godot/source validation | **PASS WITH LIMIT** | Godot parser and `npm run check` passed. Blueprint visual coverage remains incomplete with eight approved roles unresolved; this is not claimed as full visual acceptance. |

## Interpretation

The current learning loop is a deterministic observer and policy-evolution
instrument. It can expose recurring guardrail causes and produce a versioned
policy proposal, but the verified candidate remains approval-gated and does not
automatically change policy, call a model, grant a scope or actuate a physical
system. Application-layer scope checks, proposal holding and reconciliation
denial must not be described as native LessDB ACL, formal verification, safety
interlocks or actuator authorization.

## Reproduction commands

```sh
npm run scenario:test
npm run scenario:traffic-chaos
npm run physical:test
python3 tools/run_world_stability.py --binary <agentd-debug-binary> --seconds 8
python3 -m py_compile tools/*.py
```

The private Reachy/MicroDuck injection command and observer capture command are
in `docs/SECURITY-TRUST-SIMULATION.md`.
