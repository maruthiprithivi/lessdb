# Unified LessVille physical world

**Status:** local implementation and public staging review, 2026-10-11

## What is unified

Reachy Mini and the MicroDuck swarm now share an observer-side LessVille district model:

- common frame: `lessville:embodied-district/v1`;
- one logical world tick;
- one entity map containing Reachy Mini and four MicroDucks;
- official source renders kept visible below the shared map;
- explicit local, swarm and district context-scope labels;
- live LessDB telemetry for frame IDs, sequence window, scope metadata and persisted lineage counts.

## What remains separate

The official Reachy and MicroDuck simulators still own their own physics, policy timing, daemon IPC, collision handling and safety. The unified page is therefore a **shared-coordinate orchestration view**, not one merged physics engine.

LessDB remains outside the real-time control loops. It stores bounded observations, evidence references, context metadata, proposals and reconciliations. It is not an emergency stop, actuator controller, native row-ACL engine or sim-to-real safety certification.

## Live staging evidence

The staging hub was verified with:

- Reachy source render: HTTP 200;
- MicroDuck swarm source render: HTTP 200;
- official MicroDuck camera source: HTTP 200;
- daemon-driven operator source: HTTP 200;
- `/api/unified`: HTTP 200;
- `/api/telemetry`: HTTP 200;
- LessDB analytics: 500-row bounded window, frame and scope aggregates, and persisted proposal/reconciliation totals.

The staging server is temporary and observer-only. It does not expose the replay, reconciliation, daemon or actuator paths.

## Next improvements

1. Add explicit frame-transform rows and a visible transform validity/uncertainty panel.
2. Add a coordinator-owned shared-context grant table rather than deriving all district labels in the observer layer.
3. Add replay controls for duplicate, delayed and stale observations without connecting them to production.
4. Add an actual Reachy daemon-backed episode to the shared world while keeping the same proposal-first boundary.
5. Only consider a merged physics scene after the source simulators and coordinate transforms have independent validation.
