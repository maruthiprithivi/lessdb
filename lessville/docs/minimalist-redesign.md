# Generic Game Template — Precision Play

## Concept proposal

Replace the storybook picnic with a minimalist precision arcade. A graphite
surface, porcelain runner, brushed-metal sentinels, blue energy nodes, and
subtle glass edges borrow the restraint and material clarity of Apple industrial
design without logos or copied product artwork. The title is exactly
`Generic Game Template`; its primary action is `START GAME`.

Preserve the two-stage collection/chase simulation, keyboard/gamepad/touch input,
pause/retry, local standings, localization, tuning, and audio services. This is
a presentation redesign of the generic reference, not a new gameplay engine.

### Visual system

- Canvas: charcoal #101216; panels #1C2027; hairlines #353D49.
- Type: Figtree body face with Noto Sans SC for Chinese, Sora for display type (all subset WOFF2, OFL); white primary text,
  slate secondary text, generous spacing, sentence case except the start action.
- Accent: blue #70B7FF. Amber signals overdrive, teal shields, blue slow fields,
  and silver magnet fields. Shape distinguishes pickups as well as color.
- Actors: centered top-down precision pieces, no faces, ears, costumes, or bounce.
- World: quiet connected walls, small energy nodes, bounded low-amplitude
  collection bursts, restrained focus rings; all geometry retains its collision.
- Menus: a compact centered title stack with one primary action, neutral
  secondary actions, subtle generated glass artwork behind live text.
- HUD: score, stage, lives, pause, and a separate wrapping effect row so active
  pickups remain readable in portrait. Menus scale to available space.
- Audio: explicitly silent by default. No BGM or SFX assets, placeholders, or
  synthetic fallback tones ship. Missing/unassigned resources are a normal state.

## Detailed implementation plan

1. Inventory existing resource paths, cue registration, UI owners, stage data,
   localization, save contracts, and tests. Retain compatible save/tuning IDs.
2. Generate each original raster asset separately with GPT Image 2: backdrop,
   foreground, runner, sentinel, energy node, overdrive, shield, slow field,
   magnet, title glass, and pause glass. Keep text in Godot. Preserve generation
   prompts and source references; normalize display sizes without repainting.
3. Replace every creative runtime reference and remove obsolete shipped media.
   Use semantic asset directories. Verify actual transparency and silhouette
   padding; keep actors centered on their authoritative cell.
4. Introduce shared graphite UI design values, primary/secondary/focus/disabled states,
   responsive title and overlays, a readable two-row HUD, and restrained effects.
   Retheme tutorial, results, standings, confirmation, touch, and tuning surfaces.
5. Rename the application and metadata; update EN/zh-CN copy and all safe
   theme-specific code IDs together; verify the fixed common SC font with tools/install_cjk_font.py --check.
6. Preserve bounded audio pools/buses/unlock/ducking; add validated optional cue
   registration, silent missing-resource handling, safe music-route transitions,
   and tests for empty, missing, wrong-type, and registered audio.
7. Verify tutorial skipping and release tuning gates while updating those UIs.
   Keep gameplay tuning defaults; reduce only presentation defaults.
8. Run deterministic Godot tests, real-renderer native captures in landscape and
   portrait in both languages, clean-scaffold verification, Web export and
   isolated exported-pack boot/resource checks.
9. Publish final assets through the approved CDN publisher, validate byte hashes,
   regenerate the README from its recipe, and package the matching README Skill.
10. Commit and push `jun/game-dev`, fetch/merge into Sandbox `test`, run affected
    promotion checks, and push. Record CDN and deployment status independently.

## Asset contracts

