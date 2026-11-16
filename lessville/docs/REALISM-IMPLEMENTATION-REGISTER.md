# Realism implementation register

Source: supplied `lessville-claude-lite/spec/realism-spec.yaml`, 2026-11-16 SGT. Read with [the small-world plan](SMALL-WORLD-PLAN.md). Original claims below are retained for audit, not independently certified. Baseline status is the supplied report; see the execution handoff for new local results. Proposed checks do not count as implemented coverage.

188 attributes; 84 missing checks; 5 reported failing attributes.

For each proposed check: create a known-bad fixture or mutation that violates the rule, assert the measured outcome, then run the corrected implementation. References with TBD/unreviewed basis are design candidates. Pixel tolerances and scenario hours remain design choices even when an empirical source is attached.

## characters_body

### `characters_body.named_walkers.scale`

- Entity/attribute: named walkers / screen_height_ratio.
- Rule: Named character sprites must maintain a consistent pixel height relative to the map scale, appearing approximately 1.5-2x taller than the bus shelter roof line.
- Scenario target: 48-64 px
- Basis: real-world (cited). Source: D5: NCD-RisC eLife 2016 country height data (CSV) https://www.ncdrisc.org/downloads/height/NCD_RisC_eLife_2016_height_age18_countries.csv.
- Supplied reference value: D5: Singapore, born 1996: men 172.6 cm (95% UI 170.8–174.3), women 160.3 cm (158.6–162.0)
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_body.named_walkers.scale`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Compare the vertical extent of a standing character sprite against the height of the bus shelter or café window frames to ensure proportional consistency.

### `characters_body.named_walkers.stride_length`

- Entity/attribute: named walkers / stride_distance.
- Rule: The horizontal distance covered per full walk cycle animation loop must match the configured WALK_STRIDE constant.
- Scenario target: 34.0 px
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_body.named_walkers.stride_length`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Observe the foot placement during a walk cycle to ensure the step length is consistent and does not appear to slide or float.

### `characters_body.named_walkers.walk_speed`

- Entity/attribute: named walkers / velocity.
- Rule: The movement speed of named characters must equal WALK_SPEED multiplied by the current clock speed multiplier.
- Scenario target: 190.0 px/s at x1
- Basis: real-world (cited). Source: A1: Knoblauch, Pietrucha, Nitzburg, "Field Studies of Pedestrian Walking Speed and Start-Up Time", TRR 1538 (1996): abstract https://doi.org/10.1177/0361198196153800104 ; mean values summarised at https://www.usroads.com/journals/p/rej/9710/re971001/; A4: FHWA-RD-98-107, Ch. 3, Table 4 https://www.fhwa.dot.gov/publications/research/safety/pedbike/98107/section3.cfm.
- Supplied reference value: A1: 1.51 m/s (4.95 ft/s) mean; 15th percentile 1.25 m/s (4.09 ft/s); A4: ≥ 1.3 m/s average at LOS A (space ≥ 5.6 m²/ped)
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_body.named_walkers.walk_speed`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Time a character moving across a known distance to verify the speed matches the expected value for the current game speed setting.

### `characters_body.named_walkers.facing`

- Entity/attribute: named walkers / orientation.
- Rule: The sprite facing (front, back, left, right) must strictly match the direction of the velocity vector every frame.
- Scenario target: 0 deg error
- Basis: unreviewed. Source: TBD.
- Check: `suite.facing_matches_heading`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Check that when a character turns a corner, the sprite flips or rotates to face the new direction of travel immediately.

### `characters_body.named_walkers.idle_posture`

- Entity/attribute: named walkers / idle_animation.
- Rule: When a character is stationary, the sprite must switch to an idle frame and stop animating the walk cycle.
- Scenario target: static frame
- Basis: unreviewed. Source: TBD.
- Check: `suite.facing_matches_heading`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify that a stopped character does not continue to animate leg movement or slide forward while standing still.

### `characters_body.maya.attire_work`

- Entity/attribute: maya / apron_visibility.
- Rule: Maya must wear an apron layer only when located in the café or its terrace during her shift hours.
- Scenario target: shift 06:30-20:00
- Basis: real-world (cited). Source: F1: Environmental Public Health (Food Hygiene) Regulations, reg. 23(2)(a), official text via https://laws.sg/legislation/sub-leg-1987-epha-rg16/reg-23.
- Supplied reference value: F1: Law requires anyone selling or preparing food to "wear clean attire and keep his body clean". It does **not** require an apron, so "aprons on café staff at work" is a design rule, not a legal one
- Check: `suite.attire`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Inspect Maya's sprite in the café interior vs. on the town street to confirm the apron layer is present indoors and absent outdoors.

### `characters_body.bo.attire_work`

- Entity/attribute: bo / towel_and_apron.
- Rule: Bo must display both a teal apron and a white bar towel layer only when working in the café or on the terrace.
- Scenario target: shift 07:00-15:00
- Basis: real-world (cited). Source: F1: Environmental Public Health (Food Hygiene) Regulations, reg. 23(2)(a), official text via https://laws.sg/legislation/sub-leg-1987-epha-rg16/reg-23.
- Supplied reference value: F1: Law requires anyone selling or preparing food to "wear clean attire and keep his body clean". It does **not** require an apron, so "aprons on café staff at work" is a design rule, not a legal one
- Check: `suite.attire`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Zoom in on Bo's sprite to verify the towel and apron are visible during café service and removed when he is elsewhere.

### `characters_body.bo.attire_cap`

- Entity/attribute: bo / cap_persistence.
- Rule: Bo's navy cap must remain visible regardless of location or shift status, as it is personal signature attire.
- Scenario target: always visible
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `suite.attire`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Check Bo's head area in both café and non-café locations to ensure the cap is consistently present.

### `characters_body.lin.attire_commute`

- Entity/attribute: lin / cap_and_backpack.
- Rule: Lin must wear a cap and carry a backpack only when commuting (transit, bus, or outdoor street) and remove them when settled indoors.
- Scenario target: commute state only
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `suite.attire`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Compare Lin's sprite on the bus vs. at her home or café seat to confirm the backpack and cap disappear when she is seated indoors.

### `characters_body.lin.hair_profile`

- Entity/attribute: lin / hair_coverage.
- Rule: Lin's profile hair frames must cover the skull area completely without gaps when viewed from the side.
- Scenario target: full skull coverage
- Basis: unreviewed. Source: TBD.
- Check: `render.lin_hair`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Examine a side-view crop of Lin to ensure no background pixels are visible through the hair mesh.

### `characters_body.ama.pet_leash`

- Entity/attribute: ama / leash_connection.
- Rule: A visual leash line must connect Ama's hand to Mochi's collar whenever they are both present and moving.
- Scenario target: continuous line
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_body.ama.pet_leash`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Look for a thin line connecting the parent character to the dog sprite, ensuring it stretches and contracts naturally with movement.

### `characters_body.mochi.scale`

- Entity/attribute: mochi / size_ratio.
- Rule: Mochi's sprite height must be approximately 30-40% of the height of the adult character walking beside it.
- Scenario target: 0.3-0.4 ratio
- Basis: real-world (cited). Source: D5: NCD-RisC eLife 2016 country height data (CSV) https://www.ncdrisc.org/downloads/height/NCD_RisC_eLife_2016_height_age18_countries.csv.
- Supplied reference value: D5: Singapore, born 1996: men 172.6 cm (95% UI 170.8–174.3), women 160.3 cm (158.6–162.0)
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_body.mochi.scale`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Compare the dog's height to the parent's height to ensure the pet appears appropriately small relative to the human.

### `characters_body.mochi.gait`

- Entity/attribute: mochi / walk_cycle.
- Rule: Mochi's leg animation must sync with Ama's walking speed to prevent the dog from appearing to float or lag behind.
- Scenario target: synced velocity
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_body.mochi.gait`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Observe the dog's legs while walking to ensure the stride frequency matches the owner's pace.

### `characters_body.background_pedestrians.speed`

- Entity/attribute: background pedestrians / velocity.
- Rule: Background anonymous citizens must move at a fixed speed of 72 px/s, independent of the named character speed multiplier.
- Scenario target: 72.0 px/s
- Basis: real-world (cited). Source: A1: Knoblauch, Pietrucha, Nitzburg, "Field Studies of Pedestrian Walking Speed and Start-Up Time", TRR 1538 (1996): abstract https://doi.org/10.1177/0361198196153800104 ; mean values summarised at https://www.usroads.com/journals/p/rej/9710/re971001/; A4: FHWA-RD-98-107, Ch. 3, Table 4 https://www.fhwa.dot.gov/publications/research/safety/pedbike/98107/section3.cfm.
- Supplied reference value: A1: 1.51 m/s (4.95 ft/s) mean; 15th percentile 1.25 m/s (4.09 ft/s); A4: ≥ 1.3 m/s average at LOS A (space ≥ 5.6 m²/ped)
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_body.background_pedestrians.speed`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Time a background walker to ensure they move slower than named characters and maintain a consistent pace.

### `characters_body.background_pedestrians.palette`

- Entity/attribute: background pedestrians / color_variety.
- Rule: Background pedestrians must use a procedural palette for torso, legs, and head to ensure visual variety without unique sprites.
- Scenario target: procedural palette
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_body.background_pedestrians.palette`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check that multiple background walkers have different clothing colors and skin tones to avoid a 'cloned' look.

### `characters_body.seated_posture.hip_anchor`

- Entity/attribute: seated characters / hip_position.
- Rule: When sitting, the character's hip anchor point must align with the seat surface defined in the seat catalog.
- Scenario target: seat surface y-coordinate
- Basis: real-world (cited). Source: D3: UC task-chair checklist https://uhs.berkeley.edu/sites/default/files/uc_task_chair_requirements_2.pdf.
- Supplied reference value: D3: task-chair seat adjustable within 15–22 in (≈ 38–56 cm, conv.)
- Check: `render.seats`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify that the character's lower body does not intersect the chair or float above it when seated.

### `characters_body.seated_posture.facing`

- Entity/attribute: seated characters / orientation.
- Rule: Seated characters must face the direction of the seat's designated orientation (e.g., facing the desk or table center).
- Scenario target: seat-defined direction
- Basis: unreviewed. Source: TBD.
- Check: `render.seats`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Check that a character sitting at a café table faces the table, not the wall or the exit.

### `characters_body.office_typing.posture`

- Entity/attribute: office workers / typing_animation.
- Rule: Characters in the office with 'typing' activity must use the specific typing sprite frames and remain stationary at the desk.
- Scenario target: typing frames
- Basis: real-world (cited). Source: E1: MOM Hours of work https://www.mom.gov.sg/employment-practices/hours-of-work-overtime-and-rest-days; E2: Attorney-General's Chambers contact page https://www.agc.gov.sg/contact-us/contact-info/ (one agency; not a national rule).
- Supplied reference value: E1: contract hours for ≤ 5-day week: up to 9 h/day or 44 h/week; E2: 8.30am–6pm Mon–Thu, 8.30am–5.30pm Fri
- Check: `suite.realism`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Observe an office worker to ensure they are using the 'type' animation and are not standing or walking while working.

