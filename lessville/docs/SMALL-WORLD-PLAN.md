# Lessville: a small world that carries consequences forward

Prepared 2026-11-16 (Asia/Singapore). Working branch: `codex/small-world-foundations`,
based on bundle `loop/base` (`23da2ed`). This is a build plan, not a claim that
the simulated behaviour has been validated against people.

The first complete world story should be: Lin gets hungry while studying,
walks to the café during opening hours, waits for a free service position,
orders from an available worker, receives and consumes an item, and remembers
the interaction. Bo remembers an explicitly shared preference. On another day
that memory changes his choice. A closed café, empty stock, occupied seat or
missed bus changes the outcome. Save/restart preserves the consequences.
The observer can follow each visible action back to its event and memory rows.

## Reuse what exists

`backend/agentd/src/sim.rs` already owns `Agent`, `World`, minute-by-minute
`Sim::advance`, role routines, travel/arrival, café orders, stock, interactions,
memories, relationships, competency evidence and evolution proposals.
`Sim::persist` writes world checkpoints to `lv_kv` and agents to `lv_agent_state`.
Relationships already carry evidence references. Evolution proposals are currently
inactive candidates; they are not evidence that characters evolve autonomously.
Extend these paths rather than introducing another simulation beside them.

`scripts/lessville/town_view.gd` is an observer with local movement smoothing.
It currently uses a different time scale from authoritative travel. Fix its
path representation first, then reconcile arrival timing with the service.
Do not make the renderer a second owner of hunger, relationships or activity.

The delivered backend Cargo manifest refers to a sibling `lessdb-wal/repo`
checkout. That dependency must be qualified before backend integration can be
claimed; the visual suite can run independently with recorded snapshots.

## Rules and measurement

All new numerical choices below are **design choices**, not empirical findings.
The companion [attribute register](REALISM-IMPLEMENTATION-REGISTER.md) preserves
all 188 delivered IDs, rules, references, checks and crop cues. Its 84 missing
checks remain missing until an executable check catches the corresponding bad
case; a proposed test name does not increase coverage. The five reported failing
attributes are crossing spacing/facing, crossing safety, alignment and separation.
Existing check mappings need auditing: a routing pass does not prove door-art
alignment, and a scene-loading pass does not prove text readability.

Before using the catalogue for research, resolve these contradictions:

* Named walk speed is described both as linear clock scaling and square-root
  scaling. Current code uses `190 * sqrt(clamp(speed, 1, 8))` map px per wall
  second; background code uses the same multiplier on 72, despite one spec row
  saying background speed is independent of it. Preserve that compatibility in
  the first slice. It is not a physical walking-speed calibration.
* Keep map metres, simulated minutes, wall seconds and art pixels separate.
  Assign lengths to nav edges in metres, choose a documented projection, then
  use authoritative travel progress for positions. Do not infer metres from
  stylised character height or assert that a person is taller than a bus shelter.
  Adopt the supplied walking-speed references only after this calibration.
* The art can exaggerate people for readability; ground contacts, clearances and
  travel duration must use one consistent ground-plane scale.
* A 10-second wait cap is a traffic-test bound in wall seconds, not a universal
  human rule. A timeout must never allow entry into an occupied crossing. Dense
  traffic may require admission control and fair queues to meet both constraints.
* The current active-day shortcut is not overnight simulation. Needs, weather,
  memories and deadlines must advance through skipped hours with equivalent
  results, or the shortcut must be removed.
* “Cited” in the supplied catalogue means a reference was supplied, not that it
  supports every pixel threshold or has been independently reverified here.
  Aprons and working schedules remain scenario choices, not universal rules.

## Model, storage, checks and review

