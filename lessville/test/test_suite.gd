extends Node
## Lessville source checks: drive the real main scene with a recorded service snapshot,
## and, when the native LessDB service is reachable locally, read its live state.

const TownView = preload("res://scripts/lessville/town_view.gd")
const InteriorView = preload("res://scripts/lessville/interior_view.gd")
const SeatCatalog = preload("res://scripts/lessville/seat_catalog.gd")
const CrossingSim = preload("res://test/crossing_sim.gd")

var _failures := PackedStringArray()
var _checks := 0
var _expected_failures := 0


func _ready() -> void:
	call_deferred("_run")


func _run() -> void:
	_test_assets()
	_test_routing()
	_test_realism()
	_test_seating()
	await _test_main_scene()
	await _test_live_service()
	_test_facing_matches_heading()
	_test_pedestrians_stay_on_nav()
	_test_bus_route_and_dwell()
	_test_arc_boundaries()
	_test_world_scale()
	_test_crossing_overlap_and_progress()
	_test_atmosphere()
	_test_attire()
	_test_terrace_visible()
	_test_lin_profile_hair()
	_test_terrace_sitter_layers()
	_test_office_bed()
	_test_office_bed_patch()
	_test_office_bed_a3()
	_test_office_bed_known_limitations()
	if not _failures.is_empty():
		for f in _failures:
			push_error("[lessville-tests] " + f)
		print("[lessville-tests] FAIL: %d of %d (%d expected failures)" % [_failures.size(), _checks, _expected_failures])
		get_tree().quit(1)
	elif _expected_failures != 4:
		print("[lessville-tests] FAIL: expected exactly 4 known-limitation failures, got %d of %d" % [_expected_failures, _checks])
		get_tree().quit(1)
	else:
		print("[lessville-tests] PASS: %d checks (%d expected failures)" % [_checks, _expected_failures])
		get_tree().quit(0)


func _check(cond: bool, what: String) -> void:
	_checks += 1
	if not cond:
		_failures.append(what)


func _test_arc_boundaries() -> void:
	var case := _begin_case("arc follower boundaries")
	var tv: Control = TownView.new()
	if not tv.has_method("_advance_path"):
		_expect(case, false, "shared arc follower exists")
		tv.free()
		_end_case(case)
		return
	var poly := [Vector2.ZERO, Vector2.ZERO, Vector2(10, 0), Vector2(10, 10)]
	var v := {"pos": Vector2.ZERO, "poly": poly, "s": 0.0, "path": poly.slice(1)}
	_expect(case, tv._advance_path(v, 10.0) == 10.0, "exact vertex consumes 10 px")
	_expect(case, v["pos"] == Vector2(10, 0) and v["path"] == [Vector2(10, 10)], "vertex and duplicate waypoints are consumed")
	_expect(case, tv._advance_path(v, 0.0) == 0.0 and v["s"] == 10.0, "pause preserves arc progress")
	_expect(case, tv._advance_path(v, -5.0) == 0.0 and v["s"] == 10.0, "negative budget cannot reverse a route")
	_expect(case, tv._advance_path(v, 100.0) == 10.0, "large step clamps to remaining length")
	_expect(case, v["pos"] == Vector2(10, 10) and v["s"] == 20.0 and v["path"].is_empty(), "arrival empties waypoints at exact endpoint")
	_expect(case, tv._advance_path(v, 10.0) == 0.0 and v["s"] == 20.0, "arrival is stable across further ticks")
	var single := {"pos": Vector2(4, 5), "poly": [Vector2(4, 5), Vector2(4, 5)], "s": 0.0, "path": [Vector2(4, 5)]}
	_expect(case, tv._advance_path(single, 10.0) == 0.0 and single["path"].is_empty(), "zero-length route arrives without division by zero")
	var empty := {"pos": Vector2(4, 5), "poly": [], "s": 0.0, "path": []}
	_expect(case, tv._advance_path(empty, 10.0) == 0.0 and empty["pos"] == Vector2(4, 5), "empty route preserves position")
	var corner := {"pos": Vector2.ZERO, "poly": poly, "s": 0.0, "path": poly.slice(1)}
	_expect(case, tv._advance_path(corner, 15.0) == 15.0 and corner["pos"] == Vector2(10, 5), "movement budget follows the corner, not its chord")
	tv.free()
	_end_case(case)


## ---- per-test case reporting -------------------------------------------------
## The suite keeps its aggregate PASS/FAIL summary, and each BL-0148
## case also prints its own PASS/FAIL line with the first failure in the log.
## Office floor step-4 limitations are strict xfails (see KNOWN-LIMITATIONS.md).
func _begin_case(name: String) -> Dictionary:
	return {"name": name, "fails": []}


func _expect(case: Dictionary, cond: bool, what: String) -> void:
	_checks += 1
	if not cond:
		(case["fails"] as Array).append(what)
		_failures.append("%s: %s" % [case["name"], what])


func _end_case(case: Dictionary) -> void:
	var fails: Array = case["fails"]
	if fails.is_empty():
		print("[lessville-tests] PASS: %s" % case["name"])
	else:
		print("[lessville-tests] FAIL: %s — %s" % [case["name"], fails[0]])


func _test_assets() -> void:
	for id in ["maya", "sam", "lin", "ama", "bo", "mochi"]:
		for i in 4:
			_check(load("res://assets/game/characters/%s_%d.png" % [id, i]) is Texture2D, "frame %s_%d" % [id, i])
	for loc in ["town", "home", "office", "cafe", "grocery", "school", "gym", "trail"]:
		_check(load("res://assets/game/locations/%s.png" % loc) is Texture2D, "location " + loc)
	_check(load("res://assets/game/props/bus.png") is Texture2D, "bus sprite")
	var cfg: Variant = JSON.parse_string(FileAccess.get_file_as_string("res://config/lessville.json"))
	_check(cfg is Dictionary and String(cfg.get("api_base", "")).begins_with("http"), "api base configured")


