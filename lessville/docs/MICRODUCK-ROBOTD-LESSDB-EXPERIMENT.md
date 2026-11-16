# Official MicroDuck `robotd --sim` + LessDB experiment

**Date:** 2026-10-11
**Status:** local experiment verified; not a hardware test and not deployed to the public LessVille runtime.

## What ran

The official Pollen Robotics MicroDuck repositories were used rather than a fabricated event generator:

- the official `microduck` daemon checkout and `scripts/duck-sim` wrapper;
- the official `microduck_rl` MuJoCo body server and MicroDuck model;
- the real `robotd --sim` binary, policy loader and 50 Hz control loop;
- the LessVille application-owned physical replay/reconciliation endpoints backed by embedded LessDB.

The first attempt used the upstream `apartment` scene and was rejected by MuJoCo because its keyframe length did not match the current included collision model (`expected length 70`). The compatible upstream default `scene.xml` was used instead. This is recorded rather than hidden: it is a simulator-source compatibility issue, not evidence that the apartment scene works.

## Verified daemon evidence

The real wrapper reported:

- robot health: healthy;
- control loop: approximately 50 Hz;
- bus: OK;
- IMU: ready;
- policy: loaded in walk mode;
- control loop: 15 joints, 50 Hz, driving enabled;
- simulator body: MuJoCo over localhost;
- safety behavior: stale velocity intents were zeroed by the daemon deadman path after the short command expired.

A short semantic `robot.move` intent was sent through the robotd IPC socket. The official wrapper’s `scripts/duck-sim drive` command was also run for its bounded 8-second forward-walk intent; the daemon reported that the intent expired and stopped. The bridge sampled the body-server state (`sim_time`, trunk position/height, IMU and joint positions) without putting LessDB in the control loop.

## LessDB capture

The capture runner is `tools/capture_microduck_robotd.py`.

One 4-second episode at 5 samples/second produced **15 samples**. Every sample produced:

1. an observation with stable episode/observation ID, source device, sequence, timestamp, evidence hash and bounded state;
2. a proposal linked to that observation with a unique idempotency key;
3. a reconciliation linked to the proposal and executor identity.

The first sample created the episode; all subsequent samples were duplicate-safe inserts. The reconciliation path rejects an unknown proposal ID. The persisted tables are application-owned physical replay tables; they are not native robot safety tables.

Example local command:

```sh
. /path/to/reachy-mini-lab/.venv/bin/activate
python tools/capture_microduck_robotd.py \
  --robot-socket /path/to/duck-sim-state/duck-a.sock \
  --body-port 7810 \
  --lessdb-url http://127.0.0.1:8792 \
  --duration 4 --sample-hz 5 \
  --episode-id microduck-robotd-phase2
```

## Visual result

The daemon-connected preview has two views:

- **External operator view:** the official MicroDuck MuJoCo model, with its joint positions copied from the live body-server state read by the daemon path;
- **First-person view:** the official body-server UYVY camera TCP stream, converted only at the presentation edge for browser viewing.

The page also shows the sampled LessDB observation/proposal/reconciliation counts and states the control boundary. It is intentionally separate from the public LessVille deployment and does not expose a POST bridge.

## WebRTC/media limitation

Enabling `DUCK_SIM_CAMERAS=a` starts the official body camera stream successfully. The wrapper’s optional `mediad` WebRTC build was not completed because the sandbox lacks the system `gstreamer-webrtc-1.0` development package. The direct official camera TCP stream works and is sufficient for the local visual twin; this experiment must not claim that the upstream WebRTC console is verified in this environment.

## What LessDB makes possible

The useful improvement is not storing every simulator tick. It is making the agent loop inspectable:

- **episode lineage:** which simulator run produced an observation;
- **device/policy provenance:** which daemon/model/control interface emitted it;
- **bounded context:** an agent can receive recent state and evidence references without raw media or unbounded logs;
- **proposal/reconciliation:** an agent plan can be compared with what the simulator actually reported;
- **replay:** duplicate samples, delayed delivery and unknown proposal references have deterministic outcomes;
- **spatial Phase 3 foundation:** joint/body state can be anchored to a named coordinate frame with uncertainty and shared-context scope.

## Boundaries and next step

This proves a local simulator integration, not a hardware safety claim, localization accuracy claim, policy-quality claim or distributed LessDB guarantee. The next useful step is a real four-duck `duck-sim` run with distinct agent identities, explicit neighbor observation grants and a replay test showing that each verifier sees only its permitted zone/kinds/time window.
