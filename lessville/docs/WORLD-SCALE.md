# World scale contract

Decision: 2026-11-17, responding to the undersized supporting residents and traffic.
Owner: `scripts/lessville/world_scale.gd`. Evidence: native scale atlas and the
native 24-hour recording in `../outputs/lessville-live/day-recording-reviewed` relative to the checkout.

## Units and reference

This is stylized pixel art, not a metre-calibrated digital twin. Town art is
1536×1024; interiors are 768×512. The same viewport therefore magnifies interior
coordinate units by two, before actor scaling. The existing main-character canvas
is 96×112, with 104 opaque vertical pixels in the checked Maya/Lin/Sam idle art.
Keep feet fixed when changing a drawn object. Camera resizing must multiply world
objects equally; text, labels and selection controls are screen-space UI.

Main character and seat calibration stays unchanged. Town actor multiplier is
0.62/0.677, giving approximately 95.24 visible pixels. Interior multiplier 1.2 gives
124.8 visible pixels. The combined close-up is about 2.62× the on-screen town person.
This is a deliberate camera change, not a different physical person.

## Accepted art-space proportions

| Element | Town reference | Rule / implementation |
|---|---:|---|
| Main adult | 95.24 visible px | 104 opaque pixels × town actor factor |
| Supporting adult | 95.24 px target | Detailed 4×8 sprite atlas; opaque bounds normalized with ≤1 px gutter allowance |
| Adult ground footprint | radius 20 px | Both tiers use the same follower footprint |
| Mochi | 39.0 visible px | 44 opaque pixels × town dog factor; same dog/adult ratio inside |
| Car | 181 px long | Length 1.9 adult heights; retain artwork aspect ratio |
| Delivery van | 224 px long | Length 2.35 adult heights; retain artwork aspect ratio |
| Bicycle + rider | 100 px high | 1.05 adult heights including bicycle; retain artwork aspect ratio |
| Bus | 420 × 151.2 px canvas | Existing detailed art uniformly scaled 1.4×; taller than the van |
| Traffic signal | about 109.5 px high | 1.15 adult heights; anchored to existing crossing |
| Cats | 24.8 px standing | 0.26 adult heights; sleeping frames keep the same body scale |
| Garden rabbit | 18.1 px resting | 0.19 adult heights; confined to the home garden |
| Sparrows | 9.5 px perched | 0.10 adult heights; wings retain their larger span |
| Coffee prop | 14 × 18 px canvas | Small hand-held item, not the old shared bag-sized rectangle |
| Bag/backpack prop | 28 × 34 px canvas | Category-specific proportions |

These are explicit design ratios calibrated against this art. They are not claims
that a car or bus has a particular real-world dimension. Wheel, ground and shadow
anchors remain tied to the roadway route. Traffic previously reused sidewalk nodes;
road endpoints and direction-separated lanes now replace that presentation path. Supporting residents now use detailed artwork rather than procedural rectangles.
`town_art.gd` slices transparent atlases, ignores low-alpha gutter noise when
finding body bounds, caches frames, and preserves source aspect ratios. The van
uses a 69% column boundary because its generated bumper crosses the nominal grid.
Eight appearances provide two front and two back walking frames each. A stable
resident-ID mapping keeps an individual’s appearance unchanged between sessions.
These are eight reusable appearances, not 95 individually authored designs.
Walking phase follows distance; stationary residents stop cycling.

## Furniture, buildings and projection

Painted buildings, doors, tables and chairs are part of immutable background art in
this change. Their camera scaling remains uniform. Do not independently stretch
furniture overlays or buildings to compensate for a wrongly sized character.
Seats use hip row 78, shared actor factors and `config/seats.json` contact anchors;
`tools/seatlib.py` must remain numerically consistent. A future change to adult
height must update renderer, catalog, Python math, selection bounds, held items and
seat render checks together.

A metric building/door/furniture survey has NOT been completed. Existing desk/floor
artifacts and seat overflow contact remain known gaps. Before replacing painted
assets, record door clearance, chair seat/contact height, table clearance, foreground
occlusion and route clearance in the attribute register. Do not silently mark these
as physically accurate merely because the common camera multiplier is correct.

## Population and visibility

The 1536×1024 image is a town-centre viewport, not the whole district. Its central
promenade is roughly 660 px long. A 40 px adult foot diameter plus comfortable
clearance supports a handful of simultaneous walkers, not 95 bodies lined up.
The adopted local cast is **five focal + 12 supporting = 17**, with **83 supporting
residents in surrounding neighbourhoods**. All 100 continue their daily-life
simulation and retain identity/history; no residents are deleted. District membership
is stable (`citizen-01` through `citizen-12` are local). The API exposes `district`,
`population.town_center` and `population.surrounding` so this is an explicit model
boundary, not a camera randomly suppressing people at the same place.

