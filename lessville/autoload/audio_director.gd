extends Node

const BrowserBgmPlayer = preload("res://scripts/manus/browser_bgm_player.gd")

const MAX_SFX_VOICES := 8
const MAX_UI_VOICES := 2
# Optional resource paths. Empty means intentionally silent; add project audio
# here or call register_cue() after importing an AudioStream resource.
const CUES := {
	"music.title": {"path": "", "kind": "music"},
	"music.gameplay": {"path": "", "kind": "music"},
	"ui.confirm": {"path": "", "bus": "UI"},
	"ui.cancel": {"path": "", "bus": "UI"},
	"player.action": {"path": "", "bus": "SFX"},
	"enemy.impact": {"path": "", "bus": "SFX"},
	"score.reward": {"path": "", "bus": "SFX"},
	"game.pause": {"path": "", "bus": "UI"},
	"game.victory": {"path": "", "bus": "SFX"},
	"game.defeat": {"path": "", "bus": "SFX"},
}

var _streams: Dictionary = {}
var _music_players: Array[BrowserBgmPlayer] = []
var _sfx_players: Array[AudioStreamPlayer] = []
var _ui_players: Array[AudioStreamPlayer] = []
var _music_index := 0
var _music_cue := ""
var _unlocked := false
var _headless := false
var _paused := false
var _music_tween: Tween


func _ready() -> void:
	process_mode = Node.PROCESS_MODE_ALWAYS
	_headless = DisplayServer.get_name() == "headless"
	_ensure_buses()
	for index: int in 2:
		var player := BrowserBgmPlayer.new()
		player.name = "Music%d" % index
		player.bus = "Music"
		player.process_mode = Node.PROCESS_MODE_ALWAYS
		player.force_loop = true
		add_child(player)
		player.playback_type = AudioServer.PLAYBACK_TYPE_STREAM
		player.finished.connect(_on_music_finished.bind(player))
		_music_players.append(player)
	for index: int in MAX_SFX_VOICES:
		_sfx_players.append(_make_player("Sfx%d" % index, "SFX"))
	for index: int in MAX_UI_VOICES:
		_ui_players.append(_make_player("Ui%d" % index, "UI"))
	_load_streams()
	TuningStore.value_changed.connect(_on_tuning_changed)
	_apply_bus_settings()


func _exit_tree() -> void:
	release_streams()


func release_streams() -> void:
	stop_music(0.0)
	for player: AudioStreamPlayer in _sfx_players + _ui_players:
		player.stop()
		player.stream = null
	for stream: AudioStream in _streams.values():
		BrowserBgmPlayer.release_cached_stream(stream)
	_streams.clear()


func register_cue(cue_id: String, resource_path: String) -> bool:
	if not CUES.has(cue_id):
		return false
	# Avoid load() on empty/missing paths. Missing audio is expected.
	if resource_path.is_empty() or not ResourceLoader.exists(resource_path):
		unregister_cue(cue_id)
		return false
	var stream := ResourceLoader.load(resource_path) as AudioStream
	return register_stream(cue_id, stream)


func register_stream(cue_id: String, stream: AudioStream) -> bool:
	if not CUES.has(cue_id):
		return false
	unregister_cue(cue_id)
	if stream == null:
		return false
	_streams[cue_id] = stream
	return true


func unregister_cue(cue_id: String) -> void:
	if cue_id == _music_cue:
		stop_music(0.0)
	var old_stream: AudioStream = _streams.get(cue_id)
	if old_stream != null:
		for players in [_music_players, _sfx_players, _ui_players]:
			for player in players:
				if player.stream == old_stream:
					player.stop()
					player.stream = null
		BrowserBgmPlayer.release_cached_stream(old_stream)
	_streams.erase(cue_id)


func unlock_audio() -> void:
	# Called from a real button/input gesture; no dummy cue is required.
	_unlocked = true


func play_music(cue_id: String, crossfade_seconds: float = 0.35) -> bool:
	var stream: AudioStream = _streams.get(cue_id)
	if stream == null or CUES.get(cue_id, {}).get("kind", "") != "music":
		stop_music(0.0)
		return false
	if not _unlocked and not _headless:
		return false
	var current: BrowserBgmPlayer = _music_players[_music_index]
	if current.stream == stream and (current.playing or _headless):
		_music_cue = cue_id
		return true
	_kill_music_tween()
	var old_player := current
	_music_index = 1 - _music_index
	var next_player: BrowserBgmPlayer = _music_players[_music_index]
	next_player.stop()
	next_player.stream = stream
	next_player.volume_db = -40.0 if old_player.playing else 0.0
	_music_cue = cue_id
	if _headless:
		old_player.stop()
		old_player.stream = null
		return true
	next_player.play()
	_music_tween = create_tween().set_parallel(true)
	_music_tween.tween_property(next_player, "volume_db", 0.0, maxf(0.0, crossfade_seconds))
	if old_player.playing:
		_music_tween.tween_property(old_player, "volume_db", -40.0, maxf(0.0, crossfade_seconds))
		_music_tween.chain().tween_callback(func() -> void:
			old_player.stop()
			old_player.stream = null
		)
	return true