| Asset | Runtime use | Contract |
| --- | --- | --- |
| environment/backdrop.png | Full viewport | Opaque landscape, aspect-cover |
| environment/foreground.png | Viewport perimeter | Transparent center, subtle edge strokes |
| characters/runner.png | Player and title/loader identity | Centered porcelain disc, alpha, square |
| characters/sentinel.png | Enemies | Centered metallic rounded diamond, alpha, square |
| powerups/energy.png | Regular collectible | Small blue luminous bead, alpha |
| powerups/overdrive.png | Enemy vulnerability | Amber precision bolt, alpha |
| powerups/shield.png | Collision protection | Teal shield, alpha |
| powerups/slow_field.png | Enemy slowdown | Blue hourglass, alpha |
| powerups/magnet.png | Nearby collection | Silver horseshoe, alpha |
| ui/title_glass.png | Title backing | Subtle frame, transparent interior/exterior |
| ui/pause_glass.png | Pause backing | Subtle frame, transparent interior/exterior |

Image generation provenance lives in `assets/template/provenance/IMAGEGEN.md`.

## Execution record — 2026-09-07

All ten implementation steps are represented in the delivered source and its
promotion workflow. Eleven original GPT Image 2 visuals replace the complete
storybook asset set. All ten placeholder audio files are removed; optional audio
registration, pooled playback, buses, unlock, music transitions, and pause ducking
remain available. Unassigned, missing, and invalid audio resources return safely.

The title, HUD, touch controls, tutorial, pause, confirmation, results, local
leaderboard, and debug tuning panel now use the same graphite visual system.
English and Chinese layouts were captured with the native renderer at 1280×720
and 720×960. Menus scale from fixed design dimensions, touch controls remain in
the viewport, tutorial skipping works throughout onboarding, and both bundled
fonts use signed-distance-field rendering for crisp scaled text.

Verification passed 448 deterministic Godot checks. A Web release export and an
isolated exported-pack probe passed identity, resource inclusion, excluded
development content, both font catalogs, startup, gameplay, and silent audio
checks. The exported game pack is 1,555,892 bytes; engine WebAssembly is separate.
Native capture evidence covers nine menu/overlay layouts plus gameplay in both
orientations. These checks do not claim browser acceptance.

The approved asset publisher published and verified all 13 manifest resources
(11 PNGs and two fonts), totaling 1,929,783 bytes. Twelve resources were uploaded
and the existing font resource path was preserved. The current starter revision binds the UI composites through `assets/template/fonts/ui_*.tres`. Empty-directory hydration and
local size/SHA-256 checks passed. Binary media is CDN-restored rather than stored
in Git; committed import sidecars preserve texture and font settings.

The canonical README recipe was regenerated, its 12 renderer tests passed, and
the matching `webdev-readme-game-generic.zip` Skill was packaged and checked.
Skill packaging does not publish a dev-admin Skill record. Source and `test`
promotion commit IDs are recorded by Git and the delivery report.

## Cleanup follow-up — 2026-09-07

Removed the dormant global-leaderboard autoload and hidden transport UI, the
unused title tuning button, legacy wrappers/getters, unobserved signals, unused
integrity-reason bookkeeping, redundant HUD arguments, and obsolete animation
counters. Sixteen unused translations were removed from each catalog. Local
score records, eligibility, save migration, tuning boundaries, and optional audio
interfaces retain their behavior.

Native capture now lives in `tools/capture_native.gd`, outside the playable
export. `tools/audit_resources.mjs` checks runtime file reachability, asset
references, manifest hashes, and sidecar ownership. It also rejected injected
stray media, unreachable scripts, and orphaned sidecars in a disposable fixture.
All eleven images and both fonts remain actively referenced; no current media
needed deletion or republishing.

Regression coverage now includes result submission through the UI controller,
duplicate prevention, disk reload, detached leaderboard rows, language switching,
and restart after saving. Tests start with clean standings and restore user files
byte-for-byte, including on macOS where XDG_DATA_HOME does not redirect user://.
Eleven native captures cover gameplay and menus in both orientations; the source
suite, cold-cache rebuild, and isolated exported-pack checks provide the cleanup
verification evidence. Browser acceptance remains a separate user validation.
