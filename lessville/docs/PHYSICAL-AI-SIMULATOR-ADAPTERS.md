# LessDB physical-AI simulator adapters

**Status:** reference adapter design, local visual twins and embedded-LessDB replay fixtures, 2026-10-11

This document defines how LessDB can support software simulators for Reachy Mini, Reachy 2 and Hugging Face/Pollen Robotics Microduck without becoming a robot controller. The first implementation is an offline, simulator-neutral adapter. It accepts bounded simulator events and emits a versioned LessDB physical envelope containing an observation, situation, provenance and an optional **draft-only** action proposal.

The current local visual harness connects official MuJoCo model assets to a browser renderer and, when explicitly configured, to a private LessVille replay endpoint. No physical robot, motor bus, external actuator or public mutation route is connected. The official MicroDuck daemon/policy stack and Reachy Mini daemon remain separate experiments.

## Why LessDB belongs in the loop

A simulator already knows how to advance physics and expose device state. LessDB adds the durable agent layer that simulators generally do not provide by themselves:

- episode, task and agent identity;
- time-valid memory and commitments;
- source observation IDs and evidence hashes;
- bounded, scoped context for a planner or verifier;
- explanation for why an observation was included or omitted;
- proposal, approval, idempotency and reconciliation records;
- replay across duplicates, delays, policy changes and stale observations.

The loop should be:

```text
simulator / robot daemon
        │
        │ bounded event: state, sensor, sequence, time, frame, evidence ref
        ▼
LessDB physical adapter
        │
        ├── observation + provenance
        ├── situation-aware recall
        └── draft action proposal
                    │
                    ▼
        application policy + safety gate
                    │
                    ▼
        simulator/controller executor
                    │
                    ▼
        actual result + telemetry
                    │
                    └── reconciliation back to LessDB
```

LessDB must not be described as the MuJoCo physics engine, Gazebo, a motor controller, a collision-avoidance loop, an emergency stop or a functional-safety system.

## Simulator matrix

| Target | Official simulation path | Useful LessDB demonstration | Current status |
|---|---|---|---|
| Reachy Mini | MuJoCo; `reachy-mini-daemon --sim`; the simulated daemon accepts the Python SDK on localhost [1] | A companion agent recalls consented preferences and scene observations, proposes an expression or safe pose, then records the simulator outcome | Official-model browser visual twin plus prior local SDK smoke test; no public control |
| Reachy 2 | Pollen’s Docker image can launch Gazebo or MuJoCo; the SDK server is exposed from the container [2] | A manipulation verifier recalls time-valid tracked-body observations from a kitchen/table scene and proposes inspection or grasp planning | Offline adapter profile and fixtures; no Docker simulator rollout |
| Microduck | `scripts/duck-sim` runs the real daemons against a MuJoCo body; the policy stack is trained with MuJoCo/PPO and exported for the robot [3] | Compare policy episode memory, sensor evidence and actual outcome across walk, stand-up, grab or recovery episodes | Four-body official-model browser twin with private embedded-LessDB replay; daemon/policy rollout remains separate |

Reachy 2’s MuJoCo asset repository documents named scenes such as fruits, kitchen and table scenes, and supports tracked object bodies through scene metadata [4]. Those scenes are especially useful for LessDB because the object identity, coordinate frame and evidence reference can be stored without placing raw images or control-loop state in the database.

Microduck’s public simulator Space is available for interactive exploration [5]. The adapter does not scrape or control that Space. A local bridge should consume events from the simulator or daemon process and write only the bounded envelope.

## The adapter envelope

The executable bridge is:

```bash
python3 tools/physical_ai_adapter.py \\
  --profile reachy-mini \\
  --example
```

To transform JSONL simulator events:

```bash
cat simulator-events.jsonl | \\
  python3 tools/physical_ai_adapter.py --profile microduck \\
  > lessdb-envelopes.jsonl
```

Supported profiles are `reachy-mini`, `reachy-2` and `microduck`. An input event contains an episode ID, simulator timestamp, device sequence, sensor identity, event kind and optional pose, uncertainty, evidence hash, payload reference, calibration reference and proposed action.