### `characters_body.cafe_serving.posture`

- Entity/attribute: cafe staff / counter_position.
- Rule: Café staff must remain behind the counter line when serving, not crossing into the customer queue area.
- Scenario target: behind counter
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `suite.realism`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Check that Maya or Bo do not walk past the counter into the seating area while performing serving activities.

### `characters_body.props.carrying`

- Entity/attribute: characters with props / prop_visibility.
- Rule: Carried props (e.g., grocery bag, coffee) must be visible in the character's hand and occluded correctly by the arm.
- Scenario target: hand anchor
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_body.props.carrying`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Look for the prop in the character's hand area to ensure it is not floating or hidden behind the body incorrectly.

### `characters_body.transit_snapshot.position`

- Entity/attribute: transiting characters / street_rendering.
- Rule: Characters in transit (bus or walking) must be rendered on the street layer, not inside the interior close-up view.
- Scenario target: street layer
- Basis: unreviewed. Source: TBD.
- Check: `suite.realism`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify that a character on the bus is visible on the town map street, not inside the bus interior scene unless specifically zoomed in.

### `characters_body.new_sprite.palette`

- Entity/attribute: new character sprites / color_consistency.
- Rule: New sprite frames must use the same color palette as existing frames for the same character to ensure visual continuity.
- Scenario target: existing palette
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_body.new_sprite.palette`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Compare the colors of a new frame against an existing frame to ensure no hue shifts or brightness changes.

### `characters_body.new_sprite.anchors`

- Entity/attribute: new character sprites / pivot_point.
- Rule: New sprite frames must have the same pivot point (feet center) as existing frames to prevent jumping when switching animations.
- Scenario target: feet center
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_body.new_sprite.anchors`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Switch between animation frames to ensure the character does not shift vertically or horizontally unexpectedly.

### `characters_body.new_sprite.size`

- Entity/attribute: new character sprites / dimensions.
- Rule: New sprite frames must match the pixel dimensions of the existing sprite sheet for that character.
- Scenario target: existing sheet size
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_body.new_sprite.size`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check the bounding box of the new frame to ensure it aligns with the grid of the existing sprite sheet.

### `characters_body.new_sprite.front_back`

- Entity/attribute: new character sprites / view_coverage.
- Rule: New character sets must include front, back, left, and right facing frames to support all navigation directions.
- Scenario target: 4 directions
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_body.new_sprite.front_back`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify that the character can face all four cardinal directions without missing frames.

### `characters_body.new_sprite.seat_frames`

- Entity/attribute: new character sprites / seated_variants.
- Rule: New character sets must include seated frames for both east and west facing directions to support café and office seating.
- Scenario target: seat_e, seat_w
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_body.new_sprite.seat_frames`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check that the character has distinct seated poses for facing left and right.

### `characters_body.terrace_sitter.occlusion`

- Entity/attribute: terrace sitters / layer_order.
- Rule: Terrace sitters must be rendered above the furniture but below any foreground foliage or window glass that overlaps their position.
- Scenario target: correct z-index
- Basis: unreviewed. Source: TBD.
- Check: `suite.terrace_sitter_layers`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Ensure that a sitter's head is not covered by the café window glass or foreground plants when sitting on the terrace.

### `characters_body.crossing_overlap.spacing`

- Entity/attribute: walkers at crossings / footprint_distance.
- Rule: No two walker footprints may overlap at zebra crossings, maintaining a minimum separation distance.
- Scenario target: no overlap
- Basis: real-world (cited). Source: A6: FHWA-RD-98-107 Table 3 and Table 4 (same URL as A4), sources Fruin 1971, TRB 1994.
- Supplied reference value: A6: Fruin walkway LOS A ≥ 3.2 m²/ped; B 2.3–3.2; C 1.4–2.3; D 0.9–1.4; E 0.5–0.9; F ≤ 0.5. HCM (1994) LOS A ≥ 12 m²/ped. FHWA-recommended LOS A ≥ 5.6 m²/ped
- Check: `suite.crossing_overlap`. Status: **REPORTED FAIL**.
- 4× review cue: Observe dense traffic at a crossing to ensure characters do not visually merge or clip through each other.

### `characters_body.crossing_overlap.facing`

- Entity/attribute: walkers at crossings / tangent_alignment.
- Rule: Walker facing at crossings must be within 11.5 degrees of the path tangent to ensure smooth turning.
- Scenario target: <= 11.5 deg
- Basis: unreviewed. Source: TBD.
- Check: `suite.crossing_overlap`. Status: **REPORTED FAIL**.
- 4× review cue: Check that characters turn smoothly at crossings without abrupt 90-degree snaps that look unnatural.

### `characters_body.bus_passenger.seat`

- Entity/attribute: bus passengers / seat_assignment.
- Rule: Bus passengers must be assigned to specific seat indices and rendered at the corresponding seat anchors inside the bus.
- Scenario target: bus.seat.0-6
- Basis: unreviewed. Source: TBD.
- Check: `suite.seating`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify that passengers in the bus interior are sitting in distinct seats and not overlapping each other.

### `characters_body.weather_attire.rain`

- Entity/attribute: characters in rain / rain_gear.
- Rule: Characters exposed to rain must display appropriate rain gear (e.g., umbrella or wet texture) if defined in the attire model.
- Scenario target: rain state
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_body.weather_attire.rain`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check if characters have visual indicators of rain (like an umbrella) when the weather is set to rain.

### `characters_body.cafe_staff.apron`

- Entity/attribute: maya, bo (café staff) / apron at work.
- Rule: Café staff wear an apron while working at the café. This is a DESIGN CONVENTION, not a legal rule: Singapore food hygiene law only requires food handlers to wear clean attire.
- Scenario target: legal minimum: clean attire (no apron requirement)
- Basis: real-world (cited). Source: F1: Environmental Public Health (Food Hygiene) Regulations, reg. 23(2)(a), official text via https://laws.sg/legislation/sub-leg-1987-epha-rg16/reg-23.
- Check: `render.attire`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Maya and Bo at the café show an apron; elsewhere they do not; clothes look clean.

## characters_behaviour

### `characters_behaviour.maya.shift_window`

- Entity/attribute: maya / shift_window.
- Rule: Maya is present at the café (indoor or terrace) only between 06:30 and 20:00, and is absent or off-map outside this window.
- Scenario target: 06:30-20:00
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `suite.realism`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify Maya is visible at the café counter or terrace during midday frames and absent at night frames.

### `characters_behaviour.bo.shift_window`

- Entity/attribute: bo / shift_window.
- Rule: Bo is present at the café (indoor or terrace) only between 07:00 and 15:00, and is absent or off-map outside this window.
- Scenario target: 07:00-15:00
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `suite.realism`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify Bo is visible at the espresso machine or counter during morning frames and absent at night frames.

### `characters_behaviour.maya.cafe_station`

- Entity/attribute: maya / cafe_station.
- Rule: When active at the café, Maya is positioned at the 'till' station, not at the espresso machine or customer tables.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.realism`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify Maya's sprite anchor is aligned with the register/till area, distinct from Bo's espresso station.

### `characters_behaviour.bo.cafe_station`

- Entity/attribute: bo / cafe_station.
- Rule: When active at the café, Bo is positioned at the 'espresso' station or 'fridge' station, not at the till.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.realism`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify Bo's sprite anchor is aligned with the espresso machine or fridge area, distinct from Maya's till station.

### `characters_behaviour.lin.commute_gear`

- Entity/attribute: lin / commute_gear.
- Rule: Lin wears a cap and backpack only when in 'transit', 'bus_stop', or 'away' states; these layers are removed when settled indoors (home, cafe, school, office, grocery, gym).
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `suite.attire`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify Lin has a cap and backpack while walking on the street or bus, but neither item when seated at a café table or home.

### `characters_behaviour.lin.cafe_activity`

- Entity/attribute: lin / cafe_activity.
- Rule: When Lin is at the café during study hours, she is seated at 'table1' with a 'studying' pose, not standing in the queue.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `suite.realism`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify Lin is seated at a specific table with a laptop/book prop, not standing near the counter.

### `characters_behaviour.sam.office_activity`

- Entity/attribute: sam / office_activity.
- Rule: When Sam is at the office, he is seated at a desk with a 'typing' pose, or standing in a 'meeting' pose if in a stand-up.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: E1: MOM Hours of work https://www.mom.gov.sg/employment-practices/hours-of-work-overtime-and-rest-days; E2: Attorney-General's Chambers contact page https://www.agc.gov.sg/contact-us/contact-info/ (one agency; not a national rule).
- Supplied reference value: E1: contract hours for ≤ 5-day week: up to 9 h/day or 44 h/week; E2: 8.30am–6pm Mon–Thu, 8.30am–5.30pm Fri
- Check: `suite.realism`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify Sam is seated at a desk with a typing animation or standing in a group, not floating or idle-walking.

### `characters_behaviour.ama.home_activity`

- Entity/attribute: ama / home_activity.
- Rule: When Ama is at home, she is in the 'kitchen' (cooking) or 'living' (sleeping/idle) area, never in the office or café.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.realism`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify Ama is visible in the home interior kitchen or living room, not in other locations.

### `characters_behaviour.mochi.leash_constraint`

- Entity/attribute: mochi / leash_constraint.
- Rule: Mochi is always leashed to Ama and maintains a distance of less than 1.5 meters from her, moving in sync with her velocity.
- Scenario target: < 1.5 m
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_behaviour.mochi.leash_constraint`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify Mochi is always near Ama, connected by a visual leash, and does not wander independently.

### `characters_behaviour.walkers.walk_speed`

- Entity/attribute: walkers / walk_speed.
- Rule: Named characters walk at 190 px/s at 1x speed, which scales with the square root of the clock speed multiplier.
- Scenario target: 190 px/s @ 1x
- Basis: real-world (cited). Source: A1: Knoblauch, Pietrucha, Nitzburg, "Field Studies of Pedestrian Walking Speed and Start-Up Time", TRR 1538 (1996): abstract https://doi.org/10.1177/0361198196153800104 ; mean values summarised at https://www.usroads.com/journals/p/rej/9710/re971001/; A4: FHWA-RD-98-107, Ch. 3, Table 4 https://www.fhwa.dot.gov/publications/research/safety/pedbike/98107/section3.cfm.
- Supplied reference value: A1: 1.51 m/s (4.95 ft/s) mean; 15th percentile 1.25 m/s (4.09 ft/s); A4: ≥ 1.3 m/s average at LOS A (space ≥ 5.6 m²/ped)
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_behaviour.walkers.walk_speed`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify character movement speed appears consistent with the selected game speed multiplier.

### `characters_behaviour.background_walkers.speed`

