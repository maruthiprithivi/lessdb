extends SceneTree
## BL-0167 / BL-0148 step 2b G4 gallery: render the town at busy zebra moments
## of the seeded crossing sim (test/crossing_sim.gd), deterministic per seed.
## Env: CROSS_OUT (dir), CROSS_SEED, CROSS_TIMES ("t1,t2" sim seconds; default:
## first 3 busy moments in the first CROSS_SCAN s), CROSS_STRIP (frames, 0.25 s apart).
## Writes frame-<t>.png (1536x1024, map px == image px) and crossings-meta.json.
const TownView = preload("res://scripts/lessville/town_view.gd")
const Sim = preload("res://test/crossing_sim.gd")


func _initialize() -> void:
	call_deferred("_run")


func _env(k: String, d: String) -> String:
	var v := OS.get_environment(k)
	return v if v != "" else d


func _run() -> void:
	var out := _env("CROSS_OUT", "")
	if out == "":
		push_error("CROSS_OUT must name an output directory")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(out)
	var seed := int(_env("CROSS_SEED", "1"))
	var strip := int(_env("CROSS_STRIP", "6"))
	var times: Array = []
	var scan := float(_env("CROSS_SCAN", "240"))
	if _env("CROSS_TIMES", "") != "":
		for x: String in _env("CROSS_TIMES", "").split(",", false):
			times.append(["crossing_west", float(x), 0])
	else:
		var probe: Control = TownView.new()
		probe.size = Vector2(1024, 683)
		root.add_child(probe)
		await process_frame
		probe.set_process(false)
		var r: Dictionary = Sim.run(probe, {"seed": seed, "seconds": scan, "settle": 0.0})
		probe.queue_free()
		var skip := float(_env("CROSS_SKIP", "30"))
		for b: Array in r["busy"]:
			if times.size() < int(_env("CROSS_MOMENTS", "3")) and float(b[1]) >= skip:
				times.append(b)
	var meta := {"seed": seed, "moments": []}
	for m: Array in times:
		var frames: Array = []
		for k in strip:
			var t := float(m[1]) + 0.25 * float(k)
			var vp := SubViewport.new()
			vp.size = Vector2i(1536, 1024)
			vp.disable_3d = true
			vp.render_target_update_mode = SubViewport.UPDATE_ALWAYS
			root.add_child(vp)
			var tv: Control = TownView.new()
			tv.size = Vector2(1536, 1024)
			vp.add_child(tv)
			await process_frame
			tv.set_process(false)
			var r2: Dictionary = Sim.run(tv, {"seed": seed, "seconds": t, "settle": 0.0, "minute": 750.0})
			tv.state["clock"]["paused"] = true
			tv.queue_redraw()
			for _j in 3:
				await process_frame
			var path := "%s/frame-%s-%07.2f.png" % [out, String(m[0]), t]
			vp.get_texture().get_image().save_png(path)
			var people: Array = []
			for id: String in tv._vis:
				if float(tv._vis[id]["alpha"]) >= 0.5:
					people.append({"id": id, "x": tv._vis[id]["pos"].x, "y": tv._vis[id]["pos"].y, "r": Sim.R_NAMED})
			for id: String in tv._bg_vis:
				people.append({"id": id, "x": tv._bg_vis[id]["pos"].x, "y": tv._bg_vis[id]["pos"].y, "r": Sim.R_BG})
			frames.append({"t": t, "path": path, "people": people})
			vp.queue_free()
			await process_frame
		meta["moments"].append({"crossing": m[0], "t": m[1], "count": m[2], "frames": frames})
	var f := FileAccess.open("%s/crossings-meta.json" % out, FileAccess.WRITE)
	f.store_string(JSON.stringify(meta, "  "))
	f.close()
	print("[crossings-render] wrote %s (%d moments)" % [out, times.size()])
	quit(0)
