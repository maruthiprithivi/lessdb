# Lessville implementation notes (working file)

Project: <project-root> (Godot 4.7.2 generic starter, Cloud; PORT=3000 HOST=0.0.0.0).
Game runtime: <local-path>
Blueprint gbp_f36ab28147904ceec086a1e6 approved rev 3: max_effort ("Standard effort"), hybrid assets, no mockups, no online features. English only.
Delivery: npm run check -> node $GAME_RUNTIME/save-release.mjs -> npm run check -- --pack -> push exact SHA -> GET game/sharing/releases/<sha>.
Need: loading screen (web/loading.html), favicon + OG cover (read game-sharing.md), config/build true/site check.

## Architecture
- backend/agentd (Rust, axum, path deps on <lessdb-source-root> crates less-engine/less-query/less-common). Embedded LessDB engine is the single directory owner. Owns deterministic simulation + agent policies; every memory/message/decision/event is a LessDB SQL INSERT; recall uses SQL SELECT with ACL subquery on lv_scope_members. SQL log ring buffer exposed to UI.
- Godot client = renderer/observer; polls GET /api/state; POST /api/control, /api/nudge, /api/reset; GET /api/agent/:id, /api/memory/:id, /api/tables, /api/boundaries. CORS *. Port 8790, public via <sandbox-preview-url> . If unreachable: honest "LessDB service offline" screen.
- Backend dir needs .gdignore; CARGO_TARGET_DIR outside project; CARGO_HOME=<cargo-cache> offline.

## LessDB SQL facts
CREATE TABLE t (id Int64, s String, f Float64) ENGINE=Firefly ORDER BY id [UNIQUE (k)] ; UNIQUE = replace keep last, reads dedup before flush. UPDATE/DELETE/TRUNCATE supported. TTL is wall-clock (not used for sim time). Session: LessSession::new_async(engine).await; session.sql_batches(sql).await -> Vec<RecordBatch>. LessEngine::open(EngineConfig::with_data_dir(dir)) -> Arc. engine.tables_async().

