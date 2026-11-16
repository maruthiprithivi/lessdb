# Multiplayer game services

Read this when the user requests online multiplayer; single-player templates stay offline by default. This page owns
the shared service workflow, and the selected game's guide or README owns its messages, rules and tests.
Project-local `docs/game-multiplayer.md` and README-backed Skills may be older snapshots; follow this installed page
for cloud-computer selection, updates and responsive controls. A cloud computer selected in Multiplayer is a
deployment target, not an already running game server.

## Read the runnable source before adapting

For an existing offline game gaining multiplayer, keep its project and gameplay. The **Multiplayer reference source**
directory returned by init/attach holds the current Generic Multiplayer starter whatever this project's genre: read
its `scripts/net/connection.gd`, `scripts/main.gd`, `scripts/net/protocol.gd` and `server/authoritative_server.mjs`
in one parallel batch before writing the integration. Copy the connection component into the game's own source and
adapt the protocol, room adapter, server rules and UI together; never reinitialize the project or edit the installed
reference, and keep an existing working networking implementation. The Generic Multiplayer guide's **Reusable
connection component** section owns the API and cancellation/retry behavior; read its tests when changing that.

## Read and update the project's cloud computer

The project's saved multiplayer selection is the authoritative deployment target. Read and update it through
`webdev.config` (the Multiplayer panel uses the same record), never from the active device, an earlier message or a
local file.

To propose computers, use the built-in `manus-device.list_devices` and show each name, ID and hosting `region_id`
with a recognizable location (for example Singapore, `asia-southeast1`). A missing `region_id` is unknown; never
infer it from names or IDs. Suggest nearby regions from the requester's location without claiming measured latency.

**Existing Game project:** first read the current selection:

```json
{"method":"GET","path":"game/multiplayer"}
```

The response has `data.resourceId`, `data.selection.pcId`, `data.selection.revision` and `data.selectedServer`
(safe details and status, or null). `pcId: null` means nothing is selected; a non-null `pcId` with a null or
unavailable server needs attention but is still selected. Computer readiness does not prove the game service runs.

Reuse the saved computer unless the user asks for a change; with no selection, list available computers and ask.
An explicit choice — a Multiplayer button message, a natural-language choice, or a device ID from **Switch to another
device** that the message says to use for this game's multiplayer — needs no repeat in the panel. A request merely
to switch execution devices does not change the selection. Save with the exact `pcId` and the revision just read;
for example, **only if GET returned revision 3**:

```json
{"method":"PUT","path":"game/multiplayer","body":{"pcId":"USER_SELECTED_COMPUTER_ID","expectedRevision":3}}
```

Report the selection as saved only after `ok: true`; the panel refreshes from the server event, so make no second
UI write. After a lost response or a 409, GET again: if it already matches the intended computer it is saved;
otherwise resolve the conflicting choice first, never blindly retrying with a newer revision. To unlink on request,
GET then PUT `pcId: null` with the current revision. Unlinking or changing the selection does not stop, migrate or
delete an existing service or computer.

