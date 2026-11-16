extends Node
## Talks to lessville-agentd, the native service that runs the agents on embedded LessDB.
## Godot never fabricates world state: everything shown comes from these responses.

signal state_received(state: Dictionary)
signal connection_changed(online: bool, message: String)
signal reply(kind: String, ok: bool, data: Dictionary)

const CONFIG_PATH := "res://config/lessville.json"
const POLL_SECONDS := 0.4

var base_url := "http://127.0.0.1:8790"
var online := false
var last_state: Dictionary = {}
var polling := false
var _poller: HTTPRequest
var _poll_busy := false
var _poll_timer := 0.0
var _failures := 0


func _ready() -> void:
	base_url = _resolve_base_url()
	_poller = HTTPRequest.new()
	_poller.timeout = 6.0
	add_child(_poller)
	_poller.request_completed.connect(_on_poll_done)


func _resolve_base_url() -> String:
	var url := base_url
	if FileAccess.file_exists(CONFIG_PATH):
		var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(CONFIG_PATH))
		if parsed is Dictionary and String(parsed.get("api_base", "")) != "":
			url = String(parsed["api_base"])
	if OS.has_feature("web"):
		var override: Variant = JavaScriptBridge.eval("(new URLSearchParams(window.location.search)).get('api') || window.LESSVILLE_API || ''", true)
		if override is String and String(override).begins_with("http"):
			url = String(override)
		else:
			# Deployed behind the Cloudflare Worker: the API is same-origin (/api/*).
			var host: Variant = JavaScriptBridge.eval("window.location.hostname", true)
			if host is String and (String(host).ends_with("lessdb.dev") or String(host).ends_with(".workers.dev")):
				var origin: Variant = JavaScriptBridge.eval("window.location.origin", true)
				if origin is String:
					url = String(origin)
	var env := OS.get_environment("LESSVILLE_API")
	if env != "":
		url = env
	return url.rstrip("/")


func start_polling() -> void:
	polling = true
	_poll_timer = 0.0


func stop_polling() -> void:
	polling = false


func _process(delta: float) -> void:
	if not polling or _poll_busy:
		return
	_poll_timer -= delta
	if _poll_timer <= 0.0:
		_poll_timer = POLL_SECONDS
		_poll_busy = true
		if _poller.request(base_url + "/api/state") != OK:
			_poll_busy = false
			_mark_failure("Could not start request")


func _on_poll_done(result: int, code: int, _headers: PackedStringArray, body: PackedByteArray) -> void:
	_poll_busy = false
	if result != HTTPRequest.RESULT_SUCCESS or code != 200:
		_mark_failure("LessDB agent service unreachable (%s)" % (str(code) if code else "no response"))
		return
	var parsed: Variant = JSON.parse_string(body.get_string_from_utf8())
	if not (parsed is Dictionary) or not parsed.has("clock"):
		_mark_failure("Unexpected response from service")
		return
	_failures = 0
	if not online:
		online = true
		connection_changed.emit(true, "Connected to LessDB agent service")
	last_state = parsed
	state_received.emit(parsed)


func _mark_failure(message: String) -> void:
	_failures += 1
	_poll_timer = minf(4.0, 0.6 * _failures)
	if online or _failures == 1:
		online = false
		connection_changed.emit(false, message)


## One-shot GET/POST; result arrives through `reply(kind, ok, data)`.
func call_api(kind: String, path: String, body: Variant = null) -> void:
	var req := HTTPRequest.new()
	req.timeout = 10.0
	add_child(req)
	req.request_completed.connect(func(result: int, code: int, _h: PackedStringArray, raw: PackedByteArray) -> void:
		var data: Variant = JSON.parse_string(raw.get_string_from_utf8()) if raw.size() > 0 else {}
		if not (data is Dictionary):
			data = {"error": "Unexpected response"}
		var ok := result == HTTPRequest.RESULT_SUCCESS and code >= 200 and code < 300
		if not ok and not data.has("error"):
			data["error"] = "Request failed (%s)" % code
		reply.emit(kind, ok, data)
		req.queue_free()
	)
	var err: int
	if body == null:
		err = req.request(base_url + path)
	else:
		err = req.request(base_url + path, PackedStringArray(["Content-Type: application/json"]), HTTPClient.METHOD_POST, JSON.stringify(body))
	if err != OK:
		reply.emit(kind, false, {"error": "Could not start request"})
		req.queue_free()


func control(action: String, speed: int = 0) -> void:
	var body := {"action": action}
	if speed > 0:
		body["speed"] = speed
	call_api("control", "/api/control", body)


func nudge(kind: String) -> void:
	call_api("nudge", "/api/nudge", {"kind": kind})