The output schema is `lessdb-physical-envelope/v1` and includes:

- a stable observation ID derived from profile, episode, sequence, sensor and event kind;
- a situation/context ID;
- frame-aware millimetre pose and bounded uncertainty;
- an evidence hash and reference rather than unrestricted media;
- simulator and control-interface metadata;
- a draft proposal with an idempotency key and freshness limit when an action is requested;
- stop conditions for stale observation, human stop and unknown interlock;
- no approved, executing or completed state from the adapter.

For a private local LessVille service, envelopes can be persisted through
`POST /api/physical/replay`, and executor outcomes through
`POST /api/physical/reconcile`. Both routes return `403` when the service runs
in public observer mode. They are asynchronous application integration points,
not simulator control loops.

The first real Reachy Mini smoke test can be reproduced with:

```bash
reachy-mini-daemon --sim --headless
python3 tools/run_reachy_mini_experiment.py \
  --out /tmp/reachy-mini-demo.jsonl \
  --episode-id episode:reachy-mini-demo
```

The reusable visual twin is generated with:

```bash
python3 tools/render_physical_twin.py \
  /tmp/reachy-mini-demo.jsonl \
  /tmp/reachy-mini-twin.html
```

It presents the episode timeline, sampled poses, evidence hashes, draft
proposals, freshness limits and the explicit boundary between LessDB
observability and simulator-owned physics.

For a browser-visible 3D model rather than a text timeline, use the official
MuJoCo assets with the local visual servers:

```bash
export MUJOCO_GL=egl
python3 tools/serve_physical_twin.py \
  --model-root <reachy-mini-site-packaged-mjcf> \
  --port 8795

python3 tools/serve_microduck_swarm.py \
  --robot-root <microduck_rl>/src/mjlab_microduck/robot/microduck \
  --ducks 4 \
  --port 8796 \
  --lessville-url http://127.0.0.1:8792
```

The first server renders the official Reachy Mini body. The second composes
four official MicroDuck bodies into one visible MuJoCo scene and samples each
named duck into the private replay endpoint. The browser panel is an
observability surface; it is not a replacement for `reachy-mini-daemon --sim`
or `scripts/duck-sim`.

The LessDB `less-agent` incubation contract now carries the corresponding evidence hash, maximum observation age and `ActuationReconciliation` types. The application still owns validation, approval, executor calls and reconciliation.

## Reachy Mini example

The official Reachy Mini guide says that the simulation runs on MuJoCo, starts with `reachy-mini-daemon --sim`, and behaves like a Reachy Mini Lite connected through the local SDK. It includes an empty scene and a minimal scene with objects such as an apple, croissant and duck [1].

A useful LessDB scenario is **RM-01 — consented companion expression**:

1. The simulator emits a scene observation and a user-presence event.
2. The adapter stores the observation with episode, sequence, frame and evidence references.
3. The agent recalls only the user’s consented shared preference and current situation.
4. The agent proposes `look_at` or `express`; it does not execute it.
5. The application checks freshness, consent and safety policy.
6. The SDK application sends the command to the simulator.
7. The adapter writes `ActuationReconciliation::Succeeded` or a failure reason.
8. A later session can explain which observation and preference influenced the proposal.

This shows why an agent memory is more useful than a prompt transcript: the next interaction can retrieve a bounded, time-valid experience with provenance instead of replaying an entire conversation.

## Reachy 2 example

Pollen’s Reachy 2 simulation documentation uses a ready-to-run Docker image and supports Gazebo and MuJoCo. It exposes a SDK server and browser-accessible displays. MuJoCo scenes can be selected from the Reachy 2 asset repository [2]. The asset repository documents fruit, kitchen and table scenes and tracked-body metadata [4].

A useful LessDB scenario is **R2-01 — kitchen inspection handoff**:

1. A scene adapter records the positions of a tracked object and the robot’s frame.
2. A verifier agent recalls only observations for one room, task and time window.
3. It omits stale or out-of-frame evidence and records omission reasons.
4. A planner drafts `inspect` or `grasp_proposal` with source observation IDs.
5. A separate policy service checks the scene revision, freshness and approval requirement.
6. The simulator executes only after the application approves the exact proposal.
7. The result, failure or cancellation is reconciled with the same idempotency key.

