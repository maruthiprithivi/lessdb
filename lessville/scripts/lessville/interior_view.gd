extends Control
const Scale = preload("res://scripts/lessville/world_scale.gd")
const Atmosphere = preload("res://scripts/lessville/world_atmosphere.gd")
## Close-up of one place, drawn from the same service snapshot as the town.
## Characters enter at the door, walk to the station their stored activity implies
## (the service's `station` field), and perform it: typing, sitting, pouring, running.
## The "On the bus" view shows riders whose stored travel record says they are on the bus.
const TownView = preload("res://scripts/lessville/town_view.gd")
const Attire = preload("res://scripts/lessville/attire.gd")
const SeatCatalog = preload("res://scripts/lessville/seat_catalog.gd")
const NAMES := {"home": "Home — Ama & Sam's house", "office": "Office", "cafe": "Less Café",
	"grocery": "FreshMart grocery", "school": "University", "gym": "Gym", "trail": "Reservoir trail",
	"park": "Fountain Park", "bus_stop": "Bus stop", "away": "Out of town", "bus": "On the bus"}
const BG := {"home": "home", "office": "office", "cafe": "cafe", "grocery": "grocery", "school": "school",
	"gym": "gym", "trail": "trail", "bus": "bus"}
const DOORS := {"home": Vector2(380, 492), "office": Vector2(160, 492), "cafe": Vector2(380, 492),
	"grocery": Vector2(380, 492), "school": Vector2(575, 492), "gym": Vector2(380, 492),
	"trail": Vector2(230, 520), "bus": Vector2(705, 345)}
## station -> [pose, spots...]. pose: stand | seat | type | run | walk
const STATIONS := {
	"home": {"kitchen": ["stand", Vector2(520, 178), Vector2(605, 188)], "dining": ["seat", Vector2(560, 262), Vector2(668, 282)],
		"living": ["seat", Vector2(150, 200), Vector2(212, 200)]},
	# Desk spots are the seats that exist in office.png: center stool, right purple chair, left teal chair.
	# The previous single desk point (575, 262) sat in the aisle, so a typist stood mid-room.
	"office": {"desk": ["type", Vector2(404, 252), Vector2(716, 252), Vector2(104, 246)], "meeting": ["stand", Vector2(600, 405), Vector2(545, 420)],
		"coffee": ["stand", Vector2(300, 318)]},
	"cafe": {"till": ["stand", Vector2(610, 152)], "espresso": ["stand", Vector2(520, 148)], "fridge": ["stand", Vector2(668, 150)],
		"queue": ["stand", Vector2(520, 305), Vector2(455, 325), Vector2(585, 325)], "table1": ["seat", Vector2(152, 238)],
		"table2": ["seat", Vector2(142, 350)], "table3": ["seat", Vector2(58, 238), Vector2(58, 350)]},
	"grocery": {"aisle": ["stand", Vector2(330, 245), Vector2(240, 300), Vector2(560, 268)]},
	"school": {"lecture": ["seat", Vector2(250, 300), Vector2(160, 300), Vector2(330, 300)]},
	"gym": {"workout": ["run", Vector2(290, 150), Vector2(390, 150)]},
	"trail": {"path": ["walk", Vector2(470, 476), Vector2(545, 424), Vector2(615, 372), Vector2(395, 496)]},
	"bus": {"seat": ["seat", Vector2(452, 268), Vector2(296, 268), Vector2(150, 268), Vector2(632, 268), Vector2(506, 268)]},
}
const DEFAULT_STATION := {"home": "living", "office": "desk", "cafe": "queue", "grocery": "aisle", "school": "lecture",
	"gym": "workout", "trail": "path", "bus": "seat"}
