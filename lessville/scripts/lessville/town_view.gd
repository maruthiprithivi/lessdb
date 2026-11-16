extends Control
const Scale = preload("res://scripts/lessville/world_scale.gd")
const TownArt = preload("res://scripts/lessville/town_art.gd")
const RoadTraffic = preload("res://scripts/lessville/road_traffic.gd")
const TownAnimals = preload("res://scripts/lessville/town_animals.gd")
var _road := RoadTraffic.new()
const SeatCatalog = preload("res://scripts/lessville/seat_catalog.gd")
const Atmosphere = preload("res://scripts/lessville/world_atmosphere.gd")
const Attire = preload("res://scripts/lessville/attire.gd")
## Renders the neighbourhood from the latest service snapshot. Purely presentational:
## positions are interpolated from LessDB-backed travel records, never decided here.

signal agent_clicked(id: String)
signal place_clicked(loc: String)

const Nav = preload("res://scripts/lessville/town_nav.gd")
const MAP_W := 1536.0
const MAP_H := 1024.0
const NODES := {
	"A": Vector2(258, 340), "B": Vector2(258, 485), "W": Vector2(-60, 485), "C": Vector2(500, 485),
	"D": Vector2(520, 420), "E": Vector2(820, 352), "F": Vector2(820, 420), "G": Vector2(1120, 410),
	"H": Vector2(1120, 530), "I": Vector2(1440, 410), "J": Vector2(1440, 352), "X": Vector2(1600, 500),
	"K": Vector2(500, 665), "L": Vector2(820, 655), "M": Vector2(1160, 665), "N": Vector2(470, 712),
	"SW": Vector2(215, 640), "S": Vector2(215, 1080),
	"T": Vector2(1425, 515),
}
const EDGES := [["A", "B"], ["B", "C"], ["B", "W"], ["C", "D"], ["D", "F"], ["F", "E"], ["F", "G"], ["G", "H"],
	["H", "T"], ["T", "I"], ["I", "J"], ["H", "X"], ["C", "K"], ["K", "N"], ["K", "L"], ["L", "M"], ["M", "H"],
	["K", "SW"], ["SW", "S"]]
const LOC_NODE := {"home": "A", "office": "E", "cafe": "J", "park": "L", "bus_stop": "N", "grocery": "X",
	"gym": "X", "school": "X", "trail": "S", "away": "W", "transit": "C"}
const INDOOR := ["home", "office", "cafe"]
const HOTSPOTS := {
	"home": Rect2(90, 0, 340, 360), "office": Rect2(600, 20, 490, 370), "cafe": Rect2(1200, 100, 320, 290),
	"park": Rect2(700, 440, 250, 190), "bus_stop": Rect2(360, 600, 280, 160),
}
const TAGS := {"home": Vector2(258, 18), "office": Vector2(845, 20), "cafe": Vector2(1360, 92),
	"park": Vector2(825, 440), "bus_stop": Vector2(470, 598)}
## Street lamps, door lanterns and sconces are loaded from config/scene_lamps.json,
## which is derived from town.png (tools/scene_lighting.py) — not a hand-drawn list.
## Tree-canopy regions (map pixels) that shear with the wind.
const FOLIAGE := [
	Rect2(0, 0, 170, 170), Rect2(360, 0, 230, 130), Rect2(1080, 0, 150, 150),
	Rect2(1450, 0, 86, 180), Rect2(700, 430, 90, 170), Rect2(900, 430, 90, 170),
	Rect2(0, 890, 1536, 134), Rect2(200, 600, 140, 90), Rect2(1280, 360, 80, 90),
]
## Window light is clipped to assets/game/locations/town_glass_mask.png, derived
## from the pane pixels of town.png — not from hand-drawn rectangles.
var debug_anim := -1.0
const OUTDOOR_OFFSETS := [Vector2(-200, -10), Vector2(-120, -10), Vector2(40, -10), Vector2(120, -10), Vector2(200, -10)]
const BUS_WAIT_OFFSETS := [Vector2(-80, -30), Vector2(-30, -30), Vector2(20, -30), Vector2(70, -30), Vector2(-55, 10)]
const AGENT_COLORS := {"maya": Color("e9a23b"), "sam": Color("5b8def"), "lin": Color("4fb38a"),
	"ama": Color("d0679a"), "bo": Color("b4805a")}

var state: Dictionary = {}
var selected := ""
var hover_place := ""
var reduced_motion := false
var _state_age := 0.0
var _anim_t := 0.0
var _tex := {}
var _glass_tex: Texture2D
var _scene_lamps: Array[Dictionary] = []  # {pos: Vector2, kind: String}
var _adj := {}
var _rain_seed: Array[Vector2] = []
var _drawn_agents := {}  # id -> screen rect for picking
## Visual walkers: id -> {pos, path, goal, alpha, dir, hidden}. Purely presentational smoothing so
## characters walk the street graph at a steady pace instead of jumping between polled snapshots.
var _vis := {}
var _bg_vis := {}
var _last_t := -1.0
var _last_tex := {}  # id -> last Texture2D that actually drew
const WALK_SPEED := 190.0  # map px per real second at x1
const FADE_RATE := 3.5
const DOOR_PAUSE := 0.3
## Map px of travel per walk-cycle step.  Keeps the two-frame cycle in step with
## the distance actually covered, so walkers never slide or skate.
const WALK_STRIDE := 34.0
const BG_STRIDE := WALK_STRIDE
const BUS_STOP_X := 470.0
const BUS_EAST_STOP_X := 1050.0
const BUS_Y := 805.0
const BUS_DOOR := Vector2(545, 772)
## Fixed service route along the road: off-map west -> Lessville stop -> east
## stop -> off-map east.  The bus is parameterised by arc length `s` on this
## polyline; it never free-slides across the map.
const BUS_ROUTE := [Vector2(-320, 805), Vector2(470, 805), Vector2(1050, 805), Vector2(1860, 805)]
const BUS_CRUISE := 320.0  # map px per second between stops
## Presentational bus: phase idle -> approach (eases in before the stored arrival)
## -> dwell (waits for boarders, at least 3 s) -> depart -> dwell -> leave.
## Arrival and dwell times come from the service's bus record.
var _bus := {"phase": "idle", "s": 0.0, "t": 0.0, "arr": -1.0, "trip_id": "", "stop": -1}
var _bus_stops: Array = [
	{"id": "lessville", "pos": Vector2(BUS_STOP_X, BUS_Y), "s": 0.0, "dwell_min": 3.0, "dwell_max": 8.0},
	{"id": "plaza", "pos": Vector2(BUS_EAST_STOP_X, BUS_Y), "s": 0.0, "dwell_min": 3.0, "dwell_max": 3.0},
]
var _bus_stops_ready := false
var _nav: Array = []


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_STOP
	clip_contents = true
	for a: Array in EDGES:
		_adj[a[0]] = _adj.get(a[0], []) + [a[1]]
		_adj[a[1]] = _adj.get(a[1], []) + [a[0]]
	_nav = Nav.build(NODES)
	_ensure_bus_stops()
	_tex["town"] = load("res://assets/game/locations/town.png")
	_glass_tex = load("res://assets/game/locations/town_glass_mask.png")
	_load_scene_lamps()
	_tex["bus"] = load("res://assets/game/props/bus.png")
	for id: String in ["maya", "sam", "lin", "ama", "bo", "mochi"]:
		for i in 4:
			_tex["%s_%d" % [id, i]] = load("res://assets/game/characters/%s_%d.png" % [id, i])
	for id: String in ["maya", "sam", "lin", "ama", "bo"]:
		for i in 2:
			_tex["%s_back_%d" % [id, i]] = load("res://assets/game/characters/%s_back_%d.png" % [id, i])
		for i in (4 if id == "lin" else 2):
			var sp := "res://assets/game/characters/%s_seat_%d.png" % [id, i]
			if ResourceLoader.exists(sp):
				_tex["%s_seat_%d" % [id, i]] = load(sp)
		for facing: String in ["e", "w"]:
			for kind: String in ["seat", "type", "typing"]:
				var sp2 := "res://assets/game/characters/%s_%s_%s.png" % [id, kind, facing]
				if ResourceLoader.exists(sp2):
					_tex["%s_%s_%s" % [id, kind, facing]] = load(sp2)
	for name: String in ["town.cafe_table_a", "town.cafe_table_b"]:
		var fp := "res://assets/game/seats/fg/%s.png" % name
		if ResourceLoader.exists(fp):
			_tex["fg:" + name] = load(fp)
	for p: String in ["coffee", "milk", "grocery_bag", "backpack"]:
		_tex[p] = load("res://assets/game/props/%s.png" % p)
	# Hands-free variants of frames whose art bakes in a prop (leash, phone, cup).
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
	var rng := RandomNumberGenerator.new()
	rng.seed = 7
	for i in 140:
		_rain_seed.append(Vector2(rng.randf(), rng.randf()))


func set_state(s: Dictionary) -> void:
	state = s
	_state_age = 0.0
	queue_redraw()


func set_debug_anim(t: float) -> void:
	debug_anim = t
	_anim_t = t


func _wind_phase() -> float:
	return debug_anim if debug_anim >= 0.0 else _anim_t


func _process(delta: float) -> void:
	_state_age += delta
	if debug_anim >= 0.0:
		_anim_t = debug_anim
	else:
		_anim_t += delta
	if not state.is_empty():
		_update_bus(0.0 if bool(state.get("clock", {}).get("paused", true)) else delta)
		_update_road(delta)
		_update_walkers(delta)
		_update_background(delta)
		queue_redraw()


## Estimated fractional sim-minute between polls. The service advances 2 sim-minutes per real
## second at x1; the estimate is bounded and never runs backwards unless the clock really jumped.
func sim_time() -> float:
	var c: Dictionary = state.get("clock", {})
	var m := float(c.get("minute", 0))
	var t := m
	if not bool(c.get("paused", true)):
		t = m + minf(_state_age * 2.0 * float(c.get("speed", 1)), 6.0 * float(c.get("speed", 1)))
	if _last_t >= 0.0 and t < _last_t and _last_t - t < 30.0:
		t = _last_t
	_last_t = t
	return t