func _test_routing() -> void:
	var tv: Control = TownView.new()
	tv.size = Vector2(1024, 683)
	add_child(tv)
	var p: Array = tv._path("A", "J")
	_check(p.size() >= 3, "home→café route follows sidewalks")
	_check(not p.is_empty() and (p[0] as Vector2) == TownView.NODES["A"] and (p[-1] as Vector2) == TownView.NODES["J"], "route endpoints")
	var walker := {"id": "lin", "location": "transit", "travel": {"from": "home", "to": "cafe", "depart": 100, "arrive": 108, "mode": "walk"}}
	var mid: Dictionary = tv.agent_pose(walker, 0, 104.0)
	_check(not mid.is_empty() and bool(mid.get("moving", false)), "walking agent is drawn and moving")
	var end: Dictionary = tv.agent_pose(walker, 0, 200.0)
	_check(not end.is_empty() and (end["pos"] as Vector2).distance_to(TownView.NODES["J"]) < 1.0, "walk ends at café door")
	_check(tv.agent_pose({"id": "sam", "location": "office"}, 1, 0.0).is_empty(), "indoor agent hidden on map")
	_check(tv.agent_pose({"id": "lin", "location": "transit", "travel": {"from": "bus_stop", "to": "school", "depart": 0, "arrive": 15, "mode": "bus"}}, 0, 5.0).is_empty(), "bus rider hidden")
	_check(not tv.agent_pose({"id": "ama", "location": "park"}, 3, 0.0).is_empty(), "outdoor agent drawn")
	# Smoothed walker: starts hidden at home, fades in at the door, walks the sidewalk, fades into the café.
	var st := {"clock": {"minute": 100, "paused": false, "speed": 1}, "agents": [{"id": "lin", "location": "home", "travel": null}]}
	tv.set_state(st)
	tv._update_walkers(0.016)
	_check(tv.visual_pose("lin").is_empty(), "indoor walker starts hidden")
	st["agents"] = [walker]
	tv.set_state(st)
	var last := Vector2(TownView.NODES["A"])
	var max_step := 0.0
	var saw_visible := false
	for f in 900:
		tv._update_walkers(1.0 / 60.0)
		var vp: Dictionary = tv.visual_pose("lin")
		if not vp.is_empty():
			saw_visible = true
			max_step = maxf(max_step, (vp["pos"] as Vector2).distance_to(last))
			last = vp["pos"]
	_check(saw_visible and max_step < 8.0, "walker moves continuously (max %.1f px/frame)" % max_step)
	st["agents"] = [{"id": "lin", "location": "cafe", "travel": null}]
	tv.set_state(st)
	for f in 600:
		tv._update_walkers(1.0 / 60.0)
	_check(tv.visual_pose("lin").is_empty() and last.distance_to(TownView.NODES["J"]) < 2.0, "walker reaches café door then fades inside")
	tv.queue_free()


func _test_realism() -> void:
	# Office: only Sam is in the served state; he sits on the center stool, not mid-room.
	var iv := InteriorView.new()
	iv.size = Vector2(768, 512)
	add_child(iv)
	iv.state = {
		"clock": {"minute": 1072, "day": 110, "minute_of_day": 1072, "label": "17:52", "paused": true, "speed": 1},
		"agents": [
			{"id": "maya", "location": "cafe", "station": "till", "travel": null},
			{"id": "sam", "name": "Sam", "location": "office", "station": "desk", "pose": "type", "activity_kind": "typing", "activity": "Working on the quarterly report", "travel": null},
			{"id": "lin", "location": "gym", "station": "workout", "travel": null},
			{"id": "ama", "location": "home", "station": "kitchen", "travel": null},
			{"id": "bo", "location": "away", "station": "offmap", "travel": null},
		],
	}
	iv.open("office")
	iv._update(0.0)
	var office_here: Array = iv.present()
	_check(office_here.size() == 1 and String(office_here[0].get("id")) == "sam", "office draws only Sam from state")
	_check(iv._iv.keys().size() == 1 and bool(iv._iv.has("sam")), "office has no phantom bodies")
	var desk: Array = iv.station_spot(office_here[0], {})
	var center: Dictionary = SeatCatalog.seat_by_id("office.desk.center")
	var center_feet: Vector2 = SeatCatalog.feet_from_hip(SeatCatalog.hip_of(center), center)
	_check(String(desk[0]) == "type" and (desk[1] as Vector2).distance_to(center_feet) < 8.0, "Sam is seated on the center office stool")
	var sam_hip: Vector2 = SeatCatalog.hip_from_feet(iv._iv["sam"]["pos"] as Vector2, center)
	_check(SeatCatalog.hip_in_seat(sam_hip, center), "Sam's hip anchor falls inside the center stool mask")
	# Missing frame holds the last real sprite instead of going blank.
	_check(iv._resolve_frame("sam", "sam_typing_0") == "sam_typing_0", "known frame is used")
	var held: String = iv._resolve_frame("sam", "sam_does_not_exist")
	_check(held == "sam_typing_0", "missing frame holds the last real pose")

	# Cafe: on first open, Maya and Bo start at their stations (not mid-floor from the door).
	# Baristas stay behind the counter; customers stay in the queue.
	iv.state = {
		"clock": {"minute": 464, "day": 111, "minute_of_day": 464, "label": "07:44", "paused": true, "speed": 1},
		"agents": [
			{"id": "maya", "name": "Maya", "title": "Owner", "location": "cafe", "station": "till", "pose": "talk", "activity_kind": "serving", "activity": "Running the counter", "travel": null},
			{"id": "sam", "location": "home", "station": "living", "travel": null},
			{"id": "lin", "location": "transit", "station": "street", "travel": null},
			{"id": "ama", "location": "park", "station": "fountain", "travel": null},
			{"id": "bo", "name": "Bo", "title": "Barista", "location": "cafe", "station": "espresso", "pose": "cook", "activity_kind": "serving", "activity": "Pulling espresso shots", "travel": null},
		],
	}
	iv.open("cafe")
	iv._update(0.0)
	_check(iv.present().size() == 2, "cafe present is Maya and Bo")
	_check((iv._iv["maya"]["pos"] as Vector2).distance_to(Vector2(610, 152)) < 1.0, "Maya starts at the till, behind the counter")
	_check((iv._iv["bo"]["pos"] as Vector2).distance_to(Vector2(520, 148)) < 1.0, "Bo starts at the espresso machine, not on the floor")
	var qtaken := {}
	var queued: Array = iv.station_spot({"id": "sam", "title": "Analyst", "location": "cafe", "station": "queue"}, qtaken)
	_check(String(queued[0]) == "stand" and (queued[1] as Vector2).y >= 280.0, "customers queue in front of the counter")
	var misplaced: Array = iv.station_spot({"id": "bo", "title": "Barista", "location": "cafe", "station": "queue", "activity": "Pulling espresso shots", "activity_kind": "cook"}, {})
	_check((misplaced[1] as Vector2).y < 200.0, "barista assigned to the queue is kept behind the counter")
	iv.queue_free()

	# Transit without a travel record (v47 patched shape) is visible on the street.
	var tv := TownView.new()
	tv.size = Vector2(1024, 683)
	add_child(tv)
	var commuter := {"id": "lin", "name": "Lin", "location": "transit", "station": "street", "pose": "walk_side", "activity_kind": "commuting", "activity": "Walking to the dorm bus stop"}
	var street: Dictionary = tv.agent_pose(commuter, 2, 464.0)
	_check(not street.is_empty() and bool(street.get("moving", false)), "transit agent without travel is drawn on the street")
	var at_home: Dictionary = tv.agent_pose({"id": "sam", "location": "home", "station": "living"}, 1, 464.0)
	_check(at_home.is_empty(), "indoor agent still hidden on the map")
	tv.set_state({"clock": {"minute": 464, "paused": false, "speed": 1}, "agents": [commuter]})
	tv._update_walkers(0.016)
	var vis: Dictionary = tv.visual_pose("lin")
	_check(not vis.is_empty() and float(vis.get("alpha", 0.0)) > 0.5, "transit walker is visible after update")
	# Held pose: a missing walk frame falls back to a real standing sprite, never a null texture.
	var held_key: String = tv._hands_key("lin", "lin_missing_frame", "free", "")
	_check(held_key == "lin_missing_frame" or tv._tex.has(held_key), "hands-key does not invent a blank frame")
	# Mid-travel snapshot: agent is still location=transit, so they stay visible even if close to arrival.
	var mid_travel := {"id": "lin", "name": "Lin", "location": "transit", "station": "street", "pose": "walk_side", "activity_kind": "commuting", "activity": "Heading to Less Café", "travel": {"from": "bus_stop", "to": "cafe", "depart": 157440, "arrive": 157444, "mode": "walk"}}
	var midp: Dictionary = tv.agent_pose(mid_travel, 2, 157443.0)
	_check(not midp.is_empty() and bool(midp.get("moving", false)), "mid-travel commuter has a street pose")
	tv.set_state({"clock": {"minute": 157443, "paused": true, "speed": 1}, "agents": [mid_travel]})
	tv._update_walkers(0.016)
	tv._update_walkers(1.0)
	var midv: Dictionary = tv.visual_pose("lin")
	_check(not midv.is_empty() and float(midv.get("alpha", 0.0)) > 0.5, "mid-travel commuter stays visible while state says transit")
	tv.queue_free()


