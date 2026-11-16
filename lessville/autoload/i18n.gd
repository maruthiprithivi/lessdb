extends Node

signal locale_changed(locale: String)

const CATALOG_PATHS := {
	"en": "res://localization/en.json",
	"zh-CN": "res://localization/zh-CN.json",
}

var _catalogs: Dictionary = {}
var _locale := "en"


func _ready() -> void:
	for locale: String in CATALOG_PATHS:
		_catalogs[locale] = _load_catalog(str(CATALOG_PATHS[locale]))
	# A saved player choice wins; otherwise follow the OS/browser language on first launch.
	_locale = resolve_locale(SaveStore.get_locale(), OS.get_locale())
	if not _catalogs.has(_locale):
		_locale = "en"


## Map an OS/browser locale ("zh_CN", "zh-Hans-TW", "en_US", ...) to a supported catalog.
static func detect_locale(os_locale: String) -> String:
	return "zh-CN" if os_locale.strip_edges().to_lower().begins_with("zh") else "en"


## `saved` is the persisted player choice, or "" when the player never picked a language.
static func resolve_locale(saved: String, os_locale: String) -> String:
	if saved in ["en", "zh-CN"]:
		return saved
	return detect_locale(os_locale)


func _load_catalog(path: String) -> Dictionary:
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		push_error("I18n: missing catalog " + path)
		return {}
	var parsed: Variant = JSON.parse_string(file.get_as_text())
	file.close()
	if not parsed is Dictionary:
		push_error("I18n: invalid catalog " + path)
		return {}
	return parsed


func text(key: String, replacements: Dictionary = {}) -> String:
	var active: Dictionary = _catalogs.get(_locale, {})
	var fallback: Dictionary = _catalogs.get("en", {})
	var result := str(active.get(key, fallback.get(key, key)))
	for token: Variant in replacements:
		result = result.replace("{%s}" % str(token), str(replacements[token]))
	return result


func t(key: String, replacements: Dictionary = {}) -> String:
	return text(key, replacements)


func get_locale() -> String:
	return _locale


func set_locale(locale: String) -> void:
	if not _catalogs.has(locale):
		return
	# An explicit choice is persisted even when it matches the auto-detected language.
	if SaveStore.get_locale() != locale:
		SaveStore.set_locale(locale)
	if locale == _locale:
		return
	_locale = locale
	locale_changed.emit(locale)


func toggle_locale() -> void:
	set_locale("zh-CN" if _locale == "en" else "en")


func catalog_keys(locale: String) -> Array:
	var catalog: Dictionary = _catalogs.get(locale, {})
	return catalog.keys()
