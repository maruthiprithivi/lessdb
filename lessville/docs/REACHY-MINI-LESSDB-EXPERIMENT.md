# Reachy Mini + LessDB experiment

**Status:** first local end-to-end smoke test completed, 2026-10-10

## What ran

A real Reachy Mini MuJoCo daemon ran in headless mode. A Python SDK client connected through the documented localhost boundary, moved the simulated head and antennas to a target, returned to a safe pose and read back the simulated head pose.

The experiment did not place LessDB in the physics or control loop.

```text
Reachy Mini MuJoCo daemon
        │ semantic SDK target
        ▼
experiment runner
        │ sampled state + evidence hash
        ▼
LessDB physical envelope
        │ local replay/reconciliation endpoint
        ▼
embedded LessDB tables
        │
        ▼
LessVille-style visual twin
```

## Observed result

The run produced:

- one episode;
- two sampled head-pose observations;
- two draft action proposals;
- two evidence hashes and payload references;
- one successful reconciliation for the actual persisted proposal;
- duplicate replay protection for observations, proposals and reconciliation;
- rejection of an outcome referencing an unknown proposal.

The private local LessVille service reported these embedded LessDB counts after replay:

```text
lv_physical_runs             1
lv_physical_observations     2
lv_physical_proposals        2
lv_physical_reconciliations  1
```

The visual twin makes these facts visible as an episode timeline with pose, uncertainty, evidence, proposal freshness, idempotency and reconciliation state.

## What LessDB adds

The simulator already owns motion and physics. LessDB adds a durable agent layer around it:

1. **Episode identity:** the run can be found again by task and episode rather than by an unstructured log file.
2. **Situation-aware context:** an agent can receive the current observation, simulator conditions and relevant prior evidence without receiving every event.
3. **Provenance:** a proposal points back to the observation that informed it and retains an evidence hash/reference.
4. **Reconciliation:** the system records what the executor actually observed after a proposal, including failure or unknown outcome.
5. **Replay:** duplicate observations do not create duplicate rows, and an outcome cannot attach to an unknown proposal.
6. **User education:** the visual twin shows the difference between simulator-owned control and LessDB-owned memory, context and evidence.

## What this does not prove

This experiment does not prove physical-robot safety, sim-to-real transfer, deterministic replay across all hardware versions, native LessDB actuation governance, native row-level permissions or cross-system transactions. The simulator daemon and SDK remain responsible for physics, motion generation, timing and safety behavior.

The current LessVille replay endpoint is explicitly **local development only**. Public observer mode rejects these mutation routes.

## Reproduce it

Install the official Reachy Mini MuJoCo extra, start the daemon and run the project runner:

```bash
pip install "reachy-mini[mujoco]"
reachy-mini-daemon --sim --headless
python3 tools/run_reachy_mini_experiment.py \
  --out /tmp/reachy-mini-demo.jsonl \
  --episode-id episode:reachy-mini-demo
```

Start a private local LessVille service, then send each JSONL envelope to:

```text
POST /api/physical/replay
```

Send the executor result to:

```text
POST /api/physical/reconcile
```

Render the visual twin:

```bash
python3 tools/render_physical_twin.py \
  /tmp/reachy-mini-demo.jsonl \
  /tmp/reachy-mini-twin.html \
  --reconciliation /tmp/reachy-mini-reconcile.json
```

## Next experiments

1. Repeat the run with the same configuration and compare pose/error distributions.
2. Add an agent task and bounded recall packet before each semantic target.
3. Record a failed or stale proposal and show why the application rejects it.
4. Add Microduck local `duck-sim` as a second adapter and compare daemon/episode semantics.
5. Add Reachy 2 tracked-body scenes for object provenance and manipulation proposals.
6. Turn the visual twin into a selectable LessVille research district with simulator cards, context inspection, replay controls and a guarantee matrix.

## References

[1]: https://huggingface.co/docs/reachy_mini/en/platforms/simulation/get_started "Reachy Mini Simulation - Setup Guide"

[2]: https://huggingface.co/docs/reachy_mini/en/SDK/python-sdk "Reachy Mini Python SDK Reference"

[3]: https://docs.pollen-robotics.com/developing-with-reachy-2/simulation/simulation-installation/ "Reachy 2 Simulation Installation"

[4]: https://github.com/pollen-robotics/microduck/blob/main/docs/robot/simulation.md "Microduck Local Simulation"
