# Learning and evidence log

This is the durable implementation record for the current local world. Agent
memories live in LessDB; this file records what developers learned and changed.
Supplied document instructions are context, not authorization to publish or deploy.

## 2026-11-16 / 17 — 100 residents and continuous daily life

**Request.** Keep five deep main characters and a total population of 100, build a
working local simulation, and show a 24-hour run.

**Changed.** Connected the native LessDB backend and Godot locally. Added persistent
needs, timed food consumption, resource/table reservation, staged coffee handoff,
co-present social contact, practice evidence and continuous overnight time. Added
95 stable supporting identities with staggered shifts, packed meals, travel and
bounded daily histories. Added a launcher, exact-day stop and native-frame recorder.

**Evidence.** The full backend suite passed 48 tests, failed zero and skipped one
(`outputs/lessville-live/backend-final-tests.log`, relative to the parent workspace).
The subsequent daily-life suite passed all seven after the leisure-distribution
change (`scale-life-tests.log`). A pre-scale native capture completed exactly
1,440 minutes, 833 frames, 183.647 wall seconds (`day-recording-complete/run.json`).
The scaled final capture and checked outcome summary are recorded beside the final MP4.

**What the run taught us.** A successful parser/build did not catch JSON null passed
to Godot's String constructor; the actual client revealed missing character/panel
draws. Direct nullable companion comparisons fixed town, interior and Mind callers.
macOS background rendering could stall `frame_post_draw`; forcing the native draw
before capture completed the next full run. A wind flag could form invalid polygons;
clamping cloth extent and interpolating the stripe inside it removed those errors.

**Persistence.** The service emitted `World saved and flushed on shutdown` before
restart. The first actual restart resumed an older world snapshot after replacement-key
compaction. This was a failed persistence check, despite a successful flush log.
The fix retains up to 64 distinct zero-padded revision keys and reads legacy state
before compaction. A regression checks flush/merge, same-minute pause changes and
retention. Actual restart verification is recorded in the final run report. Fresh-context source review found
that snapshot read/parse failures could previously reseed; startup now aborts for
those failures. This is not an atomic event/snapshot transaction guarantee, and
corrupt-store fault injection has not been performed.

**Limits.** Rules remain scripted/bounded. Supporting histories are intentionally
shallower than focal-agent memory. External food supply is explicit, without money
or a production chain. The Rust dependency has an upstream future-compatibility
warning. No remote model provider or publishing credentials were used or changed.

## 2026-11-17 — Scale alignment and EGM bootstrap

**Observation.** Supporting adults were approximately one-fifth the height of the
main cast. Cars were 22 px long beside a 315 px bus. Size represented implementation
tier rather than the object's role in the world.

**Decision.** Use existing focal adult art and seat anchors as the reference. Match
supporting visible height to the measured 104-pixel opaque main sprite height after
town scaling. Centralize live rendering ratios, size traffic and props by category,
keep dog proportions identical between cameras, and depth-sort all people together.
See [WORLD-SCALE.md](../WORLD-SCALE.md) for values, scope and open calibration work.

**Additional finding.** Full-sized vehicles revealed that the old traffic drawing
used sidewalk nodes. Roadway projection and direction-separated lanes now replace
that route, with a regression check that traffic points stay in the road band.

**Evidence.** Native `scale-atlas.png` uses actual rendering methods, with assertions
for adult size, vehicle ordering and cross-view dog ratios. The baseline Godot run
using prior native hair fixtures reported 2 failures out of 385 checks plus 4 known
art expected failures (`godot-verified-tests-console.log`): crossing overlap and one
initial follower overlap. The footprint-expanded run also reported 2 failures out of 385 checks plus the
4 known expected failures (`godot-scale-tests-console.log`). The final roadway run must be read separately;
never reuse these counts as a clean post-scale result.

