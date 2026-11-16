extends RefCounted
## Stylized art-space ratios; see docs/WORLD-SCALE.md. These are not metres.
const ACTOR_CANVAS := Vector2(96, 112)
const TOWN_ACTOR := 0.62 / 0.677
const INTERIOR_ACTOR := 1.2
const ACTOR_INK_HEIGHT := 104.0
const ADULT_HEIGHT := ACTOR_INK_HEIGHT * TOWN_ACTOR
const ADULT_RADIUS := 20.0
const DOG_CANVAS := Vector2(72, 52)
const TOWN_DOG := 0.6 / 0.677
const INTERIOR_DOG := TOWN_DOG * INTERIOR_ACTOR / TOWN_ACTOR
const BUS_SIZE := Vector2(300, 108) * 1.4
const CAR_LENGTH := ADULT_HEIGHT * 1.9
const VAN_LENGTH := ADULT_HEIGHT * 2.35
const BICYCLE_HEIGHT := ADULT_HEIGHT * 1.05
const SIGNAL_UNIT := ADULT_HEIGHT * 1.15 / 34.0
const ANIMAL_HEIGHTS := [ADULT_HEIGHT * 0.26, ADULT_HEIGHT * 0.26, ADULT_HEIGHT * 0.19, ADULT_HEIGHT * 0.10]

static func prop_size(item: String) -> Vector2:
	match item:
		"coffee": return Vector2(14, 18)
		"bag", "shopping", "backpack": return Vector2(28, 34)
		_: return Vector2(24, 24)