The important demonstration is not “LessDB controls Reachy.” It is “LessDB lets specialist agents share the right evidence, with a replayable chain from scene observation to proposed manipulation and observed result.”

## Microduck example

The Microduck repository describes a Rust daemon stack, a 50 Hz control loop, fifteen servos, camera and device daemons, and a separate MuJoCo/PPO policy-training repository. Its `scripts/duck-sim` path runs the robot daemons against a simulated body [3]. The product page also describes camera, LiDAR, two IMUs, trained moves and a sim-to-real workflow [6].

A useful LessDB scenario is **MD-01 — policy episode reconciliation**:

1. The simulated daemons emit policy version, sensor sequence, pose and action-result events.
2. LessDB stores episode summaries and bounded evidence references, not the 50 Hz servo loop.
3. An analyst agent recalls the last failed `stand_up` attempt and the environmental conditions around it.
4. A policy agent proposes a retry or a safe-pose transition with a freshness limit.
5. The application validates the simulator/controller state and idempotency key.
6. The simulator runs the policy; LessDB records actual outcome and telemetry summary.
7. A replay compares the same episode under duplicate events, missing sequences and policy-version drift.

This makes LessDB useful to robot learning without pretending to replace the RL policy, MuJoCo, ONNX runtime, daemon IPC or servo controller.

The first browser-visible four-body experiment and its LessDB lineage findings are recorded in [MicroDuck swarm + LessDB visual experiment](MICRODUCK-SWARM-LESSDB-EXPERIMENT.md). It uses the official model assets, persists sampled observations and closes each draft proposal with a local reconciliation result.

## Scenarios to add to LessVille and the conformance kit

The current SC-01–SC-06 and PA-01–PA-06 suite should gain simulator-specific cases:

- **RM-01:** Reachy Mini consented expression with private/shared memory separation.
- **RM-02:** Reachy Mini scene object recall with time and spatial freshness.
- **R2-01:** Reachy 2 tracked-body inspection with kitchen/table scene provenance.
- **R2-02:** Reachy 2 manipulation proposal invalidated by scene revision.
- **MD-01:** Microduck policy episode reconciliation across walk/stand-up outcomes.
- **MD-02:** Microduck duplicate and missing device sequence recovery.
- **SIM-01:** common adapter replay across all three profiles with identical omission and proposal rules.

Every case must agree across simulator state, LessDB rows, ContextPacket, rendered/analyst output and replay. A model completion is never the oracle. A simulator success is not evidence that a real robot is safe.

## What LessDB should add next

The first contract additions are now present: evidence hashes, observation freshness bounds and typed reconciliation state. The next native or broker-level work should be evidence-driven:

1. Add a versioned observation-envelope schema with explicit device, episode, policy and simulator IDs.
2. Add append-oriented ingest acknowledgements with duplicate, gap and conflict outcomes.
3. Add context-packet fingerprints so a proposal records exactly which bounded packet it used.
4. Add native or broker-backed temporal/spatial candidate filtering before ranking, with clear ownership of the policy predicate.
5. Add a replay reader that can reconstruct observation, recall, proposal and reconciliation order without raw media.
6. Add simulator adapters as examples, not hard dependencies of the LessDB engine.

Until those semantics are implemented and tested, documentation should continue to call the current path an application/broker incubation layer rather than native tenant isolation, native robot governance or a distributed physical-world database.

## Unified Embodied Lab view

The local hub at `tools/serve_embodied_lab.py` is the simulator analogue of the LessVille observer experience. It presents four selectable scenes without merging their control loops:

| Scene | What is visible | Truth label |
|---|---|---|
| Reachy Mini | Official Reachy Mini MuJoCo model and sampled situation panel | Visual twin; the current motion cycle is not the live Reachy daemon |
| MicroDuck swarm | Four official MicroDuck bodies, proximity context and private replay counts | Swarm visual twin; bounded application replay, not a real multi-robot controller |
| MicroDuck robotd | Official body-server camera from the live `robotd --sim` path | Live daemon/simulator camera; not hardware |
| MicroDuck operator | Official model with joint state mirrored from the live daemon body state | Operator view; not a second controller |

