extends SceneTree
## Capture Town frames of mid-stride walkers and the bus for the step-2-a2 motion gallery.
const TownView = preload("res://scripts/lessville/town_view.gd")
const Atmosphere = preload("res://scripts/lessville/world_atmosphere.gd")

const TIMES := {
	"1230": {"label": "12:30", "minute": 750},
	"1830": {"label": "18:30", "minute": 1110},
}


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var out_dir := OS.get_environment("LESSVILLE_MOTION_OUT")
	if out_dir.is_empty():
		push_error("LESSVILLE_MOTION_OUT must name an output directory")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(out_dir)
	var day := 2
	var meta := {"times": {}}
	for key: String in ["1230", "1830"]:
		var spec: Dictionary = TIMES[key]
		var minute := int(spec["minute"])
		var label := String(spec["label"])
		var slot := {"label": label, "minute": minute, "frames": {}}
		slot["frames"]["sidewalks"] = await _capture_sidewalks(out_dir, key, day, minute, label)
		slot["frames"]["crossings"] = await _capture_crossings(out_dir, key, day, minute, label)
		slot["frames"]["bus-route"] = await _capture_bus(out_dir, key, day, minute, label, "route")
		slot["frames"]["bus-dwell"] = await _capture_bus(out_dir, key, day, minute, label, "dwell")
		meta["times"][key] = slot
	var f := FileAccess.open("%s/motion-meta.json" % out_dir, FileAccess.WRITE)
	f.store_string(JSON.stringify(meta, "  "))
	f.close()
	print("[motion-render] wrote %s" % out_dir)
	quit(0)


func _clock(day: int, minute: int, label: String, paused: bool = false) -> Dictionary:
	return {
		"minute": (day - 1) * 1440 + minute,
		"day": day,
		"minute_of_day": minute,
		"label": label,
		"paused": paused,
		"speed": 1,
	}


func _capture_sidewalks(out_dir: String, key: String, day: int, minute: int, label: String) -> Dictionary:
	# Mid-block travel so agent_pose starts them on the pavement, then we tick
	# so they are mid-stride and facing the way they are walking.
	var t0 := float((day - 1) * 1440 + minute)
	var agents := [
		_walker("lin", "home", "cafe", t0 - 5.0, t0 + 5.0),
		_walker("sam", "office", "grocery", t0 - 4.0, t0 + 6.0),
		_walker("maya", "park", "cafe", t0 - 3.5, t0 + 5.5),
		_walker("ama", "bus_stop", "office", t0 - 4.5, t0 + 5.5),
		_walker("bo", "home", "park", t0 - 3.0, t0 + 7.0),
	]
	var st := {"clock": _clock(day, minute, label, false), "agents": agents, "background": [], "bus": {}}
	var tv: Control = await _make_town(st)
	var dt := 1.0 / 60.0
	tv._update_walkers(dt)
	# Cluster three walkers mid-block on the east-west sidewalk at y=485 (B-C)
	# so a single 4x crop shows 3+ mid-stride, clearly facing their travel.
	_place_sidewalk_cluster(tv, ["lin", "bo", "ama"], 300.0, 80.0, 485.0, Vector2(900, 485))
	# A second trio on the office/café frontage sidewalk (F-G).
	_place_sidewalk_cluster(tv, ["sam", "maya"], 900.0, 90.0, 416.0, Vector2(1300, 410))
	var ready := {}
	for _i in 180:
		tv._update_walkers(dt)
		tv._state_age += dt
		ready = _collect_walkers(tv, "sidewalk")
		if int(ready.get("n_mid", 0)) >= 3:
			break
	_freeze(tv)
	tv.queue_redraw()
	for _j in 3:
		await process_frame
	var path := "%s/walkers-sidewalks-%s.png" % [out_dir, key]
	_save(tv, path, st)
	var info := _collect_walkers(tv, "sidewalk")
	info["path"] = path
	tv.get_parent().queue_free()
	await process_frame
	print("[motion-render] sidewalks %s n_mid=%s" % [key, info.get("n_mid")])
	return info


