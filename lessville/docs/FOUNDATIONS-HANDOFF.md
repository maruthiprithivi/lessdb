# Small-world foundations — local handoff

2026-11-16, Asia/Singapore. Source bundle base: `23da2ed` (`loop/base`).
Working branch: `codex/small-world-foundations`.

## Workspace

* Source: `/Users/maruthi/oasis/less/lessville` (separate Git repository).
* Unpacked brief: `/Users/maruthi/oasis/less/inputs/lessville-claude-lite`.
* Evidence: `/Users/maruthi/oasis/less/outputs/lessville-foundations`.
* Runtime: `/private/tmp/lessville-godot-runtime/Godot.app/Contents/MacOS/Godot`.
  This temporary download may be removed by macOS; reinstall the same version if
  needed. Download source: [official Godot 4.7.2 archive](https://godotengine.org/download/archive/4.7.2-stable/).
* Only remote: `origin` points to the local bundle in Downloads. No public remote
  was added and nothing was pushed or deployed. Existing LessDB checkout changes
  were left untouched. No credentials were needed or read.

## Delivered

The [small-world plan](SMALL-WORLD-PLAN.md) starts with a concrete multi-day café
story and maps needs, roles, activities, memory, relationships, skills and world
changes onto the existing Rust service and LessDB records. It separates design
values from empirical references and identifies clock/scale contradictions.
The [attribute register](REALISM-IMPLEMENTATION-REGISTER.md) retains all 188 IDs,
including 84 missing checks and the five reported failing attributes. It does
not pretend proposed checks are implemented coverage.

Job 1a is implemented: both walker types retain a full route `poly` and distance
`s`; a shared helper advances on that route, clamps arrival and consumes remaining
waypoints. Hidden relocation clears route state. Walk-cycle phase uses travelled
route length, including turns. Existing facing follows actual tick displacement,
preserving the junction-heading contract. Motion-gallery fixtures now supply
the new state when they directly position walkers.

The dense crossing oracle is unchanged. Ten boundary assertions cover exact
vertices, duplicate points, zero budget, negative budget, overshoot, arrival,
empty routes and corner distance. No new production dependency was added.

## Validation

Engine reports `4.7.2.stable.official.ed1daf0bf`. Native captures report OpenGL
Compatibility on Apple M5; headless Dummy frames were not used as visual evidence.
Resource import began from the fresh clone, with no inherited `.godot` cache.

* `baseline.log`: arc gate fails with **157,681 missing samples**, matching the
  supplied baseline. Follow-gap and crossing gates also fail.
* `fail-first.log`: the added boundary case fails before implementation.
* `parse-*.log`: all three edited GDScript files parse cleanly after implementation.
* `after.log`: **2 failures out of 383 checks, with 4 expected failures**. Arc and
  boundary gates pass. Facing, navigation, progress, bus route/dwell, seating,
  attire, atmosphere state and other previously passing cases remain passing.
  Remaining unexpected failures: crossing overlap (4,028 pair-ticks) and
  same-direction follow gap (4,059 ticks). The four office-art expected failures
  remain real known limitations, not fixes delivered here.
* The initial Mac baseline also had three hair-render failures because the suite
  invokes Linux `xvfb-run`. Native GPU captures supplied through the existing
  `LESSVILLE_LIN_HAIR_RENDERS` override resolve those without changing the oracle.
* `attire-check.log`: all 16 freshly rendered outfit cases pass.
* `atmosphere-check.log`: fresh clock/wind frames pass the pixel checks. The suite
  itself skips this pixel check when the lab-specific frame path is absent; the
  standalone check was explicitly run on the local captures.
* Live service checks were skipped: no backend was running at localhost:8790.
  This change does not establish live client/backend integration.
* `long-run-summary.json`: seeds 1, 2 and 3 each ran 10,800 test seconds plus
  240 seconds to settle (331,200 ticks per seed at 30 Hz). Across **8,571,925 arc
  samples**, missing/bad/backward arc state, off-nav positions and facing
  failures were all zero. All destinations were reached; maximum wait was
  0.0334 seconds. These are path-correctness passes, not collision passes:
  overlap pairs were 106,035 / 99,732 / 88,345 (includes the boarding apron),
  and follow-overlap ticks were 63,589 / 64,631 / 64,670 respectively.

## Reproduce locally

Run from the source checkout, using the same Python with Pillow as the suite
(`/usr/bin/python3` on this machine). The shell's default Homebrew Python lacks
Pillow; no installation is necessary.

```sh
GODOT_BIN=/private/tmp/lessville-godot-runtime/Godot.app/Contents/MacOS/Godot
EVIDENCE=/Users/maruthi/oasis/less/outputs/lessville-foundations
rtk proxy "$GODOT_BIN" --headless --path . --import --quit
rtk proxy "$GODOT_BIN" --headless --path . --check-only -s res://scripts/lessville/town_view.gd
rtk proxy env LESSVILLE_ATTIRE_OUT="$EVIDENCE/hair-renders" "$GODOT_BIN" --path . --rendering-driver opengl3 -s res://tools/render_attire.gd
rtk proxy env LESSVILLE_LIN_HAIR_RENDERS="$EVIDENCE/hair-renders" "$GODOT_BIN" --headless --path . res://test/test_suite.tscn
rtk proxy env LESSVILLE_MOTION_OUT="$EVIDENCE/motion" "$GODOT_BIN" --path . --rendering-driver opengl3 -s res://tools/render_step2_motion.gd
rtk proxy env LESSVILLE_ATMOS_OUT="$EVIDENCE/atmosphere" "$GODOT_BIN" --path . --rendering-driver opengl3 -s res://tools/render_atmosphere.gd
rtk proxy /usr/bin/python3 tools/check_attire.py --dir "$EVIDENCE/hair-renders"
rtk proxy /usr/bin/python3 tools/check_atmosphere.py --dir "$EVIDENCE/atmosphere"
rtk proxy env SIM_SEED=1 SIM_SECONDS=10800 "$GODOT_BIN" --headless --path . -s res://tools/sim_crossings.gd
```

The suite's exit code remains 1 until jobs 1b/1c are fixed. Check error text as
well as exit status; failed import/editor writes in the initial sandbox run are
recorded in `import.log`, with the successful portable import in
`import-portable.log`. Native runs needed normal application-data/display access.

## Visual review and next work

Open `/Users/maruthi/oasis/less/outputs/lessville-foundations/gallery/index.md`.
It contains ten 4× character crops (five characters at noon and dusk), plus
three seeded crossing strips. `render-sha256.json` records source/crop hashes.
Motion fixtures are staged poses; seeded crossing frames show actual simulated
traffic. Neither is proof of safe following. Independent visual review remains
pending; the author has not certified visual realism.

Next: implement job 1b speed-only following, then job 1c fair crossing admission
and cancellation on reroute. Preserve the independent nav/facing/progress gates.
Audit initial snapshot travel too: named walkers currently initialize at a
mid-route pose with an empty path and a matching goal key; the existing motion
capture explicitly forces rerouting to work around this. That inherited startup
behaviour was preserved in this slice and needs a dedicated regression test.
Render-time background nudges and physical time/scale agreement also remain
known gaps before traffic can be called realistic.

After traffic: qualify the backend dependency checkout, then implement the
arrival → queue → order → consume → need change → memory loop from the plan.
Character development and multi-day behavioural changes are **planned, not
implemented by this path-follower change**.