## A fresh mid-street snapshot has no retained segment. Choose its best connected
## junction; subsequent destination changes preserve the segment in _walker_path.
func _best_start(p: Vector2, goal: String) -> String:
	var best := _nearest_node(p)
	var best_cost := p.distance_to(NODES[best]) + _path_len(best, goal)
	for e: Array in EDGES:
		# This edge has two bent lanes; its straight chord is not a walking route.
		if e == ["SW", "S"]:
			continue
		var a: Vector2 = NODES[e[0]]
		var b: Vector2 = NODES[e[1]]
		var cp := Geometry2D.get_closest_point_to_segment(p, a, b)
		if cp.distance_to(p) > Scale.ADULT_RADIUS:
			continue
		for n: String in [e[0], e[1]]:
			var cost := p.distance_to(NODES[n]) + _path_len(n, goal)
			if cost < best_cost - 0.5:
				best_cost = cost
				best = n
	return best


func _path_len(from_node: String, to_node: String) -> float:
	var pts := _path(from_node, to_node)
	var total := 0.0
	for i in range(1, pts.size()):
		total += (pts[i] as Vector2).distance_to(pts[i - 1])
	return total


func _nearest_node(p: Vector2) -> String:
	var best := "C"
	var bd := INF
	for k: String in NODES:
		var d := p.distance_squared_to(NODES[k])
		if d < bd:
			bd = d
			best = k
	return best


## v47 patched state often places commuters at location=transit, station=street, with no travel record.
func _on_street(a: Dictionary) -> bool:
	var loc := String(a.get("location", ""))
	var st := String(a.get("station", ""))
	var kind := String(a.get("activity_kind", ""))
	var pose := String(a.get("pose", ""))
	if loc == "away":
		return false
	if loc == "transit":
		return true
	if loc in INDOOR or loc == "bus":
		return false
	if st in ["street", "walk_side", "sidewalk"] or pose == "walk_side" or kind == "commuting":
		return true
	return false


func _infer_dest_node(a: Dictionary) -> String:
	var travel: Variant = a.get("travel")
	if travel is Dictionary:
		return String(LOC_NODE.get(String(travel.get("to", "")), "C"))
	var act := String(a.get("activity", "")).to_lower()
	var pairs := [
		["bus stop", "bus_stop"], ["bus_stop", "bus_stop"], ["dorm", "bus_stop"],
		["less café", "cafe"], ["café", "cafe"], ["cafe", "cafe"],
		["office", "office"], ["freshmart", "grocery"], ["grocery", "grocery"],
		["fountain", "park"], ["park", "park"],
		["universit", "school"], ["school", "school"], ["gym", "gym"],
		["trail", "trail"], ["home", "home"], ["out of town", "away"],
	]
	for pair: Array in pairs:
		if String(pair[0]) in act:
			return String(LOC_NODE.get(String(pair[1]), "C"))
	return "N"


func _infer_from_node(a: Dictionary) -> String:
	var travel: Variant = a.get("travel")
	if travel is Dictionary:
		return String(LOC_NODE.get(String(travel.get("from", "")), "C"))
	return "C"


func _street_pose(a: Dictionary, index: int, t: float) -> Dictionary:
	var from_n := _infer_from_node(a)
	var to_n := _infer_dest_node(a)
	var travel: Variant = a.get("travel")
	var p := 0.45
	if travel is Dictionary:
		var depart := float(travel.get("depart", 0))
		var arrive := float(travel.get("arrive", depart + 1))
		p = clampf((t - depart) / maxf(arrive - depart, 0.1), 0.0, 1.0)
	else:
		# Keep the walker on the sidewalk, moving slowly, without a travel record.
		p = 0.12 + fposmod((t + float(index) * 19.0) * 0.006, 0.76)
	var res := _along(_path(from_n, to_n), p)
	return {"pos": res[0], "dir": res[1], "moving": p < 0.98}


## Where the agent should be heading and whether they should be visible once there.
func _wants_town_seat(a: Dictionary) -> String:
	var loc := String(a.get("location", ""))
	var st := String(a.get("station", ""))
	var pose := String(a.get("pose", "")).to_lower()
	var act := String(a.get("activity", "")).to_lower()
	var sitting := pose == "seat" or "sit" in act or st.ends_with("bench") or st.ends_with("_table") or st == "seat"
	if not sitting:
		return ""
	if loc == "park" or st == "park_bench":
		return "park_bench"
	if loc == "bus_stop" or st == "bus_bench":
		return "bus_bench"
	if st == "cafe_table":
		return "cafe_table"
	return ""


func _town_seat_pose(a: Dictionary, index: int) -> Dictionary:
	var station := _wants_town_seat(a)
	if station == "":
		return {}
	var placed: Dictionary = SeatCatalog.feet_for_station("town", station, index)
	if placed.is_empty():
		return {}
	return {"pos": placed["feet"], "dir": Vector2.DOWN, "moving": false, "seated": true, "facing": String(placed.get("facing", "s")), "seat": placed.get("seat", {})}


func _goal_for(a: Dictionary, index: int) -> Dictionary:
	var travel: Variant = a.get("travel")
	if travel is Dictionary:
		var mode := String(travel.get("mode", "walk"))
		if mode == "bus":
			# Boarding: wait at the shelter while the bus approaches, then step in at the door.
			var ph := String(_bus["phase"])
			if ph == "approach":
				return {"node": "N", "point": _safe_point("N", BUS_WAIT_OFFSETS[index % BUS_WAIT_OFFSETS.size()]), "hidden": false}
			if ph == "dwell" and int(_bus.get("stop", 0)) == 0:
				return {"node": "N", "point": BUS_DOOR, "hidden": false, "arrive_hidden": true}
			return {"node": "N", "point": BUS_DOOR, "hidden": true}
		var to_n: String = LOC_NODE.get(String(travel.get("to", "")), "C")
		var outdoor := String(travel.get("to", "")) in ["park", "bus_stop"]
		var off: Vector2 = _outdoor_offset(to_n, index) if outdoor else Vector2.ZERO
		# If the served state still says they are in transit, do not fade them into the
		# destination — they would vanish from the street while the snapshot still lists them there.
		var fade_inside := (not outdoor) and not _on_street(a)
		return {"node": to_n, "point": _safe_point(to_n, off), "hidden": false, "arrive_hidden": fade_inside}
	var loc := String(a.get("location", ""))
	var n: String = LOC_NODE.get(loc, "C")
	if loc == "bus_stop" and "off the bus" in String(a.get("activity", "")).to_lower() and String(_bus["phase"]) == "approach":
		return {"node": "N", "point": BUS_DOOR, "hidden": true}
	var sit: Dictionary = _town_seat_pose(a, index)
	if not sit.is_empty():
		var node := n
		if String(a.get("station", "")) == "cafe_table":
			node = LOC_NODE.get("cafe", "C")
		return {"node": node, "point": sit["pos"], "hidden": false, "seated": true, "facing": String(sit.get("facing", "s"))}
	if loc == "park" or loc == "bus_stop":
		return {"node": n, "point": _safe_point(n, _outdoor_offset(n, index)), "hidden": false}
	if _on_street(a):
		var to_n := _infer_dest_node(a)
		var outdoor := to_n in ["L", "N", "S"]
		var off: Vector2 = _outdoor_offset(to_n, index) if outdoor else Vector2.ZERO
		return {"node": to_n, "point": _safe_point(to_n, off) if outdoor else (NODES[to_n] + off), "hidden": false, "arrive_hidden": false}
	return {"node": n, "point": NODES[n], "hidden": true}


func _outdoor_offset(node: String, index: int) -> Vector2:
	var offsets: Array = BUS_WAIT_OFFSETS if node == "N" else OUTDOOR_OFFSETS
	return offsets[index % offsets.size()]


## Keep a decorative standing offset on the pavement; fall back to the graph
## node itself (always inside a corridor) if the offset would leave the nav set.
func _safe_point(node: String, off: Vector2) -> Vector2:
	var p: Vector2 = NODES[node] + off
	if off == Vector2.ZERO or _nav.is_empty() or nav_contains(p):
		return p
	return NODES[node]