const SPEED := 150.0
## Window panes in interior art (location pixels). Time of day is shown through
## these only — interiors are not given a full-room sky overlay.
const INTERIOR_WINDOWS := {
	"home": [Rect2(155, 12, 150, 72)],
	"office": [Rect2(205, 15, 80, 92)],
	"cafe": [Rect2(50, 10, 175, 70)],
}
const OUTDOOR_INTERIORS := ["trail"]
var location := ""
var state: Dictionary = {}
var town: Control
var _tex := {}
var _anim := 0.0
var _iv := {}  # id -> {pos, target, dir, alpha, leaving}
var _last_frame := {}  # id -> last texture key that actually existed


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_STOP
	for k: String in BG:
		_tex[k] = load("res://assets/game/locations/%s.png" % BG[k])
	_tex["bus_scenery"] = load("res://assets/game/locations/bus_scenery.png")
	for id: String in ["maya", "sam", "lin", "ama", "bo"]:
		for i in 4:
			_tex["%s_%d" % [id, i]] = load("res://assets/game/characters/%s_%d.png" % [id, i])
		for i in 2:
			_tex["%s_back_%d" % [id, i]] = load("res://assets/game/characters/%s_back_%d.png" % [id, i])
		for i in (4 if id == "lin" else 2):
			_tex["%s_seat_%d" % [id, i]] = load("res://assets/game/characters/%s_seat_%d.png" % [id, i])
		for facing: String in ["e", "w"]:
			for kind: String in ["seat", "type", "typing"]:
				var sp := "res://assets/game/characters/%s_%s_%s.png" % [id, kind, facing]
				if ResourceLoader.exists(sp):
					_tex["%s_%s_%s" % [id, kind, facing]] = load(sp)
	for n: String in ["sam", "priya", "tom"]:
		for i in 2:
			_tex["%s_typing_%d" % [n, i]] = load("res://assets/game/characters/%s_typing_%d.png" % [n, i])
	for key: String in ["ama_0", "ama_1", "ama_2", "ama_3", "sam_0", "sam_1", "sam_2", "sam_3", "bo_3"]:
		var path := "res://assets/game/characters/%s.png" % key.replace("_", "_free_")
		if ResourceLoader.exists(path):
			_tex[key.replace("_", "_free_")] = load(path)
	# Off-duty / at-home variants (apron, cap, backpack removed).
	var dir := DirAccess.open("res://assets/game/characters")
	if dir != null:
		dir.list_dir_begin()
		var fname := dir.get_next()
		while fname != "":
			if fname.ends_with(".png") and "_plain_" in fname:
				var key := fname.get_basename()
				_tex[key] = load("res://assets/game/characters/%s.png" % key)
			fname = dir.get_next()
		dir.list_dir_end()
	_tex["mochi"] = load("res://assets/game/characters/mochi_0.png")
	_tex["mochi_3"] = load("res://assets/game/characters/mochi_3.png")
	_load_seat_overlays()


func _load_seat_overlays() -> void:
	# Furniture tops that draw in front of sitters (tables, side desks).
	for s: Dictionary in SeatCatalog.seats():
		var name := _as_text(s.get("fg", ""))
		if name == "" or _tex.has("fg:" + name):
			continue
		var path := "res://assets/game/seats/fg/%s.png" % name
		if ResourceLoader.exists(path):
			_tex["fg:" + name] = load(path)


func _as_text(v: Variant, fallback: String = "") -> String:
	if v == null:
		return fallback
	return str(v)


func open(loc: String) -> void:
	location = loc
	_iv.clear()


func _process(delta: float) -> void:
	_anim += delta
	if visible and location != "":
		_update(delta)
		queue_redraw()


func _sim_t() -> float:
	return town.sim_time() if town else float(state.get("clock", {}).get("minute", 0))


func present() -> Array:
	var out := []
	for a: Dictionary in state.get("agents", []):
		var tr: Variant = a.get("travel")
		if location == "bus":
			if tr is Dictionary and String(tr.get("mode")) == "bus":
				out.append(a)
		elif String(a.get("location")) == location and not (tr is Dictionary):
			out.append(a)
	return out


func _leaving() -> Array:
	var out := []
	if location == "bus":
		return out
	var t := _sim_t()
	for a: Dictionary in state.get("agents", []):
		var tr: Variant = a.get("travel")
		if tr is Dictionary and String(tr.get("from")) == location and t - float(tr.get("depart", 0)) < 6.0:
			out.append(a)
	return out


