extends SceneTree
## Focused regressions for the real town walker routes, admission and following.

const TownView = preload("res://scripts/lessville/town_view.gd")
const Scale = preload("res://scripts/lessville/world_scale.gd")
const DT := 1.0 / 30.0
var failures: Array[String] = []
var checks := 0


func _initialize() -> void:
	call_deferred("_run")


func _check(ok: bool, message: String) -> void:
	checks += 1
	if not ok and not failures.has(message):
		failures.append(message)


func _town() -> Control:
	var tv: Control = TownView.new()
	root.add_child(tv)
	tv.set_process(false)
	return tv


func _run() -> void:
	_test_routes()
	_test_initial_bus_positions()
	_test_pause_fade()
	_test_admission()
	_test_opposing_crossing()
	_test_entry_occupancy()
	_test_segment_reroute()
	_test_follow_turn_lookahead()
	for failure: String in failures:
		push_error("walker-clearance: " + failure)
	print("walker-clearance: %d checks, %d failures" % [checks, failures.size()])
	quit(0 if failures.is_empty() else 1)


func _test_routes() -> void:
	var tv := _town()
	_check(Scale.ADULT_RADIUS >= 20.0, "adult collision radius must not shrink below 20 px")
	for endpoints: Array in [["K", "S"], ["S", "K"]]:
		var route: Array = tv._path(endpoints[0], endpoints[1])
		for i in range(1, route.size()):
			var a: Vector2 = route[i - 1]
			var b: Vector2 = route[i]
			var count := maxi(1, ceili(a.distance_to(b) / 2.0))
			for j in count + 1:
				_check(tv.nav_contains(a.lerp(b, float(j) / count)), "crossing route leaves navigation: %s -> %s" % [a, b])
	# Exact lane vertices, road boundaries, and an initial mid-crossing snapshot
	# must retain their travel lane when the desired destination reverses.
	for south: bool in [false, true]:
		for y: float in [662.0, 690.0, 726.0, 820.0, 912.0, 960.0]:
			for fresh: bool in [false, true]:
				var pos := Vector2(193.0 if south else 237.0, y)
				var exit_point := Vector2(pos.x, 960.0 if south else 662.0)
				var v := {"pos": pos, "path": [] if fresh else [exit_point]}
				var route: Array = tv._walker_path(v, "K" if south else "S")
				var next := pos
				for point: Vector2 in route:
					if point.distance_to(pos) > 0.01:
						next = point
						break
				var sign_y := 1.0 if south else -1.0
				_check((next.y - pos.y) * sign_y >= -0.01,
					"crossing reversal backtracks at %s, fresh=%s" % [pos, fresh])
				if y > 702.0 and y < 936.0:
					_check(absf(next.x - pos.x) <= 0.01, "crossing reversal cuts across lanes at %s" % pos)
				var previous := pos
				for point: Vector2 in route:
					for step in 21:
						_check(tv.nav_contains(previous.lerp(point, float(step) / 20.0)), "rerouted crossing leaves navigation at %s" % pos)
					previous = point
	tv.free()


func _test_initial_bus_positions() -> void:
	var tv := _town()
	var agents: Array = []
	for id: String in ["maya", "sam", "lin", "ama", "bo"]:
		agents.append({"id": id, "location": "bus_stop", "station": "street"})
	tv.set_state({"clock": {"minute": 480, "paused": true, "speed": 1}, "agents": agents, "background": []})
	for i in agents.size():
		var pose: Dictionary = tv.agent_pose(agents[i], i, 480.0)
		var goal: Dictionary = tv._goal_for(agents[i], i)
		_check((pose["pos"] as Vector2).distance_to(goal["point"]) < 0.01, "bus-stop initial pose disagrees with standing goal for %s" % agents[i]["id"])
	tv._update_walkers(DT)
	var before: Dictionary = tv._vis.duplicate(true)
	for tick in 30:
		tv._update_walkers(DT)
	for id: String in before:
		_check(tv._vis[id]["pos"] == before[id]["pos"], "paused bus-stop resident moved: " + id)
	tv.free()


func _test_pause_fade() -> void:
	var tv := _town()
	var state := {"clock": {"minute": 480, "paused": false, "speed": 1},
		"agents": [{"id": "maya", "location": "home", "station": "living"}], "background": []}
	tv.set_state(state)
	tv._update_walkers(DT)
	state["agents"] = [{"id": "maya", "location": "transit", "station": "street",
		"travel": {"from": "home", "to": "office", "depart": 480, "arrive": 500, "mode": "walk"}}]
	tv._update_walkers(DT)
	var before: Dictionary = tv._vis["maya"].duplicate(true)
	_check(float(before["alpha"]) > 0.0 and float(before["alpha"]) < 1.0, "pause fixture did not enter a partial fade")
	state["clock"]["paused"] = true
	for tick in 5:
		tv._update_walkers(DT)
	_check(tv._vis["maya"]["pos"] == before["pos"], "pause moved a fading resident")
	_check(tv._vis["maya"]["alpha"] == before["alpha"], "pause advanced a resident's fade")
	tv.free()