**Fresh-context review.** Two EGM read-only scouts independently examined the client
and backend. Client findings: implicit 2.62× interior close-up, mismatched supporting/
vehicle sizes, duplicated seat factors, differing dog ratio and shared prop size.
World findings: separate supporting/focal memory depth, authoritative one-minute
advance, application-layer recall filters, non-atomic persistence and unsafe
read/parse-to-fresh fallback. Scale/dog/prop and fallback issues were addressed;
remaining boundaries are explicit in the component contracts. Scouts were bounded
source inspections, not exhaustive verification or independent runtime tests.

**Bootstrap scope.** Root `EGM.md`, `scripts/lessville/EGM.md`,
`backend/agentd/src/EGM.md`, and `backend/agentd/src/sim/EGM.md`. Human README content
was preserved. Large asset, build, archived experiment and deployment subtrees were
not recursively bootstrapped. Intermediate `scripts`, `backend` and `backend/agentd` blueprints link these contracts.
No documentation-only commits or deployment performed.

## Open work, in priority order

1. Resolve crossing admission/right-of-way and spawn clearance with adult-sized
   footprints; require zero overlap AND eventual progress in seeded dense cases.
2. Audit painted doorway/furniture clearances and fix existing office art failures.
3. Replace supporting block figures with distinct consistent adult artwork; add
   inspectable supporting identities and interior occupancy without inventing state.
4. Add deeper supporting relationships/recall and need-driven recovery. Current
   social/fatigue parameters are scenario choices, not validated physiology.
5. Establish resource budgets, household economy, production/supply chain and
   capacity-aware destinations rather than claiming a complete small real world.
6. Audit metric unit adapters in the separate physical/robot simulators. Pixel-art
   ratios are not transferable to real robot kinematics.
7. Fault-inject persistence failures and verify no partial state is silently accepted;
   assess snapshot/event consistency separately from successful normal restart.

## Entry template for future changes

Date and user requirement → observed failure → decision and alternatives → changed
source/contracts → exact verification/evidence → residual limits → next bounded step.
Keep old failures with their fix evidence; do not rewrite them as if they never happened.


## 2026-11-17 — Detailed characters and a bounded neighbourhood

User feedback rejected enlarged block placeholders and an overcrowded promenade.
Replaced those figures with an 8-appearance, 32-frame transparent character atlas;
front/back walking phase follows distance, appearance follows stable resident ID.
Also replaced car/van/cyclist geometry with detailed art, retained aspect ratios,
and enlarged the existing bus uniformly so its body is taller than the van.
`town_art.gd` measures opaque bounds instead of scaling transparent atlas gutters.
The first atlas assertion accidentally divided integers; corrected the test to use
floating-point aspect ratios, then native atlas checks passed.

Population decision: preserve all 100 lives, but allocate 5 focal + 24 supporting
to the displayed neighbourhood and 71 supporting to abstract surrounding districts.
The API exposes district and local/regional population counts. Three-minute local
shift offsets replace cohorts of eight. No world history was deleted. A seven-day
minute-by-minute regression measured 11 supporting residents outdoors at peak
(limit 12), and all 95 supporting residents kept eating and advancing. Supporting
workplace admission is not yet governed by furniture/seat capacity. Regional streets
are abstract, not newly built maps. Eight appearances are reused, not 95 bespoke sprites.

Evidence in `../outputs/lessville-live` relative to checkout: `district-tests.log`
(14 current simulation tests passed), `capacity-tests.log` (new seven-day test passed),
`character-scale-atlas.png`, `character-scale.log`, `presentation-build.log`.
Restart resumed exactly minute 7590. A strict floating-point equality comparison
initially failed; differences were ~1e-14 serialization roundoff. Comparing all 100
identities and life fields with absolute numeric tolerance 1e-9 passed; evidence
`art-restart-verification.json`. This is not a tolerance for lost simulated minutes.

The new native day capture is `day-recording-polished`; final run/encoding and drift
review results are recorded below after completion. Prior captures are historical
before-images and must not be presented as the improved renderer.