func station_spot(a: Dictionary, taken: Dictionary) -> Array:
	var st := String(a.get("station", ""))
	var table: Dictionary = STATIONS.get(location, {})
	st = _bound_station(a, st, table)
	if not table.has(st) and SeatCatalog.seats_at(location, st).is_empty():
		st = DEFAULT_STATION.get(location, "")
		st = _bound_station(a, st, table)
	var n: int = taken.get(st, 0)
	taken[st] = n + 1
	# Catalog seats win: hip is the sit-contact; we return feet so walking still
	# ends on the floor in front of the chair and _draw_sprite stays feet-anchored.
	var placed: Dictionary = SeatCatalog.feet_for_station(location, st, n)
	if not placed.is_empty():
		return [String(placed["pose"]), placed["feet"], placed]
	if not table.has(st):
		return ["stand", Vector2(384, 380)]
	var entry: Array = table[st]
	var spots := entry.size() - 1
	var p: Vector2 = entry[1 + (n % spots)] + Vector2(0, 0 if n < spots else 14 * (n / spots))
	return [String(entry[0]), p]


## Cafe staff stay behind the counter; customers stay in the queue or at tables.
func _is_cafe_staff(a: Dictionary) -> bool:
	var id := String(a.get("id"))
	if id == "maya" or id == "bo":
		return true
	var title := String(a.get("title", "")).to_lower()
	return "owner" in title or "barista" in title


func _bound_station(a: Dictionary, st: String, table: Dictionary) -> String:
	if location != "cafe":
		return st if table.has(st) else st
	var staff := _is_cafe_staff(a)
	var staff_stations := PackedStringArray(["till", "espresso", "fridge"])
	var customer_stations := PackedStringArray(["queue", "table1", "table2", "table3"])
	if staff and st in customer_stations:
		var act := String(a.get("activity", "")).to_lower()
		var kind := String(a.get("activity_kind", "")).to_lower()
		if "espresso" in act or "shot" in act or kind == "cook":
			return "espresso"
		return "till"
	if not staff and st in staff_stations:
		return "queue"
	return st


func _update(delta: float) -> void:
	var taken := {}
	var here := {}
	# Compute once before anyone is inserted. Using _iv.is_empty() inside the loop
	# placed only the first agent at their station and sent everyone else from the door.
	var already_inside := _iv.is_empty() and location != "trail"
	for a: Dictionary in present():
		var id := String(a.get("id"))
		here[id] = true
		var spot := station_spot(a, taken)
		if not _iv.has(id):
			# Already inside when the view opened: start at the station; otherwise enter at the door.
			_iv[id] = {"pos": spot[1] if already_inside else DOORS.get(location, Vector2(384, 492)), "dir": Vector2.UP, "alpha": 1.0 if already_inside else 0.0}
		var v: Dictionary = _iv[id]
		v["target"] = spot[1]
		v["pose"] = spot[0]
		v["leaving"] = false
		v["facing"] = "s"
		v["seat_id"] = ""
		v["fg"] = ""
		v["fg_front_y"] = -1.0
		if spot.size() >= 3 and spot[2] is Dictionary:
			var meta: Dictionary = spot[2]
			var seat: Dictionary = meta.get("seat", meta)
			v["facing"] = _as_text(meta.get("facing", seat.get("facing", "s")), "s")
			v["seat_id"] = _as_text(seat.get("id", ""))
			v["fg"] = _as_text(seat.get("fg", ""))
			if seat.get("fg_front_y") != null:
				v["fg_front_y"] = float(seat.get("fg_front_y", -1))
	for a: Dictionary in _leaving():
		var id := String(a.get("id"))
		if _iv.has(id) and not here.has(id):
			_iv[id]["target"] = DOORS.get(location, Vector2(384, 492))
			_iv[id]["leaving"] = true
			here[id] = true
	for id: String in _iv.keys():
		if not here.has(id):
			_iv.erase(id)
			continue
		var v: Dictionary = _iv[id]
		var pos: Vector2 = v["pos"]
		var target: Vector2 = v["target"]
		var d := pos.distance_to(target)
		var step := SPEED * delta
		if d > 1.0:
			v["dir"] = (target - pos).normalized()
			v["pos"] = pos + (target - pos) / d * minf(step, d)
		v["moving"] = d > 1.0
		if bool(v["leaving"]) and d <= 1.0:
			v["alpha"] = maxf(0.0, float(v["alpha"]) - delta * 3.0)
		else:
			v["alpha"] = minf(1.0, float(v["alpha"]) + delta * 3.0)