- Entity/attribute: background_walkers / speed.
- Rule: Background pedestrians walk at a constant 72 px/s, independent of the game speed multiplier.
- Scenario target: 72 px/s
- Basis: real-world (cited). Source: A1: Knoblauch, Pietrucha, Nitzburg, "Field Studies of Pedestrian Walking Speed and Start-Up Time", TRR 1538 (1996): abstract https://doi.org/10.1177/0361198196153800104 ; mean values summarised at https://www.usroads.com/journals/p/rej/9710/re971001/; A4: FHWA-RD-98-107, Ch. 3, Table 4 https://www.fhwa.dot.gov/publications/research/safety/pedbike/98107/section3.cfm.
- Supplied reference value: A1: 1.51 m/s (4.95 ft/s) mean; 15th percentile 1.25 m/s (4.09 ft/s); A4: ≥ 1.3 m/s average at LOS A (space ≥ 5.6 m²/ped)
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_behaviour.background_walkers.speed`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify background pedestrians move at a slower, constant pace compared to named characters.

### `characters_behaviour.walkers.nav_constraint`

- Entity/attribute: walkers / nav_constraint.
- Rule: All named and background walkers must remain within sidewalk and crossing polygons, never entering road or building interiors without a door transition.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.pedestrians_stay_on_nav`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify no character sprite overlaps with road asphalt or building walls except during door fade transitions.

### `characters_behaviour.walkers.facing`

- Entity/attribute: walkers / facing.
- Rule: The drawn sprite front/back/flip must match the character's walk heading every frame, and face idle direction when paused.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.facing_matches_heading`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify characters face the direction of movement, not sideways or backwards while walking.

### `characters_behaviour.cafe.queue_behavior`

- Entity/attribute: cafe_customers / queue_behavior.
- Rule: Customers at the café stand in a single-file queue behind the counter, maintaining a minimum distance of 0.5 meters between them.
- Scenario target: >= 0.5 m
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_behaviour.cafe.queue_behavior`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify customers are lined up in order, not overlapping or standing in front of the counter.

### `characters_behaviour.cafe.service_interaction`

- Entity/attribute: cafe_staff / service_interaction.
- Rule: Staff (Maya/Bo) face the customer when serving, and the customer faces the staff, with a 'talk' pose active during the transaction.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_behaviour.cafe.service_interaction`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify staff and customer sprites are oriented towards each other during the order handoff.

### `characters_behaviour.seats.occupancy`

- Entity/attribute: seats / occupancy.
- Rule: Characters seated on a seat must have their hip anchor aligned with the seat surface, and their torso visible above the seat back.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: D3: UC task-chair checklist https://uhs.berkeley.edu/sites/default/files/uc_task_chair_requirements_2.pdf.
- Supplied reference value: D3: task-chair seat adjustable within 15–22 in (≈ 38–56 cm, conv.)
- Check: `render.seats`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify seated characters are not floating above the chair or clipping through the seat back.

### `characters_behaviour.seats.facing`

- Entity/attribute: seats / facing.
- Rule: Seated characters must face the direction of the furniture they are sitting on (e.g., facing the table if at a café table).
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `render.seats`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify seated characters are oriented towards the table or desk, not away from it.

### `characters_behaviour.bus.boarding`

- Entity/attribute: bus_passengers / boarding.
- Rule: Passengers board the bus only when it is in 'dwell' phase at a stop, and are seated in available bus seats.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_behaviour.bus.boarding`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify passengers enter the bus only when it is stopped, and are visible in bus seats.

### `characters_behaviour.bus.dwell_time`

- Entity/attribute: bus / dwell_time.
- Rule: The bus dwells at each stop for a minimum of 3 seconds before departing.
- Scenario target: >= 3 s
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `suite.bus_route_and_dwell`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify the bus remains stationary at the stop for a perceptible duration before moving.

### `characters_behaviour.buildings.fade_transition`

- Entity/attribute: buildings / fade_transition.
- Rule: Characters fade out at the door when entering a building and fade in at the door when exiting, with a fade rate of 3.5 units per second.
- Scenario target: 3.5 units/s
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_behaviour.buildings.fade_transition`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify characters do not teleport into buildings but fade smoothly at the entrance.

### `characters_behaviour.sleep.hours`

- Entity/attribute: characters / sleep_hours.
- Rule: Characters are in 'sleeping' or 'away' state from 22:00 to 06:30, and are not visible in public spaces during this window.
- Scenario target: 22:00-06:30
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_behaviour.sleep.hours`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify no named characters are visible on the street or in public buildings during night hours.

### `characters_behaviour.maya.grocery_trip`

- Entity/attribute: maya / grocery_trip.
- Rule: Maya visits the grocery store around 14:00, transitioning from café to grocery and back.
- Scenario target: 14:00
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_behaviour.maya.grocery_trip`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify Maya is seen walking to the grocery store around mid-afternoon.

### `characters_behaviour.bo.trail_trip`

- Entity/attribute: bo / trail_trip.
- Rule: Bo visits the trail around 15:15, transitioning from café to trail and back.
- Scenario target: 15:15
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_behaviour.bo.trail_trip`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify Bo is seen walking to the trail area in the late afternoon.

### `characters_behaviour.sam.cafe_break`

- Entity/attribute: sam / cafe_break.
- Rule: Sam visits the café for a coffee break around 12:30, ordering a flat white.
- Scenario target: 12:30
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_behaviour.sam.cafe_break`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify Sam is seen at the café counter around midday, holding a coffee cup.

### `characters_behaviour.lin.bus_commute`

- Entity/attribute: lin / bus_commute.
- Rule: Lin commutes to school by bus around 07:30, boarding at the bus stop and alighting at the school stop.
- Scenario target: 07:30
- Basis: real-world (cited). Source: A10: LTA BSEP announcement as reported by Singapore Business Review https://sbr.com.sg/transport-logistics/news/wheels-buses-go-round-800-times-more-over-5-years; E3: Princess Elizabeth Primary School https://www.princesselizabethpri.moe.edu.sg/general-information/information-for-parents/school-curriculum-hours/ (MOE booklet opened, has no hours).
- Supplied reference value: A10: Feeder services: 95% at ≤ 10 min scheduled intervals in peak periods, rest ≤ 15 min; all services within 20 min, 80% within 10 min (BSEP standard); E3: official school hours 7.30am–1.30pm on non-CCA days (one school)
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_behaviour.lin.bus_commute`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify Lin is seen at the bus stop in the morning, boarding the bus.

### `characters_behaviour.ama.dog_walk`

- Entity/attribute: ama / dog_walk.
- Rule: Ama takes Mochi for a walk in the park around 07:40, returning home by 08:00.
- Scenario target: 07:40-08:00
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.characters_behaviour.ama.dog_walk`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify Ama and Mochi are seen walking in the park area in the early morning.

### `characters_behaviour.terrace.sitter_visibility`

- Entity/attribute: terrace_sitters / sitter_visibility.
- Rule: Sitters on the café terrace must be visible, with their heads not occluded by terrace furniture or window glass.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.terrace_sitter_layers`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify terrace sitters are fully visible, with no furniture or glass covering their heads.

### `characters_behaviour.lin.hair_profile`

- Entity/attribute: lin / hair_profile.
- Rule: Lin's profile hair must cover the skull like her front frames, ensuring consistent hair geometry from all angles.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `render.lin_hair`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify Lin's hair appears consistent and covers the head properly when viewed from the side.

### `characters_behaviour.office.bed_absence`

- Entity/attribute: office / bed_absence.
- Rule: The office interior must not contain a baked bed, as it is a workplace, not a residential space.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.office_bed`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify no bed is visible in the office interior.

### `characters_behaviour.crossing.safety`

- Entity/attribute: crossings / safety.
- Rule: No walker footprint overlap is allowed at zebra crossings, and facing must be within 11.5 degrees of the path tangent.
- Scenario target: 11.5 deg
- Basis: unreviewed. Source: TBD.
- Check: `suite.crossing_overlap`. Status: **REPORTED FAIL**.
- 4× review cue: Verify pedestrians do not collide at crosswalks and face the direction of crossing.

### `characters_behaviour.walkers.progress`

- Entity/attribute: walkers / progress.
- Rule: Every walker must reach their destination, with a maximum wait time of 10 seconds at any point.
- Scenario target: 10 s
- Basis: unreviewed. Source: TBD.
- Check: `suite.walkers_progress`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify characters do not get stuck or wait excessively long at intersections or doors.

## space_vehicles

### `space_vehicles.street_graph.connectivity`

- Entity/attribute: street_graph / connectivity.
- Rule: Every named location node must be reachable from every other via the defined street graph edges without passing through non-walkable areas.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.routing`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify that pathfinding lines do not cut through buildings or foliage when connecting distant nodes.

### `space_vehicles.sidewalks.boundary`

- Entity/attribute: sidewalks / boundary.
- Rule: Named and background walkers must remain strictly within the defined sidewalk polygons and never enter the road lane or building interiors while walking.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: D1: LTA Walking and Cycling Design Guide §2.1.1 https://www.lta.gov.sg/content/dam/ltagov/industry_innovations/industry_matters/development_construction_resources/walking_cycling_plan/pdf/walking-and-cycling-design-guide.pdf.
- Supplied reference value: D1: new footpaths 1.8 m (increased from 1.5 m); wider where pedestrian volume is high
- Check: `suite.pedestrians_stay_on_nav`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Check that walker footprints do not overlap the road surface or building walls during transit.

### `space_vehicles.crossings.alignment`

- Entity/attribute: zebra_crossings / alignment.
- Rule: Walkers crossing the street must maintain a heading within 11.5 degrees of the path tangent defined by the crossing geometry.
- Scenario target: 11.5 degrees
- Basis: unreviewed. Source: TBD.
- Check: `suite.crossing_overlap`. Status: **REPORTED FAIL**.
- 4× review cue: Observe that walkers do not drift diagonally off the zebra stripes while crossing.

### `space_vehicles.crossings.separation`

- Entity/attribute: zebra_crossings / separation.
- Rule: No two walker footprints may overlap at any point during a zebra crossing traversal in a dense simulation.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.crossing_overlap`. Status: **REPORTED FAIL**.
- 4× review cue: Look for overlapping sprites or intersecting footprints on the white stripes of the crossings.

### `space_vehicles.walkers.speed`