| Area | Smallest complete model and initial design value | Persistence/query path | Automated acceptance and reviewer cue |
|---|---|---|---|
| Identity/personality | Stable agent ID; role, preferences and sociability in [0,1]. Personality stays fixed during the first 7-day scenario. | Extend `Agent` and checkpoint; use existing identity threads for provenance. | `world.identity_restart`: IDs/preferences identical after reload; observer card agrees with saved identity. |
| Needs | Hunger, fatigue, social need in [0,100]. Initial hunger +6/hour awake, meals -40 after consumption; fatigue +4/hour awake, -10/hour asleep. Clamp values. | Add typed needs with serde defaults to `Agent`; checkpoint and expose through existing state API. | `world.needs`: pause, midnight, time-step partition and reload invariance; no meal benefit from merely intending to eat. Review a before/during/after meal strip. |
| Goals/decisions | One active physical activity per agent. Choose an eligible meal at hunger >=60; sleep at fatigue >=80; otherwise retain role schedule. Needs interrupt only at safe activity boundaries. | Reuse plans, `decide`, activity fields and decision evidence; record unmet prerequisites. | `world.needs_choose_activity`: hungry vs satiated counterfactuals diverge; closed café/empty stock cause a valid alternative. Review “why this action” and the actual location. |
| Routines | Mon–Fri role obligations, weekend alternatives. Initial office 09:00–17:00 weekdays, café 06:30–20:00, grocery 08:00–22:00, university 09:00–17:00 weekdays, gym 07:00–22:00. Explicit staff breaks. | Extend existing role routines and place hours, with day-of-week derived from the same clock. | `world.week`: 7-day run visits work, meal, leisure and home states; no service outside hours. Review weekday/weekend timelines and doors. |
| Movement/time | Graph route plus monotonic arc distance; pause freezes travel. An activity requiring arrival cannot start early. | Existing `Travel` and snapshots; local `poly`/`s` only for rendering. | Existing arc/nav/facing/progress gates, then `world.arrival_agreement` at 1x/2x/8x. Review turns, doors and same-event timestamps in both views. |
| Shared space | Footprint separation; same-direction following before crossing reservations. Stable FIFO entry, cancel stale requests on reroute/disappearance. No lateral pushes. | Local traffic state first; authoritative reservation events when actions depend on access. | Existing follow/crossing/seeded multihour gates plus starvation, opposite directions and reroute tests. Review stripes and approach queues frame by frame. |
| Objects/activities | Seat capacity 1, one holder per item, nonnegative integer stock. Order states queued → accepted → prepared → handed over → consumed; cancellation releases resources. | Reuse café orders, object transitions and stock checks; IDs connect visible props to events. | `world.resource_conservation`: two buyers/one item; retry does not duplicate stock decrement; occupied seat rejects second sitter. Review cup transfer, hand/seat contacts and worker presence. |
| Social interaction | Greeting/conversation requires same reachable place and available participants; initial duration 2–5 simulated minutes. Explicit participant acceptance; no dialogue with someone asleep or in transit. | Reuse `interaction`, messages and relationship edges; event IDs deduplicate retries. | `world.social_presence`: absent/refusing participant prevents interaction; completion writes evidence exactly once. Review gaze, spacing, turn-taking and bubble ownership. |
| Memory | Remember completed observations with agent, event, time, place and scope; inferred beliefs labelled separately. Initial retrieval budget 8 relevant memories. | Existing memory/recall/context paths and LessDB scoped queries; never grant omniscient cross-agent recall. | `world.memory_effect`: a remembered preference changes next-day service; remove memory and decision differs. Isolation test proves agent B cannot retrieve A's private event. Review source → recall → decision chain. |
| Skills/development | Practice increments a bounded skill only on successful completion; initial +1 per completed task, [0,100]. Improvement changes task duration by at most 20%, never bypasses prerequisites. | Existing competency events and identity evidence; checkpoint skill values. | `world.practice`: no growth while idle or after failed work; reload equals uninterrupted run. Review day-1/day-7 task duration with evidence. |
| Relationships | Initial affinity [-100,100], +2 for completed mutually accepted positive interaction, -2 for an evidenced negative interaction; max absolute change 5/day. No change from proximity alone. | Extend existing relationship evidence, retain directional beliefs separately; query by participants/time. | `world.relationship_evidence`: duplicate event is idempotent; no evidence means no change. Review relationship history alongside shared activities. |
| Habits/evolution | First version changes one preference only after 3 consistent experiences on different days. Preserve stable identity. Existing structural evolution proposals remain separate. | Reuse lesson promotion and evolution provenance; record old/new values and evidence. | `world.habit`: one event cannot change a habit, 3-day evidence can; replay produces identical transition. Review causal day-by-day history. |
| Weather/environment | One seeded weather state drives rain, cloud cover, wind, attire, shelter choices and lighting. Use Singapore seasonal patterns from supplied references, not four temperate seasons. | Existing world weather and `world_atmosphere.gd`; persist seed/state. | `world.weather_consistency`: deterministic replay, no rain-only umbrella in clear weather, dawn/dusk continuity; review same-clock full scene and character crops. |
| Business/world evolution | Track daily visitors, stock and completed orders; next-day restock follows observed demand with a fixed cap. First version has no prices, economy or new buildings. | Reuse stock/restock/order events; aggregate by simulated day, not wall time. | `world.business_days`: inventory conservation and demand-dependent restock across 7 days. Review shop status against actual service events. |
| Observer/UI | Expose current need, activity, destination and causal evidence. Distinguish live authoritative state from visual interpolation and offline recorded examples. | Existing Mind, feed, SQL and Truth panels; stable snapshot revision. | `ui.state_truth`: all panels share revision; keyboard operation, contrast, clipping, reduced motion and resize checks. Review 1280×720 and 1600×900 with 4× text/contact crops. |

