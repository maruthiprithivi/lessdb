# Town client component contract

## Overview
Godot presents authoritative service snapshots in the 1536×1024 town and 768×512
interior coordinate spaces. Interior views are intentional close-ups.

## Interface
- [api_client.gd](api_client.gd): `start_polling`, `call_api`, `control`, `nudge`;
  signals `state_received`, `connection_changed`, `reply`. Poll interval 0.4 seconds.
- [town_view.gd](town_view.gd): `set_state`, `visual_pose`, `agent_clicked`,
  `place_clicked`; shared arc follower, west crossing lanes, entrance admission
  and depth-sorted adult/dog drawing. [town_nav.gd](town_nav.gd) owns walkable polygons.
- [interior_view.gd](interior_view.gd): `open`, `present`, `station_spot`; only
  present focal agents appear, with catalogue-backed seat placement.
- [road_traffic.gd](road_traffic.gd): shared, paused-safe per-lane visual queues for
  bus/traffic, body-length clearance and offscreen turns. Queue delays do not alter
  backend timetable/passenger state.
- [town_animals.gd](town_animals.gd): five bounded ambient animals (two cats, a
  rabbit and two sparrows), visible from 07:00 until 19:00 on the simulation clock;
  no autonomous needs or persistent animal-agent state. Mochi is a separate
  companion drawable in `town_view.gd`, shown when `presence.companion == "mochi"`.
- [world_atmosphere.gd](world_atmosphere.gd): snapshot-driven weather presentation
  and clock-driven daylight, with seeded fallback for absent weather fields.
- [town_art.gd](town_art.gd): cached transparent atlas frames, stable resident appearance,
  front/back distance-based walking frames, aspect-preserving traffic artwork.
- [world_scale.gd](world_scale.gd): actor, supporting adult, dog, vehicle, signal
  and prop proportions. [Scale contract](../../docs/WORLD-SCALE.md).
- [seat_catalog.gd](seat_catalog.gd): loads [seats.json](../../config/seats.json),
  ordered seat queries, hip/feet conversion and masks. Python counterpart:
  [seatlib.py](../../tools/seatlib.py).
- [main.gd](../main.gd): selection, panels, control requests, navigation and observer UI.

## Invariants
- Positions are presentation interpolation; the server owns travel and actions.
- Explicit `clock.raining` wins for precipitation, including `false`. If absent,
  `clock.weather` supplies rain/storm state; only when both fields are absent does
  seeded rain apply. Kind uses `clock.weather`, or rain/clear from an explicit
  rain flag, then the seeded kind. Temperature and wind use supplied values with
  per-field seeded fallback. The renderer does not expire live observations
  independently or combine authoritative dry weather with seeded rain; the
  [service weather contract](../../backend/agentd/src/EGM.md) owns freshness.
- People share an art-space adult height and foot radius regardless of focal/supporting
  tier, with at most one pixel of sprite gutter allowance. Main/supporting residents
  and the dog share depth sorting. The backend’s district/visibility field controls
  local presence. Occupied entry points queue presentation admission, including
  partially faded occupants; this does not change identities or backend activity.
  The renderer does not arbitrarily drop people to meet a crowd cap.
- Eight supporting appearances are reused across 12 local identities; this is not
  95 unique bespoke character designs. Regional street and supporting interior
  scenes are not yet rendered. Source/generated art provenance is in
  [TOWN-ART.md](../../docs/TOWN-ART.md).
- Preserve feet anchors, seat hip row 78, furniture overlays and texture fallback.
  Existing main actor scales remain town 0.62/0.677 and interior 1.2.
- Nullable `presence.companion` is compared directly, never converted with `String(null)`.
- World geometry scales with the camera; UI text/badges remain readable screen-space UI.
- West crossing traffic uses separate x=193/237 lanes with 44 px centre separation.
  A destination change while between y=662 and 960 finishes the current lane before
  returning. The approach junction is (215,640), with a paved corner polygon.
  Pausing freezes walker movement, fading and door dwell timers.
- Other mid-segment destination changes retain the current route through its next
  graph junction, including intermediate bends, before routing to the new goal.
  Around-corner follower lookahead excludes strongly opposing headings (dot product
  at most -0.7), avoiding reciprocal U-turn waits. It still follows compatible
  traffic around ordinary corners; it does not provide general head-on right-of-way.
- Known gap: head-on meetings on other narrow sidewalks and shared stationary
  destinations can still overlap. Seat overflow logic may offset seated feet;
  do not treat all physical placement as validated.

## Side Effects
Reads textures and cached configuration, polls HTTP, emits UI signals, interpolates
local poses and draws. Control buttons explicitly request backend mutations.

## Verification
Run `test/test_suite.tscn` in native Godot/headless logic mode as appropriate.
Run `tools/render_scale.gd` with `LESSVILLE_SCALE_OUT=/absolute/output.png` using
OpenGL, not the dummy headless renderer. Check the rendered atlas and live town.
The scale atlas checks ratios but does not prove crowd collision correctness.
Run `--headless --script res://test/walker_clearance.gd` for lane containment,
reroutes at exact boundaries, admission/progress, initial bus-stop poses and pause fades.
The same suite checks junction retention, bent approaches, focal/supporting goal
changes, eventual arrival at the new goal and reciprocal U-turn progress. Current
focused and dense-seed outcomes are recorded in the learning log; shared stationary
destinations remain outside the no-overlap claim.

Run `--headless --script res://test/weather_consistency.gd` for
[weather precedence regressions](../../test/weather_consistency.gd): authoritative
clear/fog/wet-after-rain suppress seeded precipitation, rain/storm legacy snapshots
remain supported, and supplied temperature/wind survive unchanged. The test is
also included in [tools/verify.sh](../../tools/verify.sh); deployment qualification
is tracked separately in [release qualification](../../docs/RELEASE-QUALIFICATION.md).