- Entity/attribute: named_walkers / speed.
- Rule: Named characters must move at a base speed of 190 map pixels per second at 1x game speed, scaling with the square root of the clock speed multiplier.
- Scenario target: 190 px/s at x1
- Basis: real-world (cited). Source: A1: Knoblauch, Pietrucha, Nitzburg, "Field Studies of Pedestrian Walking Speed and Start-Up Time", TRR 1538 (1996): abstract https://doi.org/10.1177/0361198196153800104 ; mean values summarised at https://www.usroads.com/journals/p/rej/9710/re971001/; A4: FHWA-RD-98-107, Ch. 3, Table 4 https://www.fhwa.dot.gov/publications/research/safety/pedbike/98107/section3.cfm.
- Supplied reference value: A1: 1.51 m/s (4.95 ft/s) mean; 15th percentile 1.25 m/s (4.09 ft/s); A4: ≥ 1.3 m/s average at LOS A (space ≥ 5.6 m²/ped)
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.space_vehicles.walkers.speed`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Compare the movement rate of a named character against the background pedestrians to ensure the named character is visibly faster.

### `space_vehicles.walkers.background_speed`

- Entity/attribute: background_pedestrians / speed.
- Rule: Background anonymous pedestrians must move at a constant speed of 72 map pixels per second, independent of game speed multipliers.
- Scenario target: 72 px/s
- Basis: real-world (cited). Source: A1: Knoblauch, Pietrucha, Nitzburg, "Field Studies of Pedestrian Walking Speed and Start-Up Time", TRR 1538 (1996): abstract https://doi.org/10.1177/0361198196153800104 ; mean values summarised at https://www.usroads.com/journals/p/rej/9710/re971001/; A4: FHWA-RD-98-107, Ch. 3, Table 4 https://www.fhwa.dot.gov/publications/research/safety/pedbike/98107/section3.cfm.
- Supplied reference value: A1: 1.51 m/s (4.95 ft/s) mean; 15th percentile 1.25 m/s (4.09 ft/s); A4: ≥ 1.3 m/s average at LOS A (space ≥ 5.6 m²/ped)
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.space_vehicles.walkers.background_speed`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify that background pedestrians move at a noticeably slower, steady pace compared to named agents.

### `space_vehicles.walkers.stride`

- Entity/attribute: named_walkers / stride.
- Rule: The animation stride for named walkers must be 34 map pixels, distinct from the 26 map pixel stride of background pedestrians.
- Scenario target: 34 px (named), 26 px (background)
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.space_vehicles.walkers.stride`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check that the leg animation cycle length matches the distance covered per step for both named and background characters.

### `space_vehicles.walkers.progression`

- Entity/attribute: walkers / progression.
- Rule: Every walker must reach their destination without waiting longer than 10 seconds at any intermediate node.
- Scenario target: 10 s max wait
- Basis: unreviewed. Source: TBD.
- Check: `suite.walkers_progress`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Ensure no character stands idle at a corner or intersection for an extended period without a scripted reason.

### `space_vehicles.bus.route`

- Entity/attribute: bus / route.
- Rule: The bus must follow the fixed polyline route defined by the vector sequence from (-320, 805) to (1860, 805) without deviation.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.bus_route_and_dwell`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Trace the bus path to ensure it stays on the designated road lane and does not cut corners or leave the road.

### `space_vehicles.bus.stops`

- Entity/attribute: bus / stops.
- Rule: The bus must stop at the 'lessville' stop at (470, 805) and the 'plaza' stop at (1050, 805) at each pass.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.bus_route_and_dwell`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify the bus aligns with the bus shelter and boarding apron at the designated stop coordinates.

### `space_vehicles.bus.dwell_time`

- Entity/attribute: bus / dwell_time.
- Rule: The bus must dwell at the 'lessville' stop for a random duration between 3 and 8 seconds, and at the 'plaza' stop for exactly 3 seconds.
- Scenario target: 3-8 s (lessville), 3 s (plaza)
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `suite.bus_route_and_dwell`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Observe the bus remaining stationary at the stop for the appropriate duration before departing.

### `space_vehicles.bus.speed`

- Entity/attribute: bus / speed.
- Rule: The bus must travel at a cruise speed of 320 map pixels per second between stops.
- Scenario target: 320 px/s
- Basis: real-world (cited). Source: A8: LTA OneMotoring bus registration guideline (Annex A) https://onemotoring.lta.gov.sg/content/dam/onemotoring/Buying/PDF/Importation_Guidelines/Bus.pdf (search extract); SPF Speed Limiter Announcement https://www.police.gov.sg/knowledge-hub/traffic/traffic-matters/speed-limiter-announcement (search extract). SSO page returned 403 to direct fetch.
- Supplied reference value: A8: Public service buses > 10,000 kg MLW fitted with speed limiters set at 60 km/h. In town the 50 km/h road limit is lower and applies
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.space_vehicles.bus.speed`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check that the bus moves at a consistent, high speed relative to pedestrians when not at a stop.

### `space_vehicles.bus.direction`

- Entity/attribute: bus / direction.
- Rule: The bus must never reverse direction or overshoot its stop coordinates; it must only move forward along the route polyline.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.bus_route_and_dwell`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Ensure the bus sprite orientation always matches the direction of travel and it does not backtrack.

### `space_vehicles.bus.boarding`

- Entity/attribute: bus / boarding.
- Rule: Passengers must only board or alight when the bus is in the 'dwell' phase at a designated stop, not while in motion.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.space_vehicles.bus.boarding`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify that characters only enter or exit the bus when it is fully stopped at the shelter.

### `space_vehicles.bus.transit_render`

- Entity/attribute: bus / transit_render.
- Rule: Characters in transit (on the bus) must be rendered on the street layer with appropriate occlusion, not hidden or floating.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.space_vehicles.bus.transit_render`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check that passengers are visible inside the bus windows or correctly occluded by the bus body when in transit.

### `space_vehicles.traffic_lights.state`

- Entity/attribute: traffic_lights / state.
- Rule: Traffic lights must cycle through red, yellow, and green states in a fixed sequence, and pedestrians must only cross when the light is green for them.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: A12: MOT written reply (Khaw Boon Wan) https://www.mot.gov.sg/news-resources/newsroom/written-reply-by-minister-for-transport-khaw-boon-wan-to-parliamentary-question-on-longer-pre-defined-time-period-at-signalised-pedestrian-crossings/ ; 3–13 s: DPA https://dpa.org.sg/green-man/ (secondary).
- Supplied reference value: A12: steady green man 6–10 s plus flashing green 1 s per metre of road width; Green Man+ adds up to 13 s (3–13 s by width)
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.space_vehicles.traffic_lights.state`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Observe that walkers wait at the curb when the traffic light is red and proceed when it turns green.

### `space_vehicles.road_traffic.presence`

- Entity/attribute: road_traffic / presence.
- Rule: Background road traffic (cars) must be present on the main road, moving in the correct lane direction, and not colliding with the bus or pedestrians.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: A7: LTA OneMotoring, Driving Rules https://onemotoring.lta.gov.sg/content/onemotoring/home/driving/road_safety_and_vehicle_rules/driving-rules.html ; LTA media reply 2024 https://www.lta.gov.sg/content/ltagov/en/newsroom/2024/9/media-replies/default-speed-limit-of-roads-is-50kmh--with-signs-for-different-.html.
- Supplied reference value: A7: 50 km/h unless otherwise signed
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.space_vehicles.road_traffic.presence`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Look for cars moving in the opposite direction to the bus on the other lane, and ensure they do not stop abruptly for no reason.

### `space_vehicles.map.scale`

- Entity/attribute: map / scale.
- Rule: The map scale must be consistent such that 1 map pixel corresponds to a fixed real-world distance, ensuring travel times between places are realistic.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.space_vehicles.map.scale`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Compare the visual distance between two landmarks to their known real-world equivalent to ensure the scale feels correct.

### `space_vehicles.travel_time.home_office`

- Entity/attribute: travel_time / home_to_office.
- Rule: The travel time from 'home' (Node A) to 'office' (Node E) for a named walker must be consistent with the 190 px/s speed and the graph distance.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: A1: Knoblauch, Pietrucha, Nitzburg, "Field Studies of Pedestrian Walking Speed and Start-Up Time", TRR 1538 (1996): abstract https://doi.org/10.1177/0361198196153800104 ; mean values summarised at https://www.usroads.com/journals/p/rej/9710/re971001/; A4: FHWA-RD-98-107, Ch. 3, Table 4 https://www.fhwa.dot.gov/publications/research/safety/pedbike/98107/section3.cfm.
- Supplied reference value: A1: 1.51 m/s (4.95 ft/s) mean; 15th percentile 1.25 m/s (4.09 ft/s); A4: ≥ 1.3 m/s average at LOS A (space ≥ 5.6 m²/ped)
- Check: `suite.walkers_progress`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Time how long it takes for Sam to walk from home to office and ensure it matches the expected duration based on distance and speed.

### `space_vehicles.travel_time.home_cafe`

- Entity/attribute: travel_time / home_to_cafe.
- Rule: The travel time from 'home' (Node A) to 'cafe' (Node J) for a named walker must be consistent with the 190 px/s speed and the graph distance.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: A1: Knoblauch, Pietrucha, Nitzburg, "Field Studies of Pedestrian Walking Speed and Start-Up Time", TRR 1538 (1996): abstract https://doi.org/10.1177/0361198196153800104 ; mean values summarised at https://www.usroads.com/journals/p/rej/9710/re971001/; A4: FHWA-RD-98-107, Ch. 3, Table 4 https://www.fhwa.dot.gov/publications/research/safety/pedbike/98107/section3.cfm.
- Supplied reference value: A1: 1.51 m/s (4.95 ft/s) mean; 15th percentile 1.25 m/s (4.09 ft/s); A4: ≥ 1.3 m/s average at LOS A (space ≥ 5.6 m²/ped)
- Check: `suite.walkers_progress`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Time how long it takes for Maya to walk from home to cafe and ensure it matches the expected duration based on distance and speed.

### `space_vehicles.travel_time.bus_stop_cafe`

- Entity/attribute: travel_time / bus_stop_to_cafe.
- Rule: The travel time from 'bus_stop' (Node N) to 'cafe' (Node J) for a named walker must be consistent with the 190 px/s speed and the graph distance.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: A1: Knoblauch, Pietrucha, Nitzburg, "Field Studies of Pedestrian Walking Speed and Start-Up Time", TRR 1538 (1996): abstract https://doi.org/10.1177/0361198196153800104 ; mean values summarised at https://www.usroads.com/journals/p/rej/9710/re971001/; A4: FHWA-RD-98-107, Ch. 3, Table 4 https://www.fhwa.dot.gov/publications/research/safety/pedbike/98107/section3.cfm.
- Supplied reference value: A1: 1.51 m/s (4.95 ft/s) mean; 15th percentile 1.25 m/s (4.09 ft/s); A4: ≥ 1.3 m/s average at LOS A (space ≥ 5.6 m²/ped)
- Check: `suite.walkers_progress`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Time how long it takes for Lin to walk from the bus stop to the cafe and ensure it matches the expected duration based on distance and speed.

### `space_vehicles.bus_shelter.alignment`

- Entity/attribute: bus_shelter / alignment.
- Rule: The bus shelter must be positioned adjacent to the bus stop node (Node N) such that waiting pedestrians are visible and aligned with the bus arrival point.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.space_vehicles.bus_shelter.alignment`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check that the bus shelter is correctly placed next to the road and that waiting characters are visible within the shelter area.

### `space_vehicles.cafe_terrace.access`

- Entity/attribute: cafe_terrace / access.
- Rule: The café terrace must be accessible from the sidewalk without crossing the road, and sitters must be visible from the street view.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.terrace_visible`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify that people sitting on the café terrace are visible from the main street and not occluded by the building or foliage.