func _update_walkers(delta: float) -> void:
	var agents: Array = state.get("agents", [])
	var c: Dictionary = state.get("clock", {})
	var paused := bool(c.get("paused", true))
	if paused:
		delta = 0.0
	var speed := 0.0 if paused else WALK_SPEED * sqrt(clampf(float(c.get("speed", 1)), 1.0, 8.0))
	for i in agents.size():
		var a: Dictionary = agents[i]
		var id := String(a.get("id"))
		var g := _goal_for(a, i)
		if not _vis.has(id):
			var start: Vector2 = g["point"]
			var start_dir := Vector2.RIGHT
			var start_alpha := 0.0 if bool(g["hidden"]) else 1.0
			var sp := agent_pose(a, i, sim_time() if not state.is_empty() else 0.0)
			if not sp.is_empty():
				start = sp["pos"]
				start_dir = sp["dir"]
				start_alpha = 1.0
			if not _entry_clear(id, start):
				start_alpha = 0.0
			_vis[id] = {"pos": start, "path": [], "poly": [], "s": 0.0, "goal": "",
				"alpha": start_alpha, "dir": start_dir, "hidden": bool(g["hidden"]),
				"seated": bool(sp.get("seated", false)) if not sp.is_empty() else bool(g.get("seated", false)),
				"facing": String(sp.get("facing", g.get("facing", "s"))) if not sp.is_empty() else String(g.get("facing", "s")),
				"moving": false, "phase": 0.0, "vel": Vector2.ZERO}
		var v: Dictionary = _vis[id]
		var key := String(g["node"]) + str(g["point"])
		if key != String(v["goal"]):
			v["goal"] = key
			var pts: Array = _walker_path(v, String(g["node"]))
			var path: Array = []
			for p: Vector2 in pts:
				if path.is_empty() and (v["pos"] as Vector2).distance_to(p) < 6.0:
					continue
				path.append(p)
			if path.is_empty() or (path[-1] as Vector2).distance_to(g["point"]) > 1.0:
				path.append(g["point"])
			v["path"] = path
			v["poly"] = [v["pos"]] + path
			v["s"] = 0.0
			v["wait"] = 0.0
		v["hidden"] = bool(g["hidden"]) or (bool(g.get("arrive_hidden", false)))
		if bool(g["hidden"]) and float(v["alpha"]) <= 0.01:
			# Already out of sight (indoors / on the bus): relocate silently, no ghost walk.
			v["pos"] = g["point"]
			v["path"] = []
			v["poly"] = []
			v["s"] = 0.0
			v["moving"] = false
			v["vel"] = Vector2.ZERO
			continue
		var path_now: Array = v["path"]
		var budget := speed * delta
		var start_pos: Vector2 = v["pos"]
		if float(v["alpha"]) < 0.5 and not _entry_clear(id, start_pos):
			v["moving"] = false
			v["vel"] = Vector2.ZERO
			continue
		# Hidden agents leaving a building first fade in at the door, then walk.
		var fading_in: bool = float(v["alpha"]) < 1.0 and not path_now.is_empty()
		if fading_in:
			v["alpha"] = minf(1.0, float(v["alpha"]) + delta * FADE_RATE)
			budget *= float(v["alpha"])
		var travelled := _advance_path(v, _follow_budget(v, budget, id, Scale.ADULT_RADIUS))
		# Facing is the direction actually travelled this frame.  A turn at a junction
		# therefore lands on the same frame as the movement: no stale leg, no
		# moonwalk, and no walk animation while standing still.
		var moved: Vector2 = (v["pos"] as Vector2) - start_pos
		v["vel"] = moved / maxf(delta, 0.0001)
		v["moving"] = moved.length() > 0.02
		if bool(v["moving"]):
			v["dir"] = moved.normalized()
			v["phase"] = float(v["phase"]) + travelled / WALK_STRIDE
		if path_now.is_empty():
			v["wait"] = float(v.get("wait", 0.0)) + delta
			if bool(v["hidden"]) and float(v["wait"]) > DOOR_PAUSE:
				v["alpha"] = maxf(0.0, float(v["alpha"]) - delta * FADE_RATE)
			elif bool(v["hidden"]):
				pass
			else:
				v["alpha"] = minf(1.0, float(v["alpha"]) + delta * FADE_RATE)
		if bool(g.get("seated", false)) and (v["path"] as Array).is_empty():
			v["seated"] = true
			v["moving"] = false
			v["vel"] = Vector2.ZERO
			v["facing"] = String(g.get("facing", "s"))
			var f := String(v["facing"])
			if f == "e":
				v["dir"] = Vector2.RIGHT
			elif f == "w":
				v["dir"] = Vector2.LEFT
			elif f == "n":
				v["dir"] = Vector2.UP
			else:
				v["dir"] = Vector2.DOWN
		else:
			v["seated"] = false


func _update_background(delta: float) -> void:
	var people: Array = state.get("background", [])
	var paused := bool(state.get("clock", {}).get("paused", true))
	var speed := 0.0 if paused else 72.0 * sqrt(clampf(float(state.get("clock", {}).get("speed", 1)), 1.0, 8.0))
	for person: Dictionary in people:
		var id := String(person.get("id", ""))
		var loc := String(person.get("location", "transit"))
		var target_loc := String(person.get("target", loc))
		var target_node: String = LOC_NODE.get(target_loc, "C")
		if not _bg_vis.has(id):
			_bg_vis[id] = {"pos": NODES.get(LOC_NODE.get(loc, "C"), NODES["C"]), "path": [], "poly": [], "s": 0.0, "goal": "",
				"dir": Vector2.RIGHT, "moving": false, "phase": 0.0, "alpha": 0.0, "pending_entry": true}
		var v: Dictionary = _bg_vis[id]
		if not bool(person.get("visible", true)):
			v["alpha"] = 0.0
			v["pending_entry"] = true
			v["pos"] = NODES.get(LOC_NODE.get(loc, "C"), NODES["C"])
			v["path"] = []
			v["poly"] = []
			v["s"] = 0.0
			v["goal"] = ""
			v["moving"] = false
			continue
		var key := target_node + target_loc
		if key != String(v.get("goal", "")):
			v["goal"] = key
			var points: Array = _walker_path(v, target_node)
			# Drop the node we are already standing on, but keep the first segment
			# when the walker is mid-street: cutting straight to the next junction
			# would leave the sidewalk corridor.
			var trimmed: Array = []
			for p: Vector2 in points:
				if trimmed.is_empty() and (v["pos"] as Vector2).distance_to(p) < 6.0:
					continue
				trimmed.append(p)
			if person.has("life") and target_loc == "park":
				var number := maxi(0, int(id.trim_prefix("citizen-")) - 1)
				var leisure := number % 4 == 0
				var park_x: float = [550.0, 780.0, 1130.0][(number / 4) % 3] if leisure else 1080.0
				var a: Vector2 = NODES["K"] if park_x < 820.0 else NODES["L"]
				var b: Vector2 = NODES["L"] if park_x < 820.0 else NODES["M"]
				var park_point := a.lerp(b, (park_x - a.x) / (b.x - a.x))
				park_point.y += 12.0 if leisure else -10.0
				trimmed.append(park_point)
			v["path"] = trimmed
			v["poly"] = [v["pos"]] + trimmed
			v["s"] = 0.0
		if bool(v.get("pending_entry", false)):
			if not _entry_clear(id, v["pos"]):
				v["moving"] = false
				continue
			v["alpha"] = 1.0
			v["pending_entry"] = false
		var budget := speed * delta
		var start_pos: Vector2 = v["pos"]
		var travelled := _advance_path(v, _follow_budget(v, budget, id, Scale.ADULT_RADIUS))
		var moved: Vector2 = (v["pos"] as Vector2) - start_pos
		v["moving"] = moved.length() > 0.02
		if bool(v["moving"]):
			v["dir"] = moved.normalized()
			v["phase"] = float(v["phase"]) + travelled / BG_STRIDE


func _entry_clear(id: String, pos: Vector2) -> bool:
	for group: Dictionary in [_vis, _bg_vis]:
		for other_id: String in group:
			var other: Dictionary = group[other_id]
			if other_id != id and float(other.get("alpha", 1.0)) > 0.0 and pos.distance_to(other["pos"]) < Scale.ADULT_RADIUS * 2.0 + 4.0:
				return false
	return true


func _walker_path(v: Dictionary, goal: String) -> Array:
	var pos: Vector2 = v["pos"]
	# Finish the crossing before changing direction, keeping the current walking lane.
	if pos.y >= 662.0 and pos.y <= 960.0 and absf(pos.x - 215.0) <= 24.0:
		var south := pos.x < 215.0
		var exit_point := Vector2(193.0 if south else 237.0, 960.0 if south else 662.0)
		var onward := _path("S" if south else "SW", goal)
		if not south and onward[0] == NODES["SW"]:
			onward.pop_front()
		return [exit_point] + onward
	# A snapshot may change the destination mid-stride. Finish the current
	# sidewalk segment before turning so a follower can keep a stable order.
	var current: Array = v.get("path", [])
	if not current.is_empty() and pos.distance_to(current[0]) > 0.01:
		var retained: Array = []
		for point: Vector2 in current:
			for node: String in NODES:
				if point.distance_to(NODES[node]) < 0.01:
					return retained + _path(node, goal)
			retained.append(point) # Preserve bends before the next graph junction.
	return _path(_best_start(pos, goal), goal)


# shortcut: shared-direction spacing only; add sidewalk right-of-way before increasing density.
func _follow_budget(v: Dictionary, budget: float, id: String, radius: float) -> float:
	var path: Array = v["path"]
	if path.is_empty() or budget <= 0.0:
		return 0.0
	var pos: Vector2 = v["pos"]
	var tangent: Vector2 = (path[0] as Vector2) - pos
	if tangent.length() < 0.001:
		return minf(budget, 0.001)
	var allowed := minf(budget, tangent.length())
	tangent = tangent.normalized()
	for group: Dictionary in [_vis, _bg_vis]:
		for other_id: String in group:
			if other_id == id:
				continue
			var other: Dictionary = group[other_id]
			var route: Array = other["path"]
			if float(other.get("alpha", 1.0)) < 0.5 or route.is_empty():
				continue
			var heading: Vector2 = ((route[0] as Vector2) - (other["pos"] as Vector2)).normalized()
			var shared := (route[0] as Vector2).distance_to(path[0]) <= 1.0 and tangent.dot(heading) > 0.7
			# Look around a corner, but do not mistake an oncoming walker for a
			# leader beyond a U-turn. Reciprocal U-turns otherwise wait forever.
			if path.size() > 1 and tangent.dot(heading) > -0.7 and (route[0] as Vector2).distance_to(path[1]) <= 1.0:
				shared = shared or ((path[1] as Vector2) - (path[0] as Vector2)).normalized().dot(heading) > 0.7
			if not shared:
				continue
			if (route[0] as Vector2).distance_to(path[0]) <= 1.0:
				# A shared destination gives a consistent order even when paths converge at an angle.
				var remaining := (other["pos"] as Vector2).distance_to(path[0]) - pos.distance_to(path[0])
				if remaining > 0.001 or (absf(remaining) <= 0.001 and other_id > id):
					continue
			var offset: Vector2 = (other["pos"] as Vector2) - pos
			var ahead := offset.dot(tangent)
			if ahead < -0.001 or (absf(ahead) <= 0.001 and other_id > id):
				continue
			var gap := radius + Scale.ADULT_RADIUS + 4.0
			var lateral_sq := maxf(0.0, offset.length_squared() - ahead * ahead)
			if lateral_sq < gap * gap:
				allowed = minf(allowed, maxf(0.0, ahead - sqrt(gap * gap - lateral_sq)))
	return allowed


