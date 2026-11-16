# MicroDuck swarm + LessDB visual experiment

**Status:** first browser-visible multi-body experiment completed locally, 2026-10-11

## What changed

The first physical-AI twin was intentionally replaced with a real visual simulation path:

- official MicroDuck MuJoCo meshes and robot model from [`pollen-robotics/microduck_rl`](https://github.com/pollen-robotics/microduck_rl);
- a generated four-body scene using the same model assets, with four named simulated agents;
- a browser-visible renderer with the actual robot bodies moving in a shared scene;
- a side panel showing derived proximity, bounded memory labels, proposals and persistence counts;
- sampled envelopes sent to a private LessVille replay endpoint backed by embedded LessDB;
- reconciliation outcomes written after each proposal.

The renderer is a visual twin and experiment harness. It is not the MicroDuck daemon, trained policy, motor controller or a sim-to-real safety claim.

## Verified result

A clean run produced three sampled ticks across four ducks:

```text
lv_physical_runs             1
lv_physical_observations    12
lv_physical_proposals       12
lv_physical_reconciliations 12
```

The four ducks were not collapsed into one row. Each observation and proposal retained source-specific identity, even when several ducks emitted events at the same simulation sequence.

## Important LessDB design finding

The first swarm attempt exposed two correctness bugs:

1. **Proposal identity collision:** proposal IDs were derived from profile, episode, sequence and action only. Four agents at one sequence could therefore resolve to one proposal. Proposal identity now includes the source observation ID.
2. **Short-circuit data loss:** the renderer used `any(...)` over the submission generator. The first successful POST stopped iteration, so the rest of the swarm was never persisted. The bridge now submits every duck and aggregates success separately.

These are not cosmetic fixes. A swarm database must distinguish:

```text
same episode + same sequence + different source agent
```

from:

```text
same episode + same sequence + same source observation replayed
```

The first is fan-out; the second is an idempotent duplicate.

## Data flow

```text
official MicroDuck MuJoCo meshes
              │
              ▼
      visible four-body scene
              │ sampled pose / proximity
              ▼
     physical_ai_adapter.py
              │ versioned envelope
              ▼
   LessVille /api/physical/replay
              │ embedded LessDB
              ├── lv_physical_runs
              ├── lv_physical_observations
              ├── lv_physical_proposals
              └── lv_physical_reconciliations
              │
              ▼
    /api/physical/summary + visual UI
```

The simulator remains responsible for motion and physics. LessDB stores the durable agent layer around it: episode identity, source lineage, bounded evidence references, proposals, replay protection and observed outcomes.

## Reproduce locally

Install the official MuJoCo-capable Python environment and point the renderer at the checked-out MicroDuck model directory:

```bash
export MUJOCO_GL=egl
python3 tools/serve_microduck_swarm.py \
  --robot-root <microduck_rl>/src/mjlab_microduck/robot/microduck \
  --ducks 4 \
  --port 8796
```

To use a private LessVille service for embedded persistence:

```bash
python3 tools/serve_microduck_swarm.py \
  --robot-root <microduck_rl>/src/mjlab_microduck/robot/microduck \
  --ducks 4 \
  --port 8796 \
  --lessville-url http://127.0.0.1:8792
```

The private service exposes a read-only summary for the visual inspector:

```text
GET /api/physical/summary
```

Public LessVille observer mode must continue to reject replay and reconciliation mutations.

## What an agent can learn from this scene

The same visual swarm can support progressively stronger experiments:

- **Local memory:** a duck remembers its last safe pose or nearby beacon.
- **Shared context:** a duck receives only the latest bounded observation from a named neighbour or swarm scope.
- **Temporal reasoning:** an agent distinguishes a current neighbour from a stale observation.
- **Spatial reasoning:** proximity and frame identity affect whether a memory is included.
- **Proposal lineage:** a movement proposal points to the exact observation packet that informed it.
- **Reconciliation:** the executor result closes the proposal without pretending that a draft was executed.
- **Replay and failure:** duplicated, delayed, missing or conflicting device sequences can be tested deterministically.
- **Scale:** the same contract can later compare one duck, four ducks and a larger simulated population without changing the database semantics.

## Boundaries

This experiment does **not** prove:

- physical robot safety;
- sim-to-real transfer;
- correctness of MicroDuck’s trained walking policy;
- native LessDB robot governance or native row-level ACL;
- distributed transactions across simulator, database and robot daemon;
- performance at the MicroDuck 50 Hz control-loop rate.

Sampling is deliberately outside the simulator control loop. The adapter and replay API are application-level incubation layers above LessDB, not claims that the engine directly controls the robot.

## Next implementation steps

1. Add a real sequence-gap and duplicate ledger to the physical envelope broker.
2. Add context-packet fingerprints so proposals record the exact bounded recall used.
3. Add a scene revision and frame graph to make spatial recalls explicit.
4. Add a controlled failure mode: stale observation, missing neighbour or rejected proposal.
5. Add a Reachy Mini scene selector using its official `empty` and `minimal` MuJoCo scenes.
6. Add a public LessVille research district only after privacy, cost, observer-only and resource limits are revalidated.