### `space_vehicles.park.bench_placement`

- Entity/attribute: park_bench / placement.
- Rule: Park benches must be placed on the park grass or paved area, not on the sidewalk or road, and must be accessible from the park path.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.seating`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Check that the park bench is correctly positioned within the park area and that characters sitting on it are visible and not floating.

### `space_vehicles.grocery.entrance`

- Entity/attribute: grocery / entrance.
- Rule: The grocery store entrance must be accessible from the sidewalk, and characters entering or exiting must follow the graph path to the entrance.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.routing`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Observe that characters walk to the grocery entrance and enter/exit smoothly without clipping through the building.

### `space_vehicles.school.entrance`

- Entity/attribute: school / entrance.
- Rule: The school entrance must be accessible from the sidewalk, and characters entering or exiting must follow the graph path to the entrance.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.routing`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Observe that characters walk to the school entrance and enter/exit smoothly without clipping through the building.

### `space_vehicles.gym.entrance`

- Entity/attribute: gym / entrance.
- Rule: The gym entrance must be accessible from the sidewalk, and characters entering or exiting must follow the graph path to the entrance.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.routing`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Observe that characters walk to the gym entrance and enter/exit smoothly without clipping through the building.

### `space_vehicles.trail.entrance`

- Entity/attribute: trail / entrance.
- Rule: The trail entrance must be accessible from the sidewalk, and characters entering or exiting must follow the graph path to the entrance.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.routing`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Observe that characters walk to the trail entrance and enter/exit smoothly without clipping through the building or foliage.

### `space_vehicles.office.entrance`

- Entity/attribute: office / entrance.
- Rule: The office entrance must be accessible from the sidewalk, and characters entering or exiting must follow the graph path to the entrance.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.routing`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Observe that characters walk to the office entrance and enter/exit smoothly without clipping through the building.

### `space_vehicles.home.entrance`

- Entity/attribute: home / entrance.
- Rule: The home entrance must be accessible from the sidewalk, and characters entering or exiting must follow the graph path to the entrance.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.routing`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Observe that characters walk to the home entrance and enter/exit smoothly without clipping through the building.

## objects

### `objects.building.facade_alignment`

- Entity/attribute: building facade / ground_contact.
- Rule: Building bases must align with the sidewalk edge without floating or intersecting the road surface.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.building.facade_alignment`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify that the bottom edge of the building texture sits flush with the sidewalk polygon without gaps or overlaps.

### `objects.window.glass_masking`

- Entity/attribute: window / light_clipping.
- Rule: Interior light spill must be strictly clipped to the glass mask defined in town_glass_mask.png.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `render.atmosphere`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Check that no light pixels appear outside the window glass boundaries, especially on the wall frames.

### `objects.lamp.street_lit_state`

- Entity/attribute: street lamp / daylight_visibility.
- Rule: Street lamps must be visually off (no glow, low luminance) during daylight hours (07:30-18:30).
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: B1: timeanddate, Singapore sun tables, all 12 months of 2026 https://www.timeanddate.com/sun/singapore/singapore?month=1&year=2026 (… month=12); B2: same as B1; B4: B1 tables; definition: timeanddate https://www.timeanddate.com/astronomy/civil-twilight.html (USNO page refused TLS).
- Supplied reference value: B1: 06:46 (earliest, ~21 Oct) to 07:16 (latest, ~19 Feb); B2: 18:50 (earliest, ~25 Oct) to 19:20 (latest, ~26 Feb); B4: civil dawn ≈ 20–23 min before sunrise (earliest civil start 06:24, latest 06:55); civil dusk ends 19:11–19:42. Civil twilight = sun centre ≤ 6° below horizon
- Check: `render.atmosphere`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Ensure lamp heads appear dark and do not emit visible light cones during midday frames.

### `objects.lamp.street_lit_state_2`

- Entity/attribute: street lamp / night_glow.
- Rule: Street lamps must emit a visible glow and illuminate the immediate ground at night (22:00).
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: B1: timeanddate, Singapore sun tables, all 12 months of 2026 https://www.timeanddate.com/sun/singapore/singapore?month=1&year=2026 (… month=12); B2: same as B1; B4: B1 tables; definition: timeanddate https://www.timeanddate.com/astronomy/civil-twilight.html (USNO page refused TLS).
- Supplied reference value: B1: 06:46 (earliest, ~21 Oct) to 07:16 (latest, ~19 Feb); B2: 18:50 (earliest, ~25 Oct) to 19:20 (latest, ~26 Feb); B4: civil dawn ≈ 20–23 min before sunrise (earliest civil start 06:24, latest 06:55); civil dusk ends 19:11–19:42. Civil twilight = sun centre ≤ 6° below horizon
- Check: `render.atmosphere`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify distinct light halos around lamp heads and brightened pavement at night.

### `objects.furniture.office_desk`

- Entity/attribute: office desk / occupancy_alignment.
- Rule: Characters seated at office desks must have their hip anchor aligned with the chair seat and torso visible above the desk.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: D3: UC task-chair checklist https://uhs.berkeley.edu/sites/default/files/uc_task_chair_requirements_2.pdf; D4: Penn EHRS https://ehrs.upenn.edu/health-safety/ergonomics/computer-ergonomics/selecting-office-furniture.
- Supplied reference value: D3: task-chair seat adjustable within 15–22 in (≈ 38–56 cm, conv.); D4: 25–32 in (≈ 64–81 cm, conv.) by worker height; 27–30 in (≈ 69–76 cm) for 5'7"–6'0"
- Check: `suite.realism`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Check that the character's lower body is hidden by the desk/chair and upper body is correctly positioned.

### `objects.furniture.cafe_stool`

- Entity/attribute: café stool / placement.
- Rule: Café stools must be placed only at designated counter or table positions, not in walkways.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: D3: UC task-chair checklist https://uhs.berkeley.edu/sites/default/files/uc_task_chair_requirements_2.pdf.
- Supplied reference value: D3: task-chair seat adjustable within 15–22 in (≈ 38–56 cm, conv.)
- Check: `suite.realism`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Ensure no stools are floating in the middle of the café floor or blocking the queue line.

### `objects.furniture.cafe_table`

- Entity/attribute: café table / sitter_occlusion.
- Rule: Furniture foreground layers must not occlude the head of a seated character.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: D4: Penn EHRS https://ehrs.upenn.edu/health-safety/ergonomics/computer-ergonomics/selecting-office-furniture.
- Supplied reference value: D4: 25–32 in (≈ 64–81 cm, conv.) by worker height; 27–30 in (≈ 69–76 cm) for 5'7"–6'0"
- Check: `suite.terrace_sitter_layers`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Zoom in on seated characters to ensure their heads are fully visible and not covered by table tops or chairs.

### `objects.furniture.terrace_bench`

- Entity/attribute: terrace bench / anchor_visibility.
- Rule: Characters sitting on terrace tables must be rendered at the correct seat anchor points.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.terrace_visible`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify that sitters are positioned exactly on the bench/table seats and not floating or offset.

### `objects.prop.coffee_cup`

- Entity/attribute: coffee cup / hand_prop_gating.
- Rule: Coffee cups should only be visible in the hands of characters when they are in a 'serving' or 'drinking' activity state.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.prop.coffee_cup`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check that characters walking or idle without a drink activity do not hold a cup.

### `objects.prop.milk_bottle`

- Entity/attribute: milk bottle / counter_presence.
- Rule: Milk bottles should be visible on the café counter only when the café is open and staff are present.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.prop.milk_bottle`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Ensure milk props are not visible when the café is closed or empty.

### `objects.prop.grocery_bag`

- Entity/attribute: grocery bag / carry_state.
- Rule: Grocery bags must only be rendered when a character is in the 'shopping' or 'carry' activity state.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.prop.grocery_bag`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify that characters leaving the grocery store or walking home have bags, while those entering do not.

### `objects.prop.backpack`

- Entity/attribute: backpack / commute_visibility.
- Rule: Lin's backpack must be visible during commute and outdoor activities but removed when settled indoors.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `suite.attire`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Check that Lin has a backpack on the bus/street but not when sitting at home or in the café.

### `objects.stock.oat_milk`

- Entity/attribute: oat milk stock / availability.
- Rule: Oat milk should be available for orders only when the stock count is greater than zero.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.stock.oat_milk`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Observe that if stock is depleted, the barista does not attempt to make an oat latte.

### `objects.stock.eggs`

- Entity/attribute: egg stock / restock_cycle.
- Rule: Egg stock should be replenished according to the scheduled restock logic.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.stock.eggs`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check that egg-related activities (if any) align with stock availability.

### `objects.signage.cafe_sign`

- Entity/attribute: café sign / legibility.
- Rule: Café signage must be clearly visible and not obscured by foliage or other objects.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.signage.cafe_sign`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Ensure the 'Less Café' sign is readable and not covered by trees or characters.

### `objects.foliage.tree_sway`

- Entity/attribute: tree / wind_animation.
- Rule: Tree foliage must sway in response to the scheduled wind speed.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: C5: same; C6: NOAA SPC Beaufort scale https://www.spc.noaa.gov/faq/tornado/beaufort.html + C5.
- Supplied reference value: C5: generally light, mean surface wind ~2 m/s (monthly mean 1.4 (Nov) to 2.8 m/s (Feb)); NE Monsoon surges up to 10 m/s or more, 25–35 km/h; C6: Beaufort 2 (4–6 kn ≈ 2.1–3.1 m/s, conv.): leaves rustle; 3 (7–10 kn): leaves and small twigs constantly moving; 5 (17–21 kn): small trees in leaf begin to sway. MSS mean ~2 m/s ≈ Beaufort 1–2 (conv.), so normal days = gentle leaf movement, not swaying trees
- Check: `render.atmosphere`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Observe that trees move slightly in windy conditions and are static in calm conditions.

### `objects.fountain.water`

- Entity/attribute: fountain / water_flow.
- Rule: The fountain must have visible water animation when active.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.fountain.water`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check for animated water particles or shimmer in the fountain basin.

### `objects.bus_shelter.structure`

- Entity/attribute: bus shelter / occupancy.
- Rule: Characters waiting for the bus must be positioned within the bus shelter bounds.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.pedestrians_stay_on_nav`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Ensure waiting characters are under the shelter roof and not standing in the street.

### `objects.art.office_bed`

- Entity/attribute: office art / bed_absence.
- Rule: The office interior must not contain any baked-in bed furniture.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.office_bed`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Scan the office room for any bed-like shapes or textures that should not be present.

### `objects.art.duplicate_stamps`