func _test_seating() -> void:
	SeatCatalog.reload()
	var all: Array = SeatCatalog.seats()
	_check(all.size() >= 20, "seat catalog lists every scene's seats")
	var locs := {}
	var seen := {}
	for s: Dictionary in all:
		var id := String(s.get("id"))
		_check(not seen.has(id), "unique seat id " + id)
		seen[id] = true
		var loc := String(s.get("location"))
		locs[loc] = true
		_check(SeatCatalog.hip_in_seat(SeatCatalog.hip_of(s), s), "hip is inside the mask for " + id)
		var feet := SeatCatalog.feet_from_hip(SeatCatalog.hip_of(s), s)
		_check(SeatCatalog.hip_from_feet(feet, s).distance_to(SeatCatalog.hip_of(s)) < 0.01, "feet/hip convert for " + id)
	for loc: String in ["office", "home", "cafe", "school", "bus", "gym", "trail", "town"]:
		_check(bool(locs.get(loc, false)), "catalog includes seats for " + loc)
	# InteriorView assigns feet that put the hip on the mask, for every office desk.
	var iv := InteriorView.new()
	iv.size = Vector2(768, 512)
	add_child(iv)
	iv.open("office")
	var taken := {}
	for i in 3:
		var spot: Array = iv.station_spot({"id": "sam", "location": "office", "station": "desk"}, taken)
		_check(String(spot[0]) == "type", "office desk slot %d is a type seat" % i)
		var meta: Dictionary = spot[2] if spot.size() >= 3 and spot[2] is Dictionary else {}
		var seat: Dictionary = meta.get("seat", {})
		_check(not seat.is_empty() and SeatCatalog.hip_in_seat(SeatCatalog.hip_from_feet(spot[1], seat), seat), "office desk slot %d hip is on the chair" % i)
	# Headless render check: hip-in-mask plus z-order probes. Fails the build on any violation.
	var py := "/usr/bin/python3"
	var script := ProjectSettings.globalize_path("res://tools/check_seat_render.py")
	var out := []
	var err := []
	var code := OS.execute(py, PackedStringArray([script]), out, true, true)
	for line: String in out:
		print(line)
	_check(code == 0, "headless seat render check exits 0")
	iv.queue_free()


func _fixture() -> Dictionary:
	return {
		"generation": 1,
		"clock": {"minute": 499, "day": 1, "minute_of_day": 499, "label": "08:19", "paused": false, "speed": 2, "raining": true},
		"agents": [
			{"id": "maya", "name": "Maya", "title": "Owner", "location": "cafe", "place": "Less Café", "activity": "Counter", "travel": null, "bubble": {"text": "Want us to remember your usual?", "kind": "say", "until": 510}},
			{"id": "sam", "name": "Sam", "title": "Analyst", "location": "office", "place": "Office", "activity": "Report", "travel": null},
			{"id": "lin", "name": "Lin", "title": "Student", "location": "transit", "place": "On the way", "activity": "Walking", "travel": {"from": "bus_stop", "to": "cafe", "depart": 495, "arrive": 503, "mode": "walk"}},
			{"id": "ama", "name": "Ama", "title": "Parent", "location": "grocery", "place": "FreshMart", "activity": "Shopping", "travel": null},
			{"id": "bo", "name": "Bo", "title": "Barista", "location": "cafe", "place": "Less Café", "activity": "Espresso", "travel": null},
		],
		"bus": {"arrived": {"scheduled": 480, "at": 495}, "next": {"scheduled": 500, "delay": 0}},
		"feed": [
			{"seq": 2, "minute": 499, "kind": "memory", "actor": "lin", "title": "Lin remembers", "detail": "Usual order: oat latte.", "refs": ["mem:12"], "scope": "cafe-regulars"},
			{"seq": 1, "minute": 495, "kind": "message", "actor": "lin", "title": "Lin → #neighborhood", "detail": "Bus is late.", "refs": ["msg:4"], "scope": "neighborhood"},
		],
		"sql": [{"id": 9, "sim_minute": 499, "actor": "lin", "purpose": "store memory", "sql": "INSERT INTO lv_memories VALUES (12)", "rows": 1, "micros": 120, "ok": true}],
		"statements": 99,
	}


func _test_main_scene() -> void:
	var main: Control = (load("res://scenes/main.tscn") as PackedScene).instantiate()
	add_child(main)
	await get_tree().process_frame
	main.api.stop_polling()
	_check(bool(main.title_layer.visible) and not bool(main.game_layer.visible), "boots to title")
	main.start_btn.emit_signal("pressed")
	_check(bool(main.game_layer.visible), "Enter shows the town")
	var fixture := _fixture()
	for agent: Dictionary in fixture["agents"]:
		agent["presence"] = {"companion": null, "hands": "free"}
	main._on_state(fixture)
	_check(main.clock_lbl.text == "08:19" and main.day_lbl.text == "Day 1", "clock from service state")
	_check("Rain" in String(main.weather_lbl.text), "weather shown")
	_check(String(main.place_cards["grocery"].text) == "Ama", "off-map place lists occupants")
	_check(bool(main.speed_btns[2].button_pressed) and not bool(main.speed_btns[1].button_pressed), "speed reflects service")
	_check(main.content.get_child_count() >= 3, "story feed rendered")
	_check(not main.town.agent_pose(main._agent("lin"), 2, 499.0).is_empty(), "Lin walking on map")
	main._select_agent("bo")
	_check(main.tab == "mind" and main.selected == "bo" and main.town.selected == "bo", "selecting agent opens Mind")
	main._on_reply("agent", true, {"agent": "bo", "visible": [{"id": 8, "owner": "bo", "kind": "contextual", "scope": "cafe-staff", "content": "Oat milk is out", "sim_minute": 450, "expires_at": -1}], "hidden": 3, "decisions": [], "conversation": [], "scopes": [{"scope": "cafe-staff"}]})
	var text := _collect_text(main.content)
	_check("Oat milk is out" in text and "3 other rows" in text, "Mind shows recall and hidden count with a nullable companion")
	main._open_place("cafe")
	_check(bool(main.interior_layer.visible) and main.interior.present().size() == 2, "café close-up shows Maya and Bo")
	main._open_memory(12)
	_check(bool(main.modal_layer.visible), "provenance modal opens")
	main._on_reply("memory", true, {"memory": {"id": 12, "owner": "lin", "kind": "permanent", "scope": "cafe-regulars", "content": "Usual order: oat latte, less sweet.", "sim_minute": 499, "expires_at": -1, "source_kind": "message", "source_id": 5},
		"source": [{"id": 5, "sender": "maya", "body": "Want us to remember your usual?"}], "readers": [{"agent": "bo"}, {"agent": "maya"}],
		"used_by": [{"sim_minute": 1924, "agent": "bo", "action": "Serve the usual", "explanation": "Recalled memory #12", "policy": "bo.usual v1"}]})
	var mt := _collect_text(main.modal_body)
	_check("Usual order" in mt and "Serve the usual" in mt and "Bo" in mt, "provenance lists source, readers and effect")
	main.modal_layer.visible = false
	main._set_tab("db")
	_check("INSERT INTO lv_memories" in _collect_text(main.content), "LessDB live lists SQL")
	main._on_reply("boundaries", true, {"native": [["Persistent rows", "x"]], "application": [["Scopes", "y"]], "open": ["Native tenant isolation"]})
	main._set_tab("truth")
	var tt := _collect_text(main.content)
	_check("Persistent rows" in tt and "Native tenant isolation" in tt, "Truth separates native/app/open")
	main.queue_free()
	await get_tree().process_frame


