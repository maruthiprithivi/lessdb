# LessDB Embodied Lab quickstart

The Embodied Lab is the physical-AI companion to LessVille. It gives one observer surface to four local simulator views while preserving the boundary between simulator control and LessDB provenance. The unified world view places Reachy Mini and the MicroDuck swarm into one shared LessVille district frame and clock.

## Current simulator views

- **Reachy Mini:** official MuJoCo model and sampled visual-twin situation.
- **MicroDuck swarm:** four official MuJoCo bodies with proximity-derived shared context and private replay counts.
- **MicroDuck robotd:** official body-server camera from the real `robotd --sim` path.
- **MicroDuck operator:** official model driven by the daemon body state readback.

## Start

Start the already validated local services first:

1. private LessVille agent service on port `8792`;
2. Reachy visual twin on `8795`;
3. MicroDuck swarm twin on `8796`;
4. MicroDuck robotd camera view on `8797`;
5. MicroDuck daemon-driven operator view on `8798`.

For the unified physical world, run:

```bash
cd <lessville-project>
npm run sim:world
```

Open the sandbox preview for port `8800`.

The earlier four-scene hub remains available with:

```bash
cd <lessville-project>
npm run sim:hub
```

Open the sandbox preview for port `8799`.

The unified world exposes read-only JSON for tooling:

- `/api/unified` — entities, shared frame, scopes, source health and telemetry;
- `/api/telemetry` — LessDB-backed frame, sequence, scope and replay analytics.

## What the hub proves

The hub makes the physical experiments understandable in the same way LessVille makes town behavior understandable:

```text
official simulator / daemon
          ↓
bounded observation + evidence
          ↓
LessDB context, scope, proposal and reconciliation
          ↓
read-only operator view
```

The hub does not claim that LessDB controls the robot, provides an emergency stop, supplies native tenant isolation, or proves sim-to-real safety. It is an observer and explanation surface.

## Current limitations

- Reachy Mini’s current visual twin uses the official model but is not connected to a live Reachy daemon in the hub.
- MicroDuck’s live path is one daemon/simulator body; the four-body swarm view is a separate visual/replay experiment.
- The upstream MicroDuck WebRTC media console was not built in the sandbox; the direct official camera stream is used instead.
- The public `ville.lessdb.dev` runtime is unchanged. These are local research tools.
- The unified district is an orchestration and visualization world, not one merged MuJoCo physics scene. Reachy and MicroDuck retain separate official physics/control loops.
