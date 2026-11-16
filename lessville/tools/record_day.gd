extends SceneTree
## Records the live service's next 1,440 minutes. No recorded or fabricated state.

var output := ""
var base := ""
var snapshots: FileAccess
var last_minute := -1


func _initialize() -> void:
	call_deferred("_record")


func _request(path: String, body: Variant = null) -> Dictionary:
	var request := HTTPRequest.new()
	request.timeout = 30.0
	root.add_child(request)
	var error: int
	if body == null:
		error = request.request(base + path)
	else:
		error = request.request(base + path, ["Content-Type: application/json"], HTTPClient.METHOD_POST, JSON.stringify(body))
	if error != OK:
		request.queue_free()
		return {}
	var response: Array = await request.request_completed
	request.queue_free()
	if int(response[0]) != HTTPRequest.RESULT_SUCCESS or int(response[1]) != 200:
		return {}
	var parsed: Variant = JSON.parse_string((response[3] as PackedByteArray).get_string_from_utf8())
	return parsed if parsed is Dictionary else {}


func _record() -> void:
	output = OS.get_environment("LESSVILLE_RECORD_OUT")
	base = OS.get_environment("LESSVILLE_API")
	if output.is_empty() or base.is_empty():
		push_error("Set LESSVILLE_RECORD_OUT and LESSVILLE_API to the local run.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(output + "/frames")
	for attempt in 60:
		var ready: Dictionary = await _request("/api/state")
		if ready.get("population", {}).get("total", 0) == 100: break
		await create_timer(1.0).timeout
	var pause: Dictionary = await _request("/api/control", {"action": "pause"})
	var initial: Dictionary = await _request("/api/state")
	if not pause.get("ok", false) or initial.get("population", {}).get("total", 0) != 100:
		push_error("Recording requires a healthy 100-resident local world.")
		quit(1)
		return
	var start := int(initial["clock"]["minute"])
	var target := start + 1440
	var main: Control = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await process_frame
	main._enter_game()
	main._set_tab("mind")
	snapshots = FileAccess.open(output + "/snapshots.jsonl", FileAccess.WRITE)
	main.api.state_received.connect(func(state: Dictionary) -> void:
		var minute := int(state.get("clock", {}).get("minute", -1))
		if minute != last_minute:
			snapshots.store_line(JSON.stringify(state))
			last_minute = minute
	)
	var started: Dictionary = await _request("/api/control", {"action": "run_day"})
	if not started.get("ok", false):
		push_error("Could not start the day run.")
		quit(1)
		return
	var timeline := FileAccess.open(output + "/frames.jsonl", FileAccess.WRITE)
	var presentation := FileAccess.open(output + "/presentation.jsonl", FileAccess.WRITE)
	var frame := 0
	var start_ms := Time.get_ticks_msec()
	var selected := -1
	var ids := ["lin", "sam", "ama", "maya", "bo"]
	while Time.get_ticks_msec() - start_ms < 1800000:
		var elapsed := float(Time.get_ticks_msec() - start_ms) / 1000.0
		var index := int(elapsed / 20.0) % ids.size()
		if index != selected:
			selected = index
			main._select_agent(ids[index])
		await process_frame
		RenderingServer.force_draw()
		var picture := root.get_texture().get_image()
		var filename := "frames/%06d.png" % frame
		if picture.save_png(output + "/" + filename) != OK:
			push_error("Could not save recording frame.")
			quit(1)
			return
		var minute := int(main.state.get("clock", {}).get("minute", start))
		timeline.store_line(JSON.stringify({"file": filename, "elapsed": elapsed, "minute": minute}))
		var walkers := []
		for group: Dictionary in [main.town._vis, main.town._bg_vis]:
			for id: String in group:
				var pose: Dictionary = group[id]
				if float(pose.get("alpha", 1.0)) >= 0.5:
					var pos: Vector2 = pose["pos"]
					walkers.append({"id":id,"x":pos.x,"y":pos.y})
		presentation.store_line(JSON.stringify({"elapsed":elapsed,"minute":minute,"road":main.town._road.poses.values(),"walkers":walkers,"animals":main.town.TownAnimals.poses(main.town.sim_time())}))
		timeline.flush()
		frame += 1
		if minute >= target and bool(main.state.get("clock", {}).get("paused", false)):
			var summary := {"start_minute": start, "end_minute": minute, "simulated_minutes": minute - start,
				"frames": frame, "wall_seconds": elapsed, "population": main.state.get("population"),
				"final_state": main.state, "renderer": RenderingServer.get_video_adapter_name()}
			FileAccess.open(output + "/run.json", FileAccess.WRITE).store_string(JSON.stringify(summary, "  "))
			snapshots.close()
			timeline.close()
			presentation.close()
			print("[day-run] %d simulated minutes, %d frames, %.1f wall seconds" % [minute - start, frame, elapsed])
			main.queue_free()
			await process_frame
			quit(0 if minute == target else 1)
			return
		await create_timer(0.05).timeout
	push_error("Day recording timed out before completion.")
	await _request("/api/control", {"action": "pause"})
	quit(1)
