extends Node
## BGM uses a cached Web Audio buffer in browser exports. Native playback is the
## fallback; only control changes cross the bridge after the one-time PCM upload.

signal finished

const BACKEND = preload("res://scripts/manus/browser_bgm_backend.gd")
const DECODE_CHUNK_FRAMES := 16384
const MAX_BUFFER_BYTES := 256 * 1024 * 1024

class ControlMonitor extends Node:
	var target: Node

	func _process(_delta: float) -> void:
		if is_instance_valid(target):
			target.call("_sync_backend")


class EngineLifetime extends Node:
	func _exit_tree() -> void:
		JavaScriptBridge.eval("globalThis.__manusBgm?.teardown()", true)


@export var stream: AudioStream:
	set(value):
		if stream == value:
			return
		stop()
		stream = value
		_native.stream = value
		# An unsupported/oversized track does not disable browser playback for a
		# later compatible soundtrack assigned to the same logical player.
		if _browser == null:
			_browser_failed = false

@export var volume_db: float = 0.0:
	set(value):
		volume_db = value
		_native.volume_db = value
		_sync_controls()

var volume_linear: float:
	get:
		return db_to_linear(volume_db)
	set(value):
		volume_db = linear_to_db(maxf(value, 0.0))

@export var bus: StringName = &"Master":
	set(value):
		bus = value
		_native.bus = value
		_sync_controls()

@export var pitch_scale: float = 1.0:
	set(value):
		pitch_scale = maxf(value, 0.01)
		_native.pitch_scale = pitch_scale
		_sync_controls()

@export var stream_paused: bool = false:
	set(value):
		stream_paused = value
		_sync_controls()

var playing: bool:
	get:
		if _browser != null:
			return bool(_browser.playing())
		return _native.playing
	set(value):
		if value:
			play()
		else:
			stop()

@export var autoplay := false
# Exposed for callers and native regression checks. The fallback always streams.
@export var playback_type: int = AudioServer.PLAYBACK_TYPE_STREAM
# Optional-cue directors historically repeat even streams without a loop flag.
@export var force_loop := false
# Use the original resource path when a director duplicates an imported stream.
# Reuse a key only for identical PCM; loop settings remain per player.
@export var buffer_key: String = ""

var _native := AudioStreamPlayer.new()
var _engine: JavaScriptObject
var _browser: JavaScriptObject
var _browser_failed := false
var _requested_playback := false
var _last_position := 0.0
var _last_gain := -1.0
var _last_pitch := -1.0
var _last_paused := false
var _controls_initialized := false
var _warned_effects: Dictionary = {}


func _init() -> void:
	_native.name = "GodotFallback"
	_native.playback_type = AudioServer.PLAYBACK_TYPE_STREAM
	_native.finished.connect(_on_native_finished)
	add_child(_native)
	var monitor := ControlMonitor.new()
	monitor.name = "BrowserAudioControls"
	monitor.process_mode = Node.PROCESS_MODE_ALWAYS
	monitor.target = self
	add_child(monitor)


func _ready() -> void:
	_sync_controls()
	if autoplay:
		play()


## Call during loading or a transition to move the first decode out of gameplay.
## This prepares PCM only: it never starts either renderer or consumes a gesture.
func prepare() -> bool:
	if stream == null:
		return false
	if not OS.has_feature("web"):
		return true
	# An idle browser channel must not mask a native voice already in progress.
	# Retry prewarming after it stops, or use a separate stopped preload player.
	if _browser == null and (_native.playing or _requested_playback):
		return false
	if not _ensure_browser():
		return false
	if _prepare_buffer(_buffer_key()):
		return true
	_release_browser()
	return false


## Explicit cue removal can reclaim an idle buffer without flushing other songs.
## Shared buffers that are still playing/paused remain protected by the backend.
static func release_cached_stream(audio_stream: AudioStream, cache_key: String = "") -> bool:
	if audio_stream == null or not OS.has_feature("web"):
		return true
	var engine := JavaScriptBridge.get_interface("__manusBgm")
	return engine == null or bool(engine.releaseBuffer(_stream_key(audio_stream, cache_key)))


func play(from_position: float = 0.0) -> void:
	if stream == null:
		return
	_requested_playback = true
	_last_position = maxf(from_position, 0.0)
	if _ensure_browser():
		var key := _buffer_key()
		if _prepare_buffer(key):
			var loop := _loop_settings()
			_sync_controls()
			if bool(_browser.play(key, _last_position, loop.enabled, loop.begin, loop.end)):
				_native.stop()
				return
			_disable_browser()
		else:
			# Unsupported tracks and temporary cache pressure only fall back for
			# this request. A later play can reuse a healthy browser renderer.
			_release_browser()
	_native.play(_last_position)
	_sync_controls()