## 2026-11-17 — User screenshot: overlapping traffic and crowd balance

The user’s native screenshot showed the new van painted over the bus. Artwork
quality alone was not sufficient: bus motion and ordinary traffic had independent
spacing. Added one ordered per-lane presentation controller using actual sprite
lengths and 28 px clearance. Off-map lane changes and previous-trip exit preserve
continuity. A pause regression initially detected a lane transition during pause;
fixed by freezing the existing queue before any state changes. `street-check.log`
then passed 53,983 pair checks, pause, five-person standing separation and animal
habitat/night/flight bounds. Backend timetable/boarding does not yet incorporate
visual queue delays; keep this limitation explicit.

Reduced local cast again to **5 focal + 12 supporting = 17**, preserving 83
supporting lives in surrounding districts (100 total). This supersedes the earlier
29/71 choice without deleting identities/history. `balanced-capacity-tests.log`
measured peak **7 supporting residents outdoors over seven days**, bounded by 8.
Park leisure now has three local visitors plus one local gardener, with separately
spaced focal and supporting standing positions and distinct bus shelter offsets.

Added two cats, a rabbit and two sparrows through the built-in image-generation
skill. These are clock-driven ambient lives, not full animal agents. Garden/porch
paths never enter the road; birds perch and alternate brief flights. Cat/rabbit
resting poses use a fixed species unit so curled-up animals do not inflate.
The latest native run is `day-recording-balanced`; `day-recording-polished` is now
historical evidence of the rejected overlap. Do not ship that older video as final.


## 2026-11-17 — Review corrections and traffic progress

The first balanced recording completed 1,440 simulated minutes but exposed a
backend traffic deadlock: same-lane leaders yielded to followers behind them.
The presentation queue consequently kept the bus offscreen. This is rejected
capture evidence, not a successful final demonstration. The backend now yields
only to vehicles ahead; a two-direction regression checks both leader motion and
follower recovery. `traffic-progress-test.log` passes. `street-final-check.log`
adds a progress gate: 53,983 body-clearance pair checks and 14 complete bus
crossings, plus pause, standing-space and animal habitat checks, all pass.

Wider park standing slots exposed a routing stall: a resident near a street was
sent back through the opposite junction, and converging walkers could disagree
about which one was ahead. `_best_start` now recognizes standing positions within
one adult radius of a street; a shared next waypoint orders followers by remaining
distance. The existing dense regression now reaches every goal with maximum wait
2.43 seconds (previously 33.8). Its two existing dense overlap failures remain:
4,736 zebra overlap pair-ticks and two same-direction overlap ticks. This is not a
claim of complete pedestrian collision resolution. `godot-final-tests.log` reports
2 failed of 390, with four expected failures; office art checks remain known gaps.

The fresh-context EGM source review covered ten contracts/documents sequentially.
Its six findings and resolutions are preserved in
[2026-11-17_DRIFT_REPORT.md](2026-11-17_DRIFT_REPORT.md). Corrected API field naming,
test fixture coverage, meal exceptions/timeouts and local cast arithmetic. The
scale gallery's automated assertions were overstated; the contract now accurately
labels adult silhouette equivalence as visual inspection. No test result was
invented to close that finding. Generation provenance/history were outside the
independent reviewer's verification scope.

Controlled restart at minute 10,470 retained all 100 IDs and Life state (floating
roundtrip tolerance 1e-9): `traffic-restart-verification.json`. A fresh native day
capture is `day-recording-reviewed`; earlier balanced/polished directories are
historical. Final recording and test results are appended below after completion.


### Final native run and delivered video