func _advance_path(v: Dictionary, budget: float) -> float:
	var poly: Array = v["poly"]
	if poly.is_empty():
		return 0.0
	var total := 0.0
	for i in range(1, poly.size()):
		total += (poly[i] as Vector2).distance_to(poly[i - 1])
	var before := float(v["s"])
	v["s"] = minf(before + maxf(0.0, budget), total)
	if budget > 0.0:
		var vertex := 0.0
		for i in range(1, poly.size()):
			vertex += (poly[i] as Vector2).distance_to(poly[i - 1])
			if absf(vertex - float(v["s"])) < 0.001:
				v["s"] = vertex
	v["pos"] = _poly_point(poly, float(v["s"]))
	var path: Array = v["path"]
	var at := 0.0
	for i in range(1, poly.size()):
		at += (poly[i] as Vector2).distance_to(poly[i - 1])
		# The count identifies the next unconsumed waypoint, even at duplicate points.
		if at <= float(v["s"]) and path.size() == poly.size() - i:
			path.pop_front()
	return float(v["s"]) - before


static func _poly_point(poly: Array, s: float) -> Vector2:
	if poly.is_empty():
		return Vector2.ZERO
	var left := maxf(0.0, s)
	for i in range(1, poly.size()):
		var a: Vector2 = poly[i - 1]
		var b: Vector2 = poly[i]
		var length := a.distance_to(b)
		if length == 0.0:
			continue
		if left < length:
			return a.lerp(b, left / length)
		left -= length
	return poly[-1]


func _draw_background(person: Dictionary, visual: Dictionary, s: float) -> void:
	var alpha := float(visual.get("alpha", 1.0))
	if alpha <= 0.0:
		return
	var pos: Vector2 = (visual["pos"] as Vector2) * s
	var dir: Vector2 = visual["dir"]
	var texture: Texture2D = TownArt.resident(String(person.get("id", "citizen-01")), dir.y < -0.2,
		bool(visual.get("moving", false)), float(visual.get("phase", 0.0)))
	var dimensions := texture.get_size() * (Scale.ADULT_HEIGHT * s / texture.get_height())
	_draw_ellipse(pos, Vector2(Scale.ADULT_RADIUS, 6.0) * s, Color(0, 0, 0, 0.24 * alpha))
	draw_set_transform(pos, 0.0, Vector2(-1 if dir.x < -0.2 else 1, 1))
	draw_texture_rect(texture, Rect2(Vector2(-dimensions.x / 2, -dimensions.y), dimensions), false, Color(1, 1, 1, alpha))
	draw_set_transform(Vector2.ZERO)


## Smoothed on-screen pose used for drawing; empty when the agent is fully indoors/away.
func visual_pose(id: String) -> Dictionary:
	if not _vis.has(id):
		return {}
	var v: Dictionary = _vis[id]
	if float(v["alpha"]) <= 0.01:
		return {}
	var out := {"pos": v["pos"], "dir": v["dir"], "moving": bool(v.get("moving", not (v["path"] as Array).is_empty())),
		"alpha": float(v["alpha"]), "phase": float(v.get("phase", 0.0))}
	if bool(v.get("seated", false)):
		out["seated"] = true
		out["facing"] = String(v.get("facing", "s"))
		out["moving"] = false
	return out


## Sprite facing for a heading in this top-down / three-quarter projection.
## The art faces the camera (+Y) for the front walk frames and away (-Y) for the
## `_back_*` frames; left/right is the same art mirrored, exactly as the town has
## always drawn it (dir.x < -0.2).
func facing_for(dir: Vector2) -> Dictionary:
	var d := Vector2.ZERO
	if dir.length() > 0.0001:
		d = dir.normalized()
	return {"dir": d, "back": d.y < 0.0, "flip": d.x < -0.2, "idle": d == Vector2.ZERO}


## The walk-cycle texture key for a heading, used by _draw_agent and by the
## headless facing test.  Idle uses {id}_0 (or the talking frame); a moving
## walker uses {id}_back_0/1 when heading away from the camera and {id}_1/2
## otherwise.  `phase` advances with distance travelled, so the two-frame cycle
## stays in step with the current speed.
func sprite_key(id: String, dir: Vector2, moving: bool, phase: float = 0.0, talking: bool = false) -> String:
	if not moving:
		if talking and _tex.has("%s_3" % id):
			return "%s_3" % id
		return "%s_0" % id
	var face := facing_for(dir)
	var frame := absi(int(floor(phase))) % 2
	if bool(face["back"]):
		var back := "%s_back_%d" % [id, frame]
		if _tex.has(back):
			return back
	return "%s_%d" % [id, 1 + frame]


## Everything _draw_agent needs for facing, exposed so the test can assert on the
## exact pose that is drawn (front/back frame + flip) rather than a copy.
func walker_sprite(id: String) -> Dictionary:
	if not _vis.has(id):
		return {}
	var v: Dictionary = _vis[id]
	var moving := bool(v.get("moving", false))
	var face := facing_for(v["dir"])
	return {
		"id": id,
		"pos": v["pos"],
		"dir": v["dir"],
		"vel": v.get("vel", Vector2.ZERO),
		"moving": moving,
		"back": bool(face["back"]),
		"flip": bool(face["flip"]),
		"key": sprite_key(id, v["dir"], moving, float(v.get("phase", 0.0))),
	}


## Explicit sidewalk / crossing polygons (see town_nav.gd).
func nav_polygons() -> Array:
	return _nav


func nav_contains(p: Vector2, eps: float = Nav.EPS) -> bool:
	return Nav.contains_any(_nav, p, eps)


## Nearest walkable point to p (map px), used for cosmetic offsets and for the
## background-pedestrian separation nudge.
func nav_clamp(p: Vector2) -> Vector2:
	return Nav.clamp_inside(_nav, p)


func nav_kind_at(p: Vector2, eps: float = Nav.EPS) -> String:
	for entry: Dictionary in _nav:
		if Nav.contains(entry["poly"], p, eps):
			return String(entry["kind"])
	return ""


func _scale() -> float:
	return size.x / MAP_W


func _to_screen(p: Vector2) -> Vector2:
	return p * _scale()


func _path(from_node: String, to_node: String) -> Array:
	if from_node == to_node:
		return [NODES[from_node]]
	var prev := {from_node: ""}
	var queue := [from_node]
	while not queue.is_empty():
		var n: String = queue.pop_front()
		if n == to_node:
			break
		for nb: String in _adj.get(n, []):
			if not prev.has(nb):
				prev[nb] = n
				queue.append(nb)
	if not prev.has(to_node):
		return [NODES[from_node], NODES[to_node]]
	var out := []
	var cur := to_node
	while cur != "":
		out.push_front(NODES[cur])
		cur = prev[cur]
	var lanes: Array = []
	for i in out.size():
		if out[i] == NODES["SW"] and i + 1 < out.size() and out[i + 1] == NODES["S"]:
			lanes.append(Vector2(193, 618))
			lanes.append(Vector2(193, 960))
			continue
		elif i > 0 and out[i - 1] == NODES["S"] and out[i] == NODES["SW"]:
			lanes.append(Vector2(237, 960))
			lanes.append(Vector2(237, 662))
			continue
		lanes.append(out[i])
	return lanes


func _along(points: Array, t: float) -> Array:
	# returns [position, direction]
	var total := 0.0
	for i in range(1, points.size()):
		total += (points[i] as Vector2).distance_to(points[i - 1])
	if total <= 0.01:
		return [points[0], Vector2.RIGHT]
	var want := clampf(t, 0.0, 1.0) * total
	for i in range(1, points.size()):
		var a: Vector2 = points[i - 1]
		var b: Vector2 = points[i]
		var seg := a.distance_to(b)
		if want <= seg or i == points.size() - 1:
			return [a.lerp(b, clampf(want / maxf(seg, 0.01), 0.0, 1.0)), (b - a).normalized()]
		want -= seg
	return [points[-1], Vector2.RIGHT]


## Where an agent is on the map right now, or null when indoors/away/on the bus.
func agent_pose(a: Dictionary, index: int, t: float) -> Dictionary:
	var travel: Variant = a.get("travel")
	if travel is Dictionary:
		var mode := String(travel.get("mode", "walk"))
		if mode == "bus":
			return {}
		var depart := float(travel.get("depart", 0))
		var arrive := float(travel.get("arrive", depart + 1))
		var p := (t - depart) / maxf(arrive - depart, 0.1)
		var from_n: String = LOC_NODE.get(String(travel.get("from", "")), "C")
		var to_n: String = LOC_NODE.get(String(travel.get("to", "")), "C")
		var res := _along(_path(from_n, to_n), p)
		return {"pos": res[0], "dir": res[1], "moving": p < 1.0}
	# Town seats (park bench, bus-stop bench, café terrace tables) use the
	# step-1 catalog anchors, regardless of whether location is "town" or
	# the outdoor place name. Pathing / sidewalk routing is unchanged.
	var sit: Dictionary = _town_seat_pose(a, index)
	if not sit.is_empty():
		return sit
	var loc := String(a.get("location", ""))
	if loc == "park" or loc == "bus_stop":
		return {"pos": _safe_point(LOC_NODE[loc], _outdoor_offset(LOC_NODE[loc], index)), "dir": Vector2.RIGHT, "moving": false, "phase": 0.0}
	if _on_street(a):
		return _street_pose(a, index, t)
	return {}


func _gui_input(event: InputEvent) -> void:
	if event is InputEventMouseMotion:
		var hp := _place_at(event.position)
		if hp != hover_place:
			hover_place = hp
			mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND if hp != "" or _agent_at(event.position) != "" else Control.CURSOR_ARROW
	elif event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
		if String(_bus["phase"]) != "idle" and bus_rect().has_point(event.position):
			place_clicked.emit("bus")
			accept_event()
			return
		var id := _agent_at(event.position)
		if id != "":
			agent_clicked.emit(id)
			accept_event()
			return
		var place := _place_at(event.position)
		if place != "":
			place_clicked.emit(place)
			accept_event()


func _agent_at(p: Vector2) -> String:
	for id: String in _drawn_agents:
		if (_drawn_agents[id] as Rect2).has_point(p):
			return id
	return ""


func _place_at(p: Vector2) -> String:
	var m := p / _scale()
	for k: String in HOTSPOTS:
		if (HOTSPOTS[k] as Rect2).has_point(m):
			return k
	return ""