**No Game project yet:** explain that online multiplayer needs a cloud computer, list available computers (listing
needs no switch) and confirm the choice before initialization. If none is available, direct the user to
[buy a cloud computer](https://manus.im/app#settings/my-computer/create) and refresh after startup; never purchase
one for them. Right after init, GET and PUT the chosen computer into the project before implementing or deploying
multiplayer. Do not initialize just to expose the panel.

These calls need the owner's session on a ready Game project: save before switching to the deployment computer, or
switch back to the development environment to save, never copying credentials or calling the service from a shell.
Recheck the selection before deployment. A saved selection does not grant remote-control authorization; complete the
normal device access flow and stop at a real permission denial.

## Cloud computer unavailable or device switch fails

If connecting or switching to the selected computer fails, times out or hangs, call `manus-device.list_devices`,
match `device_id` to the saved `pcId`, then call `manus-device.get_environment_status` with that record's exact
`environment_id`; both work from the current environment. Read CPU (a 0–1 ratio) and memory (bytes) from the status
response; readings with unknown sampling time may be outdated, and missing or stale metrics prove neither health nor
exhaustion. If the tools are unavailable, report that instead of inventing readings. If the computer stays unresponsive with CPU near 1 or memory near capacity, explain the readings and ask
the user to reset that computer in Manus Settings, warning that reset interrupts running work, and stop deployment
and repeated switch attempts. A slow command or a high reading on a responsive computer does not justify a reset.
After a reset, list devices, query status and check the service before resuming. Without evidence of exhaustion,
investigate availability or access errors instead.

## Client and server responsibilities

Keep the service source and lockfile with the project; static Web publication does not start a WebSocket server.

The server owns rooms, seats, simulation, legal actions, scores and round transitions. Clients send bounded input and
treat accepted snapshots as authority; client coordinates, elapsed time and scores are never authoritative. Validate
message type, shape, size, rate, room membership and the socket's current seat before mutation. Use a protocol/build
identifier and increasing snapshot or action revisions: reject incompatible clients, ignore stale snapshots and make
retried actions idempotent where needed. Define capacity, spectators, disconnect grace, expiry and restart behavior.
Resume credentials are server-issued, room-scoped secrets kept out of invitations, logs and source; tab-scoped storage
keeps two players from sharing a seat, and a replaced socket loses input authority at once. Lost storage, expired
rooms and in-memory restarts need recovery UI, not promises of durable rooms. Bound rooms, connections, queued bytes
and messages, with heartbeat cleanup. Room codes are invitations, not authentication.

## Server runtime selection

Default to a standalone socket server without Godot, normally Node.js with `ws`, following the matching template's
service and protocol. It still owns authoritative rules and validation; it is not a relay for client positions or
scores. Godot stays on the client, including when adding multiplayer to Generic or another offline template;
GDScript gameplay alone is no reason for server-side Godot.
Consider headless Godot only when reusing engine-dependent physics, navigation or scene simulation brings a material
benefit; record it, the deployment budget and the tradeoff in the plan. `--headless` still runs the engine and its
threads: build the server export in the development environment, set explicit CPU, memory, thread and room limits on
the cloud computer, and verify concurrent play and control responsiveness before claiming capacity.

## Responsive controls with minimal client logic

Never wait for a server round trip to show the player's movement or action start. For simple movement such as Pong
paddles or a free-moving character, use **local anticipation with smooth correction**: it needs only the latest
snapshot and a local movement state kept separate from authoritative state, not input history or rollback.

1. **Move locally on input.** Each local step, advance only the owned object with the current input, matching the
   server's speed and bounds, and render it immediately. Send bounded input on the network schedule; neither loop
   waits for a reply.
2. **Keep the server in charge.** Remote players and shared objects stay on the snapshot path. Play action-start
   animation or sound locally, but confirm damage, pickups, scores and round results from the server. Cards and turns
   show a pending placement restored on rejection; never guess hidden or random results or replay an effect on
   confirmation.
3. **Correct toward a comparable time.** Project the snapshot's authoritative position forward by its velocity and
   estimated age (initially capped at 150 ms, clamped to the movement bounds), estimating age from a clock mapping or
   half a smoothed RTT plus time since receipt; never subtract unrelated clocks. Add velocity and timing fields to
   both protocol ends if absent, and do not extrapolate through unknown collisions or transitions.
4. **Blend small errors; reset exceptional states.** After applying local input, move toward the target with a
   frame-rate-independent weight such as `1 - exp(-dt / 0.10)` and a small dead zone. Apply teleports, respawns,
   rejections and large divergences immediately. Ignore stale snapshots; stop anticipation on disconnect, seat loss
   or a non-playing phase; reset from a fresh snapshot after reconnect; show reconnecting instead of predicting
   indefinitely.

For Pong, anticipate **only your own paddle**; the opponent, ball collisions and scoring stay authoritative. The
current Generic Multiplayer starter implements this in `scripts/game/local_paddle.gd`. Older snapshot-only Pong
projects need a client and service adaptation, not just a smoothing constant. The approximation still shows corrections under rapid
reversals or high latency; precise, collision-heavy or competitive games need input acknowledgement and replay or an
established networking solution, not an ad hoc rollback engine.

Accept with two clients under simulated 100–200 ms RTT and jitter: prompt start, stop and reversal, sane corrections
and reconnects, convergence after input stops, matching scores and single effects. Localhost alone proves nothing
about internet play.

## Configure the public connection

Addon Game projects use the existing `multiplayer/config.json` and same-origin `multiplayer/bootstrap.json`
discovery. `signalingUrl` also names a dedicated authoritative WebSocket server; it does not imply peer-to-peer:

```json
{"build":"GAME_PROTOCOL_VERSION","signalingUrl":"wss://VERIFIED_GAME_HOST/ws","iceTransportPolicy":"all"}
```

Use the real build identifier and verified endpoint; direct WebSocket games use no ICE/TURN. Never put TURN secrets,
device credentials, resume tokens or private keys in this public file. Service and client share the build. Preview and release preparation generate the discovery descriptor;
never hand-edit generated site/, dist/, a checkpoint or the installed runtime. Resolve the descriptor relative to the
actual game page, keeping the Preview ingress prefix. The endpoint is operator-owned: invitations select rooms, never
endpoints. HTTPS pages require WSS, loopback WS is only for explicit local tests, and missing configuration shows an
actionable unavailable state, never a simulated match. Native tests supply their endpoint through the template's
native configuration, which does not configure a publication.

## Prepare and run the service

Inspect the chosen computer's runtime, ports, existing services and proxy first. Use a new project-specific
directory, service name and free port, binding loopback behind an existing reverse proxy; a remote or public bind
needs the intended authenticated TLS network setup. Never overwrite an unrelated service or proxy. Build the client in the development environment, transfer only the frozen server source,
manifest and lockfile, and install from the lockfile with the service's own package manager.
Run the server once and check its health route, then make it persistent with the OS's existing service manager (on
Linux, a project-specific user systemd unit whose status and journal you check). An isolated device terminal may need
the device tool's approved host access to reach that user manager; never become root or disable the sandbox, and
verify linger before claiming the service survives reboot.

