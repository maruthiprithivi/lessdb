extends Node

const SAVE_PATH := "user://game_generic_save.json"
const TEMP_PATH := "user://game_generic_save.tmp"
const SAVE_VERSION := 2
const TUTORIAL_VERSION := 1
const MAX_LEADERBOARD_ROWS := 10

var _data: Dictionary = {}


func _ready() -> void:
	_load()


func _default_data() -> Dictionary:
	return {
		"version": SAVE_VERSION,
		"locale": "",
		"tutorial_version": 0,
		"player_name": "PLAYER",
		"leaderboard": [],
	}


func _load() -> void:
	_data = _default_data()
	if not FileAccess.file_exists(SAVE_PATH):
		return
	var file := FileAccess.open(SAVE_PATH, FileAccess.READ)
	if file == null:
		return
	var parsed: Variant = JSON.parse_string(file.get_as_text())
	file.close()
	if not parsed is Dictionary:
		return
	var candidate: Dictionary = parsed
	var version := int(candidate.get("version", 0))
	if version not in [1, SAVE_VERSION]:
		return
	var locale := str(candidate.get("locale", ""))
	_data.locale = locale if locale in ["en", "zh-CN"] else ""
	_data.tutorial_version = (
		TUTORIAL_VERSION if version == 1 and bool(candidate.get("tutorial_completed", false))
		else clampi(int(candidate.get("tutorial_version", 0)), 0, TUTORIAL_VERSION)
	)
	_data.player_name = _sanitize_name(str(candidate.get("player_name", "PLAYER")))
	var clean_rows: Array = []
	var rows: Variant = candidate.get("leaderboard", [])
	if rows is Array:
		for row: Variant in rows:
			if row is Dictionary:
				var clean := _sanitize_score_row(row)
				if not clean.is_empty():
					clean_rows.append(clean)
	_data.leaderboard = clean_rows
	_sort_and_trim()


func save() -> bool:
	var file := FileAccess.open(TEMP_PATH, FileAccess.WRITE)
	if file == null:
		return false
	file.store_string(JSON.stringify(_data, "  "))
	file.close()
	var temporary := ProjectSettings.globalize_path(TEMP_PATH)
	var destination := ProjectSettings.globalize_path(SAVE_PATH)
	if FileAccess.file_exists(SAVE_PATH):
		var remove_error := DirAccess.remove_absolute(destination)
		if remove_error != OK:
			return false
	return DirAccess.rename_absolute(temporary, destination) == OK


## Returns the player's saved language, or "" if none was chosen yet (I18n then auto-detects).
func get_locale() -> String:
	return str(_data.get("locale", ""))


func set_locale(locale: String) -> void:
	if locale not in ["en", "zh-CN"]:
		return
	_data.locale = locale
	save()


func is_tutorial_completed(version: int = TUTORIAL_VERSION) -> bool:
	return int(_data.get("tutorial_version", 0)) >= version


func set_tutorial_completed(completed: bool, version: int = TUTORIAL_VERSION) -> void:
	_data.tutorial_version = clampi(version, 0, TUTORIAL_VERSION) if completed else 0
	save()


func get_player_name() -> String:
	return str(_data.get("player_name", "PLAYER"))


func record_result(player_name: String, result: Dictionary) -> bool:
	if not result.has("score") or not result.has("outcome"):
		return false
	var clean_name := _sanitize_name(player_name)
	_data.player_name = clean_name
	var rows: Array = _data.get("leaderboard", [])
	rows.append({
		"name": clean_name,
		"score": clampi(int(result.get("score", 0)), 0, 999999999),
		"stage": clampi(int(result.get("stage", 1)), 1, 99),
		"time": clampf(float(result.get("duration", 0.0)), 0.0, 99999.0),
		"outcome": str(result.get("outcome", "defeat")),
		"configuration_marker": str(result.get("configuration_marker", "cfg-unknown")).left(32),
		"ranked_eligible": bool(result.get("ranked_eligible", false)),
		"created_at": int(result.get("finalized_at", Time.get_unix_time_from_system())),
	})
	_data.leaderboard = rows
	_sort_and_trim()
	return save()


func get_leaderboard() -> Array:
	return (_data.get("leaderboard", []) as Array).duplicate(true)


func _sort_and_trim() -> void:
	var rows: Array = _data.get("leaderboard", [])
	rows.sort_custom(_score_before)
	if rows.size() > MAX_LEADERBOARD_ROWS:
		rows.resize(MAX_LEADERBOARD_ROWS)
	_data.leaderboard = rows


func _score_before(a: Dictionary, b: Dictionary) -> bool:
	var a_score := int(a.get("score", 0))
	var b_score := int(b.get("score", 0))
	if a_score != b_score:
		return a_score > b_score
	var a_time := float(a.get("time", 0.0))
	var b_time := float(b.get("time", 0.0))
	if not is_equal_approx(a_time, b_time):
		return a_time < b_time
	return int(a.get("created_at", 0)) < int(b.get("created_at", 0))


func _sanitize_name(value: String) -> String:
	var result := value.replace("\n", " ").replace("\r", " ").strip_edges()
	if result.is_empty():
		result = "PLAYER"
	return result.left(18)


func _sanitize_score_row(row: Dictionary) -> Dictionary:
	if not row.has("score") or not (row.score is int or row.score is float):
		return {}
	var legacy_completed := bool(row.get("completed", false))
	var outcome := str(row.get("outcome", "victory" if legacy_completed else "defeat"))
	if outcome not in ["victory", "defeat"]:
		outcome = "defeat"
	return {
		"name": _sanitize_name(str(row.get("name", "PLAYER"))),
		"score": clampi(int(row.get("score", 0)), 0, 999999999),
		"stage": clampi(int(row.get("stage", 1)), 1, 99),
		"time": clampf(float(row.get("time", 0.0)), 0.0, 99999.0),
		"outcome": outcome,
		"configuration_marker": str(row.get("configuration_marker", "cfg-legacy")).left(32),
		"ranked_eligible": bool(row.get("ranked_eligible", false)),
		"created_at": maxi(0, int(row.get("created_at", 0))),
	}
