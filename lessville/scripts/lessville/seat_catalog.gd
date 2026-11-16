extends RefCounted
## Seat anchors, masks and furniture overlays. Source of truth: res://config/seats.json
## Keep math in lockstep with tools/seatlib.py.

const PATH := "res://config/seats.json"
const Scale = preload("res://scripts/lessville/world_scale.gd")
const SPRITE := Scale.ACTOR_CANVAS
const INTERIOR_SCALE := Scale.INTERIOR_ACTOR
const TOWN_SCALE := Scale.TOWN_ACTOR
const HIP_ROW := 78.0

static var _data: Dictionary = {}
static var _loaded := false


static func data() -> Dictionary:
	if not _loaded:
		var txt := FileAccess.get_file_as_string(PATH)
		var parsed: Variant = JSON.parse_string(txt)
		_data = parsed if parsed is Dictionary else {}
		_loaded = true
	return _data


static func reload() -> void:
	_loaded = false
	data()


static func draw_scale(seat: Dictionary = {}) -> float:
	if String(seat.get("map", "")) == "town":
		return TOWN_SCALE
	return INTERIOR_SCALE


static func hip_offset(seat: Dictionary = {}) -> float:
	return (SPRITE.y - HIP_ROW) * draw_scale(seat)


static func feet_from_hip(hip: Vector2, seat: Dictionary = {}) -> Vector2:
	return hip + Vector2(0, hip_offset(seat))


static func hip_from_feet(feet: Vector2, seat: Dictionary = {}) -> Vector2:
	return feet - Vector2(0, hip_offset(seat))


static func seats() -> Array:
	var d := data()
	var arr: Variant = d.get("seats", [])
	return arr if arr is Array else []


static func seats_at(location: String, station: String = "") -> Array:
	var out := []
	for s: Variant in seats():
		if not (s is Dictionary):
			continue
		var seat: Dictionary = s
		if String(seat.get("location")) != location:
			continue
		if station != "" and String(seat.get("station")) != station:
			continue
		out.append(seat)
	out.sort_custom(func(a: Dictionary, b: Dictionary) -> bool: return int(a.get("index", 0)) < int(b.get("index", 0)))
	return out


static func seat_by_id(id: String) -> Dictionary:
	for s: Variant in seats():
		if s is Dictionary and String((s as Dictionary).get("id")) == id:
			return s
	return {}


static func hip_of(seat: Dictionary) -> Vector2:
	var h: Variant = seat.get("hip", [0, 0])
	if h is Array and (h as Array).size() >= 2:
		return Vector2(float(h[0]), float(h[1]))
	return Vector2.ZERO


static func point_in_mask(p: Vector2, mask: Dictionary) -> bool:
	var t := String(mask.get("type", "rect"))
	if t == "rect":
		var r: Variant = mask.get("xyxy", [])
		if not (r is Array) or (r as Array).size() < 4:
			return false
		return p.x >= float(r[0]) and p.x <= float(r[2]) and p.y >= float(r[1]) and p.y <= float(r[3])
	if t == "ellipse":
		var e: Variant = mask.get("ellipse", mask.get("xyxy", []))
		if not (e is Array) or (e as Array).size() < 4:
			return false
		var rx := float(e[2])
		var ry := float(e[3])
		if rx == 0.0 or ry == 0.0:
			return false
		var dx := (p.x - float(e[0])) / rx
		var dy := (p.y - float(e[1])) / ry
		return dx * dx + dy * dy <= 1.0
	if t == "polygon":
		return _pnpoly(p, mask.get("points", []))
	return false


static func _pnpoly(p: Vector2, pts: Variant) -> bool:
	if not (pts is Array) or (pts as Array).size() < 3:
		return false
	var arr: Array = pts
	var inside := false
	var j := arr.size() - 1
	for i in arr.size():
		var pi: Array = arr[i]
		var pj: Array = arr[j]
		var yi := float(pi[1])
		var yj := float(pj[1])
		var xi := float(pi[0])
		var xj := float(pj[0])
		if (yi > p.y) != (yj > p.y) and p.x < (xj - xi) * (p.y - yi) / (yj - yi + 0.0000001) + xi:
			inside = not inside
		j = i
	return inside


static func hip_in_seat(hip: Vector2, seat: Dictionary) -> bool:
	var mask: Variant = seat.get("mask", {})
	return mask is Dictionary and point_in_mask(hip, mask)


## Feet target for a station slot. Overflow wraps and nudges down.
static func feet_for_station(location: String, station: String, slot: int) -> Dictionary:
	var list := seats_at(location, station)
	if list.is_empty():
		return {}
	var n := list.size()
	var seat: Dictionary = list[slot % n]
	var feet := feet_from_hip(hip_of(seat), seat)
	if slot >= n:
		feet += Vector2(0, 14 * int(slot / n))
	return {"seat": seat, "feet": feet, "pose": String(seat.get("pose", "seat")), "facing": String(seat.get("facing", "s"))}