## HTTPS, WSS and client publication

Reuse the computer's existing HTTPS reverse proxy where available, checking that it forwards the `/ws` upgrade with
its query string, allows long-lived connections and routes health correctly. Healthy loopback does not prove a public
endpoint, and public HTTP health does not prove a WSS upgrade. Normally the client is served from the published static
origin and the service from the computer's WSS origin; set the service's Origin allowlist to the actual client origins
(the current Preview only when testing it); Origin checks complement room authentication, and HTTP CORS headers do not
authorize a WebSocket connection. An explicitly chosen combined deployment serves the complete frozen
Web release with every content-hashed WASM/PCK, keeps private source and config outside the static root, prevents
directory traversal and symlink escape, and is not the Addon's published checkpoint.

## Verify, update and troubleshoot

Test in order: protocol and simulation checks, two clients against an isolated local server, native import and the
current Web export, cloud loopback health, public HTTPS and WSS, then targeted player acceptance on the real client
origin. Cover seat ownership, scores, spectators, reconnect, expired credentials, malformed input and room isolation.

| Symptom | Check |
| --- | --- |
| Cannot reach or switch to the cloud computer | [Device diagnostics](#cloud-computer-unavailable-or-device-switch-fails) |
| Public 502 | Exact service status, listening port and proxy upstream |
| Healthy HTTP, failed WSS | Upgrade path, TLS, Origin policy and protocol version |
| One player waits | Same room/build/endpoint, second client connected, reserved seats |
| Refresh changes seat | Tab storage availability, resume grace and server restart |
| Scores disagree | Client-side simulation, stale snapshots, room identity |
| Godot fails to load | Current WASM/PCK hashes, paths and MIME types |

Updates deploy a validated new version beside the previous one, switch only this service and repeat health and WSS
checks; an in-memory restart drops rooms and scores. Never publish a temporary test endpoint as permanent hosting.
Deliver the client URL, service and health endpoints, version, restart behavior and the checks actually completed.
