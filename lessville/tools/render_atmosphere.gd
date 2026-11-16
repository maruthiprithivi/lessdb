extends SceneTree
## Headless Town / interior captures for BL-0148 step 2. tools/ is excluded from playable exports.
const TownView = preload("res://scripts/lessville/town_view.gd")
const InteriorView = preload("res://scripts/lessville/interior_view.gd")
const Atmosphere = preload("res://scripts/lessville/world_atmosphere.gd")

const TIMES := {
	"0730": {"label": "07:30", "minute": 450},
	"1230": {"label": "12:30", "minute": 750},
	"1830": {"label": "18:30", "minute": 1110},
	"2200": {"label": "22:00", "minute": 1320},
}


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var out_dir := OS.get_environment("LESSVILLE_ATMOS_OUT")
	if out_dir.is_empty():
		push_error("LESSVILLE_ATMOS_OUT must name an output directory")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(out_dir)
	var day := int(OS.get_environment("LESSVILLE_ATMOS_DAY")) if OS.get_environment("LESSVILLE_ATMOS_DAY") != "" else 2
	var only := OS.get_environment("LESSVILLE_ATMOS_ONLY")  # e.g. town, home, wind
	var anim := float(OS.get_environment("LESSVILLE_ATMOS_ANIM")) if OS.get_environment("LESSVILLE_ATMOS_ANIM") != "" else 0.0

	# In-game sized town at scale 1 (map pixels == image pixels) so gates apply directly.
	if only == "" or only == "town":
		# BL-0167 clock sweep: LESSVILLE_ATMOS_TIMES="0530,0800,..." (HHMM) adds
		# extra town frames; the default four are unchanged.
		var keys: Array = ["0730", "1230", "1830", "2200"]
		var extra := OS.get_environment("LESSVILLE_ATMOS_TIMES")
		var sweep := {}
		if extra != "":
			for k2: String in extra.split(",", false):
				var kk := k2.strip_edges()
				if not keys.has(kk):
					keys.append(kk)
				sweep[kk] = {"label": "%s:%s" % [kk.substr(0, 2), kk.substr(2, 2)], "minute": int(kk.substr(0, 2)) * 60 + int(kk.substr(2, 2))}
		for key: String in keys:
			var spec: Dictionary = TIMES[key] if TIMES.has(key) else sweep[key]
			var st := _state(day, int(spec["minute"]), String(spec["label"]), anim, key)
			var img := await _capture_town(st, anim)
			var path := "%s/town-%s.png" % [out_dir, key]
			_save(img, path, st)

	if only == "" or only == "interior":
		var interiors := [
			["0730", "home"], ["1230", "office"], ["1830", "cafe"], ["2200", "home"],
		]
		for row: Array in interiors:
			var key := String(row[0])
			var loc := String(row[1])
			var spec: Dictionary = TIMES[key]
			var st := _state(day, int(spec["minute"]), String(spec["label"]), anim, key)
			var img := await _capture_interior(loc, st)
			_save(img, "%s/%s-%s.png" % [out_dir, loc, key], st)

	if only == "" or only == "wind":
		# Two frames 0.40s apart at 12:30 on day 2 (scheduled wind 28 km/h).
		var spec: Dictionary = TIMES["1230"]
		for pair: Array in [[0.0, "wind-a"], [0.40, "wind-b"]]:
			var st := _state(day, int(spec["minute"]), String(spec["label"]), float(pair[0]), "1230")
			var img := await _capture_town(st, float(pair[0]))
			_save(img, "%s/%s.png" % [out_dir, String(pair[1])], st)

	print("[atmosphere-render] wrote %s" % out_dir)
	quit(0)


func _state(day: int, minute: int, label: String, anim: float, slot: String = "") -> Dictionary:
	var clock := {
		"minute": (day - 1) * 1440 + minute,
		"day": day,
		"minute_of_day": minute,
		"label": label,
		"paused": true,
		"speed": 1,
	}
	# Leave wind_kmh unset so the seeded schedule is used.
	var snap: Dictionary = Atmosphere.snapshot(clock)
	if bool(snap.get("raining", false)):
		clock["raining"] = true
	clock["weather"] = String(snap.get("weather_kind", "clear"))
	return {"clock": clock, "agents": _agents_for(slot), "background": [], "bus": {}, "_anim": anim}


func _agents_for(slot: String) -> Array:
	match slot:
		"0730":
			return [
				{"id": "ama", "name": "Ama", "location": "home", "station": "living", "activity": "Breakfast at home"},
				{"id": "sam", "name": "Sam", "location": "home", "station": "dining", "activity": "Morning coffee"},
			]
		"1230":
			return [
				{"id": "sam", "name": "Sam", "location": "office", "station": "desk", "activity": "Midday report work"},
			]
		"1830":
			return [
				{"id": "lin", "name": "Lin", "location": "cafe", "station": "table2", "activity": "Studying at a table"},
				{"id": "maya", "name": "Maya", "location": "cafe", "station": "till", "activity": "Closing the till"},
				{"id": "bo", "name": "Bo", "location": "cafe", "station": "espresso", "activity": "Last coffees"},
			]
		"2200":
			return [
				{"id": "sam", "name": "Sam", "location": "home", "station": "dining", "activity": "Late dinner"},
				{"id": "ama", "name": "Ama", "location": "home", "station": "living", "activity": "Winding down"},
				{"id": "lin", "name": "Lin", "location": "home", "station": "dining", "activity": "Back at the house"},
				{"id": "maya", "name": "Maya", "location": "park", "station": "park_bench", "pose": "seat", "activity": "Sitting by the fountain"},
				{"id": "bo", "name": "Bo", "location": "town", "station": "cafe", "activity": "Out in Town"},
			]
	return []


func _capture_town(st: Dictionary, anim: float) -> Image:
	var vp := _make_vp(Vector2i(1536, 1024))
	var tv: Control = TownView.new()
	tv.size = Vector2(1536, 1024)
	tv.reduced_motion = false
	vp.add_child(tv)
	await process_frame
	tv.set_state(st)
	if tv.has_method("set_debug_anim"):
		tv.call("set_debug_anim", anim)
	else:
		tv.set("_anim_t", anim)
	tv.queue_redraw()
	for _i in 4:
		await process_frame
	var img: Image = vp.get_texture().get_image()
	vp.queue_free()
	await process_frame
	return img


func _capture_interior(loc: String, st: Dictionary) -> Image:
	var vp := _make_vp(Vector2i(768, 512))
	var iv: Control = InteriorView.new()
	iv.size = Vector2(768, 512)
	vp.add_child(iv)
	await process_frame
	iv.state = st
	iv.open(loc)
	iv.queue_redraw()
	for _i in 20:
		await process_frame
	var img: Image = vp.get_texture().get_image()
	vp.queue_free()
	await process_frame
	return img


func _make_vp(sz: Vector2i) -> SubViewport:
	var vp := SubViewport.new()
	vp.size = sz
	vp.transparent_bg = false
	vp.disable_3d = true
	vp.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	vp.render_target_clear_mode = SubViewport.CLEAR_MODE_ALWAYS
	root.add_child(vp)
	return vp


func _save(img: Image, path: String, st: Dictionary) -> void:
	if img == null:
		push_error("no image for %s" % path)
		return
	var err := img.save_png(path)
	var clock: Dictionary = st.get("clock", {})
	var snap: Dictionary = Atmosphere.snapshot(clock)
	print("[atmosphere-render] %s %s phase=%s wind=%s dark=%.2f err=%s" % [
		path, clock.get("label"), snap.get("phase"), snap.get("wind_kmh"), snap.get("darkness"), error_string(err)
	])