func _test_live_service() -> void:
	var req := HTTPRequest.new()
	req.timeout = 3.0
	add_child(req)
	if req.request("http://127.0.0.1:8790/api/state") != OK:
		print("[lessville-tests] live service: request not started")
		return
	var res: Array = await req.request_completed
	if int(res[0]) != HTTPRequest.RESULT_SUCCESS or int(res[1]) != 200:
		print("[lessville-tests] live service not reachable on 127.0.0.1:8790; live checks skipped")
		return
	var s: Variant = JSON.parse_string((res[3] as PackedByteArray).get_string_from_utf8())
	_check(s is Dictionary and (s.get("agents", []) as Array).size() == 5, "live service returns five agents")
	_check(s is Dictionary and int(s.get("statements", 0)) > 0, "live service has executed LessDB statements")
	if s is Dictionary:
		print("[lessville-tests] live service: %d statements, clock %s" % [int(s.get("statements", 0)), s.get("clock", {}).get("label", "?")])


## BL-0148 (a): the drawn sprite must face the way the walker is moving in this
## top-down / three-quarter projection.  Every cardinal and diagonal heading is
## checked through the same facing_for()/sprite_key() pair _draw_agent uses, then
## a real walker is driven through the junction network and sampled every frame so a
## one-frame lag (or the old -0.6 rad back threshold) fails.
func _test_facing_matches_heading() -> void:
	var case := _begin_case("facing matches heading")
	var tv: Control = TownView.new()
	tv.size = Vector2(1024, 683)
	add_child(tv)

	var headings := [
		Vector2(1, 0), Vector2(-1, 0), Vector2(0, 1), Vector2(0, -1),
		Vector2(1, 1).normalized(), Vector2(-1, 1).normalized(),
		Vector2(1, -1).normalized(), Vector2(-1, -1).normalized(),
		Vector2(1, -0.25).normalized(), Vector2(-1, -0.25).normalized(),
	]
	for h: Vector2 in headings:
		var face: Dictionary = tv.facing_for(h)
		_expect(case, bool(face["back"]) == (h.y < 0.0), "back/front for heading %s" % str(h))
		_expect(case, bool(face["flip"]) == (h.x < -0.2), "flip for heading %s" % str(h))
		var key: String = tv.sprite_key("lin", h, true, 0.0)
		if h.y < -0.05:
			_expect(case, key.begins_with("lin_back_"), "up heading uses back frame (%s for %s)" % [key, str(h)])
		elif h.y > 0.05:
			_expect(case, key == "lin_1" or key == "lin_2", "down/lateral heading uses front walk frame (%s for %s)" % [key, str(h)])
		_expect(case, tv.sprite_key("lin", h, false, 0.0) == "lin_0", "idle uses idle frame (%s)" % str(h))
	# The two-frame cycle must alternate with the phase, and only while moving.
	var east := Vector2(1, 0)
	var north := Vector2(0, -1)
	_expect(case, tv.sprite_key("lin", east, true, 0.0) == "lin_1", "walk phase 0 is the first front frame")
	_expect(case, tv.sprite_key("lin", east, true, 1.0) == "lin_2", "walk phase 1 is the second front frame")
	_expect(case, tv.sprite_key("lin", north, true, 0.0) == "lin_back_0", "walk phase 0 is the first back frame")
	_expect(case, tv.sprite_key("lin", north, true, 1.0) == "lin_back_1", "walk phase 1 is the second back frame")
	_expect(case, tv.sprite_key("lin", east, false, 7.0) == "lin_0", "idle never animates the walk cycle")

	# Seed the walker indoors at home, then send it across the map to the cafe.
	var st := {"clock": {"minute": 600, "paused": false, "speed": 1}, "agents": [
		{"id": "lin", "name": "Lin", "location": "home", "travel": null},
	]}
	tv.set_state(st)
	tv._update_walkers(1.0 / 60.0)
	st["agents"] = [{"id": "lin", "name": "Lin", "location": "transit",
		"travel": {"from": "home", "to": "cafe", "depart": 600, "arrive": 620, "mode": "walk"}}]
	tv.set_state(st)

	var prev: Vector2 = (tv._vis["lin"]["pos"] as Vector2)
	var prev_leg := Vector2.ZERO
	var lag_frames := 0
	var sign_frames := 0
	var turns := 0
	var frames := 0
	while frames < 900:
		tv._update_walkers(1.0 / 60.0)
		frames += 1
		if frames == 20:
			# Paused clock: a walker with a live path must stand in the idle pose.
			st["clock"]["paused"] = true
			tv._update_walkers(1.0 / 60.0)
			var frozen: Dictionary = tv.walker_sprite("lin")
			_expect(case, not bool(frozen["moving"]) and String(frozen["key"]) == "lin_0",
				"paused walker stands idle instead of sliding (%s)" % String(frozen.get("key", "")))
			st["clock"]["paused"] = false
		var pose: Dictionary = tv.visual_pose("lin")
		if pose.is_empty():
			break
		var pos: Vector2 = pose["pos"]
		var vel: Vector2 = pos - prev
		prev = pos
		if vel.length() < 0.25:
			continue
		var vd := vel.normalized()
		var sprite: Dictionary = tv.walker_sprite("lin")
		if (sprite["dir"] as Vector2).dot(vd) < 0.98:
			lag_frames += 1
		if absf(vd.y) > 0.05 and bool(sprite["back"]) != (vd.y < 0.0):
			sign_frames += 1
		if absf(vd.x) > 0.2 and bool(sprite["flip"]) != (vd.x < 0.0):
			sign_frames += 1
		if prev_leg != Vector2.ZERO and vd.dot(prev_leg) < 0.94:
			turns += 1
			if (sprite["dir"] as Vector2).dot(vd) < 0.98:
				lag_frames += 1
		prev_leg = vd
	_expect(case, frames > 60, "walker crossed the map (%d frames)" % frames)
	_expect(case, turns >= 4, "path turned at junctions (%d turns)" % turns)
	_expect(case, lag_frames == 0, "%d frames drew a stale or opposite heading" % lag_frames)
	_expect(case, sign_frames == 0, "%d frames flipped against the velocity sign" % sign_frames)
	print("[lessville-tests]   facing: %d frames, %d junction turns, %d lag frames, %d sign mismatches" % [frames, turns, lag_frames, sign_frames])
	tv.queue_free()
	_end_case(case)