func _darkness(md: float) -> float:
	if md < 420.0:
		return lerpf(0.42, 0.0, clampf((md - 360.0) / 60.0, 0.0, 1.0))
	if md < 1080.0:
		return 0.0
	if md < 1170.0:
		return lerpf(0.0, 0.5, (md - 1080.0) / 90.0)
	return 0.5


func _draw() -> void:
	var s := _scale()
	draw_texture_rect(_tex["town"], Rect2(Vector2.ZERO, Vector2(MAP_W, MAP_H) * s), false)
	if state.is_empty():
		return
	var t := sim_time()
	var c: Dictionary = state.get("clock", {})
	var md := fposmod(t, 1440.0)
	var atmo: Dictionary = Atmosphere.snapshot(c, t)
	_draw_foliage_sway(s, float(atmo.get("wind_kmh", 0.0)))
	var font := get_theme_default_font()
	_drawn_agents.clear()

	# Hover outline for interactive places
	if hover_place != "" and HOTSPOTS.has(hover_place):
		var r: Rect2 = HOTSPOTS[hover_place]
		draw_rect(Rect2(r.position * s, r.size * s), Color(1, 0.95, 0.7, 0.10), true)
		draw_rect(Rect2(r.position * s, r.size * s), Color(1, 0.95, 0.7, 0.75), false, 2.0)

	# The bus and the street traffic are part of the town, not a hover state.
	_draw_bus_stops(s)
	_draw_bus(t, s)
	_draw_traffic(s)
	# Every adult uses the same ground plane and depth sorting.
	var agents: Array = state.get("agents", [])
	var drawables: Array = TownAnimals.poses(t)
	for person: Dictionary in state.get("background", []):
		var bg_id := String(person.get("id", ""))
		if _bg_vis.has(bg_id) and float(_bg_vis[bg_id].get("alpha", 1.0)) > 0.0:
			drawables.append({"background": person, "pose": _bg_vis[bg_id]})
	var indoor_counts := {}
	for i in agents.size():
		var a: Dictionary = agents[i]
		var pose := visual_pose(String(a.get("id")))
		if pose.is_empty() and not _vis.has(String(a.get("id"))):
			pose = agent_pose(a, i, t)
		if pose.is_empty():
			var loc := String(a.get("location", ""))
			if loc in INDOOR and not (a.get("travel") is Dictionary):
				indoor_counts[loc] = indoor_counts.get(loc, []) + [String(a.get("id"))]
			continue
		drawables.append({"a": a, "pose": pose})
		var pres: Dictionary = a.get("presence", {}) if a.get("presence") is Dictionary else {}
		if pres.get("companion") == "mochi":
			# Mochi trails slightly behind and beside Ama along her heading, so the
			# pair never overlaps and the dog never leads into traffic.
			var hd: Vector2 = pose["dir"]
			if hd.length() < 0.1:
				hd = Vector2(1, 0)
			var dpos: Vector2
			if bool(pose["moving"]):
				dpos = (pose["pos"] as Vector2) - hd * 34.0
			else:
				dpos = (pose["pos"] as Vector2) + Vector2(-36.0 if hd.x >= 0.0 else 36.0, 0)
			# keep paws level with Ama's feet so Mochi never covers her name plate
			dpos.y = minf(dpos.y, (pose["pos"] as Vector2).y + 1.0)
			drawables.append({"dog": true, "owner": pose["pos"], "pose": {"pos": dpos, "dir": hd, "moving": pose["moving"],
				"alpha": pose.get("alpha", 1.0), "phase": pose.get("phase", 0.0)}})
	drawables.sort_custom(func(x: Dictionary, y: Dictionary) -> bool: return (x["pose"]["pos"] as Vector2).y < (y["pose"]["pos"] as Vector2).y)
	var cafe_front := 510.0
	for d: Dictionary in drawables:
		if (d["pose"]["pos"] as Vector2).y <= cafe_front:
			_draw_town_drawable(d, s)
	for name: String in ["town.cafe_table_a", "town.cafe_table_b"]:
		var fg: Variant = _tex.get("fg:" + name)
		if fg is Texture2D:
			draw_texture_rect(fg, Rect2(Vector2.ZERO, Vector2(MAP_W, MAP_H) * s), false)
	for d: Dictionary in drawables:
		if (d["pose"]["pos"] as Vector2).y > cafe_front:
			_draw_town_drawable(d, s)

	# Day/night sky, ambient, lamps, window light, and weather — driven by the clock.
	_draw_wind_flag(s, float(atmo.get("wind_kmh", 0.0)))
	_draw_atmosphere(s, atmo, c)

	# Place tags + indoor occupancy badges (drawn above tint so they stay readable)
	for k: String in TAGS:
		var label := _place_label(k)
		var pos := (TAGS[k] as Vector2) * s
		var w := font.get_string_size(label, HORIZONTAL_ALIGNMENT_LEFT, -1, 13).x + 16
		var rect := Rect2(pos - Vector2(w / 2, 0), Vector2(w, 22))
		draw_rect(rect, Color(0.08, 0.1, 0.14, 0.78), true)
		draw_string(font, rect.position + Vector2(8, 16), label, HORIZONTAL_ALIGNMENT_LEFT, -1, 13, Color(1, 0.96, 0.86))
		if indoor_counts.has(k):
			var ids: Array = indoor_counts[k]
			var bx := rect.position.x + rect.size.x + 4
			for j in ids.size():
				var col: Color = AGENT_COLORS.get(ids[j], Color.WHITE)
				var cpos := Vector2(bx + 11 + j * 20, rect.position.y + 11)
				draw_circle(cpos, 9, Color(0.08, 0.1, 0.14, 0.9))
				draw_circle(cpos, 7, col)
				draw_string(font, cpos + Vector2(-4, 5), String(ids[j]).substr(0, 1).to_upper(), HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color(0.08, 0.1, 0.14))

	# Speech bubbles last
	for d: Dictionary in drawables:
		if d.has("a"):
			_draw_bubble(d["a"], (d["pose"]["pos"] as Vector2) * s, t, font)



func _draw_foliage_sway(s: float, wind: float) -> void:
	if reduced_motion or wind <= 0.0 or not _tex.has("town"):
		return
	var tex: Texture2D = _tex["town"]
	var phase := _wind_phase()
	for r: Rect2 in FOLIAGE:
		var x := r.position.x
		while x < r.end.x:
			var col_w := 2.0
			var off: Vector2 = Atmosphere.wind_offset(x, r.position.y, wind, phase)
			var src := Rect2(x, r.position.y, col_w, r.size.y)
			var dst := Rect2((x + off.x) * s, (r.position.y + off.y) * s, col_w * s, r.size.y * s)
			draw_texture_rect_region(tex, dst, src)
			x += col_w


func _draw_wind_flag(s: float, wind: float) -> void:
	var base := Atmosphere.FLAG_BASE * s
	var phase := _wind_phase()
	var strength := clampf(wind / 14.0, 0.15, 2.4)
	var flutter := maxf(2.0, sin(phase * 6.2) * 7.0 * strength + 10.0 * clampf(wind / 20.0, 0.2, 1.4))
	var dip := sin(phase * 4.1 + 0.6) * 4.0 * strength
	# Pole
	draw_rect(Rect2(base + Vector2(-2, -72) * s, Vector2(4, 72) * s), Color("3a2a1a"), true)
	# Cloth: flies east, ripples with the wind. This is the region the atmosphere check samples.
	var p0 := base + Vector2(2, -70) * s
	var p1 := base + Vector2(2 + flutter, -58 + dip) * s
	var p2 := base + Vector2(2 + flutter * 0.85, -40 + dip * 0.4) * s
	var p3 := base + Vector2(2, -46) * s
	draw_colored_polygon(PackedVector2Array([p0, p1, p2, p3]), Color("c4453a"))
	draw_colored_polygon(PackedVector2Array([
		p0.lerp(p3, 0.25),
		p1.lerp(p2, 0.25),
		p1.lerp(p2, 0.65),
		p0.lerp(p3, 0.65),
	]), Color("f1e6cf"))


func _load_scene_lamps() -> void:
	_scene_lamps.clear()
	var raw := FileAccess.get_file_as_string("res://config/scene_lamps.json")
	if raw.is_empty():
		push_warning("scene_lamps.json is missing")
		return
	var parsed: Variant = JSON.parse_string(raw)
	if parsed is Dictionary:
		for row: Variant in (parsed as Dictionary).get("lamps", []):
			if row is Dictionary:
				var d: Dictionary = row
				_scene_lamps.append({
					"pos": Vector2(float(d.get("x", 0)), float(d.get("y", 0))),
					"kind": String(d.get("kind", "street")),
				})


