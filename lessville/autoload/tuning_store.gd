extends Node

signal value_changed(id: String, requested_value: Variant, active_value: Variant)

const SCHEMA_PATH := "res://config/tuning.json"
const APPLY_MODES := ["LIVE", "NEXT_ACTION", "NEXT_STAGE", "NEXT_RUN"]
const INTEGRITY_TYPES := ["COSMETIC", "GAMEPLAY"]

var schema_version := 0
var _settings: Array = []
var _settings_by_id: Dictionary = {}
var _defaults: Dictionary = {}
var _requested_values: Dictionary = {}
var _active_values: Dictionary = {}
var _run_active := false
var _ranked_eligible := true


func _ready() -> void:
	process_mode = Node.PROCESS_MODE_ALWAYS
	if not _load_schema():
		push_error("TuningStore: tuning schema could not be loaded")
		return
	_requested_values = _defaults.duplicate(true)
	_active_values = _defaults.duplicate(true)


func _load_schema() -> bool:
	var file := FileAccess.open(SCHEMA_PATH, FileAccess.READ)
	if file == null:
		return false
	var parsed: Variant = JSON.parse_string(file.get_as_text())
	file.close()
	if not parsed is Dictionary:
		return false
	var raw_settings: Variant = parsed.get("settings", [])
	if int(parsed.get("version", 0)) <= 0 or not raw_settings is Array:
		return false
	var parsed_settings: Array = []
	var parsed_by_id: Dictionary = {}
	var parsed_defaults: Dictionary = {}
	for raw: Variant in raw_settings:
		if not raw is Dictionary:
			return false
		var setting: Dictionary = raw.duplicate(true)
		var id := str(setting.get("id", ""))
		var type := str(setting.get("type", ""))
		var category := str(setting.get("category", ""))
		var apply_mode := str(setting.get("apply_mode", ""))
		var integrity := str(setting.get("integrity", ""))
		if id.is_empty() or parsed_by_id.has(id) or type not in ["number", "boolean"]:
			return false
		if category not in ["UI", "Gameplay", "Audio", "Player", "Enemies", "Environment"]:
			return false
		if apply_mode not in APPLY_MODES or integrity not in INTEGRITY_TYPES:
			return false
		for required: String in ["label_key", "description_key", "unit_key"]:
			if str(setting.get(required, "")).is_empty():
				return false
		if type == "number":
			for required: String in ["default", "min", "max", "step", "decimals"]:
				if not _is_number(setting.get(required)):
					return false
			var minimum := float(setting.min)
			var maximum := float(setting.max)
			var default_value := float(setting.default)
			if maximum < minimum or default_value < minimum or default_value > maximum or float(setting.step) <= 0.0:
				return false
			setting.default = default_value
			parsed_defaults[id] = default_value
		else:
			if not setting.get("default") is bool:
				return false
			parsed_defaults[id] = bool(setting.default)
		parsed_settings.append(setting)
		parsed_by_id[id] = setting
	schema_version = int(parsed.version)
	_settings = parsed_settings
	_settings_by_id = parsed_by_id
	_defaults = parsed_defaults
	return true


func get_settings() -> Array:
	return _settings.duplicate(true)


func get_categories() -> Array[String]:
	var categories: Array[String] = []
	for setting: Dictionary in _settings:
		var category := str(setting.category)
		if category not in categories:
			categories.append(category)
	return categories


func get_setting(id: String) -> Dictionary:
	return (_settings_by_id.get(id, {}) as Dictionary).duplicate(true)


func get_value(id: String, fallback: Variant = null) -> Variant:
	return get_active_value(id, fallback)


func get_active_value(id: String, fallback: Variant = null) -> Variant:
	return _active_values.get(id, _defaults.get(id, fallback))


func get_requested_value(id: String, fallback: Variant = null) -> Variant:
	return _requested_values.get(id, _defaults.get(id, fallback))


func get_requested_values() -> Dictionary:
	return _requested_values.duplicate(true)


func set_value(id: String, value: Variant, persist: bool = true) -> bool:
	return set_values({id: value}, persist)