- Entity/attribute: town map / object_uniqueness.
- Rule: No identical prop stamps (e.g., trees, lamps) should appear in overlapping or impossible positions.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.art.duplicate_stamps`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Look for duplicated trees or lamps that look like copy-paste errors.

### `objects.building.door_alignment`

- Entity/attribute: building door / entry_point.
- Rule: Doors must align with the navigation graph entry points for each location.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: D2: BCA Code on Accessibility in the Built Environment 2025, cl. 4.4.2.1 https://file.go.gov.sg/bca-coa2025.pdf.
- Supplied reference value: D2: at least 850 mm
- Check: `suite.routing`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify that characters enter buildings through the visible door and not through walls.

### `objects.furniture.school_desk`

- Entity/attribute: school desk / grid_alignment.
- Rule: School desks must be arranged in a regular grid pattern matching the lecture hall layout.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: D4: Penn EHRS https://ehrs.upenn.edu/health-safety/ergonomics/computer-ergonomics/selecting-office-furniture.
- Supplied reference value: D4: 25–32 in (≈ 64–81 cm, conv.) by worker height; 27–30 in (≈ 69–76 cm) for 5'7"–6'0"
- Check: `suite.seating`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Check that desks are evenly spaced and aligned in rows and columns.

### `objects.furniture.gym_bench`

- Entity/attribute: gym bench / placement.
- Rule: Gym benches must be placed near exercise equipment or walls, not in the center of the floor.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.furniture.gym_bench`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Ensure benches are logically positioned for rest or weightlifting.

### `objects.prop.trail_log`

- Entity/attribute: trail log / sitting_surface.
- Rule: The trail log must be a valid sitting surface for characters.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.seating`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Check that characters sitting on the log have their hips aligned with the log surface.

### `objects.lamp.door_lantern`

- Entity/attribute: door lantern / night_illumination.
- Rule: Door lanterns must be lit at night to indicate entry points.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: B1: timeanddate, Singapore sun tables, all 12 months of 2026 https://www.timeanddate.com/sun/singapore/singapore?month=1&year=2026 (… month=12); B2: same as B1; B4: B1 tables; definition: timeanddate https://www.timeanddate.com/astronomy/civil-twilight.html (USNO page refused TLS).
- Supplied reference value: B1: 06:46 (earliest, ~21 Oct) to 07:16 (latest, ~19 Feb); B2: 18:50 (earliest, ~25 Oct) to 19:20 (latest, ~26 Feb); B4: civil dawn ≈ 20–23 min before sunrise (earliest civil start 06:24, latest 06:55); civil dusk ends 19:11–19:42. Civil twilight = sun centre ≤ 6° below horizon
- Check: `render.atmosphere`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify that small lights near doors are visible and glowing at night.

### `objects.furniture.home_sofa`

- Entity/attribute: home sofa / occupancy.
- Rule: Characters relaxing at home must be seated on the sofa or bed, not floating.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.seating`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Check that characters in 'sleeping' or 'idle' home activities are correctly positioned on furniture.

### `objects.window.reflection`

- Entity/attribute: window / daylight_reflection.
- Rule: Windows should show subtle reflections or transparency during daylight.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.window.reflection`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Observe if windows look like glass (slightly reflective) rather than solid black or white.

### `objects.signage.bus_stop`

- Entity/attribute: bus stop sign / visibility.
- Rule: Bus stop signage must be visible and clearly marked.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.signage.bus_stop`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Ensure the bus stop sign is visible and not obscured by the shelter or characters.

### `objects.furniture.cafe_counter`

- Entity/attribute: café counter / staff_position.
- Rule: Café staff must be positioned behind the counter, not in front of it.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: F1: Environmental Public Health (Food Hygiene) Regulations, reg. 23(2)(a), official text via https://laws.sg/legislation/sub-leg-1987-epha-rg16/reg-23.
- Supplied reference value: F1: Law requires anyone selling or preparing food to "wear clean attire and keep his body clean". It does **not** require an apron, so "aprons on café staff at work" is a design rule, not a legal one
- Check: `suite.realism`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify that Maya and Bo are rendered behind the counter bar, facing customers.

### `objects.furniture.queue_line`

- Entity/attribute: café queue / customer_position.
- Rule: Customers must stand in a queue in front of the counter, not behind it.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `suite.realism`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Check that waiting customers are in front of the counter, facing the staff.

### `objects.furniture.office_chair`

- Entity/attribute: office chair / rotation.
- Rule: Office chairs should rotate to match the character's facing direction.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: D3: UC task-chair checklist https://uhs.berkeley.edu/sites/default/files/uc_task_chair_requirements_2.pdf.
- Supplied reference value: D3: task-chair seat adjustable within 15–22 in (≈ 38–56 cm, conv.)
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.furniture.office_chair`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Observe if the chair back rotates with the character or stays static.

### `objects.furniture.cafe_chair`

- Entity/attribute: café chair / rotation.
- Rule: Café chairs should rotate to match the character's facing direction.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: D3: UC task-chair checklist https://uhs.berkeley.edu/sites/default/files/uc_task_chair_requirements_2.pdf.
- Supplied reference value: D3: task-chair seat adjustable within 15–22 in (≈ 38–56 cm, conv.)
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.furniture.cafe_chair`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Observe if the chair back rotates with the character or stays static.

### `objects.furniture.school_chair`

- Entity/attribute: school chair / rotation.
- Rule: School chairs should rotate to match the character's facing direction.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: D3: UC task-chair checklist https://uhs.berkeley.edu/sites/default/files/uc_task_chair_requirements_2.pdf.
- Supplied reference value: D3: task-chair seat adjustable within 15–22 in (≈ 38–56 cm, conv.)
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.furniture.school_chair`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Observe if the chair back rotates with the character or stays static.

### `objects.furniture.gym_chair`

- Entity/attribute: gym chair / rotation.
- Rule: Gym chairs should rotate to match the character's facing direction.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: D3: UC task-chair checklist https://uhs.berkeley.edu/sites/default/files/uc_task_chair_requirements_2.pdf.
- Supplied reference value: D3: task-chair seat adjustable within 15–22 in (≈ 38–56 cm, conv.)
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.furniture.gym_chair`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Observe if the chair back rotates with the character or stays static.

### `objects.furniture.home_chair`

- Entity/attribute: home chair / rotation.
- Rule: Home chairs should rotate to match the character's facing direction.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: D3: UC task-chair checklist https://uhs.berkeley.edu/sites/default/files/uc_task_chair_requirements_2.pdf.
- Supplied reference value: D3: task-chair seat adjustable within 15–22 in (≈ 38–56 cm, conv.)
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.furniture.home_chair`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Observe if the chair back rotates with the character or stays static.

### `objects.furniture.bus_seat`

- Entity/attribute: bus seat / occupancy.
- Rule: Bus seats must be occupied only by characters in the 'commuting' state.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.furniture.bus_seat`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check that only passengers are seated in the bus, and empty seats are visible.

### `objects.furniture.trail_bench`

- Entity/attribute: trail bench / occupancy.
- Rule: Trail benches must be occupied only by characters in the 'hiking' or 'resting' state.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.furniture.trail_bench`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check that only hikers are seated on the trail bench.

### `objects.furniture.park_bench`

- Entity/attribute: park bench / occupancy.
- Rule: Park benches must be occupied only by characters in the 'walking' or 'resting' state.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.furniture.park_bench`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check that only pedestrians are seated on the park bench.

### `objects.furniture.bus_stop_bench`

- Entity/attribute: bus stop bench / occupancy.
- Rule: Bus stop benches must be occupied only by characters in the 'commuting' state.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.objects.furniture.bus_stop_bench`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check that only waiting passengers are seated on the bus stop bench.

## environment

### `environment.clock.day_length`

- Entity/attribute: simulation clock / day_duration_minutes.
- Rule: A full in-game day must span exactly 1440 minutes to match real-world timekeeping.
- Scenario target: 1440 min
- Basis: real-world (cited). Source: B3: same as B1.
- Supplied reference value: B3: 12 h 03 m 04 s (~18 Dec) to 12 h 11 m 46 s (~22 Jun); nearly constant
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.environment.clock.day_length`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify the clock label increments consistently and wraps from 23:59 to 00:00 without skipping or duplicating minutes.

### `environment.clock.active_window`

- Entity/attribute: simulation clock / active_day_bounds.
- Rule: The simulation day logic must start at 06:30 (minute 390) and end at 22:00 (minute 1320), with no agent activity outside these bounds.
- Scenario target: 390-1320 min
- Basis: real-world (cited). Source: B1: timeanddate, Singapore sun tables, all 12 months of 2026 https://www.timeanddate.com/sun/singapore/singapore?month=1&year=2026 (… month=12); B2: same as B1.
- Supplied reference value: B1: 06:46 (earliest, ~21 Oct) to 07:16 (latest, ~19 Feb); B2: 18:50 (earliest, ~25 Oct) to 19:20 (latest, ~26 Feb)
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.environment.clock.active_window`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check that agents are inactive or asleep in their homes when the clock displays times before 06:30 or after 22:00.

### `environment.clock.speed_scaling`

- Entity/attribute: simulation clock / time_progression_rate.
- Rule: At 1x speed, the in-game clock must advance exactly 2 simulation minutes per real second.
- Scenario target: 2 sim-min/real-s
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.environment.clock.speed_scaling`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Observe the clock label while the game runs at 1x speed; it should tick forward visibly but not instantly.

### `environment.sky.dawn_luminance`

- Entity/attribute: sky / dawn_brightness_band.
- Rule: At 07:30, the sky luminance must fall within the 55-140 range to simulate early morning light.
- Scenario target: 55-140 luminance units
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `render.atmosphere`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Look at the sky gradient at 07:30; it should be dimmer than noon but brighter than night, with a warm hue.

### `environment.sky.dawn_hue`

- Entity/attribute: sky / dawn_color_temperature.
- Rule: The sky hue at 07:30 must be classified as 'warm' to represent sunrise colors.
- Scenario target: warm hue
- Basis: real-world (cited). Source: B1: timeanddate, Singapore sun tables, all 12 months of 2026 https://www.timeanddate.com/sun/singapore/singapore?month=1&year=2026 (… month=12); B4: B1 tables; definition: timeanddate https://www.timeanddate.com/astronomy/civil-twilight.html (USNO page refused TLS).
- Supplied reference value: B1: 06:46 (earliest, ~21 Oct) to 07:16 (latest, ~19 Feb); B4: civil dawn ≈ 20–23 min before sunrise (earliest civil start 06:24, latest 06:55); civil dusk ends 19:11–19:42. Civil twilight = sun centre ≤ 6° below horizon
- Check: `render.atmosphere`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Check that the sky and ambient light have orange or pink tones at 07:30 rather than blue or neutral.

### `environment.sky.noon_luminance`

- Entity/attribute: sky / noon_brightness_band.
- Rule: At 12:30, the sky luminance must fall within the 95-255 range to represent full daylight.
- Scenario target: 95-255 luminance units
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `render.atmosphere`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify the sky is bright and clear at 12:30, with high contrast between lit and shadowed areas.

### `environment.sky.dusk_luminance`

- Entity/attribute: sky / dusk_brightness_band.
- Rule: At 18:30, the sky luminance must fall within the 40-120 range to simulate sunset fading.
- Scenario target: 40-120 luminance units
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `render.atmosphere`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Observe the sky at 18:30; it should be dimming significantly compared to noon, with warm tones remaining.