func _capture_crossings(out_dir: String, key: String, day: int, minute: int, label: String) -> Dictionary:
	var t0 := float((day - 1) * 1440 + minute)
	# trail (S) -> bus_stop (N) crosses the west zebra; three slightly offset p-values.
	var agents := [
		_walker("lin", "trail", "bus_stop", t0 - 4.0, t0 + 6.0),  # ~p 0.40
		_walker("sam", "trail", "bus_stop", t0 - 3.2, t0 + 6.8),
		_walker("maya", "bus_stop", "trail", t0 - 3.6, t0 + 6.4),
		_walker("ama", "cafe", "trail", t0 - 8.0, t0 + 2.0),
		_walker("bo", "grocery", "trail", t0 - 7.0, t0 + 3.0),
	]
	var st := {"clock": _clock(day, minute, label, false), "agents": agents, "background": [], "bus": {}}
	var tv: Control = await _make_town(st)
	var dt := 1.0 / 60.0
	# First tick creates vis at mid-path; then steer three of them across the east zebra
	# (the street graph does not use that crossing, but the nav polygon is walkable
	# and the facing/stride logic is the same).
	tv._update_walkers(dt)
	_place_west_crossing(tv, ["lin", "sam"])
	_place_east_crossing(tv, ["ama", "bo", "maya"])
	var ready := {}
	for _i in 240:
		tv._update_walkers(dt)
		tv._state_age += dt
		ready = _collect_walkers(tv, "crossing")
		if int(ready.get("n_mid", 0)) >= 3:
			break
	_freeze(tv)
	tv.queue_redraw()
	for _j in 3:
		await process_frame
	var path := "%s/walkers-crossings-%s.png" % [out_dir, key]
	_save(tv, path, st)
	var info := _collect_walkers(tv, "crossing")
	info["path"] = path
	tv.get_parent().queue_free()
	await process_frame
	print("[motion-render] crossings %s n_mid=%s" % [key, info.get("n_mid")])
	return info




func _place_sidewalk_cluster(tv: Control, ids: Array, x0: float, dx: float, y: float, goal: Vector2) -> void:
	for i in ids.size():
		var id := String(ids[i])
		if not tv._vis.has(id):
			continue
		var v: Dictionary = tv._vis[id]
		var x := x0 + dx * float(i)
		v["pos"] = Vector2(x, y)
		v["path"] = [goal]
		v["poly"] = [v["pos"]] + v["path"]
		v["s"] = 0.0
		v["alpha"] = 1.0
		v["hidden"] = false
		v["moving"] = true
		v["dir"] = (goal - v["pos"]).normalized()
		v["phase"] = 0.28 + 0.14 * float(i)
		var g: Dictionary = tv._goal_for(_agent_by_id(tv, id), i)
		v["goal"] = String(g["node"]) + str(g["point"])


func _force_path_to_goal(tv: Control) -> void:
	# The first _update_walkers seeds vis at mid-path and already stores the
	# destination as the goal, so it never builds a path. Force a rebuild.
	var i := 0
	for a in tv.state.get("agents", []):
		var id := String(a.get("id"))
		if not tv._vis.has(id):
			i += 1
			continue
		var v: Dictionary = tv._vis[id]
		v["goal"] = "force-reroute"
		i += 1


func _freeze(tv: Control) -> void:
	# Stop _process from walking them out of mid-stride during capture frames.
	var c: Dictionary = tv.state.get("clock", {})
	c["paused"] = true
	tv.state["clock"] = c
	tv.set_process(false)


func _place_west_crossing(tv: Control, ids: Array) -> void:
	# North to south on the west zebra, staggered in x.
	var xs := [195.0, 235.0]
	for i in ids.size():
		var id := String(ids[i])
		if not tv._vis.has(id):
			continue
		var v: Dictionary = tv._vis[id]
		var x: float = xs[i % xs.size()]
		v["pos"] = Vector2(x, 720.0 + float(i) * 10.0)
		v["path"] = [Vector2(x, 910.0)]
		v["poly"] = [v["pos"]] + v["path"]
		v["s"] = 0.0
		v["alpha"] = 1.0
		v["hidden"] = false
		v["moving"] = true
		v["dir"] = Vector2.DOWN
		v["phase"] = 0.30 + 0.12 * float(i)
		var g: Dictionary = tv._goal_for(_agent_by_id(tv, id), i)
		v["goal"] = String(g["node"]) + str(g["point"])


func _place_east_crossing(tv: Control, ids: Array) -> void:
	# North curb of the east zebra down to the south curb, staggered in x.
	var xs := [1260.0, 1290.0, 1320.0]
	for i in ids.size():
		var id := String(ids[i])
		if not tv._vis.has(id):
			continue
		var v: Dictionary = tv._vis[id]
		var x: float = xs[i % xs.size()]
		# Start just north of the painted crossing and walk south through it.
		v["pos"] = Vector2(x, 700.0 + float(i) * 8.0)
		v["path"] = [Vector2(x, 910.0)]
		v["poly"] = [v["pos"]] + v["path"]
		v["s"] = 0.0
		v["alpha"] = 1.0
		v["hidden"] = false
		v["moving"] = true
		v["dir"] = Vector2.DOWN
		v["phase"] = 0.35 + 0.1 * float(i)
		# Freeze the goal key so _update_walkers does not rebuild a graph path.
		var g: Dictionary = tv._goal_for(_agent_by_id(tv, id), i)
		v["goal"] = String(g["node"]) + str(g["point"])


func _agent_by_id(tv: Control, id: String) -> Dictionary:
	for a in tv.state.get("agents", []):
		if String(a.get("id")) == id:
			return a
	return {}