func _side_frame(id: String, facing: String, pose: String) -> String:
	var keys: Array[String] = []
	if pose == "type":
		keys.append("%s_type_%s" % [id, facing])
		keys.append("%s_typing_%s" % [id, facing])
	keys.append("%s_seat_%s" % [id, facing])
	keys.append("%s_type_%s" % [id, facing])
	for k: String in keys:
		if _tex.has(k):
			return k
	return ""


func _frame_key(a: Dictionary, v: Dictionary) -> String:
	var id := String(a.get("id"))
	var f := int(_anim * 6.0) % 2
	if bool(v.get("moving", false)):
		var dir: Vector2 = v["dir"]
		if dir.y < -0.5:
			return "%s_back_%d" % [id, f]
		return "%s_%d" % [id, 1 + f]
	var pose := String(v.get("pose", "stand"))
	var act := String(a.get("activity", "")).to_lower()
	var facing := String(v.get("facing", "s"))
	if facing == "e" or facing == "w":
		var side_key := _side_frame(id, facing, pose)
		if side_key != "":
			return side_key
	match pose:
		"type":
			if id == "sam":
				return "sam_typing_%d" % (int(_anim * 3.0) % 2)
			if facing == "n" and _tex.has("%s_typing_0" % id):
				return "%s_typing_%d" % [id, int(_anim * 3.0) % 2]
			if facing == "n" and _tex.has("%s_back_0" % id) and not _tex.has("%s_seat_0" % id):
				return "%s_back_0" % id
			return "%s_seat_0" % id
		"seat":
			if facing == "n":
				if id == "sam" and _tex.has("sam_typing_0"):
					return "sam_typing_%d" % (int(_anim * 3.0) % 2)
				if id == "lin":
					if "headphone" in act or "studying" in act:
						return "lin_seat_1"
					return "lin_seat_2" if _tex.has("lin_seat_2") else "lin_seat_0"
				if _tex.has("%s_typing_0" % id):
					return "%s_typing_%d" % [id, int(_anim * 3.0) % 2]
			if id == "lin":
				if "headphone" in act or "studying" in act:
					return "lin_seat_1"
				if "lecture" in act:
					return "lin_seat_2"
				return "lin_seat_0"
			if "lunch" in act or "coffee" in act or "dinner" in act:
				return "%s_seat_1" % id
			return "%s_seat_0" % id
		"run":
			return "%s_back_%d" % [id, int(_anim * 8.0) % 2]
		"walk":
			return "%s_%d" % [id, 1 + f]
	if _talking(a):
		return "%s_3" % id
	return "%s_0" % id


func _talking(a: Dictionary) -> bool:
	var b: Variant = a.get("bubble")
	return b is Dictionary and float(b.get("until", 0)) > _sim_t()