func stop() -> void:
	_requested_playback = false
	_last_position = 0.0
	if _browser != null:
		_browser.stop()
	_native.stop()


func seek(to_position: float) -> void:
	_last_position = maxf(to_position, 0.0)
	if _browser != null:
		_browser.seek(_last_position)
	else:
		_native.seek(_last_position)


func get_playback_position() -> float:
	if _browser != null:
		_last_position = float(_browser.position())
		return _last_position
	return _native.get_playback_position()


func is_playing() -> bool:
	return playing


func uses_browser_audio() -> bool:
	return _browser != null


func _ensure_browser() -> bool:
	if _browser_failed or not OS.has_feature("web") or not is_inside_tree():
		return false
	if _browser != null:
		if bool(_browser.healthy()):
			return true
		_disable_browser()
		return false
	JavaScriptBridge.eval(BACKEND.SOURCE, true)
	_engine = JavaScriptBridge.get_interface("__manusBgm")
	if _engine != null:
		if is_inside_tree() and not get_tree().root.has_meta("manus_browser_bgm_lifetime"):
			var lifetime := EngineLifetime.new()
			lifetime.name = "ManusBrowserBgmLifetime"
			# Players may start from a scene's _ready while the root is still
			# adding that scene. Reserve the singleton before deferred insertion.
			get_tree().root.set_meta("manus_browser_bgm_lifetime", true)
			get_tree().root.add_child.call_deferred(lifetime)
		_browser = _engine.createPlayer(str(get_instance_id()))
	if _browser == null:
		_browser_failed = true
		return false
	_controls_initialized = false
	return true


func _buffer_key() -> String:
	return _stream_key(stream, buffer_key)


static func _stream_key(audio_stream: AudioStream, cache_key: String = "") -> String:
	if not cache_key.is_empty():
		return "track:" + cache_key
	# Imported files retain one cached decode when ResourceLoader releases and
	# reloads them. Generated/subresource audio retains its distinct identity.
	var path := audio_stream.resource_path
	if not path.contains("::") and path.get_extension().to_lower() in ["ogg", "mp3", "wav"]:
		return "file:" + path
	return "stream:%d" % audio_stream.get_instance_id()


func _prepare_buffer(key: String) -> bool:
	if bool(_engine.hasBuffer(key)):
		return true
	var length := stream.get_length()
	var sample_rate := AudioServer.get_mix_rate()
	if not is_finite(length) or length <= 0.0 or sample_rate <= 0.0:
		return false
	var frame_count := int(ceil(length * sample_rate))
	if frame_count <= 0 or frame_count > MAX_BUFFER_BYTES / 8:
		return false
	# Browser buffer sources support forward loops. Preserve other WAV modes by
	# selecting Godot rather than changing the soundtrack's playback semantics.
	if stream is AudioStreamWAV:
		var mode := (stream as AudioStreamWAV).loop_mode
		if mode != AudioStreamWAV.LOOP_DISABLED and mode != AudioStreamWAV.LOOP_FORWARD:
			return false
	var decoder := stream.instantiate_playback()
	if decoder == null or not bool(_engine.beginBuffer(key, frame_count, sample_rate)):
		return false
	decoder.start(0.0)
	var offset := 0
	while offset < frame_count:
		var count := mini(DECODE_CHUNK_FRAMES, frame_count - offset)
		var samples := decoder.mix_audio(1.0, count)
		var terminal_read := not decoder.is_playing()
		if samples.size() > count or (samples.size() < count and not terminal_read):
			decoder.stop()
			_engine.cancelBuffer(key)
			return false
		if samples.is_empty():
			break
		var encoded := Marshalls.raw_to_base64(samples.to_byte_array())
		if not bool(_engine.appendBuffer(key, encoded, offset)):
			decoder.stop()
			_engine.cancelBuffer(key)
			return false
		offset += samples.size()
		if samples.size() < count:
			break
	decoder.stop()
	if offset <= 0 or not bool(_engine.finishBuffer(key, offset)):
		_engine.cancelBuffer(key)
		return false
	return true