func _draw_atmosphere(s: float, atmo: Dictionary, c: Dictionary) -> void:
	var dark := float(atmo.get("darkness", 0.0))
	var phase := String(atmo.get("phase", ""))
	if dark > 0.0:
		if phase == "dusk":
			# Warm golden grade over the whole town — 18:30 is ~30 min before a
			# November SG sunset, so the body of the map must read as dusk, not a
			# cool grey haze. Night keeps the cool overlay below.
			draw_rect(Rect2(Vector2.ZERO, size), Color(0.40, 0.18, 0.05, dark * 0.70), true)
			var wash_h := 400.0 * s
			var steps := 16
			for i in steps:
				var u := float(i) / float(steps)
				var col := Color(1.0, 0.52, 0.16, 0.22 * (1.0 - u) * (1.0 - u))
				draw_rect(Rect2(0.0, wash_h * u, MAP_W * s, maxf(1.0, wash_h / float(steps) + 1.0)), col, true)
		else:
			draw_rect(Rect2(Vector2.ZERO, size), Color(0.04, 0.06, 0.16, dark), true)
	# Sky wash in the documented SKY_BAND (top of the map). Stronger at the top edge.
	var sky: Color = atmo.get("sky_top", Color(0.05, 0.08, 0.18))
	var hz: Color = atmo.get("sky_horizon", sky)
	var sky_a := float(atmo.get("sky_alpha", 0.0))
	if sky_a > 0.02:
		var band := Atmosphere.SKY_BAND
		var steps_sky := 14
		for i in steps_sky:
			var u := float(i) / float(steps_sky)
			var col := sky.lerp(hz, u)
			col.a = sky_a * (1.0 - u) * (1.0 - u)
			var y0 := (band.position.y + band.size.y * u) * s
			var y1 := (band.position.y + band.size.y * (float(i + 1) / float(steps_sky))) * s
			draw_rect(Rect2(band.position.x * s, y0, band.size.x * s, maxf(1.0, y1 - y0)), col, true)
	var lamp := float(atmo.get("lamp", 0.0))
	if lamp > 0.08:
		for rec: Dictionary in _scene_lamps:
			var sp: Vector2 = rec["pos"] * s
			# Tight halos so a street lamp just south of the terrace cannot
			# appear as an orphan glow in the café-front crop.
			var base := 22.0 if String(rec["kind"]) == "street" else 16.0
			for k in 4:
				var a := (0.07 + k * 0.05) * lamp
				draw_circle(sp, (base - k * 4.0) * s, Color(1.0, 0.85, 0.45, a))
	else:
		# Daylight: town.png bakes yellow glass into every lamp head, so the
		# unlit fixture still reads as on. Overpaint the head with cool glass
		# and draw no halo.
		for rec: Dictionary in _scene_lamps:
			var sp: Vector2 = rec["pos"] * s
			var kind := String(rec["kind"])
			var cap := Color(0.20, 0.21, 0.24, 1.0)
			if kind == "sconce":
				# Vertical office fixture glass is ~7×20 map px.
				draw_rect(Rect2(sp + Vector2(-3.5, -10.0) * s, Vector2(7.0, 20.0) * s), cap, true)
			elif kind == "lantern":
				draw_circle(sp, 8.0 * s, cap)
				draw_rect(Rect2(sp + Vector2(-6.0, -7.0) * s, Vector2(12.0, 14.0) * s), cap, true)
			else:
				# Street-lamp lantern glass extends ~±9 px; a tight first pass
				# left a yellow rim that still read as a lit head.
				draw_rect(Rect2(sp + Vector2(-9.0, -8.0) * s, Vector2(18.0, 17.0) * s), cap, true)
				draw_circle(sp, 9.0 * s, cap)
	var win := float(atmo.get("window", 0.0))
	if win > 0.08 and _glass_tex is Texture2D:
		# Warm wash only on the actual glass pixels of the window art.
		var dest := Rect2(Vector2.ZERO, Vector2(MAP_W, MAP_H) * s)
		draw_texture_rect(_glass_tex, dest, false, Color(1.0, 0.82, 0.40, 0.28 * win))
		draw_texture_rect(_glass_tex, dest, false, Color(1.0, 0.92, 0.55, 0.16 * win))
	var raining := bool(atmo.get("raining", false)) or bool(c.get("raining", false))
	if raining:
		draw_rect(Rect2(Vector2.ZERO, size), Color(0.2, 0.25, 0.32, 0.18), true)
		var off := 0.0 if reduced_motion else _wind_phase()
		for rp: Vector2 in _rain_seed:
			var x := fposmod(rp.x * size.x + off * 60.0, size.x)
			var y := fposmod(rp.y * size.y + off * 520.0, size.y)
			draw_line(Vector2(x, y), Vector2(x - 3, y + 14), Color(0.75, 0.85, 1.0, 0.45), 1.0)


func _place_label(k: String) -> String:
	match k:
		"home":
			return "Home"
		"office":
			return "Office"
		"cafe":
			return "Less Café"
		"park":
			return "Fountain Park"
		"bus_stop":
			return "Bus stop"
	return k


func _draw_animal(animal: Dictionary, s: float) -> void:
	var frames := TownArt.frames(TownArt.ANIMALS, 4, 4)
	var row := int(animal["row"])
	var texture: Texture2D = frames[row * 4 + int(animal["frame"])]
	var reference: Texture2D = frames[row * 4]
	var unit: float = Scale.ANIMAL_HEIGHTS[row] * s / reference.get_height()
	var dimensions := texture.get_size() * unit
	var pose: Dictionary = animal["pose"]
	var pos: Vector2 = pose["pos"] * s
	if animal["activity"] != "flying":
		_draw_ellipse(pos, Vector2(dimensions.x * 0.35, 2.0 * s), Color(0, 0, 0, 0.15))
	draw_set_transform(pos, 0.0, Vector2(-1 if pose.get("flip", false) else 1, 1))
	draw_texture_rect(texture, Rect2(Vector2(-dimensions.x / 2, -dimensions.y), dimensions), false)
	draw_set_transform(Vector2.ZERO)


func _draw_town_drawable(d: Dictionary, s: float) -> void:
	if d.has("animal"):
		_draw_animal(d, s)
	elif d.has("background"):
		_draw_background(d["background"], d["pose"], s)
	elif d.has("dog"):
		_draw_dog(d["pose"], s)
		_draw_leash(d["owner"], d["pose"], s)
	else:
		_draw_agent(d["a"], d["pose"], s)


func _draw_agent(a: Dictionary, pose: Dictionary, s: float) -> void:
	var id := String(a.get("id"))
	var dir: Vector2 = pose["dir"]
	var moving: bool = bool(pose["moving"])
	var lane := 0.0
	if moving:
		lane = (float(["maya", "sam", "lin", "ama", "bo"].find(id)) - 2.0) * 3.0
	var pos: Vector2 = ((pose["pos"] as Vector2) + Vector2(-dir.y, dir.x) * lane) * s
	var face := facing_for(dir)
	var key := sprite_key(id, dir, moving, float(pose.get("phase", 0.0)), _bubble_active(a))
	if bool(pose.get("seated", false)):
		var facing := String(pose.get("facing", "s"))
		var side := "%s_seat_%s" % [id, facing]
		if (facing == "e" or facing == "w") and _tex.has(side):
			key = side
		elif _tex.has("%s_seat_0" % id):
			key = "%s_seat_0" % id
	var pres: Dictionary = a.get("presence", {}) if a.get("presence") is Dictionary else {}
	var hands := String(pres.get("hands", "free"))
	key = _hands_key(id, key, hands, String(pres.get("item", "") if pres.get("item") != null else ""))
	var clock: Dictionary = state.get("clock", {}) if state.get("clock") is Dictionary else {}
	var layers: Dictionary = Attire.layers_for(a, clock)
	key = Attire.apply_key(id, key, layers, func(k: String) -> bool: return _tex.has(k))
	var tex: Texture2D = _tex.get(key) as Texture2D
	if tex != null:
		_last_tex[id] = tex
	elif _last_tex.has(id):
		tex = _last_tex[id] as Texture2D
	else:
		tex = _tex.get("%s_0" % id) as Texture2D
	if tex == null:
		return
	var scale := Scale.TOWN_ACTOR * s
	var sz := Scale.ACTOR_CANVAS * scale
	var tint := Color(1, 1, 1, float(pose.get("alpha", 1.0)))
	draw_set_transform(Vector2.ZERO)
	# shadow + selection ring
	var ring_col: Color = AGENT_COLORS.get(id, Color.WHITE)
	if id == selected:
		draw_arc(pos + Vector2(0, -2), sz.x * 0.36, 0, TAU, 32, ring_col, 3.0)
	_draw_ellipse(pos, Vector2(sz.x * 0.26, sz.x * 0.09), Color(0, 0, 0, 0.32 * tint.a))
	var flip: bool = bool(face["flip"]) and not (key.ends_with("_e") or key.ends_with("_w"))
	if flip:
		draw_set_transform(pos, 0.0, Vector2(-1, 1))
		draw_texture_rect(tex, Rect2(Vector2(-sz.x / 2, -sz.y), sz), false, tint)
		draw_set_transform(Vector2.ZERO)
	else:
		draw_texture_rect(tex, Rect2(pos + Vector2(-sz.x / 2, -sz.y), sz), false, tint)
	var hand := pos + Vector2((-sz.x * 0.2) if flip else (sz.x * 0.18), -sz.y * 0.5)
	var item: Variant = pres.get("item")
	if hands == "carrying" and item is String and _tex.has(item) and not (id == "bo" and item == "coffee" and key == "bo_3"):
		draw_texture_rect(_tex[item], Rect2(hand - Vector2(0, 4) * s, Scale.prop_size(item) * s), false, tint)
	elif hands == "phone" and id != "sam":
		# small handset held at chest height while reading a remote message
		var ph := Rect2(hand + Vector2(-2, -6) * s, Vector2(6, 10) * s)
		draw_rect(ph, Color(0.1, 0.1, 0.14, tint.a), true)
		draw_rect(ph.grow(-1.2 * s), Color(0.55, 0.8, 1.0, 0.85 * tint.a), true)
	if bool(pres.get("umbrella", false)):
		_draw_umbrella(pos + Vector2(0, -sz.y * 0.98), sz.x * 0.42, ring_col, tint.a)
	# name plate
	var font := get_theme_default_font()
	var name := String(a.get("name", id))
	var w := font.get_string_size(name, HORIZONTAL_ALIGNMENT_LEFT, -1, 12).x + 10
	var plate := Rect2(pos + Vector2(-w / 2, 4), Vector2(w, 17))
	draw_rect(plate, Color(0.08, 0.1, 0.14, 0.82), true)
	draw_rect(Rect2(plate.position, Vector2(3, plate.size.y)), ring_col, true)
	draw_string(font, plate.position + Vector2(6, 13), name, HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color.WHITE)
	_drawn_agents[id] = Rect2(pos + Vector2(-sz.x / 2, -sz.y), Vector2(sz.x, sz.y + 22))


## Pick the frame whose baked prop matches the authoritative hand state.
## Ama's art holds a leash, Sam's a phone, Bo's talking frame a cup; anyone
## whose hands are otherwise engaged or free uses the hands-free variant.
func _hands_key(id: String, key: String, hands: String, item: String) -> String:
	var free_key := key.replace("_", "_free_")
	if not _tex.has(free_key):
		return key
	match id:
		"ama":
			return key if hands == "leash" else free_key
		"sam":
			return key if hands == "phone" else free_key
		"bo":
			return key if hands == "serving" or (hands == "carrying" and item == "coffee") else free_key
	return key