`day-recording-reviewed/run.json`: minute 10470 → 11910,
exactly 1,440 simulated minutes (Day 8 06:30 → Day 9 06:30), 1644
native frames over 271.604 wall seconds, Apple M5 OpenGL / Godot 4.7.2.
All 129 sampled API states retained 100 distinct residents and bounded needs.
Presentation verification measured 18,288 same-lane vehicle pairs,
minimum body gap 28 px within floating tolerance. Bus traversed to x=1860;
bus and van were simultaneously visible in 536 frames.
Peak visible town walkers was 7 (supporting peak 5);
this is a sampled recording observation, not a universal capacity proof. All five
added animal identities appeared. Detailed evidence: `reviewed-day-verification.json`.

`lessville-24-hours.mp4` is 1280×720 H.264, 271.933 seconds, 29,827,488 bytes.
The 30 fps video holds captured frames using their recorded timing through ffmpeg's fps filter;
it does not claim 30 independently captured frames per second. Source timing is
`frames.jsonl` and the exact concat input is `frames.ffconcat`. Full decode completed
without error. `reviewed-source-manifest.json` records the uncommitted source and
asset hashes; no commit or deployment was made.

`final-restart-verification.json`: controlled SIGTERM/flush/reopen preserved minute
11,910, paused state, all 100 IDs and Life data (numeric tolerance 1e-9). The local
service remains on loopback 8790 and the native client is open. Runtime worlds and
captures remain outside source control. No secrets were read, copied or changed.


### Final test outcome

`backend-reviewed-tests.log`: **51 passed, 0 failed, 1 ignored**, 477.60 seconds.
The ignored test requires external physical JSONL; it is not counted as a pass.
The final deterministic-presence scenario compares two 900-minute traces against
real embedded LessDB and passed. `godot-final-tests.log`: **2 failures out of
390, plus 4 expected failures**; the remaining failures are dense crossing overlap
and two same-direction follower overlap ticks. No clean Godot-suite claim is made.
Road/animal checks pass; MP4 full decode passes; all 35 local links across the
11 reviewed documents resolve; `git diff --check` passes. The native client was
visually verified after entering the town, showing the resumed paused Day 9 06:30.

Remaining: autonomous animal needs/ownership, feedback from visual traffic delay
into passenger/timetable state, pedestrian intersection arbitration, workplace
capacity, and a metric building/furniture/door survey. Other physical/robot
experiments were not recalibrated in this task.


## 2026-11-17 — continuation: crossings, admission, pause and delayed arrivals

The user's “good, continue” authorized the next bounded realism repairs. No people,
animals, assets, scales or persistent identities were removed. Current scope:
west-zebra direction separation, shared entry admission, pause stability, and
correct upcoming-bus information. Full traffic/passenger coupling remains open.

### Findings and implementation

The crossing failure was structural: both walking directions used one centreline;
following only separated people sharing a direction. `town_view.gd` now uses
southbound x=193 and northbound x=237 lanes. The west approach junction moves
25 px north to (215,640), on the existing painted pavement, giving turning walkers
clearance from the crossing mouth. `town_nav.gd` includes the small paved corner.
`_best_start` cannot shortcut the bent crossing with its straight chord.
`_walker_path` preserves the lane when destinations reverse between y=662 and 960,
including exact boundaries and a newly connected mid-crossing snapshot.

Entry admission checks both resident groups and reserves 44 px around even a
partially visible occupant. Waiting identities remain queued; an eight-person
shared-start test requires every identity to enter and arrive without a visible
position jump. `agent_pose` now uses bus-stop standing offsets at initialization,
matching `_goal_for`. Paused walkers no longer advance fading or door timers.
The fresh test scout caught the pause fade and alpha=0.25 entry-reservation gaps;
both were reproduced by assertions and fixed.

The last follower failure was traced to an earlier head-on encounter on the narrow
café path: bg1 and bg7 approach on different opposing segments, then bg7 reverses
its goal while already overlapping. Broadening direction-only following produced
a 13.43 s deadlock and was rejected/reverted. The retained west crossing fix does
not claim general sidewalk collision avoidance. Shared stationary destinations
also overlap in the deliberately dense fixture (54,437 boarding-apron pair-ticks).

