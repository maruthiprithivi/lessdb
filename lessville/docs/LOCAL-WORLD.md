# Local Lessville world

Run `tools/run_local.sh` from this checkout. It starts the native LessDB service on
127.0.0.1:8790, waits for health, then opens Godot. It first imports Godot resources,
so a clean checkout does not need an existing `.godot` cache. Closing Godot saves and flushes
the world before the service exits. State is in `.local/world`; versioned world checkpoints retain the latest 64 revisions.
`LESSVILLE_DATA` can also select the existing recorded world `.local/day-run-world`.
Set `LESSVILLE_PORT` to use another local port. Runtime files stay ignored.

## Prerequisites

- Python 3 with Pillow for the pixel-art checks. The current Godot suite invokes
  `/usr/bin/python3`, so Pillow must be importable by that interpreter. On Linux,
  install Xvfb for rendered fixtures; macOS uses the native renderer.
- Rust toolchain `1.96` (`rustup toolchain install 1.96`).
- Godot 4.7.2 at `.local/Godot.app/Contents/MacOS/Godot`, or set `GODOT_BIN`.
- The nine-crate embedded engine is included in `vendor/lessdb`. Cargo dependencies
  resolve there; a private sibling checkout is no longer a build prerequisite.
  [Engine provenance](../vendor/lessdb/README.md) identifies upstream commit
  `0367d81d935a452f28cb2a15a62ecce2463698b1`, per-file hashes and the preserved
  upstream worktree patch. Registry dependencies remain pinned by `Cargo.lock`.

The launcher uses seeded weather, deterministic rules and local persistence.
It disables remote model calls and cloud checkpoints. It does not require or
copy provider credentials, and does not modify the public deployment.

`npm run dev` invokes this launcher. `npm run check` runs the Godot checks directly;
`npm run build` and `npm run export` use `tools/export_web.sh` and require Godot
4.7.2 with matching Web templates. These commands do not need the external Manus
runtime. Native backend checks remain the separate Cargo command below.

## What runs

There are exactly 100 persistent residents: Maya, Sam, Lin, Ama and Bo plus 95
supporting residents. Main residents use the existing scoped memories, decisions,
relationships and evidence-backed learning. Supporting residents have stable
identities, staggered shifts, travel, packed meals, needs and a twelve-entry daily
experience history. The town centre has 5 focal and 12 supporting neighbours; 83 supporting residents
live in abstract surrounding districts. Local outdoor figures use eight detailed
appearances with front/back walking frames at the main adults’ height. See
[WORLD-SCALE.md](WORLD-SCALE.md) for the scale and measured density policy.

Mochi is joined by two cats (Miso and Pepper), Clover the garden rabbit and two
sparrows. The additions have bounded daytime walking/resting/perching/flight
routines and share the world clock. They are ambient presentation characters,
without persistent feeding, ownership or autonomous animal needs.

One backend tick is one simulated minute. Night now advances continuously instead
of skipping directly to morning. Main residents seek food when hungry, travel to
it, reserve a portion and a free cafe table, and eat for fifteen minutes before
nutrition applies. Interrupted or unavailable meals do not grant nutrition. Sleep
reduces fatigue. Five-minute conversations require both people to remain together.
Coffee orders queue, reserve ingredients, take three minutes to prepare, and check
customer/staff availability before handoff. Work and study record situated practice.
Pending activities and needs survive persisted-world reloads.

These are explicit scenario rules, not calibrated human physiology or open-ended
human intelligence. Food comes from an explicit daily external supplier; there is
no household finance or full supply-chain economy. Supporting residents do not yet
have the same depth of relationships or scoped memory retrieval as the main cast.
Main routines still include scripted schedules. West-zebra crossing separation is
checked; head-on meetings on other narrow paths, shared stationary destinations
and some existing office artwork checks remain unresolved. Bus boarding is still
scheduled by the backend independently of rendered traffic clearance.

Mid-stride destination changes now finish the current route through its next graph
junction, preserving bends, before adopting the new destination. Around-corner
following excludes strongly opposing headings to prevent reciprocal U-turn waits.
The focused suite and four seeded dense runs pass their movement/follower/crossing
gates; this does not establish collision-free shared destinations or head-on passing.

## Record exactly 24 simulated hours

With the local service running, use a fresh output directory:

```sh
LESSVILLE_API=http://127.0.0.1:8790 \
LESSVILLE_RECORD_OUT=/absolute/path/to/day-recording \
.local/Godot.app/Contents/MacOS/Godot --path . --rendering-driver opengl3 \
  -s res://tools/record_day.gd
```

The recorder starts `run_day`, displays the actual running client, rotates the
selected main resident, captures native frames and saves timestamped API snapshots.
The service pauses at precisely start + 1,440 minutes. `run.json` records the start,
end, population, final state and renderer. `frames.jsonl` contains actual capture
timestamps; encode those durations rather than assuming a constant capture rate.
This is accelerated simulated time, not a 24-hour wall-clock endurance test.

The final release-candidate run is recorded under
`../outputs/lessville-live/release-day-recording`; the reviewable MP4 is
`../outputs/lessville-live/release-24-hours.mp4`, with a four-frame
`release-day-contact-sheet.png` beside it. It covers minute 399 → 1839 in an
isolated fresh world, exactly 1,440 simulated minutes, over 344.698 wall seconds.
The earlier `day-recording-reviewed` and `lessville-24-hours.mp4` predate the final
movement fix. Four transient offline indicators appeared during native capture
under load; successful snapshots continued. The logs do not establish a backend
outage or isolate the cause. Completion does not imply uninterrupted polling or
collision-free crowds.

## Checks

```sh
cd backend/agentd
cargo +1.96 test --locked -- --test-threads=2
```

Run `tools/check_street.gd` in headless Godot for vehicle clearance, repeated bus
traversal, pause stability, standing-space and animal habitat checks.
Run `--headless --script res://test/walker_clearance.gd` for directional pedestrian
crossings, reroute boundaries, shared entry admission and pause/fade stability.
It also checks junction-preserving reroutes and arrival at changed destinations.

The daily-life tests cover scarce food, time-delayed coffee, interrupted social
contact, continuous night, pending-meal restart and a deterministic day for all 95
supporting residents. The Godot suite is `test/test_suite.tscn`. Its hair-render
subtest automatically captures two fixtures through Linux Xvfb or the native macOS
renderer using `tools/render_attire.gd`. `LESSVILLE_LIN_HAIR_RENDERS` optionally
selects previously captured fixtures instead. Do not interpret these checks as a clean bill of
health for dense crowd crossings.

## Cloudflare release boundary

The release targets the existing `ville.lessdb.dev` Worker/Container and
`lessville-data` bucket under `lessville/v3/`. Do not copy a local practice world
over that namespace. [Release qualification](RELEASE-QUALIFICATION.md) describes
an isolated copied-checkpoint rehearsal and records what is still pending.
Source publication and the current local results do not establish deployment or
live-world restoration. The native rehearsal passed; the exact Linux release image
and deployed continuation still need verification. Historical Manus preview tools are not required for the
native world and may still require their original external runtime.
