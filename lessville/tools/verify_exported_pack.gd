extends SceneTree
var _failures: Array[String] = []
func _initialize() -> void:
	call_deferred("_verify")
func _verify() -> void:
	_check(String(ProjectSettings.get_setting("application/config/name")).begins_with("Lessville"), "exported identity")
	for path: String in [
		"locations/town.png", "locations/cafe.png", "locations/home.png", "locations/office.png",
		"locations/grocery.png", "locations/school.png", "locations/gym.png", "locations/trail.png",
		"characters/maya_0.png", "characters/sam_0.png", "characters/lin_0.png", "characters/ama_0.png",
		"characters/bo_0.png", "characters/mochi_0.png", "characters/sam_typing_0.png", "props/bus.png",
	]:
		_check(ResourceLoader.exists("res://assets/game/" + path), "pack resource: " + path)
	_check(FileAccess.file_exists("res://config/lessville.json"), "service config shipped")
	_check(not ResourceLoader.exists("res://test/test_suite.tscn"), "test scene excluded")
	_check(not ResourceLoader.exists("res://tools/capture_native.gd"), "native capture tool excluded")
	_check(not root.has_node("GlobalLeaderboard"), "no dormant network autoload")
	var font := load("res://assets/template/fonts/ui_regular.tres") as Font
	_check(font != null, "exported font")
	var packed := load("res://scenes/main.tscn") as PackedScene
	_check(packed != null, "exported main scene")
	if packed != null:
		var main := packed.instantiate()
		root.add_child(main)
		await process_frame
		await process_frame
		_check(bool((main.get("title_layer") as Control).visible), "pack boots to title")
		_check(not main.has_method("_maybe_capture"), "capture routing removed from runtime")
		_check(not main.has_method("_open_tuning"), "release tuning entry point excluded")
		main.call("_enter_game")
		await process_frame
		_check(bool((main.get("game_layer") as Control).visible), "pack enters the town")
		main.get("api").call("stop_polling")
		main.queue_free()
		await process_frame
	if _failures.is_empty():
		print("[exported-pack] PASS: identity, resources, fonts, exclusions, boot, town")
		quit(0)
	else:
		for failure: String in _failures:
			push_error("[exported-pack] " + failure)
		quit(1)
func _check(condition: bool, context: String) -> void:
	if not condition and context not in _failures:
		_failures.append(context)
