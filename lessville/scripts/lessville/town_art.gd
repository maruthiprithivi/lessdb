extends RefCounted
## Generated atlases are sliced at load time; transparent gutters never affect body scale.
const RESIDENTS := "res://assets/game/characters/residents.png"
const ANIMALS := "res://assets/game/characters/animals.png"
const VEHICLES := "res://assets/game/props/traffic.png"
static var _atlases := {}

static func frames(path: String, columns: int, rows: int) -> Array:
	if _atlases.has(path):
		return _atlases[path]
	var source := load(path) as Texture2D
	assert(source != null, "Missing town artwork: " + path)
	var pixels := source.get_image()
	var result := []
	for row in rows:
		for column in columns:
			var cell := Rect2i(Vector2i(roundi(float(column) * pixels.get_width() / columns), roundi(float(row) * pixels.get_height() / rows)), Vector2i.ZERO)
			var end := Vector2i(roundi(float(column + 1) * pixels.get_width() / columns), roundi(float(row + 1) * pixels.get_height() / rows))
			# The generated van extends past the nominal two-thirds grid line into its gutter.
			if path == VEHICLES:
				if column == 1: end.x = roundi(pixels.get_width() * 0.69)
				if column == 2: cell.position.x = roundi(pixels.get_width() * 0.69)
			cell.size = end - cell.position
			var low := end
			var high := cell.position
			for y in range(cell.position.y, end.y):
				for x in range(cell.position.x, end.x):
					if pixels.get_pixel(x, y).a >= 0.5:
						low = low.min(Vector2i(x, y))
						high = high.max(Vector2i(x + 1, y + 1))
			assert(high.x > low.x and high.y > low.y, "Empty atlas cell: " + path)
			var texture := AtlasTexture.new()
			texture.atlas = source
			texture.region = Rect2(Rect2i(low, high - low).grow(1).intersection(cell))
			texture.filter_clip = true
			result.append(texture)
	_atlases[path] = result
	return result

static func resident_index(id: String) -> int:
	var number := maxi(0, int(id.trim_prefix("citizen-")) - 1)
	return (number * 5 + number / 8) % 8

static func resident(id: String, back: bool, moving: bool, phase: float) -> Texture2D:
	var frame := int(floor(phase)) % 2 if moving else 0
	return frames(RESIDENTS, 4, 8)[resident_index(id) * 4 + (2 if back else 0) + frame]

static func vehicle(kind: String) -> Texture2D:
	var index := 1 if kind == "delivery_van" else (2 if kind == "bicycle" else 0)
	return frames(VEHICLES, 3, 1)[index]
