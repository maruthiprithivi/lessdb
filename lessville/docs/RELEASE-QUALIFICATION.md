# Lessville release qualification — 2026-11-17

This candidate is intended for `maruthiprithivi/lessdb` and the existing
`https://ville.lessdb.dev` deployment. Source publication, a successful build and
an existing live URL are distinct from deploying and verifying this candidate.
Native restoration of a copied R2 checkpoint and the client checks have passed.
The exact Linux release-image qualification and Cloudflare rollout are **pending**.
The new static pack has been uploaded to an immutable R2 asset key but is not
activated. Its upload is not evidence of a Worker or Container rollout.
Update this record with actual results and identifiers when those steps finish;
do not infer success from the existing live URL.

## Portable commands and operator setup

With Rust 1.96 and Godot 4.7.2 installed, `npm run dev` starts the local native
world, `npm run check` runs the Godot checks, and `npm run build` or `npm run export`
exports the Web client. Web export also requires matching Godot export templates.
These commands no longer require Manus `GAME_RUNTIME` or a private sibling engine
checkout. Historical preview/doctor/asset-report commands still use the optional
Manus runtime; experimental robot/art tools can retain their original prerequisites.
The [Cloudflare guide](../deploy/cloudflare/README.md) covers the separate pinned
Linux build. `tools/prepare_cloudflare_web.py` stages local Web delivery artifacts;
staging artifacts is not a remote deployment.

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
narrow-sidewalk head-on passing remain open. No new full-day native video validates
this candidate, and backend passenger scheduling still does not follow visual
traffic delays.

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

## Evidence still required

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

`release-client-check.log` records 390 passing checks with four existing expected
office-art failures. The current Web export passed with Godot 4.7.2. Those results
do not validate the Linux binary or the deployed world.

The exact Linux image still needs its own copied-checkpoint result. Deployment
evidence must identify the committed source revision,
Worker version, image, uploaded asset manifest and post-rollout health/state checks.
Check actual resumed identities and time; preserve R2 history. Until those records
exist, deployment and live restore remain pending.
