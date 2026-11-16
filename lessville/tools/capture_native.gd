extends SceneTree

# Native renderer evidence only; tools/ is excluded from every playable export.

func _initialize() -> void:
	call_deferred("_capture")


func _capture() -> void:
	var capture_path := OS.get_environment("GENERIC2D_CAPTURE_PATH")
	if capture_path.is_empty():
		push_error("GENERIC2D_CAPTURE_PATH must name an output PNG")
		quit(1)
		return
	var main: Variant = (load("res://scenes/main.tscn") as PackedScene).instantiate()
	root.add_child(main)
	await process_frame
	await process_frame
	var i18n: Variant = root.get_node("I18n")
	var saves: Variant = root.get_node("SaveStore")
	var original_locale: String = i18n.get_locale()
	var original_tutorial_state: bool = saves.is_tutorial_completed()
	if OS.get_environment("GENERIC2D_CAPTURE_ZH") == "1":
		i18n.set_locale("zh-CN")
	else:
		i18n.set_locale("en")
	if OS.get_environment("GENERIC2D_CAPTURE_PAUSE") == "1":
		saves.set_tutorial_completed(true)
		main._start_new_run()
		await process_frame
		main._toggle_pause()
	elif OS.get_environment("GENERIC2D_CAPTURE_GAME") == "1":
		main._force_tutorial = false
		if OS.get_environment("GENERIC2D_CAPTURE_TUTORIAL") == "1":
			saves.set_tutorial_completed(false)
			main._force_tutorial = true
		else:
			saves.set_tutorial_completed(true)
		main._start_new_run()
	if OS.get_environment("GENERIC2D_CAPTURE_TOUCH") == "1":
		main._set_input_method("touch")
	if OS.get_environment("GENERIC2D_CAPTURE_EFFECTS") == "1" and main._world != null:
		for cells_name: String in ["_overdrive_cells", "_shield_cells", "_slow_field_cells", "_magnet_cells"]:
			var cells: Dictionary = main._world.get(cells_name)
			main._world.call("_on_runner_entered_cell", cells.keys()[0])
		main._world.set_simulation_active(false)
	if OS.get_environment("GENERIC2D_CAPTURE_RESULT") == "1":
		main._start_new_run()
		await process_frame
		main._skip_tutorial()
		main._finish_run(true)
	if OS.get_environment("GENERIC2D_CAPTURE_LEADERBOARD") == "1":
		main._show_leaderboard()
	if OS.get_environment("GENERIC2D_CAPTURE_CONFIRM") == "1":
		main._request_destructive_action("restart")
	for _frame: int in 8:
		await process_frame
	await create_timer(0.25).timeout
	var image := root.get_texture().get_image()
	var error := image.save_png(capture_path)
	i18n.set_locale(original_locale)
	saves.set_tutorial_completed(original_tutorial_state)
	print("[capture] %s (%s)" % [capture_path, error_string(error)])
	root.get_node("AudioDirector").call("release_streams")
	await process_frame
	quit(0 if error == OK else 1)
