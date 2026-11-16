# Lessville release qualification — 2026-11-17

This candidate is intended for `maruthiprithivi/lessdb` and the existing
`https://ville.lessdb.dev` deployment. Source publication, a successful build and
an existing live URL are distinct from deploying and verifying this candidate.
The first release passed native copied-checkpoint restoration, client checks and
exact Linux-image qualification, then rolled out at 2026-11-16 19:11:45.518 UTC.
Restore, population hydration and a subsequent R2 checkpoint are verified, but the
settled probe failed its live-health gate. The second candidate, `38cd159`, corrects
weather precedence and stages a `basic` capacity trial. Its strict copied-checkpoint
restart gate exposed a JSON float-roundtrip defect. Parser correction `4bc21af` is
committed and pushed. Its focused regression and Linux CI `43943678663` passed.
Artifact integrity, the full native backend suite and exact Linux-image
qualification also passed. **Corrective deployment and production health/checkpoint
verification remain pending.** Earlier
artifact and recording evidence below identifies the version it actually covers.

## Portable commands and operator setup

With Rust 1.96 and Godot 4.7.2 installed, `npm run dev` starts the local native
world, `npm run check` runs the Godot checks, and `npm run build` or `npm run export`
exports the Web client. Web export also requires matching Godot export templates.
The client suite requires Pillow in `/usr/bin/python3`, plus Xvfb on Linux; macOS
captures fixtures natively. See [local prerequisites](LOCAL-WORLD.md#prerequisites).
These commands no longer require Manus `GAME_RUNTIME` or a private sibling engine
checkout. Historical preview/doctor/asset-report commands still use the optional
Manus runtime; experimental robot/art tools can retain their original prerequisites.
The [Cloudflare guide](../deploy/cloudflare/README.md) covers the separate pinned
Linux build. `tools/prepare_cloudflare_web.py` stages local Web delivery artifacts;
staging artifacts is not a remote deployment.

The parent repository's `.github/workflows/lessville-linux.yml` builds Linux
artifacts on a native Ubuntu runner with the same pinned
`tools/build_cloudflare_linux.py` helper. It uses no deployment credentials and
uploads binary chunks and build evidence without deploying them.

The release tooling pins Wrangler `4.163.0`. After that development-dependency
update, the Worker reviewer reports zero `npm audit` vulnerabilities, passing
TypeScript checks, all 14 Worker tests and a successful deployment dry run with
Container rollout disabled. The runtime `@cloudflare/containers` SDK remains
`0.3.7`; this tooling update did not deploy or alter the world.

The release operator completed Cloudflare MCP setup, installed 22 official
Cloudflare skills and refreshed the existing Wrangler OAuth session. Agent tools
may need a new session to discover the new MCP configuration. Authentication stays
outside this repository; no SOPS bundle or credential value was added to source.

Credential-source handoff: the operator searched canonical
`/Users/maruthi/oasis/less/secrets`, sibling
`/Users/maruthi/orca/workspaces/less/performance_improvement/secrets`, and
`main`/`origin/main` versions of `.sops.yaml` and `example.enc.yaml`. The compared
hashes matched and the bundles contained placeholders only. The nested Lessville
checkout had no credential bundle. Deployment uses the existing Wrangler OAuth
configuration at `~/Library/Preferences/.wrangler/config/default.toml`, refreshed
through browser authentication. The existing Cloudflare `INTERNAL_KEY` remains
in place; it was neither read nor rotated. No SOPS update was made, and unchanged
canonical/main SOPS state was verified. These are source-location and status records,
not credential contents.

## Source and engine scope

Publish the Godot client, native backend, configuration, runtime artwork/fonts,
historical documentation and tools, and the qualified embedded engine. Keep
runtime worlds, downloaded checkpoint copies, captures, build output, binaries,
credentials and generated `site/` distribution files outside the source commit.
Check the staged file set: media that was tracked in the original checkout can
otherwise become ignored when imported into a new repository.

The working population is 100: five focal residents and 95 supporting residents.
The town-centre cast is five plus twelve; the other 83 supporting residents keep
their daily lives in abstract surrounding districts. They retain stable identities;
the local cast boundary is not permission to delete residents from a restored world.

`vendor/lessdb` contains the nine required crates from qualified reconstructed
commit `0367d81d935a452f28cb2a15a62ecce2463698b1`. Its
[README](../vendor/lessdb/README.md), [provenance](../vendor/lessdb/PROVENANCE.json)
and [upstream patch](../vendor/lessdb/UPSTREAM-WORKTREE.patch) preserve exact
source origins, per-file hashes and the existing `less-agent` worktree change.
The workspace manifest restricts membership to the required crates. The backend's
`Cargo.lock` still pins registry dependencies. This dependency is isolated from
the parent repository's public engine; matching project names do not prove
storage-format interchangeability.

Pedestrian destination changes preserve the current route through the next graph
junction, including bends. Corner lookahead ignores strongly opposing headings,
avoiding reciprocal U-turn waits. Final focused checks and dense seeds 1/7/19/23
pass their follower, crossing and progress gates; see the
[learning log](egm/LEARNING-LOG.md). Shared stationary destinations and general
narrow-sidewalk head-on passing remain open. The full-day native capture records
the final movement fix but predates the weather correction; its sampled frames do
not establish collision-free behavior or qualify the second candidate's weather.
Backend passenger scheduling still does not follow visual traffic delays.

## Existing Cloudflare identity

Retain these configured identities during rollout:

| Boundary | Existing value |
|---|---|
| Worker and custom domain | `lessville`, `ville.lessdb.dev` |
| Container Durable Object | `LessvilleAgent`, binding `AGENT` |
| Town singleton | `town-v3-retention-2026-11-14` |
| R2 bucket and prefix | `lessville-data`, `lessville/v3/` |
| Current checkpoint | `lessville/v3/latest.tar.gz` |
| Rollback history | `lessville/v3/hourly/<hour>.tar.gz` |

The Worker forwards the public observer API, serves Web assets, mediates internal
checkpoint/model calls and runs a five-minute cron. The Container is the native
single-writer simulation owner; R2 holds flushed archives. Do not rename the town,
change the checkpoint prefix, seed a replacement world or import a local practice
world as a shortcut around a failed restore. Consult
[deployment instructions](../deploy/cloudflare/README.md) for build and rollout.

The candidate preserves the newer deployed Worker's eight-second fetch timeout,
350 ms fresh-cache label, five-second background refresh pacing, raw last-good
snapshot in the existing `Budget("global")` Durable Object, stale-state fallback
and scheduled health/state refresh. `/api/healthz` indicates a cached snapshot or
recent successful keepalive; it does not prove the simulation clock is advancing.
`LESSVILLE_REALISM_PATCH=0` disables the legacy response rewriting so the backend's
explicit activity, place, station and pose remain authoritative. The old helper
is retained for rollback, not applied to this candidate's public state.

## Restore behavior and isolated rehearsal

`backend/agentd/src/main.rs` invokes `checkpoint::R2::restore` before opening the
engine. Restore is skipped when the selected data directory's `catalog/` has
entries. Otherwise it downloads `latest.tar.gz` and extracts into that directory.
HTTP 404 permits fresh initialization; other HTTP and extraction errors abort.
After open, startup selects the newest legacy/versioned world snapshot before
compaction, parses it without a fresh-world fallback on error, hydrates missing
residents and reconciles ID counters against stored maxima. An archive that opens
but lacks a world row can still seed a fresh world, so health alone is insufficient.

The same checkpoint configuration starts periodic uploads. A rehearsal must never
give the candidate the production checkpoint URL or production internal key.

1. Obtain one read-only copy of the current R2 object outside the repository.
   Record object identity, download time, byte count and SHA-256, and retain the
   original unchanged. Inspect archive entries before extraction; reject absolute
   paths, parent escapes and links that could leave the scratch directory.
2. To exercise the actual HTTP restore path, serve a copy named `latest.tar.gz`
   from a dedicated directory with a loopback-only GET server that rejects PUT.
   Python's static HTTP server has this property. Use a fresh empty data directory,
   a separate existing temporary directory and unused loopback ports for both
   server and candidate. Do not reuse a local live world.
3. Run the candidate with a cleared environment except required process paths and
   explicit settings below. Do not inherit provider, weather or checkpoint settings.

| Setting | Rehearsal value |
|---|---|
| `LESSVILLE_DATA` | Fresh scratch data directory |
| `TMPDIR` | Separate scratch temporary directory |
| `LESSVILLE_BIND`, `LESSVILLE_PORT` | `127.0.0.1`, unused local port |
| `LESSVILLE_PUBLIC` | `0` |
| `LESSVILLE_CKPT_URL` | Local GET-server base URL only |
| `LESSVILLE_INTERNAL_KEY` | Dummy local string of at least 16 characters |
| `LESSVILLE_CKPT_LABEL` | `local-copy` |
| `LESSVILLE_CHECKPOINT_SECS` | `86400`; PUT rejection is the write barrier |
| `LESSVILLE_LIVE_WEATHER` | `0` |
| `LESSVILLE_LLM_URL` | Empty |

4. Require both the restored-archive and resumed-world startup messages. Treat
   `Seeded a fresh Lessville world` as failure. Pause only the scratch API with
   `POST /api/control` and `{"action":"pause"}`. A previously running snapshot
   can advance before this request; compare the logged resumed minute separately
   from the later observed minute. Check resident IDs, population/district counts,
   retained memories, pending activities and ID counters against the copied world,
   allowing documented hydration of newly introduced supporting residents.
5. Stop the scratch process gracefully, reopen that same scratch directory with
   checkpoint/model URLs and internal key empty, and verify persisted continuity.
   Keep logs and checks outside Git. Shut down the local archive server afterward.

For the **exact Linux release image**, extract another archive copy into an isolated
data directory and mount only that directory as `/data`. Start with Docker networking
disabled and checkpoint/model URLs and internal key empty. Inspect the local API
inside the container. This checks the deployed binary's engine/storage compatibility
and world resume, but it intentionally skips HTTP download/extraction; record that
distinction rather than calling it an end-to-end R2 restore test. Startup and shutdown
write to the scratch copy, so never mount the original archive or an active world as
the writable data directory.

## Qualification evidence and rollout

The native copied-checkpoint rehearsal passed against archive SHA-256
`51cb9d33f6ca320c0883b3a73225a0d33a84434ff34e83aa5850c75369a393ba`
(513,729 bytes) and native binary SHA-256
`6b9bd80056e05c0fb942d5b27180a47c976d43e4b25b58fa74a918fd26ec5a15`.
It resumed minute 585,195 (Day 407 09:15), 597 memories and 22 messages, with
100 unique residents, 17 town-centre and 83 surrounding residents. After pausing
the isolated copy and a graceful restart with integrations disabled, the clock
and all focal/supporting resident snapshot fields matched exactly. The local
server served one GET and rejected the intentional PUT probe; the candidate
uploaded nothing. The original archive remained unchanged and all test processes
stopped. Evidence is `native-cloudflare-restore-verification.json` under the local
`outputs/lessville-live` evidence directory, outside the source repository.

Native Ubuntu [Actions run 43932509106](https://github.com/maruthiprithivi/lessdb/actions/runs/43932509106)
succeeded using the pinned build helper. The operator verified its synthetic merge
`d40a1ce`, with parents main `44d823b` and candidate `2b50572`, and all 92 build-input
hashes against the candidate. The qualified artifacts are:

| Artifact | SHA-256 |
|---|---|
| Backend source | `61878972d7493d61e18ad0f9ee0cffa15eeea30fdf89013e303986523ce0a7df` |
| Linux binary | `0539717c754114a2ddb116e08bf3fb6e35a60f9093c62847dd74d365897fd6fb` |
| Image `lessville-agent:native-2b50572` | `8fdf192a9a7c86a85c95f319207b2f2920addc73f5bdace9b93e53d29b123ae6` |

`linux-container-verification.json` records **passed** for this immutable image:
fresh and copied-checkpoint starts, paused five-minute steps, graceful shutdown and
restart preserved exact clocks and resident arrays. Both phases retained 100 unique
residents (5/95, town/surrounding 17/83), ran as UID 10001 with networking disabled
and a 256 MiB limit, exited zero and had no OOM. Reported memory was 59.66 MiB fresh
and 80.82 MiB restored. The restored process logged Day 407 09:15, 597 memories and
22 messages; its paused observation was minute 585,196, then 585,201 after the step
and restart. The readonly source archive was unchanged and all owned containers
and volumes were removed. This checks extracted-checkpoint compatibility; the
separate native rehearsal above exercised HTTP restoration.

The optional 0.0625-CPU emulated probe was **inconclusive** at its 90-second deadline.
First readiness was observed at 66.418 seconds, and sampled clock values advanced
403 → 434. These are bounded observations, not production performance proof.

Before rollout, `cloudflare-checkpoint-before-rollout-20261116T190525Z.json` records
a separate read-only backup matching the current R2 ETag: minute 589,562 (Day 410
10:02), 529,798 bytes, SHA-256
`2d51a342078c17710fac002a0f63fffc05410ad723df5b2bbcee730f28a6daa3`.
The original rehearsal archive remained untouched; this backup is not a deployed
candidate's restore result.

`release-client-check.log` records 390 passing checks with four existing expected
office-art failures. The current Web export passed with Godot 4.7.2. Those results
do not validate the Linux binary or the deployed world.

The operator's `release-checks.json` records source commit
`9dec5558803a163a4dafb865c56b7ff00bd36ec7` (published in PR #3), 52 passing backend
tests and one ignored simulator-input test in 569.47 seconds, 14 Worker tests,
TypeScript and deployment dry-run success, and a clean imported Web-client boot
with zero browser console errors. `release-clean-import.log` records fresh Godot
resource import. The operator also reports a passing native scale-atlas run.
These recorded checks are separate from the Linux qualification above and the
actual rollout below; the dry run did not deploy a replacement Worker or Container.

The new `release-day-recording/run.json` records minute 399 → 1839 (Day 1 06:39 to
Day 2 06:39), exactly 1,440 simulated minutes, with 838 native Apple M5/OpenGL frames
over 344.698 wall seconds. All 69 sampled states plus initial/final state retained
the same 100 IDs and population split. A fresh scratch world on port 8826 had cloud,
model and live-weather integrations disabled; both processes exited zero after
graceful flush. Existing local and production worlds were untouched.

`release-24-hours.mp4` is 1280×720 H.264, 344.766667 seconds and 26,589,115 bytes;
SHA-256 `74c41f4de0a24fb9b78de0a8968c6f7ee1eb0111fe986fc01ef70ce78c473935`.
Full decode passed. The 30 fps output holds frames for their measured durations;
the largest capture interval was 8.662 seconds, so this is not 30 independent native
captures per second or a wall-clock endurance run. `frames.jsonl`, `frames.ffconcat`,
`source-manifest.json`, `verification.json` and `ffprobe.json` preserve the evidence.
`release-day-contact-sheet.png` contains four real captured frames, indexed by
`contact-sheet.json`. Four transient offline indicators appeared during native
capture under load, including the 23:09 contact-sheet frame; successful snapshots
continued. The logs do not establish a backend outage or isolate the cause.
Morning/evening clear frames and the midday rain frame showed matching weather
labels in the inspected samples. These
observations do not clear the known shared-destination, head-on or office-art limits.

### Cloudflare rollout and initial live restore

Source `67ce334be6f6e8e92f732fdfb3185dee9c8ecbb4` deployed as Worker version
`d1b2a6fa-7473-4e25-a2c4-7c0b98ce8060`. Container application version 27 completed
its rollout at **2026-11-16 19:11:45.518 UTC**, retaining the existing singleton
and R2 namespace. Evidence: `cloudflare-deploy.json`, `cloudflare-deploy.log` and
`cloudflare-container-after-rollout.json`.

The deployed Linux/amd64 manifest is
`sha256:e4ba2fd220ce256635cddae1f58fe60fe337f6c35377a2497c736003a4249970`.
`cloudflare-image-equivalence.json` confirms it belongs to qualified image index
`8fdf192a…123ae6` above, with matching runtime configuration and all 29 filesystem
layers. These different index/platform digests identify the same qualified runtime.

The first probe, `cloudflare-probe-20261116T191119919514Z.json`, deliberately remains
failed evidence of the transition: old cached 13-resident state and an initial health
503 were followed by live `lessville-agentd` health 200 reporting restoration of
531,340 bytes from the existing R2 checkpoint. The new state logged **Day 410 21:06,
minute 590,226**, with 618 memories and 27 messages, then reached minute 590,238.
Hydration expanded 13 residents to 100 unique IDs while preserving the original 13,
with 5 focal/95 supporting and 17 town-centre/83 surrounding residents. The root page
referenced the candidate pack/Wasm, and both immutable asset hashes matched; the
probe also rejected the canonical `/index.html` redirect, which needs correct
handling in final page verification.

The same live sample exposed a weather contradiction: `weather: foggy` coexisted
with `raining: true` and an older seeded-rain reason. The local correction clears
stale rain timers/reasons on dry observations while preserving wet-ground history.
Persisted `weather_live_until` is Unix time equal to the observation time plus twice
the existing refresh interval: fresh live weather takes precedence through simulated
rain deadlines and overnight updates; expiry or disabled live weather restores seeded
provenance, code -1 and observation time 0. Observer rain nudges clear live provenance
and retain their bounded duration. The renderer respects explicit rain state and
authoritative weather kind/temperature instead of reintroducing seeded rain.

Focused validation passed: six native weather tests (four new, two existing; 51
filtered out), 16 Godot weather checks and diff checking. `tools/verify.sh` now
includes the Godot check. The second-candidate qualification status follows below;
the first rollout is not the final candidate.

The initial probe reported zero uploads, but the independent read-only R2 check
`cloudflare-checkpoint-after-rollout-20261116T191835Z.json` confirms a new checkpoint
written at **2026-11-16 19:16:56.083 UTC**, 258,216 bytes, SHA-256
`4e0f07e0672651616d80fa8f7fad34f370aacb12a2bff8f82a7cd0409ae3109c`.
Its ETag matched the downloaded backup; archive statistics do not prove an exact
latest world minute. The settled 19:17 probe (`cloudflare-probe-20261116T191717183010Z.json`)
verified candidate assets, stable 100-resident identities and advancing sampled time,
but all three live-health requests returned 503. The new checkpoint is confirmed;
full live-health qualification remains unpassed. Worker healthz and cached state alone
do not establish it.

### Precision-corrected candidate `4bc21af`: qualified; deployment pending

Source `38cd1593f1db2b4ed92de05c2b6ff239c142fa0c` contains the weather correction
above and changes the existing single Container to the `basic` instance type as a
capacity trial after the first rollout's live-health failures. It retains the same
Worker, singleton and R2 identities; increased capacity is not yet a demonstrated
resolution of those failures.

The pre-trial baseline at 2026-11-16 19:34:39 UTC, roughly 23 minutes after startup,
returned health 503 in 8.531 seconds (Worker timeout 8,000 ms). Nine complete metric
buckets from 19:23 through 19:31 UTC reported weighted CPU utilization of 104.32% of
allocation, averaging 0.06521 cores. These lagging metrics make CPU saturation
plausible but do not establish the cause of the health timeout. Evidence is
`cloudflare-pre-trial-baseline-20261116T193439Z.json` and its `-summary.json`.

[Actions run 43939193796](https://github.com/maruthiprithivi/lessdb/actions/runs/43939193796)
succeeded with the same pinned helper. Artifact integrity passed: all 92 source
inputs match candidate `38cd159`; verified synthetic merge
`76eb5315c83e967a47181a3de1e23e35a49e5459` has parents main `44d823b` and candidate
`38cd159`. The refreshed artifacts are:

| Artifact | SHA-256 |
|---|---|
| Backend source | `4b6b15807563732bc5946ae9045f2a062b368e1350c8cfa07a58d04a79ca1d73` |
| Linux binary (176,410,160 bytes) | `267787639c9c9414c4de7237866c7b7264e06e5524744d765aad6fa753066d1f` |

Image `sha256:1cc7302a7d31254d44b27f160a3a08b74d0f5473b0672abbdd976b6d1f4ab624`
was built from those artifacts. At 256 MiB, the fresh phase passed, but the strict
copied-checkpoint restart gate failed resident equality; the image is **not qualified**.
The optional CPU probe was omitted. Build integrity alone does not qualify runtime
restoration or production `basic` performance.

The first failed run did not save its differing snapshots. A captured diagnostic
repeat at minute 585,202 reproduced 99 changes, all `life.social_need`:
`15.350000000000005` became `15.350000000000003` (one ULP, about 1.776×10⁻¹⁵).
Four focal and 95 supporting residents were affected; the entire paused clock and
all other resident fields matched. A standalone probe using the exact captured
values reproduced all 99 changes among 300 Life floats with `serde_json` 1.0.151's
parser lacking `float_roundtrip`. Durable evidence is
`restore-float-roundtrip/summary.json`, with probe source, inputs and results beside it.
A different paused minute passed, which does not clear the reproduced defect.

Correction `4bc21af` is committed and pushed; Linux CI
[43943678663](https://github.com/maruthiprithivi/lessdb/actions/runs/43943678663)
succeeded. Artifact `14263359801` passed all 92 source-input, merge-parent and
reconstructed-chunk checks. Its verified identities are:

| Artifact | SHA-256 |
|---|---|
| Backend source | `449e1bc9da3a97141ea7ee4bb352ce7bdea54af2dbf195f71ec7dd6cadea9da0` |
| Linux binary (176,447,336 bytes) | `9fea632cac6909229b65370d909f699620a85d83e3933a0e21a4f98505c23edb` |
| Image index | `bba85f9fdabf1c2ce1008486863feb38ae70a4d25d79cf262fe6cd8335441ed5` |
| Linux platform manifest | `774d8a82980c73f0d35c8fa48603f75d677e8cf7d8eb511239f162c4ddb5c604` |
| Image config | `394386e0d43d6cc9918d8ad1450fa7707a4d2b2496319c3cb1cc7eaeb6673c46` |

The correction enables
`serde_json`'s `float_roundtrip` feature at the same 1.0.151 version. A regression
accumulates Life values for all 100 residents,
asserts the captured value's exact bits, and checks three persisted-World JSON
roundtrips against all need-value bits and the entire persisted value. Independent
review found no actionable issue. The focused precision regression passed (one test,
57 filtered, 0.32 seconds). The full native backend suite then passed: 57 passed,
zero failed and one ignored, 767.12 seconds of test execution (789.467 seconds for
the command), exit 0. The existing ignored `external_envelope_file_replays_into_lessdb`
requires simulator-generated JSONL. Evidence: `native-backend-tests-4bc21af.json`
and `.log`. No assertion or exact-state contract was relaxed; this was a JSON reload
precision defect, not evidence of weather-driven resident normalization.

The exact image above passed both fresh and copied-checkpoint phases at 256 MiB
with networking disabled and UID 10001. Paused step/restart preserved the complete
clock and all 100 resident snapshots: fresh minute 393→398; restored 585,197→585,202.
At the previously failing minute 585,202, all 99 captured `social_need` values stayed
exactly `15.350000000000005` after restart. Fresh/restored memory observations were
60.56/87.82 MiB; both exited 0 without OOM. The original archive remained unchanged,
and all owned containers/volumes were removed. No CPU probe ran; these results do
not qualify production `basic` performance. Evidence:
`linux-container-verification-4bc21af-43943678663.json` and
`qualified-image-provenance-4bc21af.json`, with all six before/after/reopened
snapshots retained. Corrective deployment and live health/checkpoint verification
remain pending.

The fresh pre-trial backup in
`cloudflare-checkpoint-before-second-rollout-20261116T193226Z.json` matched stable R2
metadata and ETag: written 2026-11-16 19:32:13.290 UTC, 363,711 bytes, SHA-256
`a2ac2070242f2c6db9e196fd51c576d556a9682e31486555951ab2842595fdbd`.
Its multirow archive does not establish an exact latest world minute. A separate
API sample showed minute 592,312 and the same 100 resident IDs; it was marked stale
by the Worker and is not proof of the archive's latest row. Prior archives remained
unchanged.

A later pre-corrective-rollout backup,
`cloudflare-checkpoint-before-corrective-rollout-20261116T200907Z.json`, matched stable
R2 metadata/ETag: written 20:07:11.994 UTC (modified 20:07:12.468), 431,445 bytes,
SHA-256 `52861aa445304555ecfd158b2b95c2c42b54bdf0726211957857f97c08e8136e`.
Eight earlier archives stayed unchanged. Its 65 rows have minute bounds 590,280–595,334;
these are not an exact latest-row query. The independent 20:09:19 stale-cache sample
showed minute 595,424 (Day 414 11:44), unpaused, with the same 100 resident IDs.
One live health request returned 200 in 7.099 seconds (Worker 6,963 ms), with 11
checkpoint uploads and zero reported failures. This later success does not erase
the repeated earlier 503s or establish sustained health; `basic` remains a capacity trial.

The refreshed Web pack is `525042e0…afe2f`; the operator verified its immutable
GET returned 200 with 15,563,640 bytes and the expected hash. The operator inspected
`weather-client-local.png` and reported consistent weather rendering with zero
browser console errors. The sandboxed full Godot run (`weather-client-check.log`)
failed two native fixture launches while the hair-ratio checks passed. The native
macOS rerun exited zero: `weather-client-check-native.log` records **388 checks with
four expected office-art failures**, followed by **16 passing weather checks**.
Two live-local API checks were skipped because `127.0.0.1:8790` was unreachable;
those two checks account for the earlier 390 count. Production API verification is
separate. Corrective deployment and live health/checkpoint verification remain pending. The full-day video predates this weather correction.