func stop_music(fade_seconds: float = 0.25) -> void:
	_music_cue = ""
	_kill_music_tween()
	if fade_seconds <= 0.0 or _headless:
		for player: BrowserBgmPlayer in _music_players:
			player.stop()
			player.stream = null
		return
	_music_tween = create_tween().set_parallel(true)
	for player: BrowserBgmPlayer in _music_players:
		_music_tween.tween_property(player, "volume_db", -40.0, fade_seconds)
	_music_tween.chain().tween_callback(func() -> void:
		for player: BrowserBgmPlayer in _music_players:
			player.stop()
			player.stream = null
	)


func _kill_music_tween() -> void:
	if _music_tween != null and _music_tween.is_valid():
		_music_tween.kill()
	_music_tween = null


func play_cue(cue_id: String, pitch_scale: float = 1.0) -> bool:
	var stream: AudioStream = _streams.get(cue_id)
	if stream == null or CUES.get(cue_id, {}).get("kind", "") == "music":
		return false
	if not _unlocked and not _headless:
		return false
	if _headless:
		return true
	var cue: Dictionary = CUES[cue_id]
	var players := _ui_players if str(cue.get("bus", "SFX")) == "UI" else _sfx_players
	var player := _claim_voice(players)
	player.stream = stream
	player.pitch_scale = clampf(pitch_scale, 0.75, 1.35)
	player.play()
	return true


func register_button(button: Button, cancel_cue: bool = false) -> void:
	if button == null or button.has_meta("audio_registered"):
		return
	button.set_meta("audio_registered", true)
	button.pressed.connect(func() -> void:
		unlock_audio()
		play_cue("ui.cancel" if cancel_cue else "ui.confirm")
	)


func set_game_paused(paused: bool) -> void:
	if _paused == paused:
		return
	_paused = paused
	_apply_bus_settings()
	if paused:
		play_cue("game.pause")


func get_cue_ids() -> Array:
	return CUES.keys()


func get_voice_capacity() -> Dictionary:
	return {"music": _music_players.size(), "sfx": _sfx_players.size(), "ui": _ui_players.size()}


func _make_player(player_name: String, bus_name: String) -> AudioStreamPlayer:
	var player := AudioStreamPlayer.new()
	player.name = player_name
	player.bus = bus_name
	player.process_mode = Node.PROCESS_MODE_ALWAYS
	add_child(player)
	return player


func _claim_voice(players: Array) -> AudioStreamPlayer:
	for value: Variant in players:
		var player: AudioStreamPlayer = value
		if not player.playing:
			return player
	var recycled: AudioStreamPlayer = players[0]
	recycled.stop()
	return recycled


func _load_streams() -> void:
	for cue_id: String in CUES:
		register_cue(cue_id, str(CUES[cue_id].path))


func _ensure_buses() -> void:
	for bus_name: String in ["Music", "SFX", "UI"]:
		if AudioServer.get_bus_index(bus_name) >= 0:
			continue
		AudioServer.add_bus()
		AudioServer.set_bus_name(AudioServer.bus_count - 1, bus_name)


func _apply_bus_settings() -> void:
	var mappings := {
		"Master": "audio.master.volume_db",
		"Music": "audio.music.volume_db",
		"SFX": "audio.sfx.volume_db",
		"UI": "audio.ui.volume_db",
	}
	for bus_name: String in mappings:
		var index := AudioServer.get_bus_index(bus_name)
		if index >= 0:
			var duck := 8.0 if bus_name == "Music" and _paused else 0.0
			AudioServer.set_bus_volume_db(index, float(TuningStore.get_value(mappings[bus_name], 0.0)) - duck)
	var master := AudioServer.get_bus_index("Master")
	if master >= 0:
		AudioServer.set_bus_mute(master, bool(TuningStore.get_value("audio.master.muted", false)))


func _on_tuning_changed(id: String, _requested: Variant, _active: Variant) -> void:
	if id.begins_with("audio."):
		_apply_bus_settings()


func _on_music_finished(player: BrowserBgmPlayer) -> void:
	if player == _music_players[_music_index] and not _music_cue.is_empty() and player.stream != null:
		player.play()