func _capture_bus(out_dir: String, key: String, day: int, minute: int, label: String, kind: String) -> Dictionary:
	var st := {"clock": _clock(day, minute, label, false), "agents": [], "background": [],
		"bus": {"next": {"scheduled": minute + 20, "delay": 0}, "active": null}}
	if kind == "dwell":
		st["bus"]["active"] = {"id": "gallery-dwell-%s" % key, "phase": "dwell", "scheduled": minute, "at": minute}
	var tv: Control = await _make_town(st)
	tv._ensure_bus_stops()
	if kind == "route":
		# Mid-glide from the Lessville stop to the east (plaza) stop.
		var from_s: float = tv._stop_s(0)
		var to_s: float = tv._stop_s(1)
		tv._bus["phase"] = "depart"
		tv._bus["stop"] = 1
		tv._bus["t"] = 0.9  # part-way through the glide
		tv._bus["s"] = lerpf(from_s, to_s, 0.45)
		tv._update_bus(0.0, float((day - 1) * 1440 + minute))
	else:
		tv._update_bus(0.016, float((day - 1) * 1440 + minute))
	tv.queue_redraw()
	for _j in 3:
		await process_frame
	var path := "%s/bus-%s-%s.png" % [out_dir, kind, key]
	_save(tv, path, st)
	var info := {
		"path": path,
		"phase": tv.bus_phase(),
		"s": tv.bus_s(),
		"stop": tv.bus_stop_index(),
		"pos": [tv.bus_position().x, tv.bus_position().y],
	}
	tv.get_parent().queue_free()
	await process_frame
	print("[motion-render] bus %s %s phase=%s pos=%s" % [kind, key, info["phase"], info["pos"]])
	return info


func _walker(id: String, frm: String, to: String, depart: float, arrive: float) -> Dictionary:
	return {
		"id": id,
		"name": id.capitalize(),
		"location": "transit",
		"station": "street",
		"activity": "Walking",
		"travel": {"from": frm, "to": to, "depart": depart, "arrive": arrive, "mode": "walk"},
	}


func _collect_walkers(tv: Control, want_kind: String) -> Dictionary:
	var people: Array = []
	var n_mid := 0
	for id in tv._vis:
		var v: Dictionary = tv._vis[id]
		if float(v.get("alpha", 0.0)) < 0.5:
			continue
		var pos: Vector2 = v["pos"]
		var moving := bool(v.get("moving", false))
		var dir: Vector2 = v.get("dir", Vector2.RIGHT)
		var phase := float(v.get("phase", 0.0))
		var frac := fposmod(phase, 1.0)
		var mid := moving and frac >= 0.2 and frac <= 0.8
		var kind := _nav_kind(tv, pos)
		if want_kind != "" and kind != want_kind:
			continue
		if mid:
			n_mid += 1
		people.append({
			"id": id,
			"x": pos.x,
			"y": pos.y,
			"dx": dir.x,
			"dy": dir.y,
			"moving": moving,
			"mid_stride": mid,
			"phase": phase,
			"kind": kind,
		})
	return {"people": people, "n_mid": n_mid, "n": people.size()}


func _nav_kind(tv: Control, p: Vector2) -> String:
	var polys: Array = tv.Nav.build(tv.NODES)
	for rec in polys:
		if Geometry2D.is_point_in_polygon(p, rec["poly"]):
			return String(rec["kind"])
	# 2 px epsilon, matching the test.
	for rec in polys:
		var poly: PackedVector2Array = rec["poly"]
		var closest := _closest_on_poly(p, poly)
		if p.distance_to(closest) <= 2.0:
			return String(rec["kind"])
	return "off"


func _closest_on_poly(p: Vector2, poly: PackedVector2Array) -> Vector2:
	var best := p
	var best_d := INF
	for i in poly.size():
		var a: Vector2 = poly[i]
		var b: Vector2 = poly[(i + 1) % poly.size()]
		var q := Geometry2D.get_closest_point_to_segment(p, a, b)
		var d := p.distance_to(q)
		if d < best_d:
			best_d = d
			best = q
	return best


func _make_town(st: Dictionary) -> Control:
	var vp := SubViewport.new()
	vp.size = Vector2i(1536, 1024)
	vp.transparent_bg = false
	vp.disable_3d = true
	vp.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	vp.render_target_clear_mode = SubViewport.CLEAR_MODE_ALWAYS
	root.add_child(vp)
	var tv: Control = TownView.new()
	tv.size = Vector2(1536, 1024)
	tv.reduced_motion = false
	vp.add_child(tv)
	await process_frame
	tv.set_state(st)
	await process_frame
	return tv


func _save(tv: Control, path: String, st: Dictionary) -> void:
	var vp: SubViewport = tv.get_parent()
	var img: Image = vp.get_texture().get_image()
	if img == null:
		push_error("no image for %s" % path)
		return
	var err := img.save_png(path)
	print("[motion-render] %s %s err=%s" % [path, st.get("clock", {}).get("label"), error_string(err)])