func _loop_settings() -> Dictionary:
	var enabled := force_loop
	var begin := 0.0
	var end := stream.get_length()
	if stream is AudioStreamOggVorbis:
		var ogg := stream as AudioStreamOggVorbis
		enabled = enabled or ogg.loop
		begin = ogg.loop_offset
	elif stream is AudioStreamMP3:
		var mp3 := stream as AudioStreamMP3
		enabled = enabled or mp3.loop
		begin = mp3.loop_offset
	elif stream is AudioStreamWAV:
		var wav := stream as AudioStreamWAV
		enabled = enabled or wav.loop_mode != AudioStreamWAV.LOOP_DISABLED
		if wav.loop_mode != AudioStreamWAV.LOOP_DISABLED and wav.mix_rate > 0:
			begin = float(wav.loop_begin) / float(wav.mix_rate)
			if wav.loop_end > wav.loop_begin:
				end = float(wav.loop_end) / float(wav.mix_rate)
	return {"enabled": enabled, "begin": begin, "end": end}


func _sync_backend() -> void:
	_sync_controls()
	if _browser == null:
		return
	if not bool(_browser.healthy()):
		var resume_playback := _requested_playback
		# A failed resume/seek may occur after the last gameplay control tick.
		# Read its saved audio-clock cursor before releasing the failed channel.
		var resume_position := float(_browser.position())
		_disable_browser()
		if resume_playback and stream != null:
			_native.play(resume_position)
			_sync_controls()
		return
	if _requested_playback:
		_last_position = float(_browser.position())
		if not bool(_browser.playing()):
			_requested_playback = false
			finished.emit()


func _sync_controls() -> void:
	var paused := stream_paused or (is_inside_tree() and not can_process())
	if _native.stream_paused != paused:
		_native.stream_paused = paused
	if _browser == null:
		return
	var gain := _effective_gain()
	if not _controls_initialized or not is_equal_approx(gain, _last_gain):
		_browser.setGain(gain)
		_last_gain = gain
	if not _controls_initialized or not is_equal_approx(pitch_scale, _last_pitch):
		_browser.setPitch(pitch_scale)
		_last_pitch = pitch_scale
	if not _controls_initialized or paused != _last_paused:
		_browser.setPaused(paused)
		_last_paused = paused
	_controls_initialized = true


func _effective_gain() -> float:
	var gain := db_to_linear(volume_db)
	var index := AudioServer.get_bus_index(bus)
	if index < 0:
		index = 0
	var visited: Array[int] = []
	var route_is_solo := false
	while index >= 0 and index not in visited:
		visited.append(index)
		if AudioServer.is_bus_mute(index):
			return 0.0
		gain *= db_to_linear(AudioServer.get_bus_volume_db(index))
		if not AudioServer.is_bus_bypassing_effects(index):
			for effect_index: int in AudioServer.get_bus_effect_count(index):
				if not AudioServer.is_bus_effect_enabled(index, effect_index):
					continue
				var effect := AudioServer.get_bus_effect(index, effect_index)
				if effect is AudioEffectLimiter:
					# Godot applies this constant makeup even below its limiter
					# threshold. Preserve authored BGM loudness on this renderer.
					gain *= db_to_linear(effect.ceiling_db - effect.threshold_db)
				elif effect is AudioEffectAmplify:
					gain *= db_to_linear(effect.volume_db)
				elif effect is AudioEffectHardLimiter:
					# Only pre-gain is constant; ceiling and release require DSP.
					gain *= db_to_linear(effect.pre_gain_db)
				elif not _warned_effects.has(effect.get_instance_id()):
					_warned_effects[effect.get_instance_id()] = true
					push_warning("Browser BGM does not apply Godot bus effect %s; only volume, Amplify gain, legacy limiter makeup and HardLimiter pre-gain are mirrored, not limiting DSP." % effect.get_class())
		route_is_solo = route_is_solo or AudioServer.is_bus_solo(index)
		if index == 0:
			break
		index = AudioServer.get_bus_index(AudioServer.get_bus_send(index))
	if not route_is_solo:
		for bus_index: int in AudioServer.bus_count:
			if AudioServer.is_bus_solo(bus_index):
				return 0.0
	return gain if is_finite(gain) else 0.0


func _disable_browser() -> void:
	_release_browser()
	_browser_failed = true


func _release_browser() -> void:
	if _browser != null:
		_browser.dispose()
	_browser = null
	_engine = null
	_controls_initialized = false


func _on_native_finished() -> void:
	if _browser == null:
		_requested_playback = false
		finished.emit()


func _exit_tree() -> void:
	stop()
	_release_browser()
	_browser_failed = false