func _draw() -> void:
	if location == "":
		return
	var s := size.x / 768.0
	var font := get_theme_default_font()
	draw_rect(Rect2(Vector2.ZERO, size), Color("0f131b"), true)
	if location == "bus":
		_draw_scenery(s)
	if _tex.has(location):
		draw_texture_rect(_tex[location], Rect2(Vector2.ZERO, Vector2(768, 512) * s), false)
	var md := fposmod(_sim_t(), 1440.0)
	# Only agents present in the served state are drawn as character sprites.
	# Previously the office always painted Priya and Tom during 08:00–18:00 even
	# when /api/state listed a single occupant (e.g. Sam at 17:52).
	if location == "home":
		# Mochi rests at home unless presence says he is out on a walk.
		var walking := false
		for a: Dictionary in state.get("agents", []):
			var pr: Variant = a.get("presence")
			if pr is Dictionary and (pr as Dictionary).get("companion") == "mochi":
				walking = true
		if not walking:
			var dt: Texture2D = _tex["mochi_3"] if int(_anim) % 5 == 0 else _tex["mochi"]
			draw_texture_rect(dt, Rect2(Vector2(85, 372) * s - Vector2(36, 52) * s * Scale.INTERIOR_DOG, Scale.DOG_CANVAS * s * Scale.INTERIOR_DOG), false)
	# Characters, y-sorted, with furniture overlays between sitters (behind the
	# table/desk) and anyone standing in front of that furniture.
	var rows := []
	for a: Dictionary in state.get("agents", []):
		var id := String(a.get("id"))
		if _iv.has(id):
			rows.append([a, _iv[id]])
	rows.sort_custom(func(x: Array, y: Array) -> bool: return (x[1]["pos"] as Vector2).y < (y[1]["pos"] as Vector2).y)
	var behind := []
	var front := []
	for r: Array in rows:
		if _behind_furniture(r[1]):
			behind.append(r)
		else:
			front.append(r)
	for r: Array in behind:
		_draw_agent_row(r, s)
	_draw_furniture_overlays(s)
	for r: Array in front:
		_draw_agent_row(r, s)
	for r: Array in rows:
		_draw_label_and_bubble(r[0], r[1], s, font)
	if location == "trail":
		for a: Dictionary in present():
			var pr2: Variant = a.get("presence")
			var with_dog: bool = pr2 is Dictionary and (pr2 as Dictionary).get("companion") == "mochi"
			if String(a.get("id")) == "ama" and _iv.has("ama") and with_dog:
				var p: Vector2 = (_iv["ama"]["pos"] as Vector2) + Vector2(-60, 10)
				draw_texture_rect(_tex["mochi"], Rect2(p * s - Vector2(36, 52) * s * Scale.INTERIOR_DOG, Scale.DOG_CANVAS * s * Scale.INTERIOR_DOG), false)
	if present().is_empty() and _leaving().is_empty():
		var msg := "Nobody from Lessville is here right now."
		if location == "bus":
			msg = "No one from Lessville is on the bus right now."
		var w2 := font.get_string_size(msg, HORIZONTAL_ALIGNMENT_LEFT, -1, 18).x + 30
		var rr := Rect2(Vector2((size.x - w2) / 2, size.y * 0.45), Vector2(w2, 40))
		draw_rect(rr, Color(0.08, 0.1, 0.14, 0.85), true)
		draw_string(font, rr.position + Vector2(15, 27), msg, HORIZONTAL_ALIGNMENT_LEFT, -1, 18, Color.WHITE)
	if location == "bus":
		_draw_next_stop(s, font)
	_draw_interior_atmosphere(s)
	_draw_db_strip(font)
	draw_rect(Rect2(0, 0, size.x, 42), Color(0.06, 0.08, 0.11, 0.88), true)
	var title: String = NAMES.get(location, location)
	draw_string(font, Vector2(16, 28), title, HORIZONTAL_ALIGNMENT_LEFT, -1, 19, Color("f2b54a"))
	var who := "%d here" % present().size()
	draw_string(font, Vector2(16 + font.get_string_size(title, HORIZONTAL_ALIGNMENT_LEFT, -1, 19).x + 16, 28), who, HORIZONTAL_ALIGNMENT_LEFT, -1, 14, Color("9aa6bb"))


## Prefer the requested frame; if it is missing, hold the last real sprite rather than flickering or drawing a default marker.
func _resolve_frame(id: String, key: String) -> String:
	if _tex.has(key):
		_last_frame[id] = key
		return key
	if _last_frame.has(id) and _tex.has(String(_last_frame[id])):
		return String(_last_frame[id])
	var stand := "%s_0" % id
	if _tex.has(stand):
		_last_frame[id] = stand
		return stand
	return key