The backend snapshot previously dropped an 08:00 service delayed to 08:15 as soon
as the scheduled minute passed. `bus.next` now selects the earliest effective
arrival from the existing scheduler's 60-minute lookback plus 60-minute upcoming
window, retaining IDs and the response schema. Actual snapshot tests cover delayed
visibility, arrival/departure boundaries, a later on-time service arriving first,
and the end of service. Existing delays beyond the scheduler lookback are not solved.

### Verification and evidence

Evidence root: `/Users/maruthi/oasis/less/outputs/lessville-live`.

- `continuation-final-godot-tests.log`: **one failure out of 390 checks**, plus
  four existing expected office-art failures. Seed 7: 23,400 ticks, 15 walkers,
  507 destination changes, 392 west-zebra visits, **zero zebra overlap**, minimum
  footprint margin approximately 4 px, no off-nav samples, maximum wait 2.53 s,
  all goals reached. The retained failure is one follower-overlap tick at 553.17 s,
  minimum margin −14.9 px. Thresholds and 20 px adult radii were not relaxed.
- `continuation-walker-clearance.log`: **26,865 checks, zero failures** for lane
  containment, boundary reversals, reconnects, admission, progress, pause and
  initial shelter placement. Run with `--script test/walker_clearance.gd`.
- `continuation-timetable-tests.log`: **two backend timetable tests passed**.
  The new regression reproduced the old snapshot failure before the fix. Final
  backend build succeeded. The full 51-test backend run above was not repeated
  for this snapshot-only change.
- `continuation-crossings/`: six native OpenGL/Apple M5 frames at seeded stress
  moments 55/100/150 s and the following quarter-second; metadata identifies
  their artificial test state. This is not a new live-day recording. The existing
  24-hour video predates these crossing repairs.
- `continuation-restart-verification.json`: graceful shutdown flushed the live
  world; updated backend resumed minute 11,910, paused, all 100 resident states
  unchanged (numeric tolerance 1e-9). The updated native client was reopened and
  visually verified connected at Day 9 06:30. No secrets or deployment changed.

### Next realism boundary

Keep narrow-sidewalk right-of-way as an explicit open item; do not hide the failing
oracle. Routes need admission/reservation or feasible passing space before more
residents are added to the centre. Backend bus arrival/position and lane clearance
must become authoritative before passenger boarding can follow real traffic delays.
The read-only traffic scout found five delayed-dwell samples in the previous day
recording where the logical bus stop and rendered bus differed, and only two named
visual trip IDs across the day's 44 scheduled services. Fixed wall-second visual
travel cannot represent every service at accelerated time. Acceptance must include
blocked-bus delay, eventual arrival, exactly-once boarding through restart, trip-ID
continuity at speeds 1/8, and client tests updating both bus and road. No renderer
acknowledgement should make the simulation depend on an open client.

### Continuation drift review

A fresh-context EGM reviewer checked the two changed component contracts,
`WORLD-SCALE.md` and `LOCAL-WORLD.md` sequentially against current source and the
four supplied evidence files. Both findings are appended to
[2026-11-17_DRIFT_REPORT.md](2026-11-17_DRIFT_REPORT.md) and resolved in the scale
contract. The final oracle failure is a same-direction follower overlap, even
though the earlier diagnostic trace showed an opposing encounter before reversal.
Vehicle queue clearance uses reserved body lengths: the bicycle's measured
558×563 px atlas bounds produce approximately 99.118 px rendered width, safely
within its 100.006 px reservation. This was documentation drift, not a bicycle
overlap defect. The reviewer did not rerun tests or historical day/week captures.


## 2026-11-17 — publication candidate, portable engine and junction-preserving routes

The user authorized publishing the Lessville source in `maruthiprithivi/lessdb`
and updating the existing Cloudflare deployment. This entry records preparation
and local evidence; it does not claim that a commit, copied-R2 restore or rollout
has completed. [Release qualification](../RELEASE-QUALIFICATION.md) records the
exact boundary and isolated restore procedure. Runtime worlds, checkpoint copies,
captures, binaries, credentials and generated distribution output remain outside
the publication scope.

