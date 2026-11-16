extends SceneTree
## Capture the Office interior at 12:30 and 22:00 for BL-0148 step 4.
const InteriorView = preload("res://scripts/lessville/interior_view.gd")
const Atmosphere = preload("res://scripts/lessville/world_atmosphere.gd")

func _initialize() -> void:
	call_deferred("_run")

func _run() -> void:
	var out_dir := OS.get_environment("LESSVILLE_OFFICE_OUT")
	if out_dir.is_empty():
		push_error("LESSVILLE_OFFICE_OUT must name an output directory")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(out_dir)
	var jobs := [
		["1230", 750, "12:30", [
			{"id": "sam", "name": "Sam", "location": "office", "station": "desk", "activity": "Midday report work"},
		]],
		["2200", 1320, "22:00", []],
	]
	for job: Array in jobs:
		var key := String(job[0])
		var minute := int(job[1])
		var label := String(job[2])
		var agents: Array = job[3]
		var clock := {
			"minute": 1440 + minute,
			"day": 2,
			"minute_of_day": minute,
			"label": label,
			"paused": true,
			"speed": 1,
		}
		var st := {"clock": clock, "agents": agents, "background": [], "bus": {}}
		var img := await _capture(st)
		var path := "%s/office-%s.png" % [out_dir, key]
		if img == null:
			push_error("no image for %s" % path)
			continue
		var err := img.save_png(path)
		var snap: Dictionary = Atmosphere.snapshot(clock)
		print("[office-render] %s phase=%s dark=%.2f err=%s" % [path, snap.get("phase"), snap.get("darkness"), error_string(err)])
	print("[office-render] wrote %s" % out_dir)
	quit(0)

func _capture(st: Dictionary) -> Image:
	var vp := SubViewport.new()
	vp.size = Vector2i(768, 512)
	vp.transparent_bg = false
	vp.disable_3d = true
	vp.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	vp.render_target_clear_mode = SubViewport.CLEAR_MODE_ALWAYS
	root.add_child(vp)
	var iv: Control = InteriorView.new()
	iv.size = Vector2(768, 512)
	vp.add_child(iv)
	await process_frame
	iv.state = st
	iv.open("office")
	iv.queue_redraw()
	for _i in 20:
		await process_frame
	var img: Image = vp.get_texture().get_image()
	vp.queue_free()
	await process_frame
	return img
