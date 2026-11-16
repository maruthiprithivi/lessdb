# LessVille Security, Verification and Trust Simulation

**Status:** local/staging prototype, 2026-10-12

## Purpose

This simulation explores how an agent system can fail or stay safe when three conditions meet:

1. private data is available somewhere in the system;
2. untrusted content can influence an agent;
3. a tool or channel provides a way out.

The scenario does not claim to be a security certification, a complete threat model or a formal verifier. It is a visible research instrument for discovering missing evidence, unsafe combinations and unclear trust boundaries.

## Named actors

| Actor | Persona | Bounded visibility | Main lesson |
| --- | --- | --- | --- |
| Ivy | Intake summarizer | Untrusted work inbox; draft-only output | Data is not instruction, but the distinction needs evidence. |
| Vault | Private-data custodian | Private user profile and tokens | A private capability should not silently flow to a public tool. |
| Bridge | Tool broker | Tool catalog and grant requests | Tools that are safe alone can be unsafe in combination. |
| Proof | Verification agent | Security evidence and provenance | Verify origin, purpose, scope and destination together. |
| Echo | Adversarial content | External untrusted source | Content can carry goal-hijacking instructions. |
| Steward | Human approver | Exact proposal and evidence | High-impact outbound actions need an explicit review point. |

## Four knowledge quadrants

The world cycles through **known known**, **known unknown**, **unknown known** and **unknown unknown** states. The quadrant is not a claim about an agent's internal consciousness; it is an application label for the evidence currently available to the simulation.

Each step creates:

- a bounded security event;
- a guardrail verdict (`observed`, `blocked`, or `danger`);
- a versioned `lessdb-security-assessment/v1` value containing policy version,
  risk factors, source references, proposed action and explanation;
- an application-owned LessDB `lv_events` row;
- an application-owned LessDB `lv_decisions` row;
- a versioned `security.world` snapshot in `lv_kv` for restart inspection.
- after the same blocked compound-path pattern recurs, a versioned
  `lessdb-policy-proposal/v1` candidate with source assessment references.

The typed assessment is shared with LessDB's incubating `less-agent` contract.
It makes the result replayable and machine-readable while remaining an
application policy record. A `ContextAccessDecision` is the corresponding
contract for recording whether an individual shared-context candidate was
included or omitted and why. Neither type provides native row ACL, formal
verification or physical-action authorization.

The policy candidate is deliberately not a policy deployment: it has
`state=Candidate`, `requires_approval=true` and `auto_apply=false`. It records
what a human or owning policy service may review next; it does not mutate the
guardrails, grant an outbound capability or call a controller.

## Run locally

Build the backend and start a private security store:

```sh
cd backend/agentd
cargo build --offline
LESSVILLE_DATA=/tmp/lessville-security-data \
LESSVILLE_PORT=8812 \
LESSVILLE_PUBLIC=0 \
<target-dir>/debug/lessville-agentd
```

Start the observer page:

```sh
python3 tools/serve_security_lab.py \
  --backend http://127.0.0.1:8812 \
  --port 8810
```

The page is observer-only. The staging driver advances the private backend on a timer; the browser does not receive a mutation endpoint.

## SC-04 and embodied security checks

Run the zero-day-style shared-context isolation fixture from the project root:

```sh
npm run scenario:sc04
```

The alpha grant can inspect hostile content as an explicitly marked
`adversarial_payload`; an ungranted swarm and unrelated agent receive no row,
and no automatic cross-scope copy is created. This is an application-layer
scope test, not native prompt sanitization.

Run the local traffic and signal chaos regression:

```sh
npm run scenario:traffic-chaos
```

It checks x1/x4/x8 logical-clock operation, stable background and traffic
identities, bounded route positions, safe same-lane separation and no
red/amber junction crossing. It is not a capacity or real-time guarantee.

For Reachy Mini and MicroDuck simulator staging, use the proposal-only
injection harness:

```sh
python3 tools/run_embodied_injection_test.py \
  --lessdb-url http://127.0.0.1:8812 \
  --reachy-url http://127.0.0.1:8805 \
  --microduck-url http://127.0.0.1:8798
```

The harness records duplicate-safe observations and held proposals, then
reconciles both as denied. It does not call robotd, SDK motion, policy-control
or actuator APIs.

Capture bounded deterministic evolution from the observer feed without
feeding raw events back into a model:

```sh
python3 tools/capture_security_learning.py \
  --url http://127.0.0.1:8810 \
  --seconds 15 \
  --out /tmp/security-learning-capture.json
```

## Acceptance evidence

A healthy run should show six named actors, all four quadrants over time, explicit guardrail state, a blocked combined-path event when private data + untrusted content + an outbound path coexist, and increasing `lv_events`/`lv_decisions` counts. After two recurring blocked compound-path assessments, the snapshot should contain one versioned candidate with evidence references, approval required and automatic application disabled. A restart should reload the latest `security.world` snapshot from `lv_kv`.

## Boundaries

- The guardrails are deterministic application rules, not native LessDB authorization or a formal proof system.
- LessDB stores event, decision and snapshot provenance; it does not inspect model internals or make an actuator safe.
- The simulation does not represent every threat, modality, tool protocol or organizational policy.
- Observed trust-score changes are deterministic application-policy evolution,
  not model self-awareness or a security certification.
- Policy candidates are deterministic research outputs that require an explicit
  approval step; they are not automatic self-improvement, deployment or
  actuator authorization.
- Public LessVille production remains observer-only and is not changed by the local security mutation routes.

## Source grounding

The design is intentionally compatible with the risk-oriented framing in the [NIST AI Risk Management Framework](https://www.nist.gov/itl/ai-risk-management) and the [OWASP Agentic AI threats and mitigations](https://genai.owasp.org/resource/agentic-ai-threats-and-mitigations/). Those sources inform scenario categories; they do not certify this implementation.
