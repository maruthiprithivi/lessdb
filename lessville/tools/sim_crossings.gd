extends SceneTree
## CLI: seeded dense-crossing sim -> JSON on stdout (one line prefixed
## "[crossing-sim] ").  Env: SIM_SEED, SIM_SECONDS, SIM_SETTLE, SIM_BG, SIM_MINUTE.
const TownView = preload("res://scripts/lessville/town_view.gd")
const Sim = preload("res://test/crossing_sim.gd")


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var tv: Control = TownView.new()
	tv.size = Vector2(1024, 683)
	root.add_child(tv)
	await process_frame
	tv.set_process(false)
	var cfg := {
		"seed": int(OS.get_environment("SIM_SEED") if OS.get_environment("SIM_SEED") != "" else "1"),
		"seconds": float(OS.get_environment("SIM_SECONDS") if OS.get_environment("SIM_SECONDS") != "" else "3600"),
		"settle": float(OS.get_environment("SIM_SETTLE") if OS.get_environment("SIM_SETTLE") != "" else "240"),
		"background": int(OS.get_environment("SIM_BG") if OS.get_environment("SIM_BG") != "" else "10"),
		"minute": float(OS.get_environment("SIM_MINUTE") if OS.get_environment("SIM_MINUTE") != "" else "480"),
	}
	var r: Dictionary = Sim.run(tv, cfg)
	print("[crossing-sim] " + JSON.stringify(r))
	quit(0)