Run it locally after the four existing visual services and private LessDB bridge are available:

```bash
python3 tools/serve_embodied_lab.py --port 8799
```

The hub is observer-only. It proxies images and read-only state, and it does not expose `POST /api/physical/replay`, daemon sockets or actuator commands. This is the first coherent LessVille-like surface for the physical experiments; the underlying simulator, daemon and LessDB services remain independently testable.

## Unified physical world staging

The current shared-world orchestrator is `tools/serve_unified_embodied_world.py`. It places one Reachy Mini station and four MicroDuck entities into the common `lessville:embodied-district/v1` frame, derives a shared logical tick, renders the official source views, and displays LessDB frame/scope telemetry.

```bash
npm run sim:world
```

The staging server exposes only:

- `GET /api/unified` — entities, shared frame, logical tick, source health, scopes and telemetry;
- `GET /api/telemetry` — the LessDB analytics projection;
- read-only source image proxies.

The private LessVille bridge now exposes `GET /api/physical/analytics`. It reads a bounded newest-episode window from `lv_physical_observations`, extracts frame IDs and explicit or inferred scope/grant metadata from the persisted envelopes, and joins persisted proposal/reconciliation counts. It deliberately does not claim native spatial indexes, native row ACL, controller safety or cross-system atomicity.

The public review staging URL is temporary and observer-only. It is not the production `ville.lessdb.dev` runtime, and it does not expose replay, reconciliation, daemon sockets or actuator controls.

## References

[1]: https://huggingface.co/docs/reachy_mini/en/platforms/simulation/get_started "Reachy Mini Simulation - Setup Guide"

[2]: https://docs.pollen-robotics.com/developing-with-reachy-2/simulation/simulation-installation/ "Reachy 2 Simulation Installation"

[3]: https://github.com/pollen-robotics/microduck "Pollen Robotics Microduck repository"

[4]: https://github.com/pollen-robotics/reachy2_mujoco_assets "Reachy 2 MuJoCo Assets repository"

[5]: https://huggingface.co/spaces/pollen-robotics/microduck-simulator "Microduck Simulator Space"

[6]: https://pollen-robotics.com/microduck/ "Microduck product and simulator overview"


## Daemon-backed Reachy and unified staging update — 2026-10-12

The earlier Reachy page was an official-model visual twin with an internal animation loop. That was useful for rendering, but it was not enough to call it a daemon-backed simulator view. The new `tools/serve_reachy_daemon_visual.py` corrects that boundary:

- `reachy-mini-daemon --sim --headless` is started through the official SDK path;
- a bounded local demo driver sends semantic head targets through the SDK;
- the visual renderer reads the daemon's current head-pose matrix and mirrors its bounded roll/pitch/yaw state onto the official MuJoCo model;
- the page exposes daemon connection, readback sequence, evidence and LessDB sidecar counts;
- no public actuator or daemon socket is exposed.

The unified staging command can now point `--reachy-url` at this daemon-backed source while keeping the official four-body MicroDuck visual, the real MicroDuck `robotd --sim` camera and the real daemon-driven operator view as separate sources. This is a shared-coordinate orchestration view, not a merged physics engine.

**Current truth:** Reachy daemon-backed rendering and MicroDuck robotd camera/operator paths are verified locally/staging. The four-body MicroDuck swarm page is an official-model multi-body visual with sampled LessDB scope/proposal/reconciliation telemetry; it is not four independent robotd control loops. None of these experimental mutation paths is deployed into public `ville.lessdb.dev`.

Run the daemon-backed Reachy view with the SDK's actual MJCF directory:

```sh
export MUJOCO_GL=egl
python3 tools/serve_reachy_daemon_visual.py \
  --model-root "$REACHY_MJCF_ROOT" \
  --lessdb-url http://127.0.0.1:8792 \
  --port 8804
```
