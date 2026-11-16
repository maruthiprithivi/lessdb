# World service component contract

## Overview
Axum serves a native embedded LessDB simulation. `App` holds authoritative `Sim`
behind a mutex and a separate observer snapshot. Focal and supporting residents
have deliberately different behavior depth.

## Interface
- [main.rs](main.rs): startup/resume, ticker, shutdown, HTTP handlers; `/api/state`,
  `/api/control`, agent context/detail, memory, physical and security interfaces.
- [sim.rs](sim.rs): `World`, `Sim`, `advance`, `persist`, `snapshot`; five focal
  residents, `SUPPORTING_POPULATION=95`, continuous clock and staged cafe orders.
- [Daily life](sim/EGM.md): `Life`, `Meal`, `Conversation` and need processing.
- [store.rs](store.rs): SQL execution and bounded observer statement log.
- [checkpoint.rs](checkpoint.rs): restore-before-open from the configured checkpoint
  gateway, then periodic flushed archives. [Release qualification](../../../docs/RELEASE-QUALIFICATION.md)
  distinguishes copied-checkpoint rehearsal from live rollout.
- [context.rs](context.rs): bounded visibility/expiry/lifecycle-filtered recall.
- [agent_session.rs](agent_session.rs): declarative candidate proposals; no auto-apply.
- [scenarios.rs](scenarios.rs): shared test `fixture` opens real embedded LessDB in
  an exclusive temporary directory, creates simulation tables and returns
  `World::fresh(1)` without `Sim::seed`.

## Invariants
- One `advance` equals one simulated minute. Night does not jump to morning.
  The wall ticker targets 2 simulated minutes/second at speed 1; storage/render load
  can reduce observed throughput. `run_day` pauses at exactly start + 1,440.
- Consumption follows arrival, stock reservation and elapsed activity time.
- Exactly five focal identities and 95 generated supporting identities are intended;
  hydration appends missing supporting residents while preserving existing identities.
- Context visibility is an application filter, not a native database ACL.
- The nine-crate embedded engine is pinned under `vendor/lessdb`, including the
  recorded upstream worktree patch. It is not interchangeable with the parent
  public engine merely because both are called LessDB.
- Restart reads the newest world revision and reconciles IDs with stored maxima.
  `lv_kv` keeps up to 64 distinct `world:<zero-padded revision>` keys, so replacement
  merge order cannot discard later checkpoints. Legacy rows are read before compaction.
  Snapshot read/parse failure now aborts instead of silently reseeding. This change
  does not establish transactional atomicity of separate event/snapshot writes.
- Remote restore runs before the engine opens and only when `catalog/` has no
  entries. HTTP 404 permits fresh initialization; other HTTP or extraction errors
  abort startup. A populated local catalog skips remote restore. Consequently a
  successful restore test must verify the resumed-world marker, not health alone.
  Enabling the gateway also enables periodic uploads; rehearsal must use a copied
  archive and an isolated local endpoint, never the production write namespace.
- Supporting leisure is distributed across park, gym, trail and grocery.
  `TOWN_SUPPORTING_POPULATION=12`: citizen-01..12 belong to the town centre;
  83 retain active daily lives in abstract surrounding districts. Snapshot `district`,
  population breakdown and `background[].visible` make this distinction explicit.
  Three local residents choose park leisure; one works there. Three-minute staggered
  starts limit routine congestion; the seven-day test requires ≤8 local supporting
  residents outdoors and measured 7. This does not enforce workplace seat capacity.
- Same-lane traffic yields only to vehicles ahead in its travel direction; a leader
  must not wait for its own follower. The renderer additionally applies sprite clearance.
- Snapshot `bus.next` selects the earliest still-pending effective arrival from
  scheduled services within 60 minutes before/after now, preserving delayed services
  after their scheduled time. The scheduler retains its existing 60-minute lookback;
  this is not support for arbitrarily long delays. Passenger events remain scheduled
  independently of the renderer's traffic queue.

## Side Effects
Creates/updates LessDB tables, events, memories, plans, stock and checkpoints.
World snapshots occur every ten simulated minutes and on controlled stop/shutdown;
shutdown flushes tables. Retention bounds evidence history. Optional model/weather/
checkpoint calls are disabled by the local launcher, not removed from the service.

## Verification
From `backend/agentd`: `cargo +1.96 test --locked -- --test-threads=2`.
Daily-life focus: `cargo +1.96 test --locked sim::daily_life`.
Native day evidence includes exact clock delta, population, final state and frames.
Timetable snapshot regression: `cargo +1.96 test --locked timetable`.