## Cast
maya (Owner, café owner, lives above café), sam (office worker, Ama's son), lin (Customer, student, arrives by bus from 'away'), ama (Family, grandmother, walks corgi Mochi), bo (barista, from 'away' west). Mochi follows Ama (non-agent).
Scopes: private(owner only), family{ama,sam}, cafe-staff{maya,bo}, cafe-regulars{maya,bo readers}, neighborhood{all}.
Memory kinds: permanent, temporary(expires_at), contextual(context_id), spatial(location+tag), temporal.

## Scenarios (deterministic rules, labelled)
1 eggs errand: Ama 06:40 notices eggs=0 -> family msg errand.request; Sam inbox 08:30 accepts; Ama 09:30 grocery queries accept -> skips eggs (no duplicate); Sam 18:00 recalls contextual errand -> grocery -> errand.done.
2 oat milk: day1 oat=1, background order 07:30 -> 0; Bo msg cafe-staff restock.request; Maya defers to 14:00 grocery -> stock 12 -> restock.done. Lin's 08:xx oat latte fails -> private temporal memory.
3 consent: first visit Maya asks; Lin writes pref.usual into cafe-regulars; day2 Bo recalls -> prepares usual. Nudge revoke -> DELETE rows. Hidden private rows counted (Lin's exam stress note).
4 bus: day1 08:00 bus +15 (scripted road works); Lin memory bus.late.0800; day2 07:00 recall -> takes 07:40 bus.
5 trail: scripted rain day1 13:30-14:15; Bo passes trail 15:10 -> spatial neighborhood trail.muddy; Ama 16:30 recalls (12h window) -> park instead.
6 table hold: Sam 11:30 asks; Maya temporary memory until 12:45; at 12:30 recall seats Sam.
Nudges: rain, bus_delay, oat_out, eggs_out, revoke_consent. Controls: pause/resume/step(5min)/speed 1,2,4,8. Day 06:30-22:00 then night skip.
Town anchors (1536x1024): home(258,360) office(820,370) cafe(1440,380) park(820,600) bus_stop(470,720) exits east(1530,560) south(300,1010) west(0,560).

## Art (done)
tools/process_art.py -> assets/game/characters/{lin,sam,maya,ama,bo}_{0..3}.png (0 idle,1-2 walk right,3 talk; 96x112), lin_back_{0,1}, sam_typing_{0,1}, mochi_{0..3} (72x52), props/{coffee,milk,grocery_bag,backpack,bus}.png, locations/town.png (1536x1024 no bus) + {cafe,office,grocery,trail,home,school,gym}.png 768x512.
Generated sources in <recovered-assets-root>/generated.

## Client geometry (town.png 1536x1024 coords)
Nodes: A home_door(258,340) B home_gate(258,485) W west_exit(-40,485) C west_cross(500,485) D plaza_w(520,410) E office_door(820,350) F office_plaza(820,410) G plaza_e(1160,410) H east_walk(1160,500) I cafe_front(1440,410) J cafe_door(1440,350) X east_exit(1580,500) K lower_w(500,665) L park(820,650) M lower_e(1160,665) N bus_stop(480,712) SW(215,665) S south_exit(215,1060).
Edges: A-B B-C B-W C-D D-F F-E F-G G-H H-I I-J H-X C-K K-N K-L L-M M-H K-SW SW-S.
Indoor (home/office/cafe) agents hidden on map -> badge + close-up interior overlay. Outdoor park/bus_stop drawn. Offmap exits: grocery/gym->X, trail->S, bo away->W, lin away/school via bus.
Road: y 735-895, bus lane near stop y~790; bus stop x~480. Lamps at (140,560),(515,215),(1150,580),(1435,580),(325,880),(1190,880),(1390,880).
Layout plan: logical 1600x900, stretch canvas_items keep. Top bar 56px; town 1040x693 at (0,56); places strip below; right panel 560 wide: roster + tabs Story/Agent/LessDB/Truth.
Starter: keep autoloads (ManusFontTheme, SaveStore, I18n, TuningStore, AudioDirector, TuningBridge). Fonts res://assets/template/fonts/ui_{regular,medium,bold}.tres, display/ dir. Replace maze scripts; test_suite expect string in game-verification.json.
Backend build: cargo build --offline --release (tokio 'signal' feature removed: signal-hook-registry not cached). Binary at <lessdb-target>/release/lessville-agentd.

## Progress (2026-10-08 09:34)
- Backend built: <lessdb-target>/release/lessville-agentd; running as service job on 0.0.0.0:8790, data /data, log <project-root>-agentd.log. Public URL <sandbox-preview-url> (CORS *). Verified day-1 storyline (restock, consent ask, errand dedupe, table hold, bus delay) with 0 SQL errors. Article fix ("an oat latte") in sim.rs needs rebuild.
- Client written: scripts/main.gd, scripts/lessville/{api_client,town_view,interior_view,ui_kit}.gd; config/lessville.json api_base. Maze scripts/shaders removed. project.godot: 1600x900 keep, name Lessville.
- TODO: new test/test_suite.gd (expect "[lessville-tests] PASS:" in game-verification.json), tools/verify_exported_pack.gd for Lessville, tuning.json trim (maze ids), localization keys, icon (config/icon still runner.png), loading shell web/loading.html, game-sharing.json/OG, npm run check, preview build, native capture, save-release, push.

## Progress (09:49)
- npm run check passes (58 checks incl. live service); game-verification.json expects "[lessville-tests] PASS:".
- Browser: Godot canvas is 1280x1100 real; browser_click needs real coords (screenshot 893x768 × 1.433). Verified title, town, Mind, provenance modal, café interior, bus, rain.
- Day-2 consequences verified from stored rows: Lin takes 07:40 bus (mem #10), Bo starts usual (mem #12), Ama dedupes errand, Ama park-not-trail (mem #16).
- Service now job on agentd-svc3; resumes from LessDB on restart ("Resumed Day 3 ..." verified).
- Remaining: visual coverage refresh with rendered evidence, icon, loading shell, sharing, save-release, report.

## Cloudflare deployment facts (2026-10-08)
- Account <cloudflare-account-id> ("LessDB Cloudflare account"); zone lessdb.dev id <cloudflare-zone-id> (Free plan, active). Pages project `lessdb` owns lessdb.dev apex — do not touch. ville.lessdb.dev unused.
- Token CLOUDFLARE_API_TOKEN: Workers scripts, Containers, R2, Pages, workers/domains OK. DNS read and Workers AI REST = auth error (use AI binding in Worker).
- Existing workers/containers/buckets (controlzero*, cz-recall-dev, the-local-node, lessdb-downloads, lessdb-shared*) must not be touched.
- R2 bucket `lessville-data` created. R2 S3 connector "Cloudflare R2 S3 API" (uid c8c37e6d-0975-4bde-8501-d3158c2a8bcb) env R2_ACCESS_KEY_ID / R2_SECRET_ACCESS_KEY — user had not entered values yet at 14:43. Endpoint https://<cloudflare-account-id>.r2.cloudflarestorage.com region auto.
- Docker works in sandbox (`sudo dockerd` started manually, v29.1.3).
- User decisions: Workers AI `@cf/meta/llama-3.1-8b-instruct-fp8-fast`, HARD daily budget 9500 neurons (UTC day); R2 persistence; public = observer-only (no pause/speed/nudge/reset). Disclaimer text: fictional research simulation, characters not real people, resemblance coincidental, studies how agents evolve over time with long-term memory powered by LessDB.
- Pricing (https://developers.cloudflare.com/workers-ai/platform/pricing/): llama-3.1-8b-instruct-fp8-fast 4119 neurons/M input tokens, 34868 neurons/M output tokens; free 10k neurons/day; $0.011/1k neurons above.
- Design doc: docs/LESSVILLE-FLOWS-AND-VIEWS.md (flows, views, movement, bus choreography).
- Art jobs: generated/{maya,bo,ama,sam}-back-seat.png (2x2: top back-walk x2, bottom seated x2), lin-seat.png (4 seated), bus-interior.png (magenta #FF00FF windows), bus-window-scenery.png (tileable).
- Service RSS ~42MB, data dir 840KB after 13 days → lite container; checkpoint = flush+tar.gz → R2 PutObject via curl --aws-sigv4; restore on boot.

## Progress 15:02 (realism + deploy pass)
- DONE: art (back/seat frames all chars, priya/tom typing, bus.png interior keyed, bus_scenery.png); process_art.py updated.
- DONE backend: llm.rs (phrase/reflect, curl to LESSVILLE_LLM_URL with x-lessville-key, lv_llm_calls), checkpoint.rs (R2 restore/upload via curl sigv4; env LESSVILLE_R2_ENDPOINT/BUCKET/PREFIX + R2_ACCESS_KEY_ID/SECRET), main.rs (LESSVILLE_PUBLIC=1 → 403 on control/nudge/reset; /api/llm; health has llm+checkpoint), sim.rs (station field, Sam office routine 570/585/905/920, voice(), night reflections). Release build OK.
- DONE client: interior_view.gd rewritten (stations, door walking, bus view w/ scenery + next-stop, LessDB strip). Uses open(loc) — main.gd must call interior.open(loc) instead of setting location, and handle "bus".
- TODO client town_view.gd: facing (back frames dir.y<-0.5), no-backtrack re-path (edge-endpoint choice), lanes, ease/door pause, bus choreography (approach lead=min(10*speed,15) sim-min from state.bus.next arrival, dwell real-time ≥3s until boarders hidden, depart 4s), delay sign, click bus → place_clicked("bus").
- TODO main.gd: public mode (hide pause/speed/nudges/reset when state.public), disclaimer on title + footer, model status chip, api base relative on web (use window.location.origin).
- TODO deploy: backend/cloudflare worker (assets + container + /internal/llm with Budget DO 9500 neurons/UTC day + rate limit + headers + cron keepalive), Dockerfile, R2 creds check.

## Deploy progress 15:12
- R2 S3 key values were never entered (env vars empty) → checkpoint transport switched to Worker route /internal/ckpt/<latest|hourly/N>.tar.gz using R2 binding BUCKET=lessville-data, prefix lessville/v1/. Container env: LESSVILLE_CKPT_URL, LESSVILLE_LLM_URL, LESSVILLE_INTERNAL_KEY (Worker secret INTERNAL_KEY, ≥32 chars), LESSVILLE_PUBLIC=1, CHECKPOINT_SECS=60.
- Deploy dir: <project-root>/deploy/cloudflare (wrangler.jsonc, src/index.ts, container/Dockerfile ubuntu:24.04 + curl; binary copied to container/lessville-agentd; game web build copied to public/). deploy/.gdignore present.
- Docs (https://developers.cloudflare.com/containers/get-started/ , /containers/container-class/): Container class fields defaultPort, sleepAfter (default 10m), envVars, pingEndpoint (default "ping"; set "localhost/api/health"), enableInternet default true; getContainer(ns, name).fetch; image must be linux/amd64; first deploy needs several minutes provisioning. `exports` and `migrations` cannot be combined (using migrations new_sqlite_classes).
- @cloudflare/containers latest = 0.3.7. wrangler 4.148.0. Docker 29.1.3 running in sandbox.
- Budget DO "global": reserve(estimate) / settle(actual) per UTC day, cap var DAILY_NEURON_CAP=9500; /api/budget public status.

## Progress 15:33
- Local mock drill (deploy/mock_gateway.py) passed: public 403s, phrase calls ~410 chars ≈1.26 neurons, budget exhaustion → rules only + "skipped" rows, R2-route checkpoint upload + restore on wiped disk.
- FOUND: LessDB UNIQUE is eventually exact (docs/DESIGN-DECISIONS.md D6; tests/sqllogictest/05_uniqueness.slt "Duplicate keys across parts are visible until OPTIMIZE"). lv_kv had 2 'world' rows → resume picked an older snapshot → id reuse + replayed events (duplicate mem:15). Fix in main.rs: collapse() = engine.flush+engine.optimize on lv_kv/lv_agent_state before resume and before every checkpoint archive; snapshot read ORDER BY sim_minute DESC LIMIT 1; id counters raised to MAX(id). Also: embedded LessSession SQL has no OPTIMIZE statement (only CLI TUI special-cases it) → LessDB gap.
- llm.rs: context rows dedup by id; shared memories attributed "Ama, family: ...".
- Release: `node "$GAME_RUNTIME/save-release.mjs" --message ...` produces site/ (publish output); `npm run check -- --pack`; push exact SHA to main. Cloudflare public/ = copy of site/.


## Release v2.1.0
- Title-screen tips now describe passive observation and context retrieval; they no longer imply public nudges are available.
- Public version label is rendered in the title screen and set in `package.json` and `project.godot`.
- SQL observer entries shorten only the displayed SQL text to 480 characters; LessDB receives the complete statement. World checkpoints retain the newest 20 feed items because full history is stored in LessDB tables.
- The live Cloudflare rollout completed after a cache-busting container image update; the service restored from R2 and public observer mode remained active.


## Progress — budget guardrail (2026-10-08)

- Cloudflare deployment changed from `basic` to `lite` Container sizing (1/16 vCPU, 256 MiB, 2 GB), with `sleepAfter = "10m"` and the existing five-minute keep-alive.
- Checkpoint interval is now 300 seconds. R2 stores `latest.tar.gz` plus a seven-day rolling set of 168 hourly archives; the Worker prunes older hourly objects after each hourly upload.
- Workers AI remains capped at 9,500 neurons per UTC day. Cloudflare's published 10,000-neuron free allocation means this cap does not intentionally cross the free allocation.
- Conservative 30-day always-provisioned estimate: approximately $4.50 for lite Container CPU/memory/disk plus the $5 Workers Paid base = approximately $9.50 before unrelated account usage. R2 is expected to remain inside its Standard free tier at the current archive size and operation rate.