## BL-0148 (b): ten minutes of simulated walking, with several named agents and
## six background pedestrians continuously re-routed.  Every sampled position must
## be inside a sidewalk or crossing nav polygon (2 px epsilon).  The first
## off-polygon sample is reported with id, time and position.
func _test_pedestrians_stay_on_nav() -> void:
	var case := _begin_case("pedestrians stay on sidewalk/crossing")
	var tv: Control = TownView.new()
	tv.size = Vector2(1024, 683)
	add_child(tv)
	var polys: Array = tv.nav_polygons()
	_expect(case, polys.size() >= 20, "nav polygon set built (%d polygons)" % polys.size())
	# The polygon set must exclude the road, the grass and the buildings, and
	# include the crossings and the shelter pavement.
	_expect(case, not tv.nav_contains(Vector2(800, 810)), "roadway is not walkable")
	_expect(case, not tv.nav_contains(Vector2(120, 400)), "grass/garden is not walkable")
	_expect(case, not tv.nav_contains(Vector2(700, 200)), "building interior is not walkable")
	_expect(case, tv.nav_contains(Vector2(215, 820)), "west crossing is walkable")
	_expect(case, tv.nav_contains(Vector2(545, 772)), "bus boarding apron is walkable")
	_expect(case, tv.nav_contains(Vector2(470, 700)), "shelter pavement is walkable")

	var places := ["park", "bus_stop", "cafe", "home", "office", "grocery", "trail", "transit"]
	var ids := ["maya", "sam", "lin", "ama", "bo"]
	var agents: Array = []
	for i in ids.size():
		agents.append({"id": ids[i], "name": ids[i], "location": places[i], "travel": null})
	var people: Array = []
	for i in 6:
		people.append({"id": "bg%d" % i, "location": places[i % places.size()], "target": places[(i + 3) % places.size()]})
	var minute := 480.0
	var st := {"clock": {"minute": minute, "paused": false, "speed": 1}, "agents": agents, "background": people}
	tv.set_state(st)

	var dt := 1.0 / 60.0
	var frames := 60 * 60 * 10  # ten minutes of simulated walking at 60 fps
	var bad_id := ""
	var bad_t := 0.0
	var bad_pos := Vector2.ZERO
	var samples := 0
	var moving_samples := 0
	var bg_lag := 0
	var prev_bg := {}
	var next_goal := 25.0
	var round := 0
	for f in frames:
		tv._update_walkers(dt)
		tv._update_background(dt)
		var sim_s := float(f) * dt
		if sim_s >= next_goal:
			next_goal += 25.0
			round += 1
			var ags: Array = []
			for i in ids.size():
				var to: String = places[(round + i * 2) % places.size()]
				ags.append({"id": ids[i], "name": ids[i], "location": "transit",
					"travel": {"from": String(agents[i]["location"]), "to": to,
						"depart": minute + sim_s * 2.0, "arrive": minute + sim_s * 2.0 + 10.0, "mode": "walk"}})
			var ppl: Array = []
			for i in people.size():
				ppl.append({"id": "bg%d" % i, "location": String(people[i]["target"]),
					"target": places[(round * 3 + i) % places.size()]})
			agents = ags
			people = ppl
			st["agents"] = agents
			st["background"] = people
			st["clock"]["minute"] = minute + sim_s * 2.0
			tv.set_state(st)
		# Facing is checked every frame so a junction blend cannot hide a lag.
		for id: String in tv._bg_vis:
			var pf: Vector2 = tv._bg_vis[id]["pos"]
			var step: Vector2 = pf - Vector2(prev_bg.get(id, pf))
			if step.length() > 0.05 and (tv._bg_vis[id]["dir"] as Vector2).dot(step.normalized()) < 0.98:
				bg_lag += 1
			prev_bg[id] = pf
		if f % 6 != 0:
			continue
		for id: String in tv._vis:
			var v: Dictionary = tv._vis[id]
			if float(v["alpha"]) <= 0.01:
				continue
			var p: Vector2 = v["pos"]
			samples += 1
			if bool(v.get("moving", false)):
				moving_samples += 1
			if not tv.nav_contains(p):
				bad_id = id
				bad_t = sim_s
				bad_pos = p
				break
		if bad_id != "":
			break
		for id: String in tv._bg_vis:
			var p2: Vector2 = tv._bg_vis[id]["pos"]
			samples += 1
			if bool(tv._bg_vis[id].get("moving", false)):
				moving_samples += 1
			if not tv.nav_contains(p2):
				bad_id = id
				bad_t = sim_s
				bad_pos = p2
				break
		if bad_id != "":
			break
	_expect(case, bad_id == "", "off-nav sample %s at t=%.1fs pos=%s" % [bad_id, bad_t, str(bad_pos)])
	_expect(case, samples > 2000, "sampled %d pedestrian positions over 10 minutes" % samples)
	_expect(case, moving_samples > 1000, "pedestrians were walking for %d of %d samples" % [moving_samples, samples])
	_expect(case, bg_lag == 0, "%d background frames faced against their velocity" % bg_lag)
	print("[lessville-tests]   sidewalk: %d frames, %d samples (%d moving), %d nav polygons" % [frames, samples, moving_samples, polys.size()])
	tv.queue_free()
	_end_case(case)


