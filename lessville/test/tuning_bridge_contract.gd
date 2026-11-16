extends SceneTree

var observed: Array[Dictionary] = []

func _observe(id: String, requested: Variant, active: Variant) -> void:
	var store := root.get_node("TuningStore")
	observed.append({"id": id, "requested": requested, "active": active,
		"hud": store.get_value("ui.hud.opacity"), "speed": store.get_value("player.move.max_speed")})

func _initialize() -> void:
	_run.call_deferred()

func _run() -> void:
	var bridge := root.get_node("TuningBridge")
	var store := root.get_node("TuningStore")
	store.end_run()
	store.reset_defaults()
	store.value_changed.connect(_observe)
	var initial: Dictionary = bridge.describe()
	assert(initial.controls.size() == store.get_settings().size())
	assert(initial.schemaDigest.length() == 64)
	assert(not initial.has("revision"))
	var i18n := root.get_node("I18n")
	i18n.toggle_locale()
	assert(bridge.describe().schemaDigest == initial.schemaDigest, "Locale does not invalidate a connected editor")
	i18n.toggle_locale()
	for connection: Dictionary in store.value_changed.get_connections():
		assert(connection.callable.get_object() != bridge, "Gameplay events must not notify the bridge")
	for repeat_index in range(100):
		store.apply_boundary("NEXT_ACTION")
		store.begin_run()
	assert(observed.is_empty(), "Unchanged gameplay boundaries emit nothing")
	assert(not bridge.describe().has("revision"))
	store.end_run()
	bridge.handle_request({"operation": "connect"})
	var heartbeat: Dictionary = bridge.handle_request({"operation": "heartbeat", "requestId": "heartbeat-test"})
	assert(heartbeat == {"operation": "heartbeat", "requestId": "heartbeat-test", "error": ""})
	var wrong_schema: Dictionary = bridge.handle_request({"operation": "apply", "schemaDigest": "wrong", "patch": {"ui.hud.opacity": 0.8}})
	assert(wrong_schema.error == "invalid")
	var invalid: Dictionary = bridge.handle_request({"operation": "apply", "schemaDigest": initial.schemaDigest,
		"patch": {"ui.hud.opacity": 0.8, "unknown.parameter": 10}})
	assert(invalid.error == "invalid")
	assert(store.get_requested_value("ui.hud.opacity") == initial.requested["ui.hud.opacity"])
	var fractional: Dictionary = bridge.handle_request({"operation": "apply", "schemaDigest": initial.schemaDigest,
		"patch": {"player.lives": 2.5, "ui.hud.opacity": 0.8}})
	assert(fractional.error == "invalid")
	assert(store.get_requested_value("ui.hud.opacity") == initial.requested["ui.hud.opacity"])
	assert(observed.is_empty(), "Rejected patches emit nothing")
	var valid: Dictionary = bridge.handle_request({"operation": "apply", "schemaDigest": initial.schemaDigest,
		"patch": {"ui.hud.opacity": 0.8}})
	assert(valid.error == "")
	assert(is_equal_approx(valid.state.active["ui.hud.opacity"], 0.8))
	observed.clear()
	var no_op: Dictionary = bridge.handle_request({"operation": "apply", "schemaDigest": initial.schemaDigest,
		"patch": {"ui.hud.opacity": 0.8}})
	assert(no_op.error == "")
	assert(no_op.state == valid.state, "Same values can be applied repeatedly")
	assert(observed.is_empty(), "No-op Apply emits nothing")
	assert(store.set_values({"ui.hud.opacity": 0.8}))
	var batch: Dictionary = bridge.handle_request({"operation": "apply", "schemaDigest": initial.schemaDigest,
		"patch": {"ui.hud.opacity": 0.7, "player.move.max_speed": 180.0}})
	assert(batch.error == "")
	assert(observed.size() == 2)
	for snapshot: Dictionary in observed:
		assert(is_equal_approx(snapshot.hud, 0.7) and is_equal_approx(snapshot.speed, 180.0), "Every observer sees the whole batch")
	store.set_value("player.move.max_speed", 170.0, false)
	var latest: Dictionary = bridge.handle_request({"operation": "apply", "schemaDigest": initial.schemaDigest,
		"patch": {"ui.hud.opacity": 0.9}})
	assert(latest.error == "")
	assert(is_equal_approx(store.get_active_value("ui.hud.opacity"), 0.9))
	var checked_modes: Dictionary = {}
	store.begin_run()
	assert(store.is_ranked_eligible())
	for control: Dictionary in initial.controls:
		if control.integrity == "GAMEPLAY" and control.applyMode != "LIVE" and control.type == "number" and control.default != control.min:
			var before: Variant = store.get_active_value(control.id)
			var state: Dictionary = bridge.describe()
			var deferred: Dictionary = bridge.handle_request({"operation": "apply", "schemaDigest": state.schemaDigest,
				"patch": {control.id: control.min}})
			assert(deferred.error == "")
			assert(store.get_active_value(control.id) == before)
			observed.clear()
			store.apply_boundary(control.applyMode)
			assert(observed.size() == 1)
			store.apply_boundary(control.applyMode)
			assert(observed.size() == 1, "Only the first changed boundary emits")
			assert(store.get_active_value(control.id) == control.min)
			assert(not store.is_ranked_eligible())
			store.set_value(control.id, control.default, false)
			store.apply_boundary(control.applyMode)
			assert(not store.is_ranked_eligible())
			checked_modes[control.applyMode] = true
	for mode: String in ["NEXT_ACTION", "NEXT_STAGE", "NEXT_RUN"]:
		assert(checked_modes.has(mode), "Every exposed deferred mode is exercised")
	store.end_run()
	store.reset_defaults()
	var game: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(game)
	await process_frame
	var game_id := game.get_instance_id()
	var applied: Dictionary = bridge.handle_request({"operation": "apply", "schemaDigest": initial.schemaDigest,
		"patch": {"ui.hud.opacity": 0.5, "player.lives": 5.0}})
	assert(applied.error == "")
	assert(is_equal_approx(game.get("_hud").modulate.a, 0.5), "LIVE changes reach the real HUD")
	assert(store.get_active_value("player.lives") == 3.0, "NEXT_RUN stays deferred")
	game.call("_start_new_run")
	await process_frame
	assert(game.get_instance_id() == game_id, "Apply does not replace the game instance")
	assert(game.get("_run_lives") == 5, "Next run consumes the requested lives")
	assert(not game.has_method("_open_tuning"))
	game.queue_free()
	await process_frame
	bridge.handle_request({"operation": "disconnect"})
	assert(bridge.handle_request({"operation":"apply","schemaDigest":initial.schemaDigest,"patch":{}}).error == "unavailable")
	print("Tuning request-response contract passed")
	quit(0)