### `environment.sky.night_luminance`

- Entity/attribute: sky / night_brightness_band.
- Rule: At 22:00, the sky luminance must fall within the 0-52 range to represent darkness.
- Scenario target: 0-52 luminance units
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `render.atmosphere`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Check that the sky is very dark at 22:00, with only artificial lights providing illumination.

### `environment.lighting.night_darkness`

- Entity/attribute: interior lighting / night_ambient_darkness.
- Rule: During night hours, ambient darkness must be applied such that unlit areas are significantly darker than lit areas.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `render.atmosphere`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Look at unlit streets at 22:00; they should be dark, while areas near lamps or windows should be clearly illuminated.

### `environment.lighting.window_glow_mask`

- Entity/attribute: window light / glow_clipping.
- Rule: Window light emission must be strictly clipped to the town_glass_mask.png, preventing light bleed onto non-glass surfaces.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `render.atmosphere`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Zoom in on lit windows at night; the glow should only appear on the glass panes, not on the window frames or walls.

### `environment.lighting.lamp_visibility`

- Entity/attribute: street lamps / night_activation.
- Rule: Street lamps must be visible and emitting light at night (22:00) but must be off or dimmed during daylight (12:30).
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `render.atmosphere`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Check street lamps at 22:00; they should have a visible glow. At 12:30, they should appear off or very faint.

### `environment.lighting.lamp_annulus`

- Entity/attribute: street lamps / glow_radius.
- Rule: The visible glow radius around street lamps must not exceed the defined lamp_annulus_radius to maintain realistic light falloff.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `render.atmosphere`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Observe the light pool under street lamps at night; it should be circular and not extend excessively far from the lamp post.

### `environment.weather.wind_speed`

- Entity/attribute: wind / scheduled_speed.
- Rule: Wind speed must be scheduled per day and vary within a realistic range, affecting visual elements like foliage.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: C5: same.
- Supplied reference value: C5: generally light, mean surface wind ~2 m/s (monthly mean 1.4 (Nov) to 2.8 m/s (Feb)); NE Monsoon surges up to 10 m/s or more, 25–35 km/h
- Check: `suite.atmosphere`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Watch foliage sway; the intensity of movement should correlate with the wind speed value in the atmosphere state.

### `environment.weather.foliage_sway`

- Entity/attribute: foliage / sway_amplitude.
- Rule: Foliage sway amplitude must be proportional to the current wind speed, with no movement when wind is zero.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: C5: same; C6: NOAA SPC Beaufort scale https://www.spc.noaa.gov/faq/tornado/beaufort.html + C5.
- Supplied reference value: C5: generally light, mean surface wind ~2 m/s (monthly mean 1.4 (Nov) to 2.8 m/s (Feb)); NE Monsoon surges up to 10 m/s or more, 25–35 km/h; C6: Beaufort 2 (4–6 kn ≈ 2.1–3.1 m/s, conv.): leaves rustle; 3 (7–10 kn): leaves and small twigs constantly moving; 5 (17–21 kn): small trees in leaf begin to sway. MSS mean ~2 m/s ≈ Beaufort 1–2 (conv.), so normal days = gentle leaf movement, not swaying trees
- Check: `suite.atmosphere`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Look at trees and bushes; they should sway more vigorously when wind speed is high and remain still when wind is low.

### `environment.weather.rain_visuals`

- Entity/attribute: rain / precipitation_rendering.
- Rule: When weather code indicates rain, visual rain particles or streaks must be rendered, and surfaces may appear wet.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: C3: same.
- Supplied reference value: C3: 171 rain days/year (≥ 0.2 mm); 2113.3 mm/yr; most frequent in afternoons; wettest Nov–Jan, driest Feb; monthly mean rain days 9 (Feb) to 19 (Nov, Dec)
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.environment.weather.rain_visuals`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check for rain streaks or particles in the air and potential wetness on ground surfaces when the weather label indicates rain.

### `environment.weather.cloud_coverage`

- Entity/attribute: clouds / sky_obscurity.
- Rule: Cloud coverage must affect sky brightness and hue, with overcast conditions reducing luminance compared to clear skies.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: C7: MSS Climate of Singapore.
- Supplied reference value: C7: cumulus from mid-morning, ~3–4 oktas by midday; may grow to cumulonimbus in afternoon/early evening; flatten into stratiform by dusk and disperse at night; mist/poor visibility 0500–0900 in calm wind
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.environment.weather.cloud_coverage`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Compare sky brightness on a cloudy day versus a clear day; cloudy skies should appear dimmer and more grey.

### `environment.cafe.opening_hours`

- Entity/attribute: café / operational_window.
- Rule: The café must be open and staffed between 06:30 (minute 390) and 20:00 (minute 1200), with Maya and Bo present during their shifts.
- Scenario target: 390-1200 min
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `suite.realism`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify that the café interior is lit and staff are present at 07:00, and that the café is closed or empty after 20:00.

### `environment.office.opening_hours`

- Entity/attribute: office / operational_window.
- Rule: The office must be occupied by Sam and ambient typists during standard work hours, typically 09:00 to 17:00.
- Scenario target: 540-1020 min
- Basis: real-world (cited). Source: E1: MOM Hours of work https://www.mom.gov.sg/employment-practices/hours-of-work-overtime-and-rest-days; E2: Attorney-General's Chambers contact page https://www.agc.gov.sg/contact-us/contact-info/ (one agency; not a national rule).
- Supplied reference value: E1: contract hours for ≤ 5-day week: up to 9 h/day or 44 h/week; E2: 8.30am–6pm Mon–Thu, 8.30am–5.30pm Fri
- Check: `suite.realism`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Check that the office is empty or dimly lit before 09:00 and after 17:00, with active typing animations during work hours.

### `environment.grocery.opening_hours`

- Entity/attribute: grocery / operational_window.
- Rule: The grocery store must be open during standard retail hours, allowing Ama to shop for groceries.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.environment.grocery.opening_hours`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify that Ama is able to enter and shop in the grocery store during the day, and that the store appears closed at night.

### `environment.school.opening_hours`

- Entity/attribute: school / operational_window.
- Rule: The school must be open for Lin's classes during the day, with Lin present in the lecture hall during scheduled study times.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: E3: Princess Elizabeth Primary School https://www.princesselizabethpri.moe.edu.sg/general-information/information-for-parents/school-curriculum-hours/ (MOE booklet opened, has no hours).
- Supplied reference value: E3: official school hours 7.30am–1.30pm on non-CCA days (one school)
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.environment.school.opening_hours`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check that Lin is present in the school lecture hall during her scheduled study times and absent at night.

### `environment.gym.opening_hours`

- Entity/attribute: gym / operational_window.
- Rule: The gym must be open for Lin's workout sessions during the day, with appropriate lighting and equipment visible.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: E4: ask.gov.sg ActiveSG https://ask.gov.sg/activesg/questions/cm0av6zxo000g14k1t3bat8u2.
- Supplied reference value: E4: ActiveSG gyms 7am–10pm daily (two gyms 7am–9.30pm)
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.environment.gym.opening_hours`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify that Lin is present in the gym during her scheduled workout times and that the gym appears closed at night.

### `environment.bus.timetable_sync`

- Entity/attribute: bus / departure_frequency.
- Rule: Bus departures must be scheduled relative to the in-game clock, with consistent intervals between stops.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: A10: LTA BSEP announcement as reported by Singapore Business Review https://sbr.com.sg/transport-logistics/news/wheels-buses-go-round-800-times-more-over-5-years.
- Supplied reference value: A10: Feeder services: 95% at ≤ 10 min scheduled intervals in peak periods, rest ≤ 15 min; all services within 20 min, 80% within 10 min (BSEP standard)
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.environment.bus.timetable_sync`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Observe the bus at the stop; it should arrive and depart at times that align with the in-game clock, not randomly.

### `environment.bus.dwell_time`

- Entity/attribute: bus / stop_duration.
- Rule: The bus must dwell at each stop for at least 3 seconds to allow passengers to board and alight.
- Scenario target: >= 3 s
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `suite.bus_route_and_dwell`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Watch the bus at a stop; it should remain stationary for a noticeable period before departing.

### `environment.bus.route_path`

- Entity/attribute: bus / route_polyline.
- Rule: The bus must follow the fixed route polyline defined in BUS_ROUTE, without deviating or reversing.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.bus_route_and_dwell`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Trace the bus's path; it should follow the road exactly, stopping at designated stops and not taking shortcuts.

### `environment.shadows.dynamic`

- Entity/attribute: shadows / direction_and_length.
- Rule: Shadow direction and length must change with the time of day, being longest at dawn/dusk and shortest at noon.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: B5: B1 tables.
- Supplied reference value: B5: 12:48 to 13:18
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.environment.shadows.dynamic`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Compare shadows at 07:30, 12:30, and 18:30; they should be long and angled at dawn/dusk, and short and vertical at noon.

### `environment.temperature.seasonal`

- Entity/attribute: temperature / seasonal_variation.
- Rule: Temperature and seasonal visuals (e.g., foliage color) must vary based on the in-game season, if implemented.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: C1: MSS Climate of Singapore (B6 URL); B6: MSS, Climate of Singapore https://www.weather.gov.sg/climate-climate-of-singapore/.
- Supplied reference value: C1: night minimum usually not below 23–25 °C; day maximum not above 31–33 °C; monthly mean daily max 30.5 (Dec) to 32.4 °C (Apr); 24-h mean 26.8 (Dec/Jan) to 28.6 °C (May); B6: NE Monsoon Dec–early Mar (wet phase Dec–early Jan, dry phase late Jan–early Mar); inter-monsoon late Mar–May; SW Monsoon Jun–Sep; inter-monsoon Oct–Nov
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.environment.temperature.seasonal`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check if foliage colors or character attire change with the season, indicating a temperature or seasonal model.

### `environment.wind.normal_day_leaf_rustle_not_tree_sway`

- Entity/attribute: trees and foliage / wind response on a normal day.
- Rule: On a normal Singapore day (mean wind about 2 m/s, Beaufort 1-2) leaves rustle gently and trees do NOT sway; visible tree sway appears only during monsoon surges or squalls.
- Scenario target: about 2 m/s mean (monthly 1.4-2.8 m/s): leaves rustle; Beaufort 5 (17-21 kn) before small trees sway; surges up to 10 m/s, squall gusts 40-80 km/h
- Basis: real-world (cited). Source: C5: same; C6: NOAA SPC Beaufort scale https://www.spc.noaa.gov/faq/tornado/beaufort.html + C5; C4: same.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.environment.wind.normal_day_leaf_rustle_not_tree_sway`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: On a calm-day frame pair, only leaf clusters shift by a pixel or two; trunks and whole canopies do not lean or shear.

### `environment.sky.keyed_to_sg_sunrise_sunset_twilight`

