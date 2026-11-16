# Lessville: LessDB Agent World, delivery report

> **Current status — 2026-11-17:** The corrective rollout passed its final live
> health, continuity and checkpoint gates and retains one `basic` Container. See
> [release qualification](RELEASE-QUALIFICATION.md) for current evidence and costs
> (about $5.22/month extra memory/disk versus `lite`; active CPU additional).
> The older `lite` budgets and delivery evidence below are retained as history,
> not the current deployment estimate. Early capacity headroom is not endurance proof.

**Checkpoint:** backend source `f9b1810`, release/operations checkpoint `2887dcb`. The source checkpoint is published to the canonical project remote. The official MicroDuck robotd adapter and visual twin are local/research tooling and have **not** been rolled into the public `ville.lessdb.dev` container.
**Project:** `<project-root>` (Godot 4 Web client) and `<project-root>/backend/agentd` (Rust agent service built against the qualified LessDB source).

## What it is

Lessville is a 2D pixel town with five primary agents and a lightweight background population:

- **Maya**, the café owner
- **Bo**, the barista
- **Lin**, a student and café regular
- **Sam**, an office worker
- **Ama**, Sam's mother, with the corgi Mochi

Eight background residents now follow bounded town routes and occupations. They are
visual world population, not hidden agent accounts or claims of autonomous reasoning.

They run on a shared, persisted Asia/Singapore clock that you can pause, step and speed up (×1/×2/×4/×8). Every memory, message, scope membership, access check and decision is an SQL statement executed by an **embedded LessDB engine** (`less-engine` + `less-query`). The service resumes from the database after a restart; I confirmed it logged "Resumed Day 3 17:50 from LessDB (29 memories, 25 messages on disk)".

| Area | What you can do |
| --- | --- |
| Town | Watch primary agents and background residents walk along bounded paths. Seeded weather changes affect readings and behaviour, a named bus runs finite approach/dwell/depart phases, and day/night lighting plus speech/memory bubbles remain visible. Click a building to open its interior close-up. |
| Story | Read the conversation, memory and decision feed, using rows read back from LessDB. Each chip links to the row it came from. |
| Mind | See what one agent can recall, how many rows the scope rule hides from them, the exact recall SQL, and their recent decisions. |
| Provenance modal | For any memory: the source row, who may read it, and the decisions that later used it. Each part shows its SQL. |
| LessDB live | The latest SQL statements the agents ran, with timing and status. |
| Truth | Separates native LessDB guarantees from rules this app adds, and lists open work. |
| Nudges | Start a shower, delay the next bus, empty the oat milk, use the last eggs, revoke Lin's consent. Agents only learn about a nudge if they experience it or someone tells them. |

## Behaviour verified from stored rows (Day 2)