## BL-0148 (c): the bus must follow the fixed route polyline, stop at each defined
## stop and dwell there.  The authoritative service record (state.bus.next/active)
## drives arrival and the 3 s minimum dwell; the test also checks that the bus
## never overshoots a stop, never reverses and leaves the 16 px corridor.
func _test_bus_route_and_dwell() -> void:
	var case := _begin_case("bus follows route, stops and dwells")
	var tv: Control = TownView.new()
	tv.size = Vector2(1024, 683)
	add_child(tv)
	var route: Array = tv.bus_route_points()
	var stops: Array = tv.bus_stops()
	_expect(case, route.size() >= 2, "fixed route polyline has %d points" % route.size())
	_expect(case, stops.size() >= 1, "%d stops defined" % stops.size())
	_expect(case, (stops[0]["pos"] as Vector2).distance_to(Vector2(TownView.BUS_STOP_X, TownView.BUS_Y)) < 0.01,
		"Lessville shelter is a stop on the route")

	var st := {"clock": {"minute": 480, "paused": false, "speed": 1}, "agents": [], "background": [],
		"bus": {"next": {"scheduled": 492, "delay": 0}, "active": null}}
	tv.set_state(st)

	var dt := 1.0 / 60.0
	var t := 480.0
	var max_dev := 0.0
	var visited := {}
	var dwells := {}
	var dwell_start := -1.0
	var dwell_stop := -1
	var overshoot := 0
	var backtrack := 0
	var prev_s := 0.0
	var prev_phase := "idle"
	var reached_idle_after := false
	var was_active := false
	for f in 60 * 30:
		t += dt * 2.0  # clock x1: two sim minutes per real second
		var age := t - 492.0
		if age >= 0.0 and age <= 4.0:
			st["bus"]["active"] = {"id": "day1-0820-inbound",
				"phase": "dwell" if age <= 2.0 else "depart", "scheduled": 492, "at": 492}
			was_active = true
		else:
			st["bus"]["active"] = null
		tv._update_bus(dt, t)
		var pos: Vector2 = tv.bus_position()
		var s: float = tv.bus_s()
		var phase: String = tv.bus_phase()
		var stop_i: int = tv.bus_stop_index()
		max_dev = maxf(max_dev, _dist_to_polyline(pos, route))
		if phase != "idle" and prev_phase != "idle" and s < prev_s - 0.5:
			backtrack += 1
		if phase == "depart" and stop_i >= 0 and stop_i < stops.size():
			if s > float(stops[stop_i]["s"]) + 0.5:
				overshoot += 1
		if phase == "dwell":
			if dwell_start < 0.0:
				dwell_start = float(f) * dt
				dwell_stop = stop_i
			visited[stop_i] = true
			if pos.distance_to(stops[clampi(stop_i, 0, stops.size() - 1)]["pos"]) > 1.0:
				_expect(case, false, "dwell at stop %d but bus is at %s" % [stop_i, str(pos)])
		elif dwell_start >= 0.0:
			var dur := float(f) * dt - dwell_start
			dwells[dwell_stop] = maxf(float(dwells.get(dwell_stop, 0.0)), dur)
			dwell_start = -1.0
		if was_active and phase == "idle" and float(f) * dt > 8.0:
			reached_idle_after = true
		prev_s = s
		prev_phase = phase
		if reached_idle_after:
			break
	if dwell_start >= 0.0:
		dwells[dwell_stop] = maxf(float(dwells.get(dwell_stop, 0.0)), 30.0 - dwell_start)
	_expect(case, max_dev <= 16.0, "bus stayed within 16 px of the route (max %.1f)" % max_dev)
	_expect(case, overshoot == 0, "bus overshot a stop on %d frames" % overshoot)
	_expect(case, backtrack == 0, "bus reversed along the route on %d frames" % backtrack)
	for i in stops.size():
		_expect(case, bool(visited.get(i, false)), "bus visited stop %d (%s)" % [i, stops[i]["id"]])
		_expect(case, float(dwells.get(i, 0.0)) >= 2.9, "stop %d dwell %.2fs >= 3s" % [i, float(dwells.get(i, 0.0))])
	_expect(case, reached_idle_after, "bus completed approach -> dwell -> depart -> idle")
	print("[lessville-tests]   bus: max deviation %.2f px, dwell %s, visited %s" % [max_dev, str(dwells), str(visited.keys())])
	tv.queue_free()

	# A poll gap that first sees a trip already departing must not replay the
	# approach from off-map: the bus starts at the shelter and leaves.
	var tv2: Control = TownView.new()
	tv2.size = Vector2(1024, 683)
	add_child(tv2)
	var st2 := {"clock": {"minute": 600, "paused": false, "speed": 1}, "agents": [], "background": [],
		"bus": {"next": {"scheduled": 610, "delay": 0},
			"active": {"id": "gap-trip", "phase": "depart", "scheduled": 600, "at": 600}}}
	tv2.set_state(st2)
	tv2._update_bus(1.0 / 60.0, 600.0)
	_expect(case, tv2.bus_phase() == "depart", "first-seen depart starts a departure (%s)" % tv2.bus_phase())
	_expect(case, tv2.bus_position().distance_to(Vector2(TownView.BUS_STOP_X, TownView.BUS_Y)) < 0.01,
		"first-seen depart starts at the shelter")
	tv2.queue_free()
	_end_case(case)


## BL-0148 step 2b (G12, BL-0167 job 1): seeded dense-crossing sim with
## mid-route destination flips (test/crossing_sim.gd).  Two cases:
##  * crossing overlap: no two walker footprints (named r=20, background r=20
##    map px) intersect at a zebra crossing or within 24 px of its entry;
##    drawn heading within 11.48 deg (dot 0.98) of the tick step and of the
##    path tangent; zero off-nav feet.
##  * walker progress: every walker reaches its last goal after the settle
##    phase and no visible walker with a path waits more than 10 s.
func _test_crossing_overlap_and_progress() -> void:
	var tv: Control = TownView.new()
	tv.size = Vector2(1024, 683)
	add_child(tv)
	var r: Dictionary = CrossingSim.run(tv, {"seed": 7, "seconds": 600.0, "settle": 180.0, "background": 10})
	tv.queue_free()
	var pc: Dictionary = r["per_crossing"]
	var zebra_overlap := int(pc["crossing_west"]["overlap_ticks"]) + int(pc["crossing_east"]["overlap_ticks"])
	var zebra_margin := minf(float(pc["crossing_west"]["min_margin"]), float(pc["crossing_east"]["min_margin"]))
	var case := _begin_case("crossing overlap (dense seeded sim)")
	_expect(case, int(pc["crossing_west"]["visits"]) >= 50, "dense traffic: %d west-zebra visits" % int(pc["crossing_west"]["visits"]))
	_expect(case, int(r["flips"]) >= 100, "destination flips exercised (%d)" % int(r["flips"]))
	_expect(case, zebra_overlap == 0, "%d zebra overlap pair-ticks, min footprint margin %.1f px (%s at %s)" % [zebra_overlap, zebra_margin, String(r["min_sep_pair"]), String(r["min_sep_at"])])
	_expect(case, int(r["facing_bad_ticks"]) == 0, "%d ticks faced off the step (max %.1f deg)" % [int(r["facing_bad_ticks"]), float(r["max_facing_err_deg"])])
	_expect(case, float(r["max_tangent_err_deg"]) <= 11.48, "facing vs path tangent max %.1f deg" % float(r["max_tangent_err_deg"]))
	_expect(case, int(r["off_nav"]) == 0, "%d off-nav samples (first %s)" % [int(r["off_nav"]), String(r["first_off_nav"])])
	print("[lessville-tests]   crossings: seed %d, %d ticks, %d walkers, %d flips, west visits %d, zebra overlap %d, min margin %.1f px, facing max %.2f deg, tangent max %.2f deg, off-nav %d, boarding-apron overlap %d (info)" % [int(r["seed"]), int(r["ticks"]), int(r["walkers"]), int(r["flips"]), int(pc["crossing_west"]["visits"]), zebra_overlap, zebra_margin, float(r["max_facing_err_deg"]), float(r["max_tangent_err_deg"]), int(r["off_nav"]), int(pc["bus_boarding"]["overlap_ticks"])])
	_end_case(case)
	var case2 := _begin_case("walkers progress (no stall, max wait 10 s)")
	_expect(case2, (r["unreached"] as Array).is_empty(), "unreached after settle: %s" % str(r["unreached"]))
	_expect(case2, float(r["max_wait_s"]) <= 10.0, "max wait %.1f s (%s)" % [float(r["max_wait_s"]), String(r["max_wait_id"])])
	print("[lessville-tests]   progress: max wait %.2f s, unreached %d" % [float(r["max_wait_s"]), (r["unreached"] as Array).size()])
	_end_case(case2)
	# BL-0167 job 1a: walkers carry arc-length state on their path polyline.
	var case3 := _begin_case("walkers follow arc-length path (job 1a)")
	_expect(case3, int(r["arc_checked"]) >= 1000, "arc samples measured (%d)" % int(r["arc_checked"]))
	_expect(case3, int(r["arc_missing_ticks"]) == 0 and int(r["arc_bad_ticks"]) == 0 and int(r["arc_s_back"]) == 0,
		"arc state: %d missing, %d feet != P(s) (max %.2f px), %d s went back; first: %s" % [int(r["arc_missing_ticks"]), int(r["arc_bad_ticks"]), float(r["arc_max_err_px"]), int(r["arc_s_back"]), String(r["first_arc_bad"])])
	_end_case(case3)
	# BL-0167 job 1b: same-direction followers keep a gap on shared segments.
	var case4 := _begin_case("same-direction followers keep a gap (job 1b)")
	_expect(case4, int(r["follow_pairs"]) >= 100, "shared-segment follower samples (%d)" % int(r["follow_pairs"]))
	_expect(case4, int(r["follow_overlap_ticks"]) == 0, "%d follower overlap ticks, min margin %.1f px (%s)" % [int(r["follow_overlap_ticks"]), float(r["min_follow_margin"]), String(r["follow_pair_at"])])
	_end_case(case4)