func _draw_leash(owner: Vector2, dog: Dictionary, s: float) -> void:
	var a := (owner + Vector2(0, -30)) * s
	var b := ((dog["pos"] as Vector2) + Vector2(0, -16)) * s
	var mid := (a + b) * 0.5 + Vector2(0, 7 * s)
	var pts := PackedVector2Array([a, a.lerp(mid, 0.5) + Vector2(0, 2 * s), mid, mid.lerp(b, 0.5) + Vector2(0, 2 * s), b])
	draw_polyline(pts, Color(0.75, 0.16, 0.16, float(dog.get("alpha", 1.0))), 1.6 * s)
func _draw_umbrella(top: Vector2, r: float, col: Color, alpha: float) -> void:
	var pts := PackedVector2Array()
	for k in 13:
		var ang := PI + PI * float(k) / 12.0
		pts.append(top + Vector2(cos(ang) * r, sin(ang) * r * 0.55))
	var canopy := Color(col.r * 0.85, col.g * 0.85, col.b * 0.85, 0.92 * alpha)
	draw_colored_polygon(pts, canopy)
	draw_polyline(pts, Color(0.08, 0.08, 0.12, alpha), 1.5)
	draw_line(top, top + Vector2(0, r * 0.9), Color(0.15, 0.15, 0.18, alpha), 1.5)
func _draw_dog(pose: Dictionary, s: float) -> void:
	var pos: Vector2 = (pose["pos"] as Vector2) * s
	var moving: bool = bool(pose["moving"])
	var phase := float(pose.get("phase", 0.0))
	var frame := (absi(int(floor(phase))) % 2) + 1 if moving else 0
	var tex: Texture2D = _tex["mochi_%d" % frame]
	var sz := Scale.DOG_CANVAS * Scale.TOWN_DOG * s
	var tint := Color(1, 1, 1, float(pose.get("alpha", 1.0)))
	_draw_ellipse(pos, Vector2(sz.x * 0.3, sz.y * 0.1), Color(0, 0, 0, 0.3 * tint.a))
	if (pose["dir"] as Vector2).x < -0.2:
		draw_set_transform(pos, 0.0, Vector2(-1, 1))
		draw_texture_rect(tex, Rect2(Vector2(-sz.x / 2, -sz.y), sz), false, tint)
		draw_set_transform(Vector2.ZERO)
	else:
		draw_texture_rect(tex, Rect2(pos + Vector2(-sz.x / 2, -sz.y), sz), false, tint)


func _draw_ellipse(c: Vector2, r: Vector2, col: Color) -> void:
	var pts := PackedVector2Array()
	for i in 20:
		var ang := TAU * i / 20.0
		pts.append(c + Vector2(cos(ang) * r.x, sin(ang) * r.y))
	draw_colored_polygon(pts, col)


func _bubble_active(a: Dictionary) -> bool:
	var b: Variant = a.get("bubble")
	return b is Dictionary and float(b.get("until", 0)) > sim_time()


func _draw_bubble(a: Dictionary, pos: Vector2, _t: float, font: Font) -> void:
	if not _bubble_active(a):
		return
	var b: Dictionary = a["bubble"]
	var text := String(b.get("text", ""))
	var kind := String(b.get("kind", "say"))
	var lines := wrap_text(font, text, 13, 200.0)
	if lines.size() > 4:
		lines = lines.slice(0, 4)
		lines[3] = String(lines[3]) + "…"
	var w := 0.0
	for l: String in lines:
		w = maxf(w, font.get_string_size(l, HORIZONTAL_ALIGNMENT_LEFT, -1, 13).x)
	var box := Vector2(w + 18, lines.size() * 17 + 12)
	var top := pos + Vector2(-box.x / 2, -box.y - 66)
	top.x = clampf(top.x, 4, size.x - box.x - 4)
	top.y = maxf(top.y, 4)
	var bg := Color(1, 1, 1, 0.96)
	var fg := Color(0.1, 0.12, 0.16)
	if kind == "msg":
		bg = Color("dbe9ff")
	elif kind == "think":
		bg = Color(0.92, 0.92, 0.95, 0.94)
		fg = Color(0.3, 0.32, 0.38)
	draw_rect(Rect2(top + Vector2(2, 3), box), Color(0, 0, 0, 0.25), true)
	draw_rect(Rect2(top, box), bg, true)
	draw_rect(Rect2(top, box), Color(0.1, 0.12, 0.16, 0.9), false, 1.5)
	var tip := PackedVector2Array([Vector2(pos.x - 6, top.y + box.y), Vector2(pos.x + 6, top.y + box.y), Vector2(pos.x, top.y + box.y + 9)])
	draw_colored_polygon(tip, bg)
	for i in lines.size():
		draw_string(font, top + Vector2(9, 18 + i * 17), lines[i], HORIZONTAL_ALIGNMENT_LEFT, -1, 13, fg)


func _bus_arrival() -> float:
	var nb: Variant = state.get("bus", {}).get("next")
	if nb is Dictionary:
		return float(nb.get("scheduled", 0)) + float(nb.get("delay", 0))
	return -1.0


func _riders_waiting() -> bool:
	for a: Dictionary in state.get("agents", []):
		var tr: Variant = a.get("travel")
		if tr is Dictionary and String(tr.get("mode")) == "bus" and String(tr.get("from")) == "bus_stop":
			var id := String(a.get("id"))
			if _vis.has(id) and float(_vis[id]["alpha"]) > 0.02:
				return true
	return false


## Fill in each stop's arc length once the route constants are available.
func _ensure_bus_stops() -> void:
	if _bus_stops_ready:
		return
	for st: Dictionary in _bus_stops:
		st["s"] = _route_s(st["pos"])
	_bus_stops_ready = true


func _route_length() -> float:
	var total := 0.0
	for i in range(1, BUS_ROUTE.size()):
		total += (BUS_ROUTE[i] as Vector2).distance_to(BUS_ROUTE[i - 1])
	return total


## Arc length of the closest point on the fixed route to p.
func _route_s(p: Vector2) -> float:
	var best := INF
	var best_s := 0.0
	var acc := 0.0
	for i in range(1, BUS_ROUTE.size()):
		var a: Vector2 = BUS_ROUTE[i - 1]
		var b: Vector2 = BUS_ROUTE[i]
		var seg := a.distance_to(b)
		var q := Geometry2D.get_closest_point_to_segment(p, a, b)
		var d := p.distance_to(q)
		if d < best:
			best = d
			best_s = acc + a.distance_to(q)
		acc += seg
	return best_s


## Point at arc length s along the fixed route.
func _route_point(s: float) -> Vector2:
	var left := maxf(s, 0.0)
	for i in range(1, BUS_ROUTE.size()):
		var a: Vector2 = BUS_ROUTE[i - 1]
		var b: Vector2 = BUS_ROUTE[i]
		var seg := a.distance_to(b)
		if left <= seg or i == BUS_ROUTE.size() - 1:
			return a.lerp(b, clampf(left / maxf(seg, 0.01), 0.0, 1.0))
		left -= seg
	return BUS_ROUTE[-1]


func _stop_s(i: int) -> float:
	_ensure_bus_stops()
	return float(_bus_stops[clampi(i, 0, _bus_stops.size() - 1)]["s"])


func bus_route_points() -> Array:
	return BUS_ROUTE.duplicate()


func bus_stops() -> Array:
	_ensure_bus_stops()
	return _bus_stops.duplicate(true)


func bus_position() -> Vector2:
	if _road.poses.has("bus"):
		return Vector2(float(_road.poses["bus"]["x"]), BUS_Y)
	return _route_point(float(_bus["s"]))


func bus_s() -> float:
	return float(_bus["s"])


func bus_phase() -> String:
	return String(_bus["phase"])


func bus_stop_index() -> int:
	return int(_bus["stop"])


## Glide along the route towards `to_s` over `dur` seconds.  Smoothstep leaves and
## enters each stop at zero speed and can never overshoot the target.
func _bus_glide(from_s: float, to_s: float, dur: float) -> bool:
	var k := clampf(float(_bus["t"]) / maxf(dur, 0.05), 0.0, 1.0)
	var eased := k * k * (3.0 - 2.0 * k)
	_bus["s"] = lerpf(from_s, to_s, eased)
	if k >= 1.0:
		_bus["s"] = to_s
		return true
	return false