func _test_admission() -> void:
	var tv := _town()
	var agents: Array = []
	var ids := ["maya", "sam", "lin", "ama", "bo"]
	var destinations := ["home", "office", "cafe", "park", "bus_stop"]
	for i in ids.size():
		agents.append({"id": ids[i], "location": "transit", "station": "street", "travel": {
			"from": "trail", "to": destinations[i], "depart": 480, "arrive": 500, "mode": "walk"}})
	var background: Array = []
	for i in 3:
		background.append({"id": "entry-%d" % i, "location": "trail", "target": destinations[i], "visible": true})
	var state := {"clock": {"minute": 480, "paused": false, "speed": 1}, "agents": agents, "background": background}
	tv.set_state(state)
	var admitted := {}
	var previous := {}
	for tick in 3000:
		tv._update_walkers(DT)
		tv._update_background(DT)
		for group: Dictionary in [tv._vis, tv._bg_vis]:
			for id: String in group:
				var v: Dictionary = group[id]
				if float(v.get("alpha", 1.0)) >= 0.5:
					admitted[id] = true
					if previous.has(id):
						var max_step: float = (TownView.WALK_SPEED if tv._vis.has(id) else 72.0) * DT
						_check((v["pos"] as Vector2).distance_to(previous[id]) <= max_step + 0.01, "visible walker snapped: " + id)
					previous[id] = v["pos"]
	_check(admitted.size() == 8, "not every queued identity entered the town: %s" % [admitted.keys()])
	for group: Dictionary in [tv._vis, tv._bg_vis]:
		for id: String in group:
			_check((group[id]["path"] as Array).is_empty() and not bool(group[id].get("pending_entry", false)), "queued resident failed to arrive: " + id)
	tv.free()


func _test_opposing_crossing() -> void:
	var tv := _town()
	tv.set_state({"clock": {"minute": 480, "paused": false, "speed": 1}, "agents": [], "background": [
		{"id": "north", "location": "trail", "target": "home", "visible": true},
		{"id": "south", "location": "transit", "target": "trail", "visible": true}]})
	# Reconnect while both travellers approach opposite curbs.
	for id: String in ["north", "south"]:
		var south := id == "south"
		tv._bg_vis[id] = {"pos": Vector2(193, 700) if south else Vector2(237, 940),
			"path": [], "poly": [], "s": 0.0, "goal": "", "dir": Vector2.DOWN if south else Vector2.UP,
			"moving": false, "phase": 0.0, "alpha": 1.0, "pending_entry": false}
	var samples := 0
	for tick in 1200:
		tv._update_background(DT)
		var a: Dictionary = tv._bg_vis["north"]
		var b: Dictionary = tv._bg_vis["south"]
		var pa: Vector2 = a["pos"]
		var pb: Vector2 = b["pos"]
		if float(a["alpha"]) >= 0.5 and float(b["alpha"]) >= 0.5 and pa.y > 702 and pa.y < 936 and pb.y > 702 and pb.y < 936:
			samples += 1
			_check(pa.distance_to(pb) >= 2.0 * Scale.ADULT_RADIUS - 0.01, "opposing pedestrians overlap at west crossing")
	_check(samples > 20, "opposing-crossing fixture did not exercise simultaneous crossing")
	for id: String in tv._bg_vis:
		_check((tv._bg_vis[id]["path"] as Array).is_empty(), "opposing crossing stalled: " + id)
	tv.free()


func _test_entry_occupancy() -> void:
	var tv := _town()
	var occupied := {"pos": Vector2(237, 820), "path": [], "alpha": 1.0}
	tv._bg_vis = {"occupied": occupied}
	_check(not tv._entry_clear("new", Vector2(237, 820)), "occupied spawn point admits a second visible pedestrian")
	occupied["alpha"] = 0.25
	_check(not tv._entry_clear("new", Vector2(237, 820)), "partially visible resident loses its entry reservation")
	occupied["alpha"] = 0.0
	_check(tv._entry_clear("new", Vector2(237, 820)), "hidden pedestrian incorrectly blocks admission")
	tv.free()


