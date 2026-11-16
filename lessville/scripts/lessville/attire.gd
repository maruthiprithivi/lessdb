extends RefCounted
## Role-and-place outfit layers for Lessville characters (BL-0148 step 3).
## Single source of truth: res://config/attire.json (same file the Python
## checker reads). Aprons only on café staff at the café or its terrace
## during their shift. Lin's cap and backpack come off when she is settled
## indoors.

const MODEL_PATH := "res://config/attire.json"
const LAYER_KEYS := ["apron", "cap", "backpack", "towel"]

static var _model: Dictionary = {}


static func model() -> Dictionary:
	if _model.is_empty():
		var f := FileAccess.open(MODEL_PATH, FileAccess.READ)
		if f == null:
			push_error("attire: cannot read %s" % MODEL_PATH)
			return {}
		var parsed: Variant = JSON.parse_string(f.get_as_text())
		if parsed is Dictionary:
			_model = parsed
	return _model


static func reload() -> void:
	_model = {}
	model()


static func minute_of_day(clock: Dictionary, fallback: int = 0) -> int:
	if clock.has("minute_of_day"):
		return int(clock["minute_of_day"]) % 1440
	if clock.has("minute"):
		return int(clock["minute"]) % 1440
	return fallback % 1440


static func _is_terrace(station: String, mdl: Dictionary) -> bool:
	var s := station.strip_edges().to_lower()
	if s == "":
		return false
	for t: Variant in mdl.get("terrace_stations", []):
		var ts := String(t).to_lower()
		if s == ts or s.ends_with("." + ts) or ts.ends_with(s):
			return true
	if "cafe" in s and "table" in s:
		return true
	return false


static func layers_for(agent: Dictionary, clock: Dictionary = {}) -> Dictionary:
	var mdl := model()
	var cid := String(agent.get("id", agent.get("character", "")))
	var chars: Dictionary = mdl.get("characters", {})
	var layers := {}
	for k: String in LAYER_KEYS:
		layers[k] = false
	if not chars.has(cid):
		return layers
	var spec: Dictionary = chars[cid]
	var base: Dictionary = spec.get("base_layers", {})
	for k: String in LAYER_KEYS:
		layers[k] = bool(base.get(k, false))
	var loc := String(agent.get("location", ""))
	var station := String(agent.get("station", ""))
	var travel: Variant = agent.get("travel")
	var travelling := travel is Dictionary and not (travel as Dictionary).is_empty()
	var md := minute_of_day(clock, int(agent.get("minute", 0)))
	if cid == "maya" or cid == "bo":
		var shift: Dictionary = spec.get("shift", {})
		var on_shift := int(shift.get("start_minute", 0)) <= md and md <= int(shift.get("end_minute", 0))
		var at_terrace := loc == "town" and _is_terrace(station, mdl)
		if on_shift and (loc == "cafe" or at_terrace):
			var work: Dictionary = spec.get("work_layers", {})
			for k: String in work:
				if layers.has(k):
					layers[k] = bool(work[k])
	if cid == "lin":
		var indoor := false
		for p: Variant in mdl.get("indoor_settled_locations", []):
			if loc == String(p):
				indoor = true
				break
		var settled := indoor and not travelling
		if not settled:
			var commute: Dictionary = spec.get("commute_layers", {})
			for k: String in commute:
				if layers.has(k):
					layers[k] = bool(commute[k])
	return layers


static func plain_key(key: String) -> String:
	var i := key.find("_")
	if i < 0:
		return key
	return key.substr(0, i) + "_plain_" + key.substr(i + 1)


static func needs_plain(id: String, layers: Dictionary) -> bool:
	if id == "maya" or id == "bo":
		return not bool(layers.get("apron", false))
	if id == "lin":
		return (not bool(layers.get("cap", false))) or (not bool(layers.get("backpack", false)))
	return false


static func apply_key(id: String, key: String, layers: Dictionary, has_tex: Callable) -> String:
	# LESSVILLE_ATTIRE_FORCE_BASE=1 renders the baked default frames (fail-before / gallery "before").
	if OS.get_environment("LESSVILLE_ATTIRE_FORCE_BASE") == "1":
		return key
	if not needs_plain(id, layers):
		return key
	var p := plain_key(key)
	if bool(has_tex.call(p)):
		return p
	return key
