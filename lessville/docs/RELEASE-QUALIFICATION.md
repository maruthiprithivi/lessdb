# Lessville release qualification — 2026-11-17

This candidate is intended for `maruthiprithivi/lessdb` and the existing
`https://ville.lessdb.dev` deployment. Source publication, a successful build and
an existing live URL are distinct from deploying and verifying this candidate.
Native restoration of a copied R2 checkpoint, the client checks and the exact
Linux release-image qualification have passed. Cloudflare completed the candidate
rollout at 2026-11-16 19:11:45.518 UTC. Initial live restore and population hydration
are observed; **final post-checkpoint verification remains pending**. The deployed
page references the candidate pack and the sampled immutable asset hashes match.
Rollout completion is not yet a claim that all production verification gates passed.

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
narrow-sidewalk head-on passing remain open. A fresh full-day native capture now
records this candidate; its sampled frames do not establish collision-free behavior.
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
includes the Godot check. Refreshed Linux/Web artifacts and a corrective rollout
remain pending; the first rollout is not the final candidate.

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