func _behind_furniture(v: Dictionary) -> bool:
	# A sitter bound to a furniture overlay always draws under it (the overlay
	# is only the table/desk top, not the chair). Walkers use front_y.
	if _as_text(v.get("fg", "")) != "":
		return true
	var front_y := float(v.get("fg_front_y", -1.0))
	if front_y < 0.0:
		return false
	return (v["pos"] as Vector2).y <= front_y + 8.0


func _draw_agent_row(r: Array, s: float) -> void:
	var a: Dictionary = r[0]
	var v: Dictionary = r[1]
	var base := _hands_free(a, _frame_key(a, v))
	var clock: Dictionary = state.get("clock", {}) if state.get("clock") is Dictionary else {}
	var layers: Dictionary = Attire.layers_for(a, clock)
	var key := _resolve_frame(String(a.get("id")), Attire.apply_key(String(a.get("id")), base, layers, func(k: String) -> bool: return _tex.has(k)))
	var facing := String(v.get("facing", "s"))
	var profile := key.ends_with("_e") or key.ends_with("_w")
	var flip: bool = (bool(v.get("moving", false)) and (v["dir"] as Vector2).x < -0.2) or (not bool(v.get("moving", false)) and facing == "w" and not profile)
	_draw_sprite(key, v["pos"], s, float(v.get("alpha", 1.0)), flip, 1.0)


func _draw_furniture_overlays(s: float) -> void:
	var drawn := {}
	for sdict: Dictionary in SeatCatalog.seats_at(location):
		var name := _as_text(sdict.get("fg", ""))
		if name == "" or drawn.has(name):
			continue
		var tex: Variant = _tex.get("fg:" + name)
		if tex is Texture2D:
			draw_texture_rect(tex, Rect2(Vector2.ZERO, Vector2(768, 512) * s), false)
			drawn[name] = true


func _draw_sprite(key: String, feet: Vector2, s: float, alpha: float, flip: bool, scale_k: float) -> void:
	if not _tex.has(key) or alpha <= 0.01:
		return
	var sz := Scale.ACTOR_CANVAS * s * Scale.INTERIOR_ACTOR * scale_k
	var pos := feet * s
	var tint := Color(1, 1, 1, alpha)
	if flip:
		draw_set_transform(pos, 0.0, Vector2(-1, 1))
		draw_texture_rect(_tex[key], Rect2(Vector2(-sz.x / 2, -sz.y), sz), false, tint)
		draw_set_transform(Vector2.ZERO)
	else:
		draw_texture_rect(_tex[key], Rect2(pos - Vector2(sz.x / 2, sz.y), sz), false, tint)


