# Town artwork provenance and integration

Generated on 2026-11-17 with the built-in image-generation tool, using existing
Lessville art as style references. The original generated files remain in the
Codex generated-images directory. Project-bound copies are:

- `assets/game/characters/residents.png`: RGBA, 887×1774, eight rows/four columns.
- `assets/game/characters/animals.png`: RGBA, four rows/four columns.
- `assets/game/props/traffic.png`: RGBA, 2172×724, car/van/cyclist columns.

The tool did not return the requested canvas dimensions. Integration reads actual
image dimensions and measures alpha ≥0.5 per cell. Low-alpha noise in transparent
gutters must not enlarge the actor. The van’s bumper crosses the nominal two-thirds
boundary; the split is explicitly moved to 69%, inside the blank gutter. Preserve
alpha and source proportions. `town_art.gd` caches AtlasTextures; it does not write
new raster variants. `world_scale.gd` owns world dimensions. The native scale atlas
checks all 32 resident cells, distinct walk/back frames, eight mapped appearances,
vehicle ordering and dog consistency between town and interior views.

Appearance is stable by resident number: `(number * 5 + number / 8) % 8`, with
zero-based number and integer division. Eight illustrated appearances are reused.
They are not intended to establish a resident’s gender, ethnicity or age model.
Walking has two front and two back frames; sideward travel mirrors the front view.
A dedicated side-view cycle and individually authored art for every resident remain
future art work. The cyclist has one pose, not a pedalling animation.

## Resident atlas prompt

Create a production-ready pixel-art NPC animation ATLAS for the cozy town simulation
Lessville. The three reference images show the existing cast's exact style:
expressive friendly hand-pixelled adults, dark subtle outlines, compact chibi
proportions, finely detailed face and clothing, warm limited palettes, about 48x56
logical pixels per person. Match this aesthetic, not block placeholders or
photorealism. This is ONE sprite atlas asset, transparent background with no text,
no labels, no grid lines, no scenery, no ground shadows. Exact regular layout:
4 equal columns by 8 equal rows on a 1024x2048 canvas; each cell 256x256. Each row
is ONE distinct adult resident, the exact same identity and outfit in all four
columns. Columns: 1 front-facing walking left-foot-forward; 2 front-facing walking
right-foot-forward; 3 BACK-facing walking left-foot-forward; 4 BACK-facing walking
right-foot-forward. Back frames show back of head and hair, never eyes/face.
Row 1 warm brown skin woman, chestnut bob, sage cardigan, cream shirt, indigo
trousers. Row 2 tan man, wavy black hair, burgundy jacket, gray trousers. Row 3 dark
brown skin woman, curly black hair, mustard blouse, plum trousers. Row 4 elderly
light-skinned man, gray swept hair, round spectacles, navy cardigan, beige trousers.
Row 5 light brown woman, long braid, terracotta blouse, navy skirt and boots. Row 6
medium brown man, short dark hair, teal overshirt, cream trousers. Row 7 pale woman,
short auburn hair, muted blue jacket, charcoal jeans. Row 8 older brown-skinned
woman, silver hair in bun, mauve cardigan, dark teal trousers. Everyday believable
townspeople, no props, no hats, no backpacks, no clones of main cast. Each full-body
character is centered horizontally within its own cell, entire head and both shoes
visible; soles baseline at cell y=232; total body height 208px with consistent size
across ALL cells. Keep generous transparent margins so no figure overlaps any other
cell. Crisp pixel clusters, no anti-aliased painted look, no gradients, no
checkerboard drawn into background. All 32 characters completely isolated with
genuine transparent alpha. Animation frames must preserve identical outfit, facial
identity, head size and body height across each row.

References: `maya_plain_0.png`, `sam_free_1.png`, `lin_plain_0.png` in the character
asset directory. Output was inspected before integration. The native small-scale
appearance is the acceptance evidence, not the generator’s promises.

## Traffic atlas prompt

Create a game-ready pixel-art vehicle atlas for Lessville, matching the reference
bus's charming detailed hand-pixelled style, warm dark outlines, crisp pixel
clusters, soft muted color palette. ONE atlas on a transparent background, no text,
no labels, no grids, no background scene or cast shadows. Landscape 1536x512 canvas,
exactly THREE equally wide 512x512 cells side by side. Cell 1 a small muted
terracotta red compact hatchback car facing RIGHT, pure side view, with two round
detailed wheels, blue tinted glass, mirrors, headlights, subtle panel shading,
proper realistic car proportions. Cell 2 a warm cream delivery panel van facing
RIGHT, pure side view, two detailed wheels, cab, windshield and sliding cargo side
door, no company logo or writing. Cell 3 an adult cyclist on a simple teal bicycle
facing RIGHT, side view, one adult with chestnut hair and sage cardigan riding
naturally, hands on handlebars and feet on pedals, two thin spoked circular wheels,
properly proportioned adult body, no childlike tiny rider. All three isolated with
genuine transparent alpha. Each object fully inside its own cell, centered
horizontally; generous 32px minimum clear margin. Keep baseline wheels at y=450 for
each object. Car fills about448px wide by210 high, van448wide by250high,
cyclist+bike340wide by400high. The provided bus is a STYLE reference only; do not
draw a bus. Render beautiful professional sprite art, not simple rectangles,
not vector art, not a photo.

Reference: `assets/game/props/bus.png`. Neither atlas changes the main cast’s art.
Source images are explicitly exempted from the scaffold’s blanket media ignore
rule so a later reviewed commit can include them; this task has not committed or
published the changes.

## Animal atlas prompt

A single professional pixel-art animal animation atlas for the cozy Lessville town
game. Match the provided game art's friendly detailed hand-pixelled style, dark
fine outlines, soft natural palette, crisp small pixel clusters. Transparent
background, no text, no labels, no grid lines, no grass or floor shadows. EXACTLY
FOUR equal columns and FOUR equal rows, square 1024x1024 image. One isolated
complete animal centered inside each256x256cell with generous transparent margins.
All animals face RIGHT in all frames. Each row has the identical animal and
identical colors across four columns. Row1 a friendly orange tabby CAT with white
chest: col1 standing idle, col2 walking left legs forward, col3 walking right legs
forward, col4 curled up sleeping. Row2 a black-and-white tuxedo CAT: same idle,
left-step, right-step, curled sleeping. Row3 a cream domestic RABBIT with brown
ears: col1 resting alert, col2 hopping stretched, col3 hopping crouched, col4
resting with eyes closed. Row4 a tiny warm brown house SPARROW: col1 perched
wingsfolded, col2 hopping wingsfolded, col3 flying with wingsraised, col4 flying
with wingslowered. Each pose must stay within its owncell. Two walk frames remain
sameoverallbody scale and samegroundbaseline, sleeping animals naturally lower but
samebodylength. Bird wingsmay spread but birdbody same size. Readable expressive
cuteanimals with believable anatomy, nohumanoid shapes, no toys. True transparent
alpha. This is one sprite-sheet asset; do not draw extra animals, patterns, ground,
icons, or decorative elements.

References: `mochi_0.png` and the generated `residents.png`. Used the built-in tool;
no external API credentials. All project assets are copied into this checkout.
