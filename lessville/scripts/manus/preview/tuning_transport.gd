extends Node

# Request/response transport only. The adapter delegates to the game's existing
# parameter manager; it never owns values or subscribes to gameplay events.
var _callback: JavaScriptObject
var _window: JavaScriptObject
var _lease_until := 0

func _ready() -> void:
	if not OS.is_debug_build() or not OS.has_feature("web"):
		return
	_window = JavaScriptBridge.get_interface("window")
	if _window == null or _window.__manusRegisterGameTuning == null:
		return
	_callback = JavaScriptBridge.create_callback(_on_request)
	_window.__manusRegisterGameTuning(_callback, 2)

func _on_request(args: Array) -> void:
	if args.size() != 1 or not args[0] is String or args[0].length() > 24000:
		return
	var request: Variant = JSON.parse_string(args[0])
	if request is Dictionary:
		_window.__manusGameTuningReply(JSON.stringify(handle_request(request)))

func handle_request(request: Dictionary) -> Dictionary:
	if not OS.is_debug_build():
		return {}
	var operation := str(request.get("operation", ""))
	var reply := {"requestId": str(request.get("requestId", "")), "operation": operation, "error": ""}
	if operation in ["connect", "heartbeat"]:
		_lease_until = Time.get_ticks_msec() + 6000
		return reply
	if operation == "disconnect":
		_lease_until = 0
		return reply
	if operation not in ["describe", "read", "apply"]:
		return {}
	if not is_instance_valid(store()):
		reply.error = "unavailable"
		return reply
	var state := describe()
	if operation == "read":
		_lease_until = Time.get_ticks_msec() + 6000
	if operation == "apply":
		var patch: Variant = request.get("patch")
		if _lease_until <= Time.get_ticks_msec():
			reply.error = "unavailable"
		elif request.get("schemaDigest") != state.schemaDigest or not patch is Dictionary or patch.size() > 128:
			reply.error = "invalid"
		else:
			var by_id := {}
			for control: Dictionary in state.controls:
				by_id[control.id] = control
			for id: Variant in patch:
				if not id is String or not by_id.has(id) or not valid_value(by_id[id], patch[id]):
					reply.error = "invalid"
					break
			if reply.error.is_empty():
				if not commit(patch):
					reply.error = "invalid"
				state = describe()
			_lease_until = Time.get_ticks_msec() + 6000
	reply["state"] = state
	return reply

func describe() -> Dictionary:
	var controls: Array = []
	var schema: Array = []
	var requested_values := {}
	var active_values := {}
	for setting: Dictionary in settings():
		var control := control_for(setting)
		controls.append(control)
		# Locale changes only presentation; they must not invalidate a connected editor.
		var definition: Dictionary = control.duplicate(true)
		for field: String in ["category", "label", "description", "unit"]: definition.erase(field)
		if definition.has("options"):
			for option: Dictionary in definition.options: option.erase("label")
		schema.append(definition)
		requested_values[control.id] = wire_value(setting, requested(setting))
		active_values[control.id] = wire_value(setting, active(setting))
	return {"schemaDigest": JSON.stringify(schema).sha256_text(), "controls": controls,
		"requested": requested_values, "active": active_values}

func control_for(setting: Dictionary) -> Dictionary:
	var kind := str(setting.type)
	if kind in ["int", "integer", "float"]: kind = "number"
	if kind == "bool": kind = "boolean"
	var mode := str(setting.get("apply_mode", setting.get("mode", "LIVE"))).replace(" ", "_")
	var mode_map := {"NEXT_ATTACK": "NEXT_ACTION", "NEXT_WAVE": "NEXT_STAGE", "NEXT_BOSS": "NEXT_SPAWN",
		"NEXT_DISTRICT": "NEXT_STAGE", "NEXT_BATTLE": "NEXT_STAGE", "NEXT_DEPLOY": "NEXT_SPAWN", "NEXT_SUMMON": "NEXT_SPAWN"}
	var wire_mode: String = mode_map.get(mode, mode)
	if wire_mode not in ["LIVE", "NEXT_ACTION", "NEXT_STAGE", "NEXT_SPAWN", "NEXT_RUN"]: wire_mode = "NEXT_ACTION"
	var description := display_text(setting, "description")
	if wire_mode != mode: description += (" " if not description.is_empty() else "") + mode.replace("_", " ").capitalize() + "."
	var control := {"id": str(setting.id), "category": display_text(setting, "category"),
		"label": display_text(setting, "label", str(setting.id)),
		"description": description, "unit": display_text(setting, "unit"),
		"type": kind, "default": wire_value(setting, setting.default), "applyMode": wire_mode,
		"integrity": "COSMETIC" if str(setting.get("integrity", "GAMEPLAY")) == "COSMETIC" else "GAMEPLAY"}
	if kind == "number":
		control.merge({"min": setting.min, "max": setting.max, "step": setting.step})
	if kind == "enum":
		var options: Array = []
		for option: Variant in setting.get("options", []):
			options.append(option if option is Dictionary else {"value": option, "label": translate(str(option))})
		if setting.has("choices"):
			for index: int in setting.choices.size():
				options.append({"value": index, "label": translate(str(setting.choices[index]))})
		control["options"] = options
	return control

func display_text(setting: Dictionary, field: String, fallback := "") -> String:
	return translate(str(setting[field + "_key"])) if setting.has(field + "_key") else str(setting.get(field, fallback))

func wire_value(setting: Dictionary, value: Variant) -> Variant:
	if str(setting.type) in ["boolean", "bool"]: return bool(value)
	if value is Color: return "#" + value.to_html(true)
	return value

func valid_value(control: Dictionary, value: Variant) -> bool:
	match control.type:
		"boolean": return value is bool
		"color": return value is String and value.length() in [7, 9] and value.begins_with("#") and Color.html_is_valid(value)
		"enum":
			for option: Dictionary in control.options:
				if typeof(value) == typeof(option.value) and value == option.value: return true
				if (value is int or value is float) and (option.value is int or option.value is float) and value == option.value: return true
			return false
		"number":
			if not (value is int or value is float) or not is_finite(float(value)) or value < control.min or value > control.max: return false
			if value == control.default: return true
			var ticks := (float(value) - float(control.min)) / float(control.step)
			return is_finite(ticks) and absf(ticks - roundf(ticks)) <= 0.0000001
	return false

func translate(value: String) -> String:
	var i18n := get_node_or_null("/root/I18n")
	return str(i18n.call("t", value)) if i18n != null and not value.is_empty() else value

# Implemented by each thin adapter. No template IDs or game-specific paths here.
func store() -> Variant: return null
func settings() -> Array: return []
func requested(_setting: Dictionary) -> Variant: return null
func active(_setting: Dictionary) -> Variant: return null
func commit(_patch: Dictionary) -> bool: return false