- Lin took the **07:40 bus** because of temporal memory #10 ("The 08:00 bus ran 15 min late").
- Bo **started Lin's usual before she ordered**, from shared memory #12 in the `cafe-regulars` scope. Lin had shared it only after Maya asked her consent.
- Ama **shopped without eggs** because Sam's message had already claimed the errand.
- Ama **walked Mochi in the park, not on the trail**, after Bo recorded that the trail was muddy (memory #16, still within its relevance window).
- The scope filter **hid 4 of Lin's private memories from Bo**. Each hide was logged as an access row.

## Validation

- `npm run check`: passes. That includes 58 Lessville checks: assets, path routing, state rendering, panels, and a live-service run.
- `npm run check -- --pack`: passes on the release candidate.
- Browser check of the preview: title screen, town, Mind panel, provenance modal, café interior, bus, rain and clicks all worked.
- Checkpoint readback: `preparation: ready`, `gamePublishReady: true`. A favicon and a 1200×630 cover are registered.

## Honest limits

1. **The public runtime is split deliberately.** The Worker serves the static Godot game at `ville.lessdb.dev`, while one lite Cloudflare Container runs `lessville-agentd` with embedded LessDB. Public API access is observer-only; the local config remains a development fallback.
2. **Agent policies are deterministic, versioned rules, not language-model reasoning.** The UI labels them that way. No model budget was used.
3. **Scope and permission enforcement is an application-layer SQL predicate**, not native LessDB access control or tenant isolation.
4. The service uses the qualified public-source LessDB candidate (b32e600 plus the reviewed WAL/stability/ownership patches) as a **single local owner**. It is not certified for distributed or mesh use, full ACID, power loss or capacity.
5. **Still open, and labelled that way:** richer relationship modelling, competency transfer, multi-step dialogue, user-facing retention/forgetting workflows, schema evolution and native access-control semantics. Identity-thread continuity and bounded competency-state events now have explicit application-owned state and provenance; that is not a native LessDB guarantee.
6. A few rows from earlier testing contained an emoji. The world was reset afterwards, so current rows are clean.

SC-01 scoped café context and SC-02 lifecycle replay now execute against a
fresh embedded LessDB store through the real context broker. They are
test-only fixtures, make no model call and do not touch production.

### Realism slice — 2026-10-11

- Weather is now a persisted, seeded world condition with explicit condition,
  temperature, wind and change time. Rain/wet conditions update bounded agent
  behaviour and background-citizen mood; the result is deterministic for replay,
  not an untraceable random source.
- Sam has a stored Day-2 occupation transition to **Operations analyst**, with
  an event, permanent memory and occupation-history record. This is an example
  evolution rule, not a general employment system.
- Eight background residents have stable identities, occupations, routes,
  targets and activities. They are rendered as secondary silhouettes so the
  world has population without conflating every person with a LessDB agent.
- The bus snapshot now exposes an authoritative finite `active` trip with a
  trip ID and `dwell`/`depart` phase. The client no longer revives a stale
  arrival row or loops a bus from an old trip.
- The client updates facing before consuming short path segments, preventing
  fast polling from making a character visibly walk backward.

### World dynamics and speed validation — 2026-10-12

- The authoritative snapshot now includes six non-primary traffic actors: cars,
  bicycles and a delivery van. Each has a bounded route position, lane, direction
  and phase. Same-lane following yields to a safe gap; endpoints turn at a named
  junction instead of teleporting or restarting a bus-like loop.
- LessVille can optionally poll Open-Meteo once per hour through the container's
  preinstalled `curl`, with an eight-second timeout. The current reading is stored
  with `weather_code`, observed time and `weather_source`; failed polls retain the
  seeded deterministic weather. The UI labels the source as live or fallback.
  This is not an uptime or production-weather guarantee. Attribution and limits are
  recorded in `OPEN-METEO-INTEGRATION-NOTES.md`.
- The fresh executable passed `npm run scenario:stability`: x4 and x8 runs each
  produced 27 samples, with monotonic logical time, eight stable background
  residents, unique traffic identities, bounded route positions and the explicit
  `seeded-fallback` source. The harness now allocates free ports and fails if its
  child service exits, preventing stale-service false positives.

The local slice was release-gated and rolled into the public `ville.lessdb.dev`
runtime on 2026-10-12. The SC-04 fixture and embodied/security injection
harnesses remain private test/staging tools; they do not add public mutation
routes.

### Security and traffic hardening — 2026-10-12

- The authoritative world now persists two named traffic signals with a
  deterministic green/amber/red cycle. Vehicles stop before the main junction
  during red or amber, and the Godot view renders the backend phase rather than
  inventing a client-only signal.
- SC-04 is executable: a hostile payload is visible only inside its granted
  shared-context scope, while an ungranted swarm and unrelated agents cannot
  recall it and no automatic cross-scope copy is created.
- The traffic-chaos harness passed x1/x4/x8 checks for 27 samples, six traffic
  actors and eight background residents, including stable identities, bounded
  routes, same-lane separation and no red/amber crossings.
- Proposal-only embodied injection passed for Reachy Mini and MicroDuck in
  private staging. Duplicate observations remained duplicate-safe, proposal
  reconciliations were denied, and actuator flags stayed false. These are
  staging adapter tests, not robot safety or production deployment.
- A bounded security observation capture showed deterministic trust/quadrant
  projections and a blocked compound-path verdict. Its short window did not
  add events, so no new self-learned pattern is claimed.

The detailed evidence is in `docs/SECURITY-CHAOS-VERIFICATION-2026-10-12.md`.
The source and staging harnesses remain separate from production and must pass
the release gate before any rollout.

### Production rollout — 2026-10-12

- Source checkpoint `8640f5d` was built as an optimized release binary and
  deployed in the existing non-root `lite` container. The deployed binary
  hash is recorded in `deploy/cloudflare/container/release-marker.txt`.
- The refreshed Godot PCK is served from the content-addressed R2 path with
  SHA-256 `57abb5ab939ca8429d9079f28400814248224e798bb3461ce3d8188751cbfcf1`;
  the engine WASM remains `fc74679e3b97f76878947fcd4fbe1268cbfa6188182a2e33bbc3f5dc9bfa57d0`.
  Both live routes returned HTTP 200 with immutable caching.
- Post-rollout `/api/state` exposed two authoritative traffic lights, six
  traffic actors and eight background residents. Health returned `public=true`,
  checkpoint restore from R2 and zero checkpoint failures at verification time.
- `POST /api/control`, `POST /api/security/step` and `POST /api/physical/replay`
  each remained HTTP 403, preserving observer-only public operation.
- The deployed surface does not include the Reachy Mini, MicroDuck or Security
  Lab staging servers. Those remain temporary observer/research environments.

### Approval-gated policy proposal rollout — 2026-10-12

- Release commit `d368913` (backend source checkpoint `81cd481`) deployed the incubating
  `lessdb-policy-proposal/v1` contract and deterministic recurring-pattern
  aggregation in the existing optimized backend. The release marker pins the
  binary hash and records `auto_apply=false` semantics.
- A fresh private replay verified that two repeated blocked compound-path
  assessments create one candidate with two source references,
  `requires_approval=true`, `state=Candidate` and unchanged guardrails.
- The live read-only `/api/security/state` schema now includes
  `policy_proposals`. Its public list is empty by design because public
  security mutation routes remain blocked; the production surface cannot
  manufacture a proposal or apply one.
- Live verification retained two traffic lights, six traffic actors and eight
  background residents. No static game asset changed in this rollout.

### Agent-session adapter rollout — 2026-10-18

- The optimized backend built from source checkpoint `de3814c` was deployed
  to the existing non-root `lite` container. The release marker records binary
  SHA-256 `44df04f4545143cfe45b9dc47ee3f83b2a1a34562b704296821113100a66ff1b`.
- The live read-only `GET /api/agent/session/catalog` returned HTTP 200 with
  schema `lessdb-agent-session-catalog/v1`, and listed catalog, compose,
  explain, propose, simulate and activate as typed adapter operations. Its
  boundary states that proposals and simulations remain owner-reviewed
  application operations above embedded LessDB.
- Public `POST /api/agent/session/compose`, `/propose` and `/simulate` each
  returned HTTP 403. No public activation, actuator or automatic schema
  change was introduced.
- Live health returned `public=true`, checkpointing enabled with zero
  failures, and the configured Workers AI cap remained 9,500 neurons/day.
  State exposed two authoritative traffic lights, six traffic actors, eight
  background residents and the read-only `research_frontier` projection.
- The nine executable LessVille adoption scenarios passed locally, including
  agent-session, teaching/counterfactual and bounded-emergence replays. The
  production binary also passed physical-adapter, traffic-chaos, world-
  stability, Godot parser/project and typed LessDB contract gates.

### Character competency and reciprocity rollout — 2026-10-18

- The optimized backend built from source checkpoint `084d44a` passed 29 backend
  tests with one external simulator-file test ignored by design, all ten
  adoption scenarios, physical-adapter, traffic-chaos, x4/x8 world-stability,
  Godot parser/project and Python tooling gates. The binary SHA-256 is
  `54a34aa130bef1eb2dd8a500213493a577267faa3e29daa82fe9a27ff677536e`.
- The production container now serves the read-only agent detail fields
  `competencies` and `interactions`, and the public session catalog lists
  `lessdb-competency/v1` and `lessdb-social-interaction/v1`. Live state retained
  the restored world clock, two traffic lights, six traffic actors and eight
  background residents.
- A versioned singleton rebind was required because the prior Durable Object
  container instance continued serving its startup image after the image
  rollout. The new `town-character-v1` instance restored from the existing R2
  checkpoint. A temporary two-instance overlap was used only for handover and
  the production configuration is back to one `lite` instance.
- Live health reported `public=true`, a 9,500-neuron budget cap, zero checkpoint
  failures and `/api/budget` reported the same 9,500 cap. Public `POST` control,
  compose, propose and simulate routes each remained HTTP 403. The competency
  and interaction records remain application/incubation projections with
  `auto_apply=false`; no native permission, global trust or actuator authority
  is claimed.

### Public research map — 2026-10-12

- `https://ville.lessdb.dev/simulations` is now the stable visitor directory
  for the companion research surfaces.
- The production landing/loading shell links to that directory so visitors can
  discover the Security Lab, the unified Reachy Mini/MicroDuck Embodied Lab,
  the official-model MicroDuck robotd view and the four-body MicroDuck swarm.
- The cards link to the currently healthy temporary public staging services;
  they are not proxied into the production container. The page labels them as
  observer-only staging and states that simulator policies are deterministic or
  simulator-backed, not physical robot control or native security guarantees.
- This mapping adds no public mutation route, no actuator route and no new
  model/provider budget. Because the staging services are session-hosted, the
  links may be unavailable if their staging session is stopped; a durable
  always-on embodied deployment remains a separate infrastructure decision.

## How to run it locally (sandbox)

```sh
LESSVILLE_DATA=/data LESSVILLE_PORT=8790 \
  <lessdb-target>/release/lessville-agentd
cd <project-root> && npm run dev   # then npm run preview:build
```

## Cloudflare deployment — ville.lessdb.dev (2026-10-08)

| Piece | What runs | Verified live |
|---|---|---|
| Worker `lessville` | Serves the static game, proxies read-only `/api/*`, rate-limits 400 req/min per IP, adds security and same-origin isolation headers | Site 200; `POST /api/control` 403; `/internal/*` 404 without key; headers present |
| Game binaries | 39.5 MB engine wasm and 11.8 MB pack are above the static-asset limit, so they live in R2 (`lessville-data/lessville/game/sha256/<hash>`) and are served immutably at the exact paths the verified release references | SHA-256 round-trip match; `application/wasm`, immutable cache |
| Container `lessville-lessvilleagent` | One `lite` instance of the LessDB agent service (non-root uid 10001, COPY-only image on Ubuntu 24.04) | Health 200, public mode on |
| Durable storage | Flushed, atomic LessDB checkpoint to R2 every 300 s through a key-protected Worker route; restore-before-open refuses to reseed on a restore error | Live rollout restored 92.6 KB from R2; the first new five-minute upload completed with zero failures |
| Model calls | Workers AI `@cf/meta/llama-3.1-8b-instruct-fp8-fast`, two single-purpose tasks (`phrase`, `reflect`), ~150 input + ~22 output tokens, ~1.4 neurons per call, global cap 9,500 neurons/day enforced in a Durable Object; above the cap agents fall back to rule-based lines | 25 accepted, 1 rejected by validation, ~36 neurons used at check time |
| Keep-alive | Cron every 5 min | Deployed |

Model-quality fixes from reviewing real live outputs: typographic dashes and non-breaking spaces are normalised before validation; phrase prompts treat memories as tone-only background; temperature is 0.3 for phrasing and 0.5 for reflection.

Operational notes:
- Redeploy: `cd deploy/cloudflare && cp <release binary> container/ && npx wrangler deploy --containers-rollout immediate`. Running instances roll over gracefully (about 4 minutes here) and restore from R2.
- `INTERNAL_KEY` was generated once, attached with `--secrets-file`, and the file was shredded. It is not stored anywhere outside Cloudflare; rotate by deploying a new value the same way.
- Up to about 300 s of world progress can be lost on an unplanned container loss (checkpoint interval).
- Public release v2.1.0 removes the title-screen nudges instruction; observer mode remains watch-only.


## Cost guardrail and archive policy — 2026-10-08

The public deployment is constrained to a **$10/month Lessville infrastructure budget**.

- The container is now Cloudflare **`lite`**: 1/16 vCPU, 256 MiB memory and 2 GB disk.
- The container sleeps after 10 minutes of inactivity; the existing five-minute keep-alive preserves the shared town while traffic is active, while still giving the platform a sleep fallback.
- Checkpoints run every **300 seconds** instead of every 60 seconds. `latest.tar.gz` remains the restore source.
- R2 keeps the newest **168 hourly checkpoints (seven days)** under `lessville/v1/hourly/`; older hourly objects are deleted by the Worker. R2 `DeleteObject` is free, and the current archive remains in R2 rather than on container disk.
- The configured Workers AI limit remains **9,500 neurons/day**, below the documented 10,000-neuron daily free allocation.

Using Cloudflare's published rates as a conservative 30-day upper bound for a continuously provisioned lite container: CPU is about `$2.79`, memory about `$1.40`, and disk about `$0.31`; with the `$5` Workers Paid base, the estimated ceiling is about **$9.50/month before unrelated account usage**. R2 remains within its documented free tier while storage stays below 10 GB-month and operations below the included monthly limits. This is a guardrail and estimate, not a provider billing guarantee; Cloudflare account usage should still be monitored.

Sources: [Workers pricing](https://developers.cloudflare.com/workers/platform/pricing/), [Containers pricing](https://developers.cloudflare.com/containers/platform/pricing/), [R2 pricing](https://developers.cloudflare.com/r2/pricing/), and [Workers AI pricing](https://developers.cloudflare.com/workers-ai/platform/pricing/).

### Live guardrail evidence — 2026-10-11

The current public health check reported `public=true`, Workers AI `2,173` accepted calls, `348` rejected outputs, `0` failed provider calls, `1,259` skipped/fallback calls and `966.95` reported neurons against the `9,500` daily cap (about `10.2%` at check time). Checkpointing was enabled at `300` seconds, restored from R2 successfully, reported `488` uploads and `0` failures, and reported a latest successful archive of `494,280` bytes. These are service health counters, not a Cloudflare invoice.

The account list API was available, but billing profile/history/usage/pay-as-you-go endpoints returned `403 Authentication error` for the configured token. An R2 S3 aggregate listing returned `InvalidArgument`/HTTP 400 in this session, so no current R2 object-byte total is claimed here. The actual billed amount therefore remains unverified; only the published-rate guardrail estimate above is supported.


### Embodied and security staging — 2026-10-12

The physical simulations are now in a clearer state than the original visual-twin-only presentation:

- Reachy Mini has a local official `reachy-mini-daemon --sim --headless` session. The new daemon-backed visual reads SDK pose state and mirrors it onto the official MuJoCo model. It is available through temporary staging only.
- MicroDuck has three distinct staging sources: a four-body official-model swarm visual with LessDB scope/proposal telemetry, the official `robotd --sim` camera stream, and the official-model operator view driven by robotd body readback. They are shown together but are not falsely presented as one merged physics engine or four independent robotd control loops.
- The unified staging hub was re-run with all four sources live and the shared frame `lessville:embodied-district/v1`; the page reported Reachy, swarm, camera and operator sources as healthy and exposed LessDB physical analytics.
- The new Security Lab models six named personas, four knowledge quadrants, prompt-injection content, compound private-data/outbound risk, explicit guardrail verdicts and an evolving trust loop. Each step writes application-owned `lv_events`, `lv_decisions` and a `security.world` snapshot to embedded LessDB. It is a local/staging research simulation, not a security certification.

The Reachy Mini, MicroDuck and Security Lab experimental surfaces are not rolled
into the public `ville.lessdb.dev` container. The traffic-light/world-hardening
slice is deployed, while public production remains observer-only with its
existing budget, checkpoint, model-cap and access guardrails.


### Identity continuity and competency-state rollout — 2026-10-19

- The optimized backend built from source checkpoint `f9b1810` passed 30 release-mode backend tests with one external simulator-file test ignored by design. Traffic chaos passed x1/x4/x8 with 15 samples; world stability passed at x4/x8; the physical adapter, Rust formatting, Godot parser/project checks and Python tooling checks passed. The deployed binary SHA-256 is `72b7eaa5a98b74f2a04e4c2419d6050a0dad5f8ee6b2e4958b9697abe6853a8d`.
- The public session catalog now exposes `lessdb-identity-thread/v1` and `lessdb-competency-state/v1`. Live `GET /api/agent/maya` reports one identity-root record and zero competency-state events for that selected character at verification time; the fields and counts are read-only projections.
- A stale durable instance required a controlled `town-character-v2` rebind. A temporary two-lite-instance overlap was used only for handover; production is restored to one lite instance with the existing 300-second checkpoint, seven-day archive and 9,500-neuron guardrails. The new instance restored the shared world from R2.
- The refreshed Godot Web export includes the continuity and competency-state Mind panels. Its content-addressed PCK `cef31937d412c684d1581ea0045fa3bdf44d1ca94d83b1c6c635fc00f0b2ceb4` is live from R2; the root HTML references it and the immutable route returned HTTP 200.
- Final live checks returned HTTP 200 for health, catalog, agent detail and state; state retained two traffic lights; public control and physical replay writes remained HTTP 403. The contract and UI remain application/incubation projections, not native LessDB ACL, consciousness, automatic policy change or physical authorization.

## Incident 2026-11-14 — town API outage from unbounded hot-table growth (recovered)
- **Symptom:** from about 2026-11-10 the single lite (256 MiB, 1/16 vCPU) container logged out-of-memory and lost-connectivity errors; median town API latency rose from ~100 s (2026-11-07) to ~800 s against the 900 s ceiling, so public requests timed out. The static page stayed up.
- **Root cause:** the backend has no retention for its working tables. After ~4 weeks the checkpoint held roughly 150k memories, 186k events, 144k decisions and ~103k novelty events / evolution proposals; per-tick queries over these tables exhausted the instance. The "168-hour archive" guardrail bounded only checkpoint *archives* in R2, never live rows. Restarts reloaded the same oversized checkpoint.
- **Recovery (no new binary):** the Worker checkpoint prefix moved from `lessville/v1/` to `lessville/v2/` and the town Durable Object was rebound (`town-fresh-2026-11-14`). The complete v1 history (169 checkpoints, ~2.3 GB) is preserved read-only in R2 and nothing was deleted. The already-deployed image digest was reused through a temporary untracked config, with a bounded two-instance overlap that was then returned to `max_instances=1`. Verified: health 200, fresh Day 1 world, five residents, two traffic lights, public mode, `/api/control` POST 403, first v2 `latest` + hourly checkpoint written.
- **Open:** the permanent fix is bounded hot-table retention/compaction (and bounded source-reference lists) in the backend. Until it ships the fresh world will slow down again over roughly 2–3 weeks.

## 2026-11-14 — retention fix on reconstructed LessDB candidate (deployed)

Root cause of the outage: append-mostly hot tables (and one unbounded competency evidence list) grew with world age until the single lite instance stalled. Fix: nightly bounded compaction (`lessville-retention/v1`, 3–7 day windows, permanent memories capped at 160 per owner; older evidence remains only in R2 checkpoint history) and a 24-ref cap on per-row evidence lists. Evidence: engine-backed scenario test; 5-night soak with flat per-step p95 (0.112s → 0.129s), no retention failures; backend 33 passed/1 ignored; traffic chaos x1/x4/x8 (15 samples), world stability x4/x8, scenario 12/12, physical, Godot parse and Python all passed.

The local-only LessDB candidate was lost in the sandbox reset. The backend is now built on a reconstructed candidate (private archive 84ae452 + rebuilt `less-agent`, private branch commit 0367d81). It is freshly re-tested, not the earlier 124-test qualified build. Its engine fails closed on the old checkpoint format, so production starts a fresh town at `lessville/v3/`; v1 and v2 checkpoints are untouched. Live: source ffa61ea, binary sha256 005bc977c775d5c0896c093e980867963adc838d5a4cfa647730219c4b24e6d6, one lite instance after a brief two-instance overlap, first v3 checkpoint written, public mutations 403.

Not yet restored: the pre-reset uncommitted situated-presence, Singapore climate and traffic.v2 work was lost and is being rebuilt next.
## 2026-11-14 — situated presence release (deployed)
- Backend source checkpoint `ca18484` (deterministic `presence/v1`): a phone, leash/Mochi, umbrella or carried item is shown only when an authoritative, replayable situation rule justifies it (e.g. a leash only on an outdoor Mochi walk, a backpack set down on arrival, a drink finished, no phone while serving). Each change is appended to `lv_presence_events` with a reason code. Hands-free character frames are derived reproducibly by `tools/derive_handsfree_sprites.py`. This is rule-based situational state, not perception or awareness.
- Release binary sha256 prefix `61042e579847` (full hash in `deploy/cloudflare/container/release-marker.txt`). It restored the live v3 checkpoint locally (presence fields and table present; public control 403) before rollout.
- Delivery change: single large image-layer and R2 uploads from the build sandbox were repeatedly cut off. The container now ships the exact binary as 24 ordered layer chunks. `start.sh` reassembles them and refuses to start unless the sha256 matches the release marker (tested with a tampered marker: exit 1). The Worker can serve a content-addressed PCK/WASM assembled from R2 parts plus a manifest, but only if the joined bytes hash to the sha256 in the path; otherwise it returns 502. Single objects still take precedence.
- Client PCK `7348f35277a16cc478010e900270a7f054f8a70e152547e6f65a4ea220af17ac` (13,641,992 bytes, WASM unchanged) is served from six R2 parts. The production download re-hashed identically twice, and the root HTML references it. Fixed a duplicated "simulations" link: the mirrored static HTML already contained the link the Worker injects, so injection is now idempotent.
- Live checks: root and health 200; state exposes `presence` on every agent with `deterministic:presence/v1`; `lv_presence_events` is present; public `/api/control`, `/api/physical/replay` and `/api/nudge` POSTs return 403; one lite instance. The browser loaded the new pack and connected (the first poll briefly showed the offline banner). Visual verification only covers the views captured; it is not full visual acceptance.
- Still open (lost in the reset and not yet rebuilt): Singapore monsoon climate model, traffic.v2 (multi-vehicle buses, cars, bicycles, signal queueing), street-light/wind/foliage sway and passenger rendering.