func _test_segment_reroute() -> void:
	var routes := _town()
	for south: bool in [false, true]:
		var from_node := "K" if south else "S"
		var to_node := "S" if south else "K"
		var old_route: Array = routes._path(from_node, to_node)
		var start: Vector2 = old_route.pop_front()
		var pos := start.lerp(old_route[0], 0.4)
		var new_route: Array = routes._walker_path({"pos": pos, "path": old_route}, from_node)
		_check(new_route[0] == old_route[0], "crossing approach reversal discards its lane bend")
		_check(new_route.has(TownView.NODES[to_node]), "crossing approach reversal loses its connecting junction")
	for edge: Array in TownView.EDGES:
		if edge == ["SW", "S"]:
			continue # The two bent crossing lanes have their own boundary tests.
		for reverse: bool in [false, true]:
			var from_node: String = edge[1] if reverse else edge[0]
			var to_node: String = edge[0] if reverse else edge[1]
			var endpoint: Vector2 = TownView.NODES[to_node]
			var pos: Vector2 = (TownView.NODES[from_node] as Vector2).lerp(endpoint, 0.5)
			var route: Array = routes._walker_path({"pos": pos, "path": [endpoint]}, from_node)
			_check(not route.is_empty() and route[0] == endpoint, "mid-segment reversal loses junction on %s" % str(edge))
			for point: Vector2 in route:
				for step in 21:
					_check(routes.nav_contains(pos.lerp(point, step / 20.0)), "retained segment leaves navigation on %s" % str(edge))
				pos = point
	routes.free()
	# Both actor tiers receive new snapshots during a walk. A reversal must reach
	# the current junction first, then still arrive at the newly requested goal.
	for background: bool in [false, true]:
		for south: bool in [false, true]:
			var tv := _town()
			var endpoint: Vector2 = TownView.NODES["K" if south else "C"]
			var pos := Vector2(500, 575)
			var target := "home" if south else "trail"
			var heading := (endpoint - pos).normalized()
			var v := {"pos": pos, "path": [endpoint], "poly": [pos, endpoint], "s": 0.0,
				"goal": "old", "dir": heading, "moving": true, "phase": 0.0, "alpha": 1.0,
				"pending_entry": false, "hidden": false}
			var st := {"clock": {"minute": 480, "paused": false, "speed": 1}, "agents": [], "background": []}
			if background:
				st["background"] = [{"id": "turner", "location": "transit", "target": target}]
				tv._bg_vis["turner"] = v
			else:
				st["agents"] = [{"id": "maya", "location": "transit", "station": "street",
					"travel": {"from": "transit", "to": target, "depart": 480, "arrive": 500, "mode": "walk"}}]
				tv._vis["maya"] = v
			tv.set_state(st)
			var reached_junction := false
			for tick in 1000:
				var before: Vector2 = v["pos"]
				if background:
					tv._update_background(DT)
				else:
					tv._update_walkers(DT)
				var after: Vector2 = v["pos"]
				if tick == 0:
					_check((after - before).dot(heading) > 0.0, "snapshot reversal turns before the current junction")
					_check((v["poly"] as Array)[1] == endpoint, "reroute discarded the current segment")
				if after.distance_to(endpoint) < 0.01:
					reached_junction = true
				_check(tv.nav_contains(after), "reroute leaves the sidewalk")
				_check(after.distance_to(before) <= (72.0 if background else TownView.WALK_SPEED) * DT + 0.01,
					"reroute snaps to a new position")
				_check(float(v["alpha"]) == 1.0, "reroute hides a visible resident")
			_check(reached_junction, "reroute never reached its retained junction")
			_check((v["path"] as Array).is_empty(), "rerouted resident failed to finish the new trip")
			_check((v["pos"] as Vector2).distance_to(TownView.NODES[TownView.LOC_NODE[target]]) < 0.01,
				"rerouted resident arrived at the obsolete goal")
			tv.free()


func _test_follow_turn_lookahead() -> void:
	var tv := _town()
	var a := {"pos": Vector2(1000, 660.2941), "path": [TownView.NODES["M"], TownView.NODES["L"]], "alpha": 1.0}
	var b := {"pos": Vector2(1044, 661.5882), "path": [TownView.NODES["L"], TownView.NODES["M"]], "alpha": 1.0}
	tv._bg_vis = {"a": a, "b": b}
	# This checks false following dependencies, not general head-on avoidance.
	# A future shared waypoint must not create reciprocal waits on a U-turn.
	for id: String in tv._bg_vis:
		_check(is_equal_approx(tv._follow_budget(tv._bg_vis[id], 2.4, id, Scale.ADULT_RADIUS), 2.4),
			"opposing U-turn routes create a false follower wait")
	# A real leader after an ordinary corner must still constrain the follower.
	a["pos"] = Vector2(500, 620)
	a["path"] = [TownView.NODES["K"], TownView.NODES["L"]]
	b["pos"] = Vector2(520, 664.375)
	b["path"] = [TownView.NODES["L"]]
	_check(tv._follow_budget(a, 10.0, "a", Scale.ADULT_RADIUS) < 10.0,
		"corner lookahead stopped protecting a real leader")
	tv.free()