The native backend now resolves its engine dependencies to `vendor/lessdb`: nine
crates from qualified commit `0367d81d935a452f28cb2a15a62ecce2463698b1` with the
existing `less-agent/src/lib.rs` worktree change preserved. The vendor manifest
narrows workspace membership; registry resolution remains pinned by the backend
lockfile. Per-file hashes and `UPSTREAM-WORKTREE.patch` make the imported bytes
reviewable. Patch SHA-256 is
`f1174e9869c9ad994ab88b765951f08ddf0adeffd457952163cb8ed0c165323c`.
The vendoring agent reports offline native metadata/build success; that does not
qualify a Linux Container or prove compatibility with the deployed R2 archive.

The remaining seed-7 follower failure came from changing destinations mid-segment.
`_walker_path` now keeps the existing path, including intermediate bends, through
its next graph junction before adopting the changed goal. The west-crossing lane
override remains. An intermediate candidate created a 14.67 s seed-23 wait:
reciprocal U-turn routes incorrectly considered one another future leaders.
`_follow_budget` now excludes headings with dot product at most -0.7 from that
around-corner lookahead while retaining ordinary-corner following. No pedestrian
radius, population, navigation geometry or existing dense oracle was relaxed.

The reviewer ran the final source and supplied these logs, which were read directly
for this documentation update. They are preserved under the local evidence root
`/Users/maruthi/oasis/less/outputs/lessville-live`, outside source control:

| Evidence | Configuration | Result |
|---|---|---|
| `lessville-review-targeted-final2.log` | Focused walker suite | 40,440 checks, zero failures |
| `lessville-review-seed7-final2.log` | 600 + 180 s, 30 Hz | 23,400 ticks; 507 flips; 418 west visits; 45,649 follower pairs; max wait 2.267 s |
| `lessville-review-seed1-final2.log` | 300 + 180 s, 30 Hz | 14,400 ticks; 252 flips; 194 west visits; 20,725 follower pairs; max wait 0.867 s |
| `lessville-review-seed19-final2.log` | 300 + 180 s, 30 Hz | 14,400 ticks; 252 flips; 213 west visits; 25,668 follower pairs; max wait 1.700 s |
| `lessville-review-seed23-final2.log` | 300 + 180 s, 15 Hz | 7,200 ticks; 253 flips; 194 west visits; 11,825 follower pairs; max wait 1.067 s |

All four dense runs used 15 walkers, reached every final goal and reported zero
zebra/follower overlaps and zero arc/facing/off-nav errors. Minimum follower margin
was approximately 4 px; the minimum crossing-zone margin was 1.24249 px in seeds
7/19, so 44 px lane-centre separation is not a universal 4 px approach margin.
The focused suite covers normal edges in both directions, both bent approaches,
focal/supporting reroutes, no snaps/hiding, retained-junction visits, new-goal arrival
and false U-turn waits. Godot emitted its existing macOS certificate-query warning;
it was not an assertion failure. These are logic runs, not fresh native captures.
The previous 24-hour video predates the change. Shared stationary/boarding-apron
overlaps, general head-on passing and authoritative traffic/passenger coupling remain
open. Earlier failed evidence above remains historical and is not rewritten as passing.

The intended Cloudflare boundary stays Worker `lessville` at `ville.lessdb.dev`,
Container class `LessvilleAgent`, singleton `town-v3-retention-2026-11-14`, R2 bucket
`lessville-data` and prefix `lessville/v3/`. At the time of this preparation entry,
copied-checkpoint and actual deployed continuation evidence were pending. A local
build and an existing public URL are not substitutes for that evidence.

### Native copied-checkpoint rehearsal and portable commands