Regional locations currently share the same abstract activity vocabulary but belong
to their own district; individual surrounding streets/buildings are not rendered.
Main and supporting behavior depth remains different. Supporting interior occupancy
is not individually rendered, and building seat capacity does not yet constrain
supporting workplace admission. This is a bounded visual population policy, not a
validated housing/land-use capacity model.

Local starts are staggered every three minutes across 36 minutes. Three local
supporting residents choose park leisure; one has park work. Leisure positions
use x=550/780/1130, with the gardener at x=1080. Five focal park positions
use x=620/700/860/940/1020; the bus shelter has its own five standing slots.
This avoids reusing a single cluster of offsets for every outdoor place. Town drawing
shows local travel and outdoor stays; indoor and regional residents keep simulating.
A seven-day minute-by-minute check found a peak of **7 local supporting residents
outdoors** (test ceiling 8). The five focal residents are additional, so the
absolute tested supporting ceiling is not a total crowd cap. All visible people and
the dog share depth sorting. The west zebra now has direction-separated walking
lanes and a queued entry rule. Head-on meetings on other narrow sidewalks and
multiple people stopping at a shared destination remain unresolved.

## Verification and extension

- `tools/render_scale.gd` calls the real actor, supporting resident, dog, traffic
  and bus drawing methods. It asserts atlas frame counts and source bounds, vehicle
  ordering and equal dog/adult ratios between views, then writes a native comparison
  image. Adult height equivalence is visually inspected in that image; it is not
  an automated measurement of the rendered opaque silhouettes.
- Run `test/test_suite.tscn` for navigation, seat, attire and existing art checks.
  The crossing oracle uses radius 20 for BOTH adult tiers.
- Run `test/walker_clearance.gd` for the west crossing's 44 px lane separation,
  exact-boundary destination reversals, entry admission, pause stability and
  junction-preserving route changes, including bent approaches and U-turn progress.
- Inspect the live town at work departure, park arrival and night, not just an atlas.
- Record source revision/diff, renderer, scene time, test outcomes and open gaps in
  [the learning log](egm/LEARNING-LOG.md).
- Other experimental physical/robot/security simulators retain their own coordinate
  units. Do not copy these pixel ratios into metre-based robot geometry. Their
  unit adapters and object-level calibration remain a separate audit item.

## Road clearance and animals

`road_traffic.gd` is the shared presentation-space controller for the bus, car,
van and bicycle. Each direction is an ordered lane queue; centre separation is
half the sum of reserved body lengths plus 28 px. The bicycle's 100 px reservation
conservatively covers its approximately 99.1 px rendered width; the other vehicles
reserve their rendered lengths. Cars turn only after leaving the view.
A previous bus leaves before a new trip reuses its visual; pauses freeze traffic.
This replaces the independent bus and traffic drawing that overlapped in the user’s
screenshot. Backend targets and bus timetable remain authoritative intent: renderer
queue delays do not yet feed back into passenger arrival or backend traffic events.
Do not call that a coupled traffic/passenger physics model.

The west pedestrian approach junction is (215,640), set back on the existing
pavement so a turning walker does not obstruct the crossing mouth. Southbound
feet use x=193, northbound feet x=237, with the corner covered by an explicit
walkable polygon. Mid-crossing reroutes finish their current lane before returning.
Newly visible residents wait for 44 px clearance at shared entry points; a partially
visible person reserves that space. This is presentation queuing, not a population
cap or backend travel-time correction. Other mid-segment reroutes preserve their
current route through the next graph junction. Follower lookahead excludes strongly
opposing headings so reciprocal U-turns do not wait on each other.
Final dense seeds 1, 7, 19 and 23 report zero zebra and same-direction follower
overlaps, with every final goal reached. Minimum crossing-zone margin across those
runs was approximately 1.24 px; the 44 px lane-centre separation does not guarantee
a 4 px margin throughout the approaches. Shared stationary destinations and general
head-on passing remain unresolved, so there is no general collision-free claim.
The native 24-hour video above predates these crossing and rerouting repairs.

`town_animals.gd` adds two cats (Miso and Pepper), Clover the garden rabbit, and
two sparrows. All are ambient, clock-driven routines rather than autonomous agents.
They are visible 07:00–19:00, remain in defined porch/garden/roof habitats, and only
one bird flies at once. Resting and sleeping frames keep each animal’s physical
sprite unit; they are not enlarged to the standing pose height. They add no people
to the 100-resident population and do not yet have persistent needs or ownership
interactions. Mochi retains the existing owner/leash behavior.