func _draw_label_and_bubble(a: Dictionary, v: Dictionary, s: float, font: Font) -> void:
	if float(v.get("alpha", 1.0)) < 0.5:
		return
	var pos: Vector2 = (v["pos"] as Vector2) * s
	var sz := Scale.ACTOR_CANVAS * s * Scale.INTERIOR_ACTOR
	var id := String(a.get("id"))
	var name := "%s · %s" % [a.get("name", id), a.get("activity", "")]
	var w := minf(font.get_string_size(name, HORIZONTAL_ALIGNMENT_LEFT, -1, 13).x + 12, 260.0)
	var plate := Rect2(pos + Vector2(-w / 2, 4), Vector2(w, 20))
	plate.position.x = clampf(plate.position.x, 4, size.x - w - 4)
	draw_rect(plate, Color(0.08, 0.1, 0.14, 0.86), true)
	draw_string(font, plate.position + Vector2(6, 15), name, HORIZONTAL_ALIGNMENT_LEFT, w - 10, 13, Color.WHITE)
	if not _talking(a):
		return
	var b: Dictionary = a["bubble"]
	var lines := TownView.wrap_text(font, String(b.get("text", "")), 14, 230.0)
	var by := String(b.get("by", ""))
	var bw := 0.0
	for l: String in lines:
		bw = maxf(bw, font.get_string_size(l, HORIZONTAL_ALIGNMENT_LEFT, -1, 14).x)
	var extra := 14 if by != "" else 0
	var box := Vector2(maxf(bw, 120.0 if by != "" else 0.0) + 18, lines.size() * 18 + 12 + extra)
	var top := pos + Vector2(-box.x / 2, -sz.y - box.y - 6)
	top.x = clampf(top.x, 6, size.x - box.x - 6)
	top.y = maxf(top.y, 50)
	var bg := Color("dbe9ff") if String(b.get("kind")) == "message" else Color(1, 1, 1, 0.97)
	draw_rect(Rect2(top + Vector2(2, 3), box), Color(0, 0, 0, 0.3), true)
	draw_rect(Rect2(top, box), bg, true)
	draw_rect(Rect2(top, box), Color(0.1, 0.12, 0.16), false, 1.5)
	for j in lines.size():
		draw_string(font, top + Vector2(9, 19 + j * 18), lines[j], HORIZONTAL_ALIGNMENT_LEFT, -1, 14, Color(0.1, 0.12, 0.16))
	if by != "":
		draw_string(font, top + Vector2(9, 19 + lines.size() * 18 + 2), "voiced by " + by, HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color("6b7487"))


func _draw_scenery(s: float) -> void:
	var tex: Texture2D = _tex["bus_scenery"]
	var moving := not present().is_empty()
	var w := 1194.0 * 0.45
	var h := 512.0 * 0.45
	var off := fposmod(_anim * (70.0 if moving else 0.0), w)
	var y := 40.0
	var x := -off
	while x < 768.0:
		draw_texture_rect(tex, Rect2(Vector2(x, y) * s, Vector2(w, h) * s), false)
		x += w


func _draw_next_stop(s: float, font: Font) -> void:
	var riders := present()
	var text := ""
	if riders.is_empty():
		var nb: Variant = state.get("bus", {}).get("next")
		if nb is Dictionary:
			var sched := int(nb.get("scheduled", 0))
			var delay := int(nb.get("delay", 0))
			text = "Next bus %02d:%02d%s" % [(sched % 1440) / 60, sched % 60, (" (+%d min)" % delay) if delay > 0 else ""]
	else:
		var tr: Dictionary = riders[0]["travel"]
		var to := String(tr.get("to", ""))
		var eta := maxi(0, int(ceil(float(tr.get("arrive", 0)) - _sim_t())))
		var dest := "University" if to == "school" else ("Lessville stop" if to == "bus_stop" else "Out of town")
		text = "Next: %s · %d min" % [dest, eta]
	var r := Rect2(Vector2(102, 22) * s, Vector2(118, 26) * s)
	draw_rect(r, Color("1a0f08"), true)
	draw_string(font, r.position + Vector2(6, r.size.y * 0.72), text, HORIZONTAL_ALIGNMENT_LEFT, r.size.x - 8, int(12 * s / 0.9), Color("ffb347"))


## What LessDB did most recently for the people in this scene (newest first).
func _draw_db_strip(font: Font) -> void:
	var ids := {}
	for a: Dictionary in present():
		ids[String(a.get("id"))] = true
	var items := []
	for f: Dictionary in state.get("feed", []):
		if ids.has(String(f.get("actor", ""))) and String(f.get("kind")) in ["memory", "message", "decision", "access", "say"]:
			items.append(f)
		if items.size() >= 2:
			break
	if items.is_empty():
		return
	var h := 24.0 * items.size() + 30.0
	var top := size.y - h
	draw_rect(Rect2(0, top, size.x, h), Color(0.05, 0.07, 0.1, 0.9), true)
	draw_string(font, Vector2(14, top + 19), "What LessDB recorded here", HORIZONTAL_ALIGNMENT_LEFT, -1, 13, Color("f2b54a"))
	for i in items.size():
		var f: Dictionary = items[i]
		var refs: Array = f.get("refs", [])
		var line := "%s  %s — %s" % ["[" + ", ".join(refs) + "]" if not refs.is_empty() else "", String(f.get("title", "")), String(f.get("detail", ""))]
		draw_string(font, Vector2(14, top + 42 + i * 24), line.strip_edges(), HORIZONTAL_ALIGNMENT_LEFT, size.x - 28, 13, Color("dfe6f2"))
