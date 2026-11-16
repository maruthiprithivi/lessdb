extends SceneTree
## Headless Town / interior captures for BL-0148 step 3 attire cases.
const TownView = preload("res://scripts/lessville/town_view.gd")
const InteriorView = preload("res://scripts/lessville/interior_view.gd")
const Atmosphere = preload("res://scripts/lessville/world_atmosphere.gd")
const Attire = preload("res://scripts/lessville/attire.gd")

var _cases: Array = []
var _out := ""


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	_out = OS.get_environment("LESSVILLE_ATTIRE_OUT")
	if _out.is_empty():
		push_error("LESSVILLE_ATTIRE_OUT must name an output directory")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	var mdl := Attire.model()
	_cases = mdl.get("cases", [])
	var only := OS.get_environment("LESSVILLE_ATTIRE_ONLY")
	var state_out := {}
	for c: Variant in _cases:
		var spec: Dictionary = c
		var cid := String(spec.get("id"))
		if only != "" and only != cid:
			continue
		var rec := await _capture_case(spec)
		state_out[cid] = rec
	var f := FileAccess.open("%s/outfit-state.json" % _out, FileAccess.WRITE)
	f.store_string(JSON.stringify({"cases": state_out}, "\t"))
	f.close()
	print("[attire-render] wrote %s (%d cases)" % [_out, state_out.size()])
	quit(0)


func _capture_case(spec: Dictionary) -> Dictionary:
	var cid := String(spec.get("id"))
	var view := String(spec.get("view", "interior"))
	var agent := _agent(spec)
	var clock := _clock(spec)
	var st := {"clock": clock, "agents": [agent], "background": [], "bus": {}}
	# Companion characters so home/cafe scenes feel inhabited for the gallery.
	if view == "interior" and String(agent.get("location")) == "home":
		if String(agent.get("id")) != "ama":
			st["agents"].append(_extra("ama", "home", "living", "Winding down", int(spec.get("minute", 0))))
		if String(agent.get("id")) != "sam":
			st["agents"].append(_extra("sam", "home", "dining", "Late dinner", int(spec.get("minute", 0))))
	if view == "interior" and String(agent.get("location")) == "cafe":
		if String(agent.get("id")) == "maya":
			st["agents"].append(_extra("bo", "cafe", "espresso", "Pulling espresso shots", int(spec.get("minute", 0)), "serving"))
		elif String(agent.get("id")) == "lin":
			st["agents"].append(_extra("maya", "cafe", "till", "Running the counter", int(spec.get("minute", 0))))
			st["agents"].append(_extra("bo", "cafe", "espresso", "Pulling espresso shots", int(spec.get("minute", 0)), "serving"))
	var rec := {"id": cid, "layers": Attire.layers_for(agent, clock), "view": view, "character": String(agent.get("id"))}
	var captured: Dictionary
	if view == "town":
		captured = await _capture_town(st, agent)
	else:
		captured = await _capture_interior(String(agent.get("location")), st, agent)
	var img: Image = captured.get("img")
	var outfit: Dictionary = captured.get("outfit", {})
	var case_dir := "%s/%s" % [_out, cid]
	DirAccess.make_dir_recursive_absolute(case_dir)
	if img == null:
		push_error("attire-render: no image for %s (need an X11/OpenGL driver, not dummy headless)" % cid)
		print("[attire-render] %s FAILED no image" % cid)
		return rec
	var view_path := "%s/view.png" % case_dir
	var err := img.save_png(view_path)
	rec["view_png"] = view_path
	if err != OK:
		push_error("attire-render: save_png %s -> %s" % [view_path, error_string(err)])
	rec["frame"] = String(outfit.get("frame", ""))
	rec["base_frame"] = String(outfit.get("base_frame", ""))
	if outfit.get("layers") is Dictionary:
		rec["layers"] = outfit["layers"]
	var crop_rect: Rect2 = outfit.get("rect", Rect2())
	if crop_rect.size.x > 4 and crop_rect.size.y > 4:
		var pad := 20.0
		var r := Rect2(crop_rect.position - Vector2(pad, pad), crop_rect.size + Vector2(pad, pad) * 2.0)
		r = r.intersection(Rect2(Vector2.ZERO, Vector2(img.get_width(), img.get_height())))
		if r.size.x > 4 and r.size.y > 4:
			var cropped := img.get_region(Rect2i(Vector2i(r.position), Vector2i(r.size)))
			cropped.resize(maxi(cropped.get_width() * 4, 8), maxi(cropped.get_height() * 4, 8), Image.INTERPOLATE_NEAREST)
			var crop_path := "%s/crop@4x.png" % case_dir
			cropped.save_png(crop_path)
			rec["crop"] = crop_path
	print("[attire-render] %s frame=%s layers=%s" % [cid, rec.get("frame"), rec.get("layers")])
	return rec