The root agent downloaded the current R2 archive read-only. The authorized native
rehearsal used a separate `.local/cloudflare-restore-wnvfqscx` directory and a
loopback GET-only server. Its environment contained no production URL or credential;
a dummy local key exercised the exact restore path. An intentional PUT probe was
rejected with HTTP 405. The initial sandbox denied local socket binding; the approved
loopback execution then completed. Archive inspection found 362 entries and no
absolute paths, parent escapes or links. No live service was paused, reset or written.

Input archive: 513,729 bytes, SHA-256
`51cb9d33f6ca320c0883b3a73225a0d33a84434ff34e83aa5850c75369a393ba`.
Native binary SHA-256:
`6b9bd80056e05c0fb942d5b27180a47c976d43e4b25b58fa74a918fd26ec5a15`.
Restore resumed minute 585,195, Day 407 09:15, with 597 memories, 22 messages and
100 unique residents (5 focal, 95 supporting; 17 town-centre, 83 surrounding).
The scratch API was paused, flushed and stopped, then reopened with checkpoint
integration disabled. Clock and every focal/supporting resident snapshot field
matched exactly. Both processes exited zero after graceful flush; no candidate
upload occurred. The local server stopped and the input archive hash was unchanged.
Evidence: `native-cloudflare-restore-verification.json`,
`native-cloudflare-http-restore.log` and `native-cloudflare-local-reopen.log` in the
evidence root above. This qualifies the native candidate against the copied archive;
the exact Linux release image and live deployment are still pending.

The root also removed the old sibling-engine launch guard and the blanket media
ignore rules. `npm run dev`, `check`, `build` and `export` now use local repository
helpers; core operation no longer needs Manus `GAME_RUNTIME`. The hair-render test
selects Linux Xvfb or macOS native capture without relaxing assertions.
`release-client-check.log` records **390 passing checks, four expected office-art
failures**. Godot 4.7.2 Web export passed. The Cloudflare Web preparation helper
stages local artifacts using the deployed fetch mapping; it does not deploy them.

Operator setup completed Cloudflare MCP configuration, 22 official Cloudflare
skills and refresh of the existing Wrangler OAuth session. New agent sessions may
be needed to discover those tools. No SOPS bundle was edited or credential value
added to source. The newer deployed Worker reliability changes are being reconciled
before rollout; the local native rehearsal does not validate those edge changes.

### Operator credential handoff and edge reconciliation

The release operator searched `/Users/maruthi/oasis/less/secrets` and sibling
`/Users/maruthi/orca/workspaces/less/performance_improvement/secrets`, plus
`main`/`origin/main` versions of `.sops.yaml` and `example.enc.yaml`. Compared
hashes matched and only placeholders were present; the nested Lessville checkout
had no credential bundle. The source actually used is the existing Wrangler OAuth
configuration at `~/Library/Preferences/.wrangler/config/default.toml`, refreshed
through browser authentication. Cloudflare's existing `INTERNAL_KEY` was preserved,
never read or rotated. No SOPS bundle changed; canonical/main SOPS state was verified
unchanged. No credential value is recorded here.

The Worker reconciliation preserves reliability behavior from deployed version
`366d146e-3a77-437b-b4e4-b978f7113ddb`: eight-second fetch timeout, 350 ms freshness
label, five-second background refresh pacing, raw persisted last-good state in
`Budget("global")`, stale response headers, `/api/healthz`, and scheduled
health/state refresh. Healthz can succeed with an old cached state, so release
verification must inspect actual simulation progress. The explicit flag
`LESSVILLE_REALISM_PATCH=0` prevents legacy text/time rules from rewriting
authoritative backend activity/location/station/pose. The retained legacy helper
is available for rollback. The Worker agent reports 14 fixture/persistence/routing
tests passing, TypeScript checks passing and helper parity against the downloaded
bundle. No identity, secret or checkpoint namespace changed. The root uploaded the
new immutable static pack to R2; it is not yet activated. Linux-image qualification,
Worker/Container rollout and live-world continuation remain pending for the root's
final release entry.