- Entity/attribute: sky and lighting / sky keyframes keyed to real clock times.
- Rule: Sky colour, ambient light and lamp state follow the sourced 2026 Singapore sunrise, sunset and civil/nautical twilight times: night before civil dawn, twilight colours only inside the civil twilight window, full day after sunrise, dusk colours only inside the civil dusk window.
- Scenario target: sunrise 06:46-07:16, sunset 18:50-19:20, civil twilight ~20-23 min each side (civil dawn 06:24-06:55, civil dusk ends 19:11-19:42)
- Basis: real-world (cited). Source: B1: timeanddate, Singapore sun tables, all 12 months of 2026 https://www.timeanddate.com/sun/singapore/singapore?month=1&year=2026 (… month=12); B2: same as B1; B4: B1 tables; definition: timeanddate https://www.timeanddate.com/astronomy/civil-twilight.html (USNO page refused TLS).
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.environment.sky.keyed_to_sg_sunrise_sunset_twilight`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: At 05:30 the sky is night-dark with lamps lit; at 07:00-07:30 it reads as early-morning day; at 19:00 dusk colours; at 22:00 and 02:30 night.

## ui

### `ui.title_screen.contrast`

- Entity/attribute: title screen / contrast_ratio.
- Rule: The title text must have a contrast ratio of at least 4.5:1 against the background to ensure readability in low light.
- Scenario target: 4.5:1
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.ui.title_screen.contrast`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Verify that the 'Lessville' title is clearly legible without squinting, even if the background is dark or busy.

### `ui.title_screen.enter_button_focus`

- Entity/attribute: title screen / keyboard_focus.
- Rule: The Enter button must be the default focus target and respond to the Enter key to start the game.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.main_scene`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Check that the Enter button has a visible focus ring or highlight when the game loads.

### `ui.top_bar.clock_format`

- Entity/attribute: top bar clock / time_format.
- Rule: The clock must display time in 24-hour HH:MM format to match the simulation's internal minute tracking.
- Scenario target: HH:MM
- Basis: unreviewed. Source: TBD.
- Check: `suite.main_scene`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Ensure the clock label shows a valid 24-hour time string that updates as the simulation progresses.

### `ui.top_bar.day_label_sync`

- Entity/attribute: top bar day label / sync_accuracy.
- Rule: The day counter must increment exactly when the simulation minute wraps past 1440 to 0.
- Scenario target: 1440 min
- Basis: unreviewed. Source: TBD.
- Check: `suite.main_scene`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify that the day number increases by 1 only when the clock resets to 00:00.

### `ui.top_bar.weather_icon`

- Entity/attribute: top bar weather / icon_accuracy.
- Rule: The weather icon must correspond to the WMO code returned by the backend, mapping rain codes to rain icons and clear codes to sun/clouds.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: C3: same.
- Supplied reference value: C3: 171 rain days/year (≥ 0.2 mm); 2113.3 mm/yr; most frequent in afternoons; wettest Nov–Jan, driest Feb; monthly mean rain days 9 (Feb) to 19 (Nov, Dec)
- Check: `suite.main_scene`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Confirm that if the sky is dark/rainy in the town view, the top bar shows a rain or cloud icon, not a sun.

### `ui.top_bar.db_count_update`

- Entity/attribute: top bar db count / latency.
- Rule: The LessDB statement count must update within 2 seconds of a new SQL statement being logged by the backend.
- Scenario target: < 2 s
- Basis: unreviewed. Source: TBD.
- Check: `suite.main_scene`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Observe that the number next to the DB icon increments shortly after an agent performs an action that generates a log entry.

### `ui.top_bar.bus_label_status`

- Entity/attribute: top bar bus label / state_reflection.
- Rule: The bus label must reflect the current phase (idle, approach, dwell, depart) and location of the bus entity.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: DESIGN CHOICE (no source). Source: DESIGN CHOICE (no source).
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.ui.top_bar.bus_label_status`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check that the label says 'Dwell' when the bus is stopped at a shelter and 'Approach' when it is moving.

### `ui.top_bar.model_label`

- Entity/attribute: top bar model label / accuracy.
- Rule: The model label must display the name of the AI model currently driving the agent behaviors.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.main_scene`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify the label is not empty and matches the configuration set in the backend.

### `ui.controls.speed_buttons_state`

- Entity/attribute: speed buttons / active_state_visual.
- Rule: The currently active speed button (1x, 2x, 4x, 8x) must have a distinct visual state (e.g., pressed color) different from inactive buttons.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.main_scene`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Click 4x and verify only the 4x button appears highlighted/pressed while others remain normal.

### `ui.controls.pause_button_toggle`

- Entity/attribute: pause button / icon_toggle.
- Rule: The pause button must toggle its icon between a pause symbol and a play symbol based on the simulation state.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.main_scene`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Click pause and verify the icon changes to a play triangle; click again to revert to pause bars.

### `ui.roster.buttons_selection`

- Entity/attribute: roster buttons / selection_highlight.
- Rule: The button for the currently selected agent must be visually distinguished (e.g., border or color) from other agents.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.main_scene`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Select 'Maya' and ensure her button in the roster list has a unique highlight compared to 'Sam' or 'Lin'.

### `ui.tabs.story_feed_render`

- Entity/attribute: story feed tab / content_rendering.
- Rule: The story feed must render the latest narrative events from the backend without truncation or encoding errors.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.main_scene`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Scroll the feed and ensure text is complete, legible, and correctly formatted with timestamps.

### `ui.tabs.mind_recall`

- Entity/attribute: mind tab / memory_display.
- Rule: The Mind tab must display the agent's recalled memories or context when an agent is selected.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.main_scene`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Select an agent and verify the Mind tab populates with relevant memory snippets rather than being empty.

### `ui.tabs.db_sql_syntax`

- Entity/attribute: db tab / sql_highlighting.
- Rule: The LessDB live SQL tab must highlight SQL keywords and strings to improve readability of the query log.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.main_scene`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Look at the SQL log and verify that keywords like SELECT or INSERT are colored differently from values.

### `ui.tabs.truth_boundaries`

- Entity/attribute: truth tab / boundary_display.
- Rule: The Truth tab must clearly list the native, app, and open boundaries of the simulation state.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.main_scene`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Verify that the tab displays distinct sections or lists for different types of simulation truths.

### `ui.place_cards.occupant_list`

- Entity/attribute: place cards / occupant_accuracy.
- Rule: Place cards for off-map locations must list the names of all agents currently present in that location.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.main_scene`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: If Lin is at the school, the school place card must list 'Lin' as an occupant.

### `ui.interior.layer_visibility`

- Entity/attribute: interior close-up layer / z_order.
- Rule: The interior close-up layer must render above the town map but below the UI overlay when a location is selected.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.main_scene`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Open the café interior and ensure the UI buttons are still clickable and visible on top of the interior scene.

### `ui.modal.provenance_content`

- Entity/attribute: provenance modal / data_source.
- Rule: The provenance modal must display the source and timestamp of the specific data point being inspected.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `suite.main_scene`. Status: **REPORTED PASS (mapping requires audit)**.
- 4× review cue: Open the modal and verify it contains a timestamp and a reference to the backend source.

### `ui.toast.auto_dismiss`

- Entity/attribute: toast / duration.
- Rule: Toast notifications must automatically disappear after a fixed duration (e.g., 3 seconds) to avoid cluttering the screen.
- Scenario target: 3 s
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.ui.toast.auto_dismiss`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Trigger a toast and verify it fades out after a few seconds without user interaction.

### `ui.town.picking_accuracy`

- Entity/attribute: town map / click_precision.
- Rule: Clicking on an agent sprite must select the correct agent, even if they are overlapping with other sprites or props.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.ui.town.picking_accuracy`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Click on a crowded area and verify the correct agent is selected in the UI panel.

### `ui.town.hover_feedback`

- Entity/attribute: town map / hover_highlight.
- Rule: Hovering over an interactive element (agent, prop) must provide immediate visual feedback (e.g., cursor change or outline).
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.ui.town.hover_feedback`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Move the mouse over an agent and verify the cursor changes or the agent highlights.

### `ui.speech.bubble_position`

- Entity/attribute: speech bubbles / anchor_point.
- Rule: Speech bubbles must anchor to the top of the agent's head sprite and not overlap with the agent's face.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.ui.speech.bubble_position`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Check that the bubble tail points to the head and the text does not cover the eyes.

### `ui.labels.map_tags`

- Entity/attribute: map labels / occlusion.
- Rule: Map labels for locations must remain visible and not be occluded by foreground foliage or traffic.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.ui.labels.map_tags`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Ensure the 'Café' label is readable even when trees or cars are in front of it.

### `ui.settings.reduced_motion`

- Entity/attribute: reduced motion setting / animation_suppression.
- Rule: When reduced motion is enabled, background swaying foliage and character walk cycles must be static or significantly slowed.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: real-world (cited). Source: C5: same; C6: NOAA SPC Beaufort scale https://www.spc.noaa.gov/faq/tornado/beaufort.html + C5.
- Supplied reference value: C5: generally light, mean surface wind ~2 m/s (monthly mean 1.4 (Nov) to 2.8 m/s (Feb)); NE Monsoon surges up to 10 m/s or more, 25–35 km/h; C6: Beaufort 2 (4–6 kn ≈ 2.1–3.1 m/s, conv.): leaves rustle; 3 (7–10 kn): leaves and small twigs constantly moving; 5 (17–21 kn): small trees in leaf begin to sway. MSS mean ~2 m/s ≈ Beaufort 1–2 (conv.), so normal days = gentle leaf movement, not swaying trees
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.ui.settings.reduced_motion`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Enable reduced motion and verify that trees do not sway and characters do not animate their legs.

### `ui.settings.hud_opacity`

- Entity/attribute: HUD opacity setting / transparency_level.
- Rule: The HUD opacity slider must adjust the alpha value of the top bar and side panels without affecting the main game view.
- Scenario target: 0.0-1.0
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.ui.settings.hud_opacity`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Lower the opacity and verify the UI becomes see-through while the town map remains fully opaque.

### `ui.text.wrapping`

- Entity/attribute: text labels / overflow_handling.
- Rule: Long text in UI panels must wrap to the next line rather than overflowing the container boundaries.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.ui.text.wrapping`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Enter a long string in a text field and verify it wraps within the box.

### `ui.text.readability`

- Entity/attribute: text labels / font_size.
- Rule: Body text in UI panels must be at least 14px to ensure readability on standard displays.
- Scenario target: 14 px
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.ui.text.readability`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Zoom in on small text and verify it is not blurry or too small to read comfortably.

### `ui.layout.window_responsiveness`

- Entity/attribute: window layout / resize_behavior.
- Rule: UI elements must reflow or scale appropriately when the window is resized, preventing clipping or overlap.
- Scenario target: Boolean invariant: 0 violations of the rule across all listed entities/poses and boundary times (proposed design acceptance).
- Basis: unreviewed. Source: TBD.
- Check: `MISSING`. Status: **MISSING**.
- Proposed check: `realism.ui.layout.window_responsiveness`; target and reference require the scale/time corrections in the plan before asserting physical units.
- 4× review cue: Resize the window to a small size and verify that buttons do not overlap or go off-screen.
