extends SceneTree
const TownView = preload("res://scripts/lessville/town_view.gd")
const Atmosphere = preload("res://scripts/lessville/world_atmosphere.gd")
const Attire = preload("res://scripts/lessville/attire.gd")

func _out_dir() -> String:
	var d := OS.get_environment("LESSVILLE_TERRACE_OUT")
	if d.is_empty():
		d = "/workspace/shared/ai-hq/results/lessdb/evidence/BL-0148/gallery-step3-a3/terrace"
	return d

func _initialize() -> void:
	call_deferred("_run")

func _run() -> void:
	var OUT := _out_dir()
	DirAccess.make_dir_recursive_absolute(OUT)
	for rec in [
		{"tag": "maya-terrace-1230", "minute": 750, "label": "12:30"},
		{"tag": "maya-terrace-1830", "minute": 1110, "label": "18:30"},
	]:
		await _capture(rec)
	print("[terrace-render] wrote %s" % OUT)
	quit(0)

func _capture(rec: Dictionary) -> void:
	var minute := int(rec["minute"])
	var clock := {
		"minute": 1440 + minute, "day": 2, "minute_of_day": minute,
		"label": String(rec["label"]), "paused": true, "speed": 1,
	}
	var snap: Dictionary = Atmosphere.snapshot(clock)
	clock["weather"] = String(snap.get("weather_kind", "clear"))
	var maya := {
		"id": "maya", "name": "Maya", "location": "town", "station": "cafe_table",
		"activity": "On the café terrace", "pose": "seat", "travel": null,
	}
	# A second sitter so table_b is also occupied for the crop context.
	var sam := {
		"id": "sam", "name": "Sam", "location": "town", "station": "cafe_table",
		"activity": "At the terrace", "pose": "seat", "travel": null,
	}
	var st := {"clock": clock, "agents": [maya, sam], "background": [], "bus": {}}
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
	tv.queue_redraw()
	for _i in 10:
		await process_frame
	var img: Image = vp.get_texture().get_image()
	var view_path := "%s/%s-view.png" % [_out_dir(), rec["tag"]]
	img.save_png(view_path)
	# Crop around Maya if drawn.
	if tv._drawn_agents.has("maya"):
		var r: Rect2 = tv._drawn_agents["maya"]
		var pad := 24.0
		var cr := Rect2(r.position - Vector2(pad, pad), r.size + Vector2(pad, pad) * 2.0)
		cr = cr.intersection(Rect2(Vector2.ZERO, Vector2(img.get_width(), img.get_height())))
		if cr.size.x > 4 and cr.size.y > 4:
			var cropped := img.get_region(Rect2i(Vector2i(cr.position), Vector2i(cr.size)))
			cropped.resize(cropped.get_width() * 4, cropped.get_height() * 4, Image.INTERPOLATE_NEAREST)
			cropped.save_png("%s/%s@4x.png" % [_out_dir(), rec["tag"]])
			print("[terrace-render] %s crop frame=%s" % [rec["tag"], tv.debug_outfit(maya, tv.agent_pose(maya, 0, float(clock["minute"])))])
	else:
		print("[terrace-render] %s Maya not drawn" % rec["tag"])
	# Also a tighter terrace ROI around both tables.
	var roi := Rect2i(1180, 330, 320, 280)
	var terr := img.get_region(Rect2i(roi))
	terr.resize(terr.get_width() * 4, terr.get_height() * 4, Image.INTERPOLATE_NEAREST)
	terr.save_png("%s/%s-tables@4x.png" % [_out_dir(), rec["tag"]])
	vp.queue_free()
	await process_frame
