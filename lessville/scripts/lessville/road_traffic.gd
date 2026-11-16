extends RefCounted
## Presentation-space traffic follows backend targets; one queue owns bus and vehicle clearance.
const Scale = preload("res://scripts/lessville/world_scale.gd")
const WEST := -320.0
const EAST := 1860.0
const GAP := 28.0
var poses := {}

static func length_for(kind: String) -> float:
	match kind:
		"bus": return Scale.BUS_SIZE.x
		"delivery_van": return Scale.VAN_LENGTH
		"bicycle": return Scale.BICYCLE_HEIGHT
		_: return Scale.CAR_LENGTH

func update(traffic: Array, bus: Dictionary, delta: float, paused: bool) -> void:
	if paused and not poses.is_empty(): return
	var wanted := {}
	for actor: Dictionary in traffic:
		var id := String(actor["id"])
		wanted[id] = true
		var direction := String(actor.get("direction", "east"))
		var target := lerpf(-220.0, 1756.0, float(actor.get("position", 0)) / 100.0)
		if not poses.has(id):
			poses[id] = {"id":id, "kind":actor.get("kind", "car"), "direction":direction, "x":target, "target":target}
		var pose: Dictionary = poses[id]
		if direction != pose["direction"]:
			# Finish leaving the viewport before changing lanes at the off-map junction.
			var outside: bool = float(pose["x"]) > 1536.0 + length_for(pose["kind"]) / 2 if pose["direction"] == "east" else float(pose["x"]) < -length_for(pose["kind"]) / 2
			if outside:
				pose["direction"] = direction
			else:
				target = EAST if pose["direction"] == "east" else WEST
		pose["target"] = target
	if not bus.is_empty():
		if not poses.has("bus"):
			poses["bus"] = {"id":"bus", "kind":"bus", "direction":"east", "x":WEST, "target":WEST, "trip":bus.get("trip", "")}
		var bp: Dictionary = poses["bus"]
		if bp["trip"] != bus.get("trip", ""):
			if float(bp["x"]) > 1536.0 + Scale.BUS_SIZE.x / 2:
				bp["x"] = WEST
				bp["trip"] = bus.get("trip", "")
			else:
				bp["target"] = EAST
		if bp["trip"] == bus.get("trip", ""):
			bp["target"] = bus["x"]
		wanted["bus"] = true
	elif poses.has("bus") and float(poses["bus"]["x"]) < EAST:
		poses["bus"]["target"] = EAST
		wanted["bus"] = true
	for id: String in poses.keys():
		if not wanted.has(id): poses.erase(id)
	for direction: String in ["east", "west"]:
		var lane: Array = poses.values().filter(func(p: Dictionary) -> bool: return p["direction"] == direction)
		var sign := 1.0 if direction == "east" else -1.0
		lane.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
			if is_equal_approx(float(a["x"]), float(b["x"])): return String(a["id"]) < String(b["id"])
			return float(a["x"]) * sign > float(b["x"]) * sign)
		var leader: Dictionary = {}
		for pose: Dictionary in lane:
			var current := float(pose["x"]) * sign
			var desired := maxf(current, float(pose["target"]) * sign)
			var next := move_toward(current, desired, (320.0 if pose["kind"] == "bus" else 220.0) * (0.0 if paused else delta))
			if not leader.is_empty():
				var clearance := (length_for(pose["kind"]) + length_for(leader["kind"])) / 2.0 + GAP
				next = minf(next, float(leader["x"]) * sign - clearance)
			pose["x"] = next * sign
			leader = pose