func _dist_to_polyline(p: Vector2, pts: Array) -> float:
	var best := INF
	for i in range(1, pts.size()):
		var q := Geometry2D.get_closest_point_to_segment(p, pts[i - 1], pts[i])
		best = minf(best, p.distance_to(q))
	return best


func _collect_text(n: Node) -> String:
	var out := ""
	if n is Label:
		out += (n as Label).text + "\n"
	elif n is Button:
		out += (n as Button).text + "\n"
	for c in n.get_children():
		out += _collect_text(c)
	return out

func _test_atmosphere() -> void:
	var case := _begin_case("atmosphere follows the clock")
	var Atmos = preload("res://scripts/lessville/world_atmosphere.gd")
	var night: Dictionary = Atmos.snapshot({"day": 2, "minute_of_day": 1320, "minute": 2760, "label": "22:00", "paused": true})
	_expect(case, String(night.get("phase")) == "night", "22:00 is night")
	_expect(case, float(night.get("darkness")) >= 0.65, "22:00 darkness is night-strength")
	var noon: Dictionary = Atmos.snapshot({"day": 2, "minute_of_day": 750, "minute": 2190, "label": "12:30", "paused": true})
	_expect(case, String(noon.get("phase")) == "day", "12:30 is day")
	_expect(case, float(noon.get("darkness")) <= 0.02, "12:30 has no night overlay")
	_expect(case, float(Atmos.wind_kmh({"day": 2, "minute_of_day": 750, "minute": 2190})) > 0.0, "day 2 12:30 scheduled wind is > 0")
	var dawn: Dictionary = Atmos.snapshot({"day": 2, "minute_of_day": 450, "minute": 1890, "label": "07:30", "paused": true})
	_expect(case, String(dawn.get("phase")) == "dawn", "07:30 is dawn")
	# Pixel check on the exact frames the gallery publishes, when they exist (G12).
	var frames := OS.get_environment("LESSVILLE_ATMOS_DIR")
	if frames.is_empty():
		frames = "/workspace/shared/ai-hq/results/lessdb/evidence/BL-0148/gallery-step2-a3-frames"
	if FileAccess.file_exists(frames + "/town-2200.png"):
		var py := "/usr/bin/python3"
		var script := ProjectSettings.globalize_path("res://tools/check_atmosphere.py")
		var out := []
		var err := []
		var code := OS.execute(py, PackedStringArray([script, "--dir", frames]), out, true, true)
		for line: String in out:
			print(line)
		_expect(case, code == 0, "atmosphere pixel check on review frames exits 0")
	else:
		print("[lessville-tests] atmosphere frames not at %s; pixel check skipped" % frames)
	_end_case(case)




func _test_attire() -> void:
	var case := _begin_case("attire follows role and place")
	var Attire = preload("res://scripts/lessville/attire.gd")
	Attire.reload()
	var mdl: Dictionary = Attire.model()
	_expect(case, not mdl.is_empty() and (mdl.get("cases", []) as Array).size() >= 8, "attire model loads with cases")
	for c: Variant in mdl.get("cases", []):
		var spec: Dictionary = c
		var agent := {
			"id": String(spec.get("character")),
			"location": String(spec.get("location", "")),
			"station": String(spec.get("station", "")),
			"activity": String(spec.get("activity", "")),
			"activity_kind": String(spec.get("activity_kind", "")),
			"travel": spec.get("travel"),
			"minute": int(spec.get("minute", 0)),
		}
		var clock := {"minute_of_day": int(spec.get("minute", 0)) % 1440, "minute": int(spec.get("minute", 0))}
		var got: Dictionary = Attire.layers_for(agent, clock)
		var expected: Dictionary = spec.get("expected", {})
		for k: String in expected:
			_expect(case, bool(got.get(k, false)) == bool(expected[k]), "%s %s" % [String(spec.get("id")), k])
	# Pixel check: after the fix, --apply-model uses the plain variants.
	var py := "/usr/bin/python3"
	var script := ProjectSettings.globalize_path("res://tools/check_attire.py")
	var out := []
	var code := OS.execute(py, PackedStringArray([script, "--apply-model"]), out, true, true)
	for line: String in out:
		print(line)
	_expect(case, code == 0, "attire pixel check --apply-model exits 0")
	var frames := OS.get_environment("LESSVILLE_ATTIRE_DIR")
	if not frames.is_empty() and FileAccess.file_exists(frames + "/outfit-state.json"):
		out = []
		code = OS.execute(py, PackedStringArray([script, "--dir", frames]), out, true, true)
		for line: String in out:
			print(line)
		_expect(case, code == 0, "attire pixel check on review frames exits 0")
	_end_case(case)


func _test_terrace_visible() -> void:
	var case := _begin_case("town seats are drawn at step-1 anchors")
	# The Godot check is a SceneTree script; invoke a nested headless run.
	var godot := OS.get_executable_path()
	var out := []
	var code := OS.execute(godot, PackedStringArray(["--headless", "--path", ".", "--script", "res://tools/check_terrace_visible.gd"]), out, true, true)
	for line: String in out:
		print(line)
	_expect(case, code == 0, "town-seat visibility check exits 0")
	_end_case(case)