func set_values(values: Dictionary, _persist: bool = true) -> bool:
	var candidate: Dictionary = _requested_values.duplicate(true)
	for id: Variant in values:
		if not id is String:
			return false
		var clean: Variant = _clean_value(id, values[id])
		if clean == null:
			return false
		candidate[id] = clean
	if not _validate_bundle(candidate):
		return false
	var changed: Array[String] = []
	for id: String in values:
		var live := str(_settings_by_id[id].apply_mode) == "LIVE"
		if _values_equal(_requested_values[id], candidate[id]) and (not live or _values_equal(_active_values[id], candidate[id])):
			continue
		changed.append(id)
		_requested_values[id] = candidate[id]
		if live:
			_active_values[id] = candidate[id]
			_mark_gameplay_tuning_if_needed(id)
	# Observers see the complete committed bundle, never a partially applied patch.
	for id: String in changed:
		value_changed.emit(id, _requested_values[id], _active_values[id])
	return true


func reset_setting(id: String) -> bool:
	if not _defaults.has(id):
		return false
	return set_value(id, _defaults[id])


func reset_defaults() -> void:
	var changed: Array[String] = []
	for setting: Dictionary in _settings:
		var id := str(setting.id)
		var active: Variant = setting.default if str(setting.apply_mode) == "LIVE" or not _run_active else _active_values[id]
		if not _values_equal(_requested_values[id], setting.default) or not _values_equal(_active_values[id], active):
			changed.append(id)
		_requested_values[id] = setting.default
		_active_values[id] = active
	for id: String in changed:
		value_changed.emit(id, _requested_values[id], _active_values[id])


func apply_boundary(apply_mode: String) -> void:
	if apply_mode not in APPLY_MODES or apply_mode == "LIVE":
		return
	for setting: Dictionary in _settings:
		if str(setting.apply_mode) != apply_mode:
			continue
		var id := str(setting.id)
		if _values_equal(_active_values[id], _requested_values[id]):
			continue
		_active_values[id] = _requested_values[id]
		_mark_gameplay_tuning_if_needed(id)
		value_changed.emit(id, _requested_values[id], _active_values[id])


func begin_run(tutorial_run: bool = false) -> void:
	_run_active = true
	_ranked_eligible = true
	apply_boundary("NEXT_RUN")
	apply_boundary("NEXT_STAGE")
	apply_boundary("NEXT_ACTION")
	for setting: Dictionary in _settings:
		_mark_gameplay_tuning_if_needed(str(setting.id))
	if tutorial_run:
		_ranked_eligible = false


func end_run() -> void:
	_run_active = false


func is_ranked_eligible() -> bool:
	return _ranked_eligible


func pending_count() -> int:
	var count := 0
	for id: String in _requested_values:
		if not _values_equal(_requested_values[id], _active_values[id]):
			count += 1
	return count


func configuration_marker() -> String:
	var parts: Array[String] = []
	for setting: Dictionary in _settings:
		var id := str(setting.id)
		parts.append("%s=%s" % [id, str(_active_values.get(id, setting.default))])
	return "cfg-%s" % "|".join(parts).sha256_text().left(10)


func get_persisted_deltas() -> Dictionary:
	var deltas: Dictionary = {}
	for id: String in _requested_values:
		if not _values_equal(_requested_values[id], _defaults[id]):
			deltas[id] = _requested_values[id]
	return deltas


func _clean_value(id: String, value: Variant) -> Variant:
	var setting: Dictionary = _settings_by_id.get(id, {})
	if setting.is_empty():
		return null
	if str(setting.type) == "boolean":
		return bool(value) if value is bool else null
	if not _is_number(value) or not is_finite(float(value)):
		return null
	var numeric := float(value)
	if numeric < float(setting.min) or numeric > float(setting.max):
		return null
	return numeric


func _validate_bundle(candidate: Dictionary) -> bool:
	for setting: Dictionary in _settings:
		var id := str(setting.id)
		if not candidate.has(id) or _clean_value(id, candidate[id]) == null:
			return false
	# Cross-field guard: a disabled filter may retain its intensity for later, but audio
	# gains must always remain at or below unity (0 dB).
	for id: String in ["audio.master.volume_db", "audio.music.volume_db", "audio.sfx.volume_db", "audio.ui.volume_db"]:
		if float(candidate.get(id, 0.0)) > 0.0:
			return false
	return true


func _mark_gameplay_tuning_if_needed(id: String) -> void:
	if not _run_active:
		return
	var setting: Dictionary = _settings_by_id.get(id, {})
	if str(setting.get("integrity", "COSMETIC")) != "GAMEPLAY":
		return
	if not _values_equal(_active_values.get(id), _defaults.get(id)):
		_ranked_eligible = false


func _values_equal(a: Variant, b: Variant) -> bool:
	if _is_number(a) and _is_number(b):
		return is_equal_approx(float(a), float(b))
	return a == b


func _is_number(value: Variant) -> bool:
	return value is int or value is float
