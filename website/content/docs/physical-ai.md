# Physical AI with LessDB

Physical AI agents do more than exchange messages. They observe sensors, maintain spatial and temporal situation, coordinate with other agents and propose actions that may affect a home, facility, vehicle, field or infrastructure.

LessDB is intended to be the **context, memory and provenance substrate** for that loop—not the robot controller or safety interlock.

> **LessDB stores bounded observations, evidence references, situation context and action provenance. Edge systems own real-time control. Applications own policy and approval. Executors own actuation and reconciliation.**

## The embodied-agent loop

```text
sensor/device → edge normalisation → observation evidence
                                      ↓
                              LessDB context broker
                                      ↓
                       model proposes or explains a plan
                                      ↓
                              policy and safety gate
                                      ↓
                         executor / robot controller
                                      ↓
                        result and telemetry observation
                                      ↓
                              LessDB reconciliation
```

A model completion or an `approved` database row is never a motor command and is not a physical safety guarantee.

## What the context contract adds

The incubating `less-agent` contract provides typed values for:

- **Spatial pose:** coordinate-frame identity and quantized position/orientation.
- **Spatial uncertainty:** bounded location radius, confidence metadata and optional covariance reference.
- **Physical observation:** device and sensor identity, observation time, sequence, valid-until, calibration and evidence references.
- **Safety envelope:** allowed action kinds, speed/duration limits, stop conditions and required approvals.
- **Actuation proposal:** target, source observations, context ID, idempotency key, approval state and executor identity.

These are transport-neutral coordination types. They do not claim sensor fusion, motion planning, real-time deadlines, collision avoidance, emergency-stop behavior, device authentication or functional-safety certification.

## Adoption profiles

### Home and assistive robots

Keep raw audio and video on the device or separately governed evidence store by default. LessDB can retain consented preferences, bounded room-state observations, source references and action outcomes. A human or certified safety service approves non-destructive actions, reminders or escalation according to the deployment policy.

### Warehouses, factories and laboratories

Use site → zone → line or experiment → device → run or sample scopes. Attach calibration, coordinate frame, uncertainty and device sequence to observations. A verifier or planner gets a bounded packet; PLC/robot-controller interlocks and real-time control remain outside LessDB.

### Field robotics and environmental systems

Make regions, coverage, freshness and uncertainty explicit. Distinguish measurement, inference, forecast and proposed intervention. Offline queues, duplicate sequences, missing observations and later reconciliation are expected rather than hidden.

### Mobility, logistics and infrastructure

Treat vehicle, route, cargo, person, facility and operator as separate identities and scopes. A dispatch or maintenance agent may propose; a certified fleet, building or infrastructure service re-checks freshness and constraints before execution.

## Safety and governance rules

Physical-AI deployments should fail closed when they lack:

- verified device or operator identity;
- a valid coordinate frame;
- fresh observations within the approved time window;
- an active policy version and safety envelope;
- a valid approval after stop conditions are considered;
- an idempotency key and executor identity.

LessDB records the blocked proposal and reason. The application or controller is responsible for actually preventing the physical action.

## Conformance scenarios

The public adoption catalog now includes PA-01 through PA-06:

1. sensor provenance and spatial uncertainty;
2. spatial-temporal bounded recall;
3. proposal-first actuation;
4. offline edge reconciliation;
5. least-privilege fleet handoff;
6. human stop and stale approval.

They are simulated fixtures first. They verify agreement across device state, LessDB rows, ContextPacket visibility, operator projections and replay behavior. They do not issue live actuator commands.

Generate deterministic physical fixtures locally:

```bash
python3 tools/generate_agent_scenarios.py \
  --seed 20261010 \
  --count 6 \
  --profile embodied-household \
  --profile industrial-operations \
  --profile field-robotics \
  --out /tmp/lessdb-physical-ai-scenarios.json
```

## Honest boundary

LessDB can help an embodied agent answer **“what do I know, where did it come from, is it still valid, who may see it and what proposal is being considered?”**

It should not answer **“is it safe to move this motor right now?”** without the independent controller, interlocks, approvals and deployment controls that own that decision.

See the [agent deployment blueprints](/docs/agent-blueprints) and the [agent adoption scenarios](/docs/agent-scenarios). LessVille remains the public fictional simulation for observing these context, provenance and coordination ideas without operating real devices.