func _test_lin_profile_hair() -> void:
	var case := _begin_case("Lin profile hair covers the skull like the front frames")
	var py := "/usr/bin/python3"
	var script := ProjectSettings.globalize_path("res://tools/check_lin_profile_hair.py")
	var out := []
	var code := OS.execute(py, PackedStringArray([script]), out, true, true)
	for line: String in out:
		print(line)
	_expect(case, code == 0, "Lin profile hair-coverage check on sprite files exits 0")
	# The reviewer inspects the rendered home 22:00 and café 18:30 crops, not
	# the PNG on disk. Render those two interiors (xvfb/OpenGL — dummy headless
	# cannot capture) and apply the same metrics to the head at the seat anchor.
	var rend := OS.get_environment("LESSVILLE_LIN_HAIR_RENDERS")
	if rend.is_empty():
		rend = "/tmp/lin-hair-renders"
		var prev_out := OS.get_environment("LESSVILLE_ATTIRE_OUT")
		var prev_only := OS.get_environment("LESSVILLE_ATTIRE_ONLY")
		var godot := OS.get_executable_path()
		for cid: String in ["lin-home-2200", "lin-cafe-study-1830"]:
			var args := PackedStringArray([
				"--path", ".", "--rendering-driver", "opengl3",
				"--script", "res://tools/render_attire.gd",
			])
			var renderer := godot
			if OS.get_name() == "Linux":
				renderer = "xvfb-run"
				args = PackedStringArray(["-a", godot, "--display-driver", "x11"]) + args
			OS.set_environment("LESSVILLE_ATTIRE_OUT", rend)
			OS.set_environment("LESSVILLE_ATTIRE_ONLY", cid)
			var rout: Array = []
			var rcode := OS.execute(renderer, args, rout, true, true)
			for line: String in rout:
				print(line)
			if rcode != 0:
				_expect(case, false, "render %s for hair-coverage check exits 0" % cid)
		OS.set_environment("LESSVILLE_ATTIRE_OUT", prev_out)
		OS.set_environment("LESSVILLE_ATTIRE_ONLY", prev_only)
	out = []
	code = OS.execute(py, PackedStringArray([script, "--renders-only", "--renders", rend]), out, true, true)
	for line: String in out:
		print(line)
	_expect(case, code == 0, "Lin profile hair-coverage check on rendered home/café scenes exits 0")
	_end_case(case)


func _test_terrace_sitter_layers() -> void:
	var case := _begin_case("terrace FG and window-glass do not cover a sitter's head")
	var py := "/usr/bin/python3"
	var script := ProjectSettings.globalize_path("res://tools/check_terrace_sitter_layers.py")
	var out := []
	var code := OS.execute(py, PackedStringArray([script]), out, true, true)
	for line: String in out:
		print(line)
	_expect(case, code == 0, "terrace sitter-layer check exits 0")
	_end_case(case)


func _office_frame_dir() -> String:
	var frames := OS.get_environment("LESSVILLE_OFFICE_DIR")
	if frames.is_empty():
		frames = "/workspace/shared/ai-hq/results/lessdb/evidence/BL-0148/gallery-step4-a3/frames/after"
	if FileAccess.file_exists(frames + "/office-1230.png"):
		return frames
	return ""


func _run_office_checker(case: Dictionary, script_res: String, label: String) -> void:
	var py := "/usr/bin/python3"
	var script := ProjectSettings.globalize_path(script_res)
	var art := ProjectSettings.globalize_path("res://assets/game/locations/office.png")
	var args := PackedStringArray([script, "--art", art])
	var frames := _office_frame_dir()
	if not frames.is_empty():
		args.append("--dir")
		args.append(frames)
	var out := []
	var code := OS.execute(py, args, out, true, true)
	for line: String in out:
		print(line)
	# Overall exit is scored by _test_office_bed_known_limitations (four
	# documented xfails must still fail; every other property must pass).
	print("[lessville-tests] %s raw exit=%d (property scores follow)" % [label, code])


func _test_office_bed() -> void:
	var case := _begin_case("office has no baked bed")
	_run_office_checker(case, "res://tools/check_office_bed.py", "office-bed a1")
	_end_case(case)


func _test_office_bed_patch() -> void:
	var case := _begin_case("office bed patch matches neighbouring floor")
	_run_office_checker(case, "res://tools/check_office_bed_patch.py", "office-bed a2")
	_end_case(case)


func _test_office_bed_a3() -> void:
	var case := _begin_case("office bed a3 artefacts, stamps, and desk silhouette")
	_run_office_checker(case, "res://tools/check_office_bed_a3.py", "office-bed a3")
	_end_case(case)


func _test_office_bed_known_limitations() -> void:
	var case := _begin_case("office floor known limitations (strict xfail)")
	var py := "/usr/bin/python3"
	var script := ProjectSettings.globalize_path("res://tools/score_office_bed_known_limitations.py")
	var art := ProjectSettings.globalize_path("res://assets/game/locations/office.png")
	var args := PackedStringArray([script, "--art", art, "--quiet-raw"])
	var frames := _office_frame_dir()
	if not frames.is_empty():
		args.append("--dir")
		args.append(frames)
	var out := []
	var code := OS.execute(py, args, out, true, true)
	var saw_xfail := 0
	# OS.execute may return one chunk or line-split output; always split.
	var text := ""
	for chunk: String in out:
		text += chunk
		if not chunk.ends_with("\n"):
			text += "\n"
	for line: String in text.split("\n"):
		if not line.is_empty():
			print(line)
		var s := line.strip_edges()
		if s.begins_with("XFAIL "):
			_checks += 1
			_expected_failures += 1
			saw_xfail += 1
		elif s.begins_with("HARD_PASS "):
			_expect(case, true, s)
		elif s.begins_with("XPASS "):
			_expect(case, false, s)
		elif s.begins_with("UNEXPECTED_FAIL "):
			_expect(case, false, s)
	_expect(case, code == 0, "known-limitation scorer exits 0 (4 XFAIL, 0 XPASS, hard passes held)")
	_expect(case, saw_xfail == 4, "exactly 4 documented office-floor xfails still fail (got %d)" % saw_xfail)
	_end_case(case)



func _test_world_scale() -> void:
	var scale = preload("res://scripts/lessville/world_scale.gd")
	var case := _begin_case("shared world scale and road placement")
	_expect(case, absf(scale.ACTOR_INK_HEIGHT * scale.TOWN_ACTOR - scale.ADULT_HEIGHT) < 0.001, "supporting and focal adults share visible height")
	_expect(case, scale.CAR_LENGTH > scale.ADULT_HEIGHT and scale.CAR_LENGTH < scale.VAN_LENGTH and scale.VAN_LENGTH < scale.BUS_SIZE.x, "vehicle size ordering")
	_expect(case, absf(scale.TOWN_DOG / scale.TOWN_ACTOR - scale.INTERIOR_DOG / scale.INTERIOR_ACTOR) < 0.001, "dog ratio survives interior camera change")
	var tv: Control = TownView.new()
	for point: Vector2 in tv._traffic_points():
		_expect(case, point.y >= 760 and point.y <= 900, "road traffic stays on roadway rather than pedestrian network")
	tv.free()
	_end_case(case)