## Phased build order

1. **Movement foundation (current slice):** reproduce the arc failure; share a
   small arc-distance advancement helper across named and background walkers;
   preserve route choice, speed, fades and arrival; update motion fixtures that
   write walker state directly. Add boundary tests for pause, duplicate points,
   exact vertices, large timesteps, reroute and final arrival. Capture real-GPU
   motion frames. Gate: arc pass and no formerly passing suite case regresses.
2. **Safe traffic:** speed-only following, then crossing reservations. Measure
   true initial overlaps and stationary arrivals as well as moving pairs; do not
   “fix” overlap by hiding a walker or excluding it from the check. Seed 1/2/3
   multihour runs, destination flips, wait bound and independent crop review.
3. **Authoritative day:** qualify backend dependencies and local persistence;
   add needs, weekday schedules and arrival-gated activities to existing service.
   Run one complete café meal scenario with stock/seat/queue conservation.
   Test overnight advancement and snapshot/client agreement before new art.
4. **Consequences across days:** preference recall, evidenced relationships,
   practice and a single habit update. Seven-day seeded replay and restart test;
   same seed with memory disabled establishes a causal baseline. Show visible
   behaviour differences, not just growing tables or narrative text.
5. **Coverage and presentation:** work through the register by impact: physical
   contacts/props, scale/time, opening hours/weather, then UI. Add a failing
   fixture or mutation for each new assertion. Include seeds, duration, engine,
   code revision and reviewer verdict in every research figure's evidence.

## Scope of the first change

Reach: `_update_walkers`, `_update_background`, shared path helper, direct-state
setups in `tools/render_step2_motion.gd`, boundary tests in `test/test_suite.gd`.
The existing `test/crossing_sim.gd` stays an independent measurement oracle.
No backend schema, public API, secrets, deployment, dependency or art changes
are needed for the path representation. Render-time background nudges are an
existing mismatch between logical feet and drawn feet; address them with actual
separation, not by claiming this arc-state check proves visual collision safety.

Success for this slice does not mean the whole 188-attribute world is complete.
Follow-gap/crossing failures and unreviewed visuals must remain explicit in the
handoff. No publishing or production connection is part of this work.