func _query_outfit(view: String, st: Dictionary, agent: Dictionary) -> Dictionary:
	# Rebuild a short-lived view just to ask debug_outfit / drawn rect.
	if view == "town":
		# The capture already discarded the view; report from the model + a fresh view.
		var tv: Control = TownView.new()
		tv.size = Vector2(1536, 1024)
		root.add_child(tv)
		tv.set_state(st)
		tv.queue_redraw()
		# Can't await here easily; the rect may be empty. Frame is still valid.
		var out: Dictionary = tv.debug_outfit(agent, {})
		# Prefer a live pose if the agent is visible.
		if tv.has_method("agent_pose"):
			var pose: Dictionary = tv.agent_pose(agent, 0, float(st["clock"].get("minute", 0)))
			if not pose.is_empty():
				out = tv.debug_outfit(agent, pose)
		if tv._drawn_agents.has(String(agent.get("id"))):
			out["rect"] = tv._drawn_agents[String(agent.get("id"))]
		tv.queue_free()
		return out
	var iv: Control = InteriorView.new()
	iv.size = Vector2(768, 512)
	root.add_child(iv)
	iv.state = st
	iv.open(String(agent.get("location")))
	# Positions are assigned in open / process. Give it a moment via the caller.
	var out2: Dictionary = iv.debug_outfit(agent)
	if iv._iv.has(String(agent.get("id"))):
		var v: Dictionary = iv._iv[String(agent.get("id"))]
		var pos: Vector2 = v.get("pos", Vector2.ZERO)
		# Interior sprites are drawn at ~1.0 scale around the hip/feet. Approximate a rect.
		out2["rect"] = Rect2(pos * (iv.size.x / 768.0) + Vector2(-40, -90), Vector2(80, 110))
	iv.queue_free()
	return out2


func _agent(spec: Dictionary) -> Dictionary:
	var a := {
		"id": String(spec.get("character")),
		"name": String(spec.get("character")).capitalize(),
		"location": String(spec.get("location")),
		"station": String(spec.get("station", "")),
		"activity": String(spec.get("activity", "")),
		"activity_kind": String(spec.get("activity_kind", "")),
		"travel": spec.get("travel"),
		"pose": "seat" if String(spec.get("station", "")).contains("bench") or String(spec.get("station", "")).contains("table") or String(spec.get("station", "")) == "dining" or String(spec.get("station", "")) == "living" else "stand",
	}
	# Town seating: park/bus_stop locations sit the agent.
	if String(a["station"]) == "park_bench":
		a["location"] = "park"
		a["pose"] = "seat"
	elif String(a["station"]) == "bus_bench":
		a["location"] = "bus_stop"
		a["pose"] = "seat"
	elif String(a["location"]) == "town" and a.get("travel") is Dictionary:
		var tr: Dictionary = a["travel"]
		# Travel windows must use the same absolute clock as state.clock.minute (day 2).
		var md := int(spec.get("minute", 0))
		var abs_min := 1440 + md
		if not tr.has("depart"):
			tr["depart"] = abs_min - 6
		if not tr.has("arrive"):
			tr["arrive"] = abs_min + 6
		a["travel"] = tr
		a["location"] = "transit"
	return a


func _extra(id: String, loc: String, station: String, act: String, minute: int, kind: String = "") -> Dictionary:
	return {"id": id, "name": id.capitalize(), "location": loc, "station": station, "activity": act, "activity_kind": kind, "travel": null, "minute": minute}


func _clock(spec: Dictionary) -> Dictionary:
	var minute := int(spec.get("minute", 0))
	var day := 2
	var clock := {
		"minute": (day - 1) * 1440 + minute,
		"day": day,
		"minute_of_day": minute,
		"label": String(spec.get("label", "")) if String(spec.get("label", "")) != "" else "%02d:%02d" % [minute / 60, minute % 60],
		"paused": true,
		"speed": 1,
	}
	var snap: Dictionary = Atmosphere.snapshot(clock)
	if bool(snap.get("raining", false)):
		clock["raining"] = true
	clock["weather"] = String(snap.get("weather_kind", "clear"))
	return clock


func _capture_town(st: Dictionary, agent: Dictionary) -> Dictionary:
	var vp := _make_vp(Vector2i(1536, 1024))
	var tv: Control = TownView.new()
	tv.size = Vector2(1536, 1024)
	tv.reduced_motion = false
	vp.add_child(tv)
	await process_frame
	tv.set_state(st)
	if tv.has_method("set_debug_anim"):
		tv.call("set_debug_anim", 0.35)
	tv.queue_redraw()
	for _i in 10:
		await process_frame
	var img: Image = vp.get_texture().get_image()
	var pose: Dictionary = tv.agent_pose(agent, 0, float(st["clock"].get("minute", 0)))
	var outfit: Dictionary = tv.debug_outfit(agent, pose)
	if tv._drawn_agents.has(String(agent.get("id"))):
		outfit["rect"] = tv._drawn_agents[String(agent.get("id"))]
	vp.queue_free()
	await process_frame
	return {"img": img, "outfit": outfit}


func _capture_interior(loc: String, st: Dictionary, agent: Dictionary) -> Dictionary:
	var vp := _make_vp(Vector2i(768, 512))
	var iv: Control = InteriorView.new()
	iv.size = Vector2(768, 512)
	vp.add_child(iv)
	await process_frame
	iv.state = st
	iv.open(loc)
	iv.queue_redraw()
	for _i in 24:
		await process_frame
	var img: Image = vp.get_texture().get_image()
	var outfit: Dictionary = iv.debug_outfit(agent)
	if iv._iv.has(String(agent.get("id"))):
		var v: Dictionary = iv._iv[String(agent.get("id"))]
		var pos: Vector2 = v.get("pos", Vector2.ZERO)
		var s := iv.size.x / 768.0
		outfit["rect"] = Rect2(pos * s + Vector2(-48, -100) * s, Vector2(96, 120) * s)
	vp.queue_free()
	await process_frame
	return {"img": img, "outfit": outfit}


func _make_vp(sz: Vector2i) -> SubViewport:
	var vp := SubViewport.new()
	vp.size = sz
	vp.transparent_bg = false
	vp.disable_3d = true
	vp.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	vp.render_target_clear_mode = SubViewport.CLEAR_MODE_ALWAYS
	root.add_child(vp)
	return vp