## Same rule as the town: baked-prop frames only when presence justifies them.
func _hands_free(a: Dictionary, key: String) -> String:
	var free_key := key.replace("_", "_free_")
	if not _tex.has(free_key):
		return key
	var pr: Variant = a.get("presence")
	var hands := String((pr as Dictionary).get("hands", "free")) if pr is Dictionary else "free"
	match String(a.get("id")):
		"ama":
			return key if hands == "leash" else free_key
		"sam":
			return key if hands == "phone" else free_key
		"bo":
			return key if hands == "serving" else free_key
	return key

func _draw_interior_atmosphere(s: float) -> void:
	var clock: Dictionary = state.get("clock", {}) if not state.is_empty() else {}
	var atmo: Dictionary = Atmosphere.snapshot(clock, _sim_t())
	if location in OUTDOOR_INTERIORS:
		# Trail is outdoors: use the same ambient/sky wash as Town, not window-only.
		var dark := float(atmo.get("darkness", 0.0))
		if dark > 0.0:
			draw_rect(Rect2(Vector2.ZERO, size), Color(0.04, 0.06, 0.16, dark), true)
		var sky: Color = atmo.get("sky_top", Color(0.05, 0.08, 0.18))
		sky.a = float(atmo.get("sky_alpha", 0.0)) * 0.7
		if sky.a > 0.02:
			draw_rect(Rect2(0, 0, size.x, 80.0 * s), sky, true)
		return
	# Interiors: only the window glass changes with the clock.
	var panes: Array = INTERIOR_WINDOWS.get(location, [])
	var dark := float(atmo.get("darkness", 0.0))
	var sky: Color = atmo.get("sky_top", Color(0.05, 0.08, 0.18))
	var hz: Color = atmo.get("sky_horizon", sky)
	var phase := String(atmo.get("phase", "day"))
	for r: Rect2 in panes:
		var rr := Rect2(r.position * s, r.size * s)
		if phase == "night" or dark > 0.35:
			# Night glass: the baked day view is covered by a dark sky.
			var glass := Color(sky.r, sky.g, sky.b, clampf(0.55 + dark * 0.4, 0.55, 0.92))
			draw_rect(rr, glass, true)
			# A faint warm lamp spill on the inner sill, so the room still feels lit.
			draw_rect(Rect2(rr.position + Vector2(0, rr.size.y - 8.0 * s), Vector2(rr.size.x, 8.0 * s)), Color(1.0, 0.85, 0.55, 0.28), true)
		elif phase == "dawn" or phase == "dusk":
			var warm := Color(hz.r, hz.g, hz.b, 0.40)
			draw_rect(rr, warm, true)
		# Day: leave the baked outdoor view; a very soft warm shaft is enough.
		elif float(atmo.get("sky_alpha", 0.0)) > 0.0:
			draw_rect(rr, Color(1.0, 0.97, 0.90, 0.06), true)




func debug_outfit(a: Dictionary, v: Dictionary = {}) -> Dictionary:
	if v.is_empty() and _iv.has(String(a.get("id"))):
		v = _iv[String(a.get("id"))]
	var base := _hands_free(a, _frame_key(a, v)) if not v.is_empty() else "%s_0" % String(a.get("id"))
	var clock: Dictionary = state.get("clock", {}) if state.get("clock") is Dictionary else {}
	var layers: Dictionary = Attire.layers_for(a, clock)
	var key := Attire.apply_key(String(a.get("id")), base, layers, func(k: String) -> bool: return _tex.has(k))
	return {"frame": key, "layers": layers, "base_frame": base}