## The bus runs a fixed route with two stops.  The service record
## (state.bus.next / active) owns arrival and dwell at the Lessville stop; the
## departure and the second stop are finished locally so a closed active window
## can never strand the bus mid-route.
func _update_bus(delta: float, sim_t: float = -1.0) -> void:
	var t := sim_t if sim_t >= 0.0 else sim_time()
	var c: Dictionary = state.get("clock", {})
	var spd := clampf(float(c.get("speed", 1)), 1.0, 8.0)
	var lead := minf(10.0 * spd, 15.0)
	var arr := _bus_arrival()
	_bus["t"] = float(_bus["t"]) + delta
	_ensure_bus_stops()

	var active: Variant = state.get("bus", {}).get("active")
	var auth := ""
	var fresh_trip := false
	if active is Dictionary:
		var trip_id := String(active.get("id", ""))
		if trip_id != String(_bus.get("trip_id", "")):
			_bus["trip_id"] = trip_id
			_bus["t"] = 0.0
			# A new trip always starts from the route origin, even if the previous
			# bus was still running its departure.
			_bus["phase"] = "idle"
			_bus["s"] = 0.0
			_bus["stop"] = -1
			fresh_trip = true
		auth = String(active.get("phase", "dwell"))

	# The service says the bus is at the Lessville stop: hold there for as long as
	# the authoritative record says so.
	if auth == "dwell":
		_bus["phase"] = "dwell"
		_bus["stop"] = 0
		_bus["s"] = _stop_s(0)
		return

	# A poll gap can hide the whole dwell window.  If the first sight of a trip is
	# already "depart", put the bus at the stop and let it leave rather than
	# replaying an approach from off-map.
	if auth == "depart" and fresh_trip and String(_bus["phase"]) == "idle":
		_bus["s"] = _stop_s(0)
		_bus["phase"] = "depart"
		_bus["stop"] = 1
		_bus["t"] = 0.0

	# The service says it has left.  Release the stop, but never before the 3 s
	# minimum dwell (the record's own dwell window is shorter than that at x1).
	if auth == "depart" and String(_bus["phase"]) == "dwell" and int(_bus["stop"]) == 0 \
			and float(_bus["t"]) >= float(_bus_stops[0]["dwell_min"]):
		_bus["phase"] = "depart"
		_bus["stop"] = 1
		_bus["t"] = 0.0

	match String(_bus["phase"]):
		"idle":
			_bus["s"] = 0.0
			if arr > 0.0 and t >= arr - lead and t < arr:
				_bus["phase"] = "approach"
				_bus["arr"] = arr
				_bus["t"] = 0.0
				_bus["stop"] = 0
		"approach":
			var k := clampf(1.0 - (float(_bus["arr"]) - t) / lead, 0.0, 1.0)
			_bus["s"] = lerpf(0.0, _stop_s(0), 1.0 - pow(1.0 - k, 2.2))
			if t >= float(_bus["arr"]) or k >= 1.0:
				_bus["phase"] = "dwell"
				_bus["s"] = _stop_s(0)
				_bus["stop"] = 0
				_bus["t"] = 0.0
		"dwell":
			var i := clampi(int(_bus["stop"]), 0, _bus_stops.size() - 1)
			var st: Dictionary = _bus_stops[i]
			var riders := i == 0 and _riders_waiting()
			if float(_bus["t"]) >= float(st["dwell_min"]) and (not riders or float(_bus["t"]) >= float(st["dwell_max"])):
				_bus["t"] = 0.0
				if i + 1 < _bus_stops.size():
					_bus["phase"] = "depart"
					_bus["stop"] = i + 1
				else:
					_bus["phase"] = "leave"
					_bus["stop"] = -1
		"depart":
			var i2 := clampi(int(_bus["stop"]), 1, _bus_stops.size() - 1)
			var from_s := _stop_s(i2 - 1)
			var to_s := _stop_s(i2)
			var dur := maxf(0.6, absf(to_s - from_s) / BUS_CRUISE)
			if _bus_glide(from_s, to_s, dur):
				_bus["phase"] = "dwell"
				_bus["t"] = 0.0
		"leave":
			var from2 := _stop_s(_bus_stops.size() - 1)
			var to2 := _route_length()
			var dur2 := maxf(0.6, (to2 - from2) / BUS_CRUISE)
			if _bus_glide(from2, to2, dur2):
				_bus["phase"] = "idle"
				_bus["s"] = 0.0
				_bus["stop"] = -1
				_bus["t"] = 0.0


func bus_rect() -> Rect2:
	var s := _scale()
	var sz := Scale.BUS_SIZE * s
	var pos := bus_position() * s
	return Rect2(pos + Vector2(-sz.x / 2, -sz.y), sz)


## Small procedural flag for the stop whose shelter/sign is not baked into the
## town art (the Lessville shelter sign is part of town.png).
func _draw_bus_stops(s: float) -> void:
	var base := Vector2(BUS_EAST_STOP_X, 734) * s
	draw_rect(Rect2(base + Vector2(-2, -53) * s, Vector2(4, 53) * s), Color("2b3242"), true)
	var sign := Rect2(base + Vector2(-22, -104) * s, Vector2(44, 51) * s)
	draw_rect(sign.grow(2.0 * s), Color("1b2a33"), true)
	draw_rect(sign, Color("2b8fa3"), true)
	# Cream bus glyph, matching the baked shelter sign.
	var bus := Rect2(sign.position + Vector2(7, 9) * s, Vector2(30, 33) * s)
	draw_rect(bus, Color("f1e6cf"), true)
	draw_rect(Rect2(bus.position + Vector2(4, 4) * s, Vector2(9, 9) * s), Color("2b8fa3"), true)
	draw_rect(Rect2(bus.position + Vector2(17, 4) * s, Vector2(9, 9) * s), Color("2b8fa3"), true)
	draw_rect(Rect2(bus.position + Vector2(4, 24) * s, Vector2(6, 6) * s), Color("2b8fa3"), true)
	draw_rect(Rect2(bus.position + Vector2(20, 24) * s, Vector2(6, 6) * s), Color("2b8fa3"), true)


func _draw_bus(_t: float, s: float) -> void:
	var nb: Variant = state.get("bus", {}).get("next")
	if nb is Dictionary and int(nb.get("delay", 0)) > 0:
		var font := get_theme_default_font()
		var txt := "Bus %02d:%02d  +%d min" % [(int(nb.get("scheduled", 0)) % 1440) / 60, int(nb.get("scheduled", 0)) % 60, int(nb.get("delay", 0))]
		var p := Vector2(392, 640) * s
		var w := font.get_string_size(txt, HORIZONTAL_ALIGNMENT_LEFT, -1, 12).x + 12
		draw_rect(Rect2(p, Vector2(w, 18)), Color("1a0f08"), true)
		draw_string(font, p + Vector2(6, 13), txt, HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color("ffb347"))
	if String(_bus["phase"]) == "idle" and not _road.poses.has("bus"):
		return
	var tex: Texture2D = _tex["bus"]
	var r := bus_rect()
	_draw_ellipse(Vector2(r.get_center().x, r.end.y + 2), Vector2(r.size.x * 0.46, r.size.y * 0.1), Color(0, 0, 0, 0.3))
	draw_texture_rect(tex, r, false)


func _traffic_points() -> Array:
	return [Vector2(-220, BUS_Y), Vector2(MAP_W + 220, BUS_Y)]


func _update_road(delta: float) -> void:
	var bus := {}
	if String(_bus["phase"]) != "idle":
		bus = {"x":_route_point(float(_bus["s"])).x, "trip":_bus.get("trip_id", "")}
	_road.update(state.get("traffic", []), bus, delta, bool(state.get("clock", {}).get("paused", true)))


func _draw_traffic(s: float) -> void:
	_draw_traffic_lights(s)
	var points := _traffic_points()
	for vehicle: Dictionary in state.get("traffic", []):
		var p := _along(points, float(vehicle.get("position", 0.0)) / 100.0)
		var pos: Vector2 = p[0]
		var direction := String(vehicle.get("direction", "east"))
		var id := String(vehicle.get("id", ""))
		if _road.poses.has(id):
			pos.x = float(_road.poses[id]["x"])
			direction = String(_road.poses[id]["direction"])
		pos.y += 78.0 if direction == "west" else 0.0
		var kind := String(vehicle.get("kind", "car"))
		var texture: Texture2D = TownArt.vehicle(kind)
		var dimensions: Vector2
		if kind == "bicycle":
			dimensions = texture.get_size() * (Scale.BICYCLE_HEIGHT / texture.get_height())
		else:
			var length: float = Scale.VAN_LENGTH if kind == "delivery_van" else Scale.CAR_LENGTH
			dimensions = texture.get_size() * (length / texture.get_width())
		var center := pos * s
		_draw_ellipse(center, Vector2(dimensions.x * 0.43, 7) * s, Color(0, 0, 0, 0.24))
		draw_set_transform(center, 0.0, Vector2(-1 if direction == "west" else 1, 1))
		draw_texture_rect(texture, Rect2(Vector2(-dimensions.x / 2, -dimensions.y) * s, dimensions * s), false)
		draw_set_transform(Vector2.ZERO)



func _draw_traffic_lights(s: float) -> void:
	var u := s * Scale.SIGNAL_UNIT
	var lights: Array = state.get("traffic_lights", [])
	var positions := [Vector2(155, 730), Vector2(1280, 730)]
	var font := get_theme_default_font()
	for index in range(mini(lights.size(), positions.size())):
		var light: Dictionary = lights[index]
		var phase := String(light.get("phase", "red"))
		var col := Color("69d18b") if phase == "green" else Color("f1c75b") if phase == "amber" else Color("e56b6f")
		var center: Vector2 = positions[index] * s
		draw_rect(Rect2(center + Vector2(-5, -34) * u, Vector2(10, 30) * u), Color("18212b"), true)
		draw_circle(center + Vector2(0, -25) * u, 4 * u, Color("e56b6f") if phase != "red" else col)
		draw_circle(center + Vector2(0, -15) * u, 4 * u, Color("f1c75b") if phase != "amber" else col)
		draw_circle(center + Vector2(0, -5) * u, 4 * u, Color("69d18b") if phase != "green" else col)
		draw_string(font, center + Vector2(9, -18) * s, phase, HORIZONTAL_ALIGNMENT_LEFT, -1, 10, col)


static func wrap_text(font: Font, text: String, fsize: int, width: float) -> Array:
	var out := []
	var line := ""
	for word: String in text.split(" "):
		var cand := word if line == "" else line + " " + word
		if font.get_string_size(cand, HORIZONTAL_ALIGNMENT_LEFT, -1, fsize).x > width and line != "":
			out.append(line)
			line = word
		else:
			line = cand
	if line != "":
		out.append(line)
	return out


func debug_outfit(a: Dictionary, pose: Dictionary = {}) -> Dictionary:
	var id := String(a.get("id"))
	var clock: Dictionary = state.get("clock", {}) if state.get("clock") is Dictionary else {}
	var layers: Dictionary = Attire.layers_for(a, clock)
	var key := "%s_0" % id
	if not pose.is_empty():
		key = sprite_key(id, pose.get("dir", Vector2.DOWN), bool(pose.get("moving", false)), float(pose.get("phase", 0.0)), _bubble_active(a))
		if bool(pose.get("seated", false)):
			var facing := String(pose.get("facing", "s"))
			var side := "%s_seat_%s" % [id, facing]
			if (facing == "e" or facing == "w") and _tex.has(side):
				key = side
			elif _tex.has("%s_seat_0" % id):
				key = "%s_seat_0" % id
	var pres: Dictionary = a.get("presence", {}) if a.get("presence") is Dictionary else {}
	var hands := String(pres.get("hands", "free"))
	key = _hands_key(id, key, hands, String(pres.get("item", "") if pres.get("item") != null else ""))
	var applied := Attire.apply_key(id, key, layers, func(k: String) -> bool: return _tex.has(k))
	return {"frame": applied, "layers": layers, "base_frame": key}
