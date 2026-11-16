# Lessville — A LessDB Agent World

**Live demo:** [ville.lessdb.dev](https://ville.lessdb.dev)

Lessville is a fictional research simulation showing how a small world of scripted agent policies can use **LessDB** for permanent, contextual, spatial and temporal memory. The client is a Godot pixel-art observer; the backend is a native LessDB service. Every displayed memory, message, decision and SQL activity entry is backed by LessDB data rather than browser local storage.

> **Notice:** This is a fictional research simulation. The characters are not real people; any resemblance to real people is coincidental. It explores how agents may evolve over time with long-term memory powered by LessDB.

## Release v2.1.0

- Public Cloudflare deployment at `ville.lessdb.dev`.
- Observer-only public mode; visitors cannot mutate the shared town.
- R2-backed LessDB checkpoints with restore-before-open behavior.
- One concise model task per call, bounded LessDB context, output validation and a 9,500-neuron daily cap; deterministic rules remain the source of decisions.
- The title screen and Truth panel distinguish native LessDB behavior, application rules and open capabilities.

## Repository map

Start with the [EGM blueprint](EGM.md) for current component contracts, evidence and continuation rules.

The current release candidate includes 100 persistent residents (17 in the town
centre), junction-preserving pedestrian reroutes and a pinned embedded engine in
`vendor/lessdb`. [Release qualification](docs/RELEASE-QUALIFICATION.md) separates
verified local results and the passed native copied-checkpoint rehearsal from the
pending Linux-image qualification and Cloudflare rollout. The existing live demo
is not evidence that this candidate is deployed.

- `scripts/` — Godot client and observability UI.
- `backend/agentd/` — native LessDB-backed simulation service.
- `deploy/cloudflare/` — Worker, Container and R2 deployment configuration.
- `docs/LESSVILLE-FLOWS-AND-VIEWS.md` — scenario and movement design.
- `docs/LESSVILLE-DELIVERY.md` — validation, deployment and operations report.
- `docs/lessville-implementation-notes.md` — implementation decisions and continuity notes.

## Local development

For the 100-resident local simulation, run `tools/run_local.sh`. See
[Local world and recording](docs/LOCAL-WORLD.md) for setup, behavior and limits.

Install Rust 1.96 and Godot 4.7.2. `npm run dev` starts the local world;
`npm run check` runs the client checks; `npm run build` exports the Web client
with matching Godot templates. The engine source is included in `vendor/lessdb`;
these commands do not require a private engine checkout or Manus runtime.
See the [Cloudflare guide](deploy/cloudflare/README.md) for the separate Linux
Container build and rollout boundary.

The public deployment is the authoritative shared world. For local development, use the project workflow and a local agent service; do not point experiments at the public write paths. See the deployment report for the exact build, checkpoint and rollout process.

---

# Generic Game Template

This versioned source is owned by the Webdev Addon. Use the installed Game workflow for initialization, preview, release and publishing. Matching gameplay source: template-provenance.json. Existing projects are never overwritten by a new starter.

For online multiplayer, read [the shared service guide](docs/game-multiplayer.md).

## Shared game-development workflow

Applies to Max effort, not `fast_prototype`; Fast uses its dedicated section instead. Keep edits scoped and
preserve explicit constraints.

### Preserve the selected template
Extend the initialized project and keep its working simulation, controls, saves and delivery. Integrate supplied
games into that project. The first Preview must show the requested game, not a renamed demo; replace the Generic
maze gameplay unless a maze was requested.

### First-checkpoint completion contract
Deliver the requested goal, actions and outcome loop with understandable controls and feedback, recovery or replay
where it fits, coherent readable visuals, the necessary start, pause and results screens, and any requested
deliverable. Match run length and content to the brief; there is no minimum playtime, campaign, extra mode or screen
quota. Prioritize gameplay, broken resources, responsiveness and visible defects, and stop when the requested
experience works. Add art, audio or polish only for a concrete benefit. Leaderboards, new Tweak controls and extra
languages are optional; required Share OG and explicit media, concept or feature requests are not.
Default to desktop. Add or test touch controls, portrait layouts or mobile-browser support only when the user asks.
Sharing a Web link is not a mobile request. Keep inherited controls working without expanding mobile coverage.

Choose the [delivery scope](game-delivery.md#delivery-scope-and-stopping-boundary) from the request. Ordinary managed
delivery is check → save-release.mjs → applicable check --pack → push the exact SHA and confirm the checkpoint.
Documentation-only edits need no engine boot. Do not add audit reports, speculative test suites, a separate
self-review phase or duplicate validation workers.

### Ownership boundaries
Keep flow, authoritative simulation and scoring, input, entities, tuning, presentation and persistence in their
existing owners; cosmetics never decide collisions or rewards. Offline authority stays in Godot and online authority
on the dedicated server. WASD is reserved for player movement or camera panning, never abilities or menu actions;
leave it unbound when nothing moves, and update remapping, tutorials and hints when replacing conflicting shortcuts.

### Gameplay feedback and optional polish
Make actions, damage, rewards and outcomes readable with the simplest fitting cues. Reuse existing feedback and
add effects only where they improve readability or the requested feel; no particle set, reward flights, screen shake
or animated counters are required. Keep overlays input-transparent and cosmetics independent of simulation, and
preserve reduced-motion settings.

### Tutorial content boundary
Explain the necessary goals, controls and rules briefly in the game's language; a short start-screen hint is enough
for simple games. A staged tutorial needs a working Skip that releases input and pause locks and remembers the dismissal. Keep implementation
values and Tweak controls out of player instructions.

### Optional local leaderboard
Add local standings only when requested or integral to scoring and replay, and keep a useful existing one; changes
keep bounded records, once-only terminal submissions and deterministic ties. Online
rankings need an explicit request and the installed leaderboards guide; never provision a backend for an offline
game by default.

### Native verification
Reuse existing checks for changed gameplay, input, audio, saves, localization or tuning. For visual questions,
capture with the real renderer (`get_viewport().get_texture().get_image()` and `Image.save_png()`); headless Dummy
rendering proves nothing visual. Do not drive the Preview with browser automation, screenshots or self-tests unless
the user asks for browser debugging; use logs and report Web input, audio and layout as pending user acceptance.

### Concept work by request
Concepts are conditional, not a routine production step. Follow the Blueprint's recorded mockup choice and do not
offer a second concept workflow when the direction is already clear. For an explicit request, produce distinct,
coherent interpretations of the brief and keep the selected identity through revisions. The Game Concept Design flow
needs exactly three separate mockups generated under the recorded image consent, presented through the existing card
and explicitly approved; never label mockups as screenshots. Outside that flow, honor the requested count and
approval method. Mockup consent is not consent for production assets.
## Shared art and audio production

### Asset reuse and targeted production
Reuse suitable supplied, template and already available assets within their recorded permissions and the user's
source restrictions; template reuse needs no extra approval. Replace only assets that conflict with the requested
theme, lack necessary states or look visibly wrong. A new game or reskin does not require replacing every image,
animation, icon, track or effect unless the user asks for original art or a full replacement. Retain unused
reference files and supplied license notices, describe reused assets honestly, and update visible titles and themed
copy. Rename internal IDs only when needed, together with their consumers, saves and tests.

### Coherent scene and title art
Fulfill the requested visual scope with readable actors, hazards, UI and a coherent scene. Simple intentional
visuals are acceptable within the recorded source choice. Title and loading screens may reuse permitted game art
with live text; no separate key art is needed. A download is not integration: inspect the affected scene.

### 3D texture budgets
Budget the project's stored textures, including bytes embedded in GLB/glTF, not catalog masters or import limits.
Longest edge: 256–512 for small or distant props, 1024 by default, 2048 for close main actors or shared atlases,
4096 only for a justified visible difference. Never upscale, and keep UVs, channel packing and material bindings
intact. Remove replaced embedded bytes, keep ancestry and sync the changed files. If suitable tools are missing,
report it rather than replacing working assets. Claim only measured savings.

### Continuous animation and alignment
Use the approved animation method. AI continuous motion goes from approved static references to a guided
fixed-camera, in-place video to fixed-FPS extracted loops; independent poses need an approved stepped style. Keep a
shared canvas, scale, pivot and contact anchor across states and facings instead of per-state placement patches, and
keep raw masters outside the game. Check representative movement, reversal and ground contact with the native
renderer.

### Mandatory background removal for composited assets
Composited sprites, icons, portraits, cursors and loader art need clean real alpha that survives resize, import and
export; check changed edges on contrasting backgrounds at display scale. Opaque plates and terrain are exempt. Keep
words, numbers and HUD labels live, and never slice concept collages into runtime assets. Generate cursors on a
hot-pink or neon-green background and remove it before use.

### Seamless parallax backgrounds
Repeating layers must tile left and right without mirroring; check the joins between adjacent copies at the intended
scale after any crop, import or scale change. Side-scrollers keep a low scenic foreground above world actors and
below the HUD without hiding combat.

### Audio production as needed
BGM is optional unless requested or essential to the brief. Reuse a fitting permitted template, supplied or catalog
track when the source restrictions allow; silence with safe empty routes is valid, and a new Max game is not a
reason to generate custom BGM. Honor explicit silence, supplied-only, AI-only and custom-music requests, and add only
useful event SFX. Generate audio only for an explicit request or a concrete
unmet need within recorded permissions: one usable result per needed track or cue, retried only for an actionable
correction. If requested custom music cannot be produced, report the unmet requirement. Audition changed audio and
ship compressed runtime audio, not raw WAV masters.

### Audio integration
Reuse the existing Music/SFX routing, gesture unlock and bounded voices, and keep empty routes safe across pause,
retry and scenes. New games that use audio keep the absolute base gains BGM 3× (+9.5424 dB) and SFX/UI 2×
(+6.0206 dB) over the inherited originals: place MusicBase/SfxBase before senders so each route crosses once,
replace old targets instead of stacking boosts, and keep the ≤0 dB guards, saved settings and explicit mixes.
Silent games need no gain plumbing, and existing delivered mixes stay unchanged. Spatial audio keeps real source
positions; music and global HUD sounds are separate. When music is used, keep the shared browser BGM adapter and
its native fallback.

### Asset generation mode gate
Recorded permissions decide what may be generated, regardless of mode or available tools. Hybrid, AI or explicit
permission covers its scoped generated visuals and SFX; mockup consent does not. For authorized images choose the
latest GPT Image model offered by the live schema unless the user named one. Tools record generation metadata
automatically; keep tool, provider, model and internal path details out of user-facing copy.

### Asset-production choice wording
Fast prototype follows its bounded workflow. New managed games call `webdev.init_project` once direction is clear,
with the complete brief; the canonical card settles unresolved choices, not parallel chat questions or a separate
Blueprint. Session-agent catalog search, access, downloads and delegation wait for successful initialization plus a
settled Blueprint, required plan and requested concept approval; in-flight or failed init is not success. Only
initialization's bounded metadata assessment may precede the card, never production downloads, generation or
integration. Standalone search or retrieval needs no project but does not justify speculative new-build sourcing.
Use the conversation language and call the catalog 游戏素材库 in Chinese. Recommend Hybrid mode for 2D, 3D and
unknown games. Card options for 2D/unknown: Manus game assets catalog, Hybrid mode, AI generation; Hybrid means
catalog plus AI art/audio, not scratch visuals. Known 3D: Game asset library and procedural generation, Hybrid mode,
AI generation. Only initialization's evidence-backed insufficient core-visual fit changes the first 3D label to
Procedural generation (Faster, lightweight, minimal token consumption); Unknown/sufficient fit or missing decoration
does not. Preserve historical accepted permissions; never infer them from option names or recommendations.
Only missing card support or a structured `planning_unavailable` receipt moves unresolved setup to one native
single-choice question; canonical state wins. For 2D/unknown: Manus game assets catalog, Hybrid (Recommended), AI
generated. Known 3D: Curated asset catalog and procedural (lowest token cost; catalog first, permitted gaps),
Hybrid (Recommended), AI generated (highest token cost). Keep these labels and this order, and record the answers.
No-questions sessions use
the recommended option within existing permission, which is not new consent. Additions to an existing game ask only
an unresolved production choice; delegation inherits constraints.
Accepted `choices.visualAssetSource: procedural` needs no repeated visual search/gap proof; audio follows its own
source restrictions and optional-production workflow. Catalog and Hybrid sourcing, including required-gap handling,
follows the game-asset-catalog Skill. Procedural permission never implies music composition.
## Shared Web runtime and delivery contract

### Browser-safe lifecycle
Bound and reuse voices, projectiles, particles, timers and subscriptions across pause, retry and scenes; change
pause or stream state on transitions, not every frame. Browser BGM uses the independent Web Audio renderer in
`scripts/manus/browser_bgm_player.gd`, never frame- or timer-fed PCM or finished-callback loops. It decodes once into
a bounded LRU AudioBuffer cache: call `prepare()` during loading or a transition before gameplay, then `play()`
reuses the buffer. Imported duplicates share an explicit original-path `buffer_key`, and changed PCM needs a new key.
Decode imported resources from the PCK, keep SFX in Godot and keep `thread_support=false`.
Route track, loop, position, pitch, mute, pause, duck, crossfade and Music/Master controls through the adapter.
External BGM does not inherit buses: mirror linear gain, limiter makeup and AudioEffectAmplify only. Keep native
Godot and the guarded Web fallback (`PLAYBACK_TYPE_STREAM` for long BGM) without overlapping backends; pending
autoplay waits for valid input, and stale starts are guarded. Stop voices and release removed cues with
`release_cached_stream()`, evict only unreferenced tracks, keep OS/bfcache suspend and restore, and release the context
on real teardown. Native tests cannot prove browser continuity through
main-thread stalls, so report that as pending user acceptance.

### Fast, recoverable startup
Keep the title and controls interactive with only the required fonts, UI, settings and small art. Defer scenes,
generation, media and decoding until needed, in bounded per-frame work; instantiate scenes on the main thread. Start
blocks duplicate transitions and reports recoverable content errors. Lazy PCK content reduces work, not transfer
bytes. Loading progress uses real bytes or phases and an indeterminate state for unknown totals; slow or stalled
notices are nonfatal, offer Retry, and clear on late success. When changing this mechanism, check cold load,
repeated Start, stall, unknown total, late success and Retry.

### Animated backgrounds without startup blocking
Show a small static placeholder immediately and keep it as the fallback. Load animations asynchronously after the
title is interactive, never as a Start or preload dependency, and swap on an actual ready frame while guarding exited
scenes and stale completions.

### Large explorable maps: presentation culling and live state
Offscreen simulation, pathfinding, orders, combat, economy, timers, replication, saves and the minimap stay live.
Cull only cosmetics against the current camera bounds, with sprite margins and separate fog, and never remove
gameplay actors, projectiles or collision by distance. Re-entering a region shows current state without replaying
missed effects. Optimize measured bottlenecks with existing spatial or dirty-region mechanisms, and compare crowded,
zoomed and revisited views against a baseline; empty-map FPS proves nothing.

### Destructible props
Destroyed rubble and fragments render below actors in effective world z while keeping footprint, collision release
and reward position; reset restores the intact state.

### Language scope and bundled fonts
Use the requested game language, defaulting to the conversation language. New games need no extra language or
EN/CN toggle unless requested; keep working localization on existing projects, and every exposed locale must be
complete. Keep player-facing words live instead of baked into art. Do not rebuild or re-audit the unchanged bundled
fonts; check changed text bindings, layouts and supported player names only. Keep name validation and save behavior,
and do not claim unsupported character coverage.

### Default typography
Default to the template's bundled theme: its display faces for titles/headings, its body face
(`assets/template/fonts/ui_{regular,medium,bold}.tres`) for UI text, then Noto Sans SC (6,547-codepoint repertoire)
for Chinese; Web loaders embed a small Noto Sans SC subset. Noto Sans SC is variable and its default instance is Thin,
so bind a weight through the template's font resources, not the raw file. Keep each bundled font's license
(`<Family>-OFL.txt`) beside it when copying, renaming or adding fonts; the Web export lists them on its Open Source
Licenses page. Missing template fonts restore from `assets.lock.json`; do not substitute system fonts. Existing
ManusCC0 projects keep their fonts unless the user asks for new typography. User-uploaded fonts are unrestricted: no
licensing, family, glyph-coverage, font-file, font-resource or font-pack size gates, and no rights review,
redistribution proof or notices. Theme or mood alone does not imply replacing the default fonts. This supersedes font
restrictions in frozen READMEs and local checks, including older ManusCC0 defaults: remove those policy gates when
integrating uploads, and keep only technical file-integrity and path checks.

### Viewport and background coverage
For fixed-coordinate Godot layouts set `[display]` `window/stretch/mode="canvas_items"`,
`window/stretch/aspect="keep"` and the logical viewport size; keep working adaptive layouts. Input targets the active
surface, not letterbox margins. Check affected desktop resize, aspect and DPR. Isometric zoom uses the ordinary
wheel or trackpad without Ctrl/Meta, with pointer-anchored picking; the HUD keeps a fixed scale. Mobile, touch and
portrait work follows the workflow's user-request rule even when template guides list those cases.

### Development Tweak when needed
The Addon generates the floating frosted-glass Tweak popover from the game's descriptors; never create an in-game
panel, launcher or F10 shortcut. Adapt the existing catalog to the requested game, extend it for meaningful tuning
and remove controls with no real consumer (at most 128). Keep one parameter manager with stable IDs, typed defaults,
bounds/options, localized labels, honest apply timing and existing run eligibility. Edits/reset affect only the
Addon draft. Apply validates and commits the whole patch without disk writes or preview reload; unchanged values
emit nothing. Gameplay boundaries consume pending values but never send catalogs. Do not add numeric revisions.
Save with Manus sends selected values to the task for source edits and the normal build/checkpoint flow; it does
not publish. Keep the adapter in `scripts/manus/preview/`, which release excludes with its autoload entry. Normal
player Settings and gameplay consumers remain. Existing genre rules below identify the actual parameter owners.

### Export constraints
Include runtime JSON/CSV/TXT through include_filter or Godot resources. Keep Web thread_support=false, resource paths discoverable, and the template-specific size budget. Use the Session port and the Addon preview/export workflow; do not hardcode the old Sandbox port or overwrite generated site/dist. Change identity through project.godot and the editable game loading shell, not generated HTML.

## Reference identity and purpose
`game-generic` is the Godot 4.7.2 fallback scaffold for Manus GameDev. Its reference game is **Generic Game Template**, a two-stage collection/chase game with a minimalist graphite, porcelain, and brushed-metal presentation. The title action is **START GAME**. Collect every energy node and power-up while avoiding sentinels; complete both circuits to win.

Select it only without a matching specialized genre; follow the [game-generic exception](#preserve-the-selected-template) for non-maze gameplay/UI.

The template intentionally contains **no default audio media**. Its silent baseline is an explicit template-owner choice; future adaptations follow the shared audio requirements and their user's choices.

## Source architecture
For an explicitly requested 3D game, initialization selects `scenes/manus_world_3d.tscn` with `scripts/manus/world_3d.gd`: this single owner supplies the Node3D world, camera and movement. Read those actual paths first. The layout below describes the original 2D reference, whose maze owners are not the active 3D scene. The 3D overlay is a starting scaffold; replace its primitive world and scaffold checks with the requested game and meaningful outcomes.
```text
game-generic/
├── project.godot                  # identity, input, autoloads, font, viewport
├── template.json                 # stable game-generic catalog identity
├── assets.lock.json              # Runtime v2 permanent CDN mappings
├── assets/template/
│   ├── environment/              # studio backdrop and sparse foreground
│   ├── characters/               # porcelain runner and metal sentinel
│   ├── powerups/                 # energy, overdrive, shield, slow field, magnet
│   ├── ui/                       # text-free title and pause glass artwork
│   ├── fonts/                    # Template display/body fonts, Noto Sans SC + OFL licenses
│   └── provenance/               # generation prompts and optional audio contract
├── autoload/
│   ├── audio_director.gd         # optional cues, buses, streaming, bounded voices
│   ├── i18n.gd                   # catalogs and fallback/interpolation
│   ├── save_store.gd             # local scores, player/locale/tutorial state
│   └── tuning_store.gd           # authoritative values and apply boundaries
├── scripts/
│   ├── main.gd                   # title, session, HUD, menus, tutorial routing
│   ├── manus/preview/            # Addon-only tuning transport and adapter
│   ├── game/{stage_catalog,maze_world}.gd
│   ├── entities/{runner,sentinel}.gd
│   ├── ui/{screen_factory,tutorial_callout}.gd
│   ├── effects/effects_director.gd
│   └── score/run_score_service.gd
├── config/tuning.json            # typed six-category tuning catalog
├── localization/{en,zh-CN}.json
├── docs/minimalist-redesign.md   # concept, implementation plan, evidence
├── test/test_suite.gd
└── tools/                       # finite verification adapters, native capture, bounded font installation
```

`MazeWorld` owns simulation, `RunScoreService` owns score/finalization, and `TuningStore` owns configuration and run integrity. UI, art, shaders, particles and audio present that state. Save keys, stage IDs and tuning IDs remain compatible with earlier template runs; creative runtime IDs have been renamed to neutral terms.

## Game Addon tweaks
Reuse `config/tuning.json`, `TuningStore` and `scripts/manus/preview/tuning_adapter.gd`. The Addon generates its floating frosted-glass popover from game descriptors without resizing the preview; game code supplies parameters, not panel styling. Adapt or extend the examples for the requested game, with real consumers, stable IDs, localized labels/categories/descriptions, defaults, bounds/steps, integrity and honest LIVE/NEXT_ACTION/NEXT_STAGE/NEXT_RUN timing. Generic currently supports up to 128 number/boolean controls; other types need game-side support.

Keep one store and existing run-eligibility rules. Edits and resets change only the panel draft; unchanged or invalid drafts cannot Apply. Apply validates/commits the whole patch without persistence or reload; deferred values activate at their declared gameplay boundaries. Unchanged values emit nothing, and gameplay events never push catalogs. Do not add numeric tuning revisions.

After Apply, Save with Manus reads a snapshot and sends it to the task for source edits and normal build/checkpoint, never auto-publish. Release excludes the debug bridge and overrides while retaining source defaults and gameplay consumers.

These rules override frozen README tuning instructions: do not recreate a native panel, launcher, F10 shortcut or native Tweak capture. Preserve existing protocol/runtime guards; ordinary game generation needs no Addon transport implementation reading.

## Run and verify
```bash
npm run check                   # finite import, boot and game-owned outcome hooks
npm run build                   # current standalone export through the shared runtime
npm run check -- --pack          # verify that current export and applicable outcome hooks
```
Use the runtime directory returned by init/attach and the installed Game workflow. Keep meaningful game-specific hooks in optional `game-verification.json`; an absent hook file is normal before one is needed. Fresh compatibility aliases delegate to this same finite verifier. Older READMEs and `tools/verify.sh` may hardcode the maze scene, obsolete resource-audit assumptions or a different `dist/index.pck`; adapt relevant outcome hooks instead of repairing that separate pipeline. Asset sync/build own dependency, hash and current-export checks. The Web preset uses `thread_support=false`; developer tools and fixtures stay outside the playable export.

The original 2D reference's native capture uses the actual viewport and renderer; adapt its scene/state selectors for another game and run without `--editor`:
```bash
GENERIC2D_CAPTURE_PATH=/absolute/path/title.png godot --path . --script tools/capture_native.gd
GENERIC2D_CAPTURE_PATH=/absolute/path/game.png GENERIC2D_CAPTURE_GAME=1 godot --path . --script tools/capture_native.gd
GENERIC2D_CAPTURE_PATH=/absolute/path/pause.png GENERIC2D_CAPTURE_PAUSE=1 godot --path . --script tools/capture_native.gd
```
`GENERIC2D_CAPTURE_ZH=1` selects Chinese; `GENERIC2D_CAPTURE_TUTORIAL=1` with the game flag captures onboarding. Use a real renderer; headless Dummy output is not visual evidence. Browser acceptance belongs to the user under the shared workflow.

## Controls and playable loop
| Action | Keyboard | Gamepad | Pointer/touch |
| --- | --- | --- | --- |
| Move | WASD / arrows | Left stick / D-pad | Direction pad |
| Pause/resume | Escape | Start | Pause / Resume |
| Activate focused UI | Enter / Space | Primary action | Click / tap |

Title → Start → Stage 1 → Stage 2 → Result → Retry / Title. Pause supports resume, confirmed stage restart and confirmed return to title. Local results persist through `SaveStore`. Developer tuning is rendered by the Addon outside the game. Both EN and zh-CN are bundled.

Event-driven callouts teach movement, collection and power-ups; title arms replay. Preserve [shared skip/dismissal and developer-control separation](#tutorial-content-boundary).

## Stage and score contracts
`StageCatalog.STAGES` stores 21×17 layouts, palette, spawn positions and speed multipliers. Validation checks legal symbols, rectangular rows, exactly one player spawn, enemies, collectibles and connected walkable cells. Add stages through catalog data rather than duplicating the controller.

| Symbol | Meaning | Score/event |
| --- | --- | --- |
| `#` | Connected solid wall | None |
| `.` | Energy node | `energy`: 10 |
| `o` | Overdrive; permits disabling sentinels | `overdrive`: 50 |
| `s` | Shield; collision protection | `powerup`: 75 |
| `t` | Slow field; enemy slowdown | `powerup`: 75 |
| `m` | Magnet; nearby connected-node collection | `powerup`: 75 |
| `P` | Player spawn | None |
| `E` | Sentinel spawn | `sentinel_disable`: 200 when overdriven |

Clearing a stage awards `stage_clear`: 500. `RunScoreService.authored_collectible_maximum()` excludes repeatable sentinel disables. Score is bounded to 999,999,999; `finalize()` produces an immutable once-only result containing score, stage, outcome, duration, tutorial status and integrity marker.

## Minimalist presentation
`ScreenFactory` owns graphite panels, hairlines, cool blue focus states, neutral secondary buttons and the light primary action. Real Godot text renders every label. The HUD uses separate status and wrapping effect rows; power-up timers remain visible in portrait. World scaling fits below the HUD and leaves touch-control space. Actor sprites remain centered on their grid positions without vertical bobbing.

The reference uses eleven separately generated original raster assets. Prompt/source/runtime mappings are recorded in `assets/template/provenance/IMAGEGEN.md`. Original generated outputs remain in generation history; shipped media is restored from permanent CDN mappings. No old picnic art or default sound assets are part of the current payload. Connected maze walls and path-facing borders remain code-native.

The effects director owns twelve recycled burst emitters and one ambient emitter. Reduced Motion disables ambient movement and uses restrained feedback. The default particle density is 0.4 and studio filter strength is 0.18. There is no continuous player trail.

## Optional audio engine
`AudioDirector.CUES` retains semantic IDs for logical title/gameplay music, confirm/cancel, action, impact, reward, pause, victory and defeat. All default paths are empty. Master/Music/SFX/UI buses, two music channels through the [shared browser BGM adapter/native fallback](#browser-safe-lifecycle), eight effect voices, two UI voices, crossfade, looping, pause ducking and mute/volume remain intact.
Apply [shared audio sourcing](#audio-production-as-needed): music routes remain empty for silent BGM or share one approved track; required SFX are independent.

```gdscript
# After importing project-owned audio:
AudioDirector.register_cue("score.reward", "res://assets/template/audio/reward.ogg")
# Only when BGM is implemented under the shared audio workflow:
AudioDirector.register_cue("music.title", "res://assets/template/audio/music.ogg")
AudioDirector.register_cue("music.gameplay", "res://assets/template/audio/music.ogg")
```
`register_stream()` accepts an existing AudioStream; `unregister_cue()` clears it. Empty/missing resources, wrong resource types, null streams and unknown IDs return `false` safely. A missing music route stops previous music. There are no synthetic fallback tones or automatic downloads. A real button/input gesture unlocks playback. Tests use an in-memory fixture, never a shipped placeholder file. See `assets/template/provenance/AUDIO.md`.

## Local standings
Local top-ten standings persist scores and configuration provenance. No dormant global-leaderboard adapter or hidden network UI ships. Add global integration only when explicitly requested.

## Localization and fonts
Uploads follow [shared typography](#default-typography); update project-local checks. Visible copy uses `I18n.t(key, replacements)`. By default `assets/template/fonts/ui_regular.tres` binds the template body face with `assets/template/fonts/NotoSansSC-VF.subset.woff2` fallback. Verify its full common-character repertoire:
```bash
python3 tools/install_cjk_font.py --check
```
Checks cover key parity, Chinese/player-name glyphs and the primary font. Default runtime/loader fonts are OFL; keep each `<Family>-OFL.txt` beside its font. `tools/build_cjk_font_subset.py` launches the default-font installer; fixed loader text uses a separately pinned subset.
