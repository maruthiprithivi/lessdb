extends SceneTree
const Town = preload("res://scripts/lessville/town_view.gd")
const Scale = preload("res://scripts/lessville/world_scale.gd")

class Gallery extends Town:
	func _traffic_points() -> Array:
		return [Vector2(90, 530), Vector2(950, 530)]
	func bus_position() -> Vector2:
		return Vector2(1270, 530)
	func _draw() -> void:
		draw_rect(Rect2(0, 0, 1536, 720), Color("17202c"))
		var font := get_theme_default_font()
		draw_string(font, Vector2(40, 45), "LESSVILLE · SHARED SCALE / actual renderer", HORIZONTAL_ALIGNMENT_LEFT, -1, 25, Color.WHITE)
		for y: float in [250, 530]:
			draw_line(Vector2(50, y), Vector2(1480, y), Color("5c7185"), 1)
		var pose := {"pos": Vector2(100, 250), "dir": Vector2.DOWN, "moving": false, "phase": 0.0, "alpha": 1.0}
		_draw_agent({"id":"maya", "name":"Maya", "presence":{"companion":null}}, pose, 1.0)
		for i in 8:
			_draw_background({"id":"citizen-%02d" % (i + 1)}, {"pos":Vector2(250 + i * 140,250), "dir":Vector2.DOWN, "moving":false, "alpha":1.0}, 1.0)
		_draw_dog({"pos":Vector2(1410,250), "dir":Vector2.RIGHT, "moving":false, "alpha":1.0}, 1.0)
		draw_string(font, Vector2(245, 290), "Eight resident appearances · 32 front/back walking frames · shared adult height", HORIZONTAL_ALIGNMENT_LEFT, -1, 18, Color.WHITE)

		for row in 4:
			_draw_animal({"row":row,"frame":0,"activity":"resting","pose":{"pos":Vector2(350 + row * 250,350),"flip":false}}, 1.0)
		draw_string(font, Vector2(40,350), "Cats · rabbit · sparrow", HORIZONTAL_ALIGNMENT_LEFT, -1, 18, Color.WHITE)
		_draw_traffic(1.0)
		_draw_bus(720, 1.0)
		for pair: Array in [[110,"Car"],[420,"Delivery van"],[770,"Bicycle + rider"],[1080,"Bus"]]:
			draw_string(font, Vector2(pair[0],565), pair[1], HORIZONTAL_ALIGNMENT_LEFT, -1, 18, Color.WHITE)
		draw_string(font, Vector2(40,630), "Adult visible height: 95 px · Car: 181 px · Van: 224 px · Bus: 420 px", HORIZONTAL_ALIGNMENT_LEFT, -1, 21, Color("c1d4e5"))
		draw_string(font, Vector2(40,674), "Art-space proportions. Interiors are a close-up; furniture hip anchors remain unchanged.", HORIZONTAL_ALIGNMENT_LEFT, -1, 18, Color("93a8bd"))

func _initialize() -> void:
	call_deferred("_run")
func _run() -> void:
	var art = preload("res://scripts/lessville/town_art.gd")
	var frames := art.frames(art.RESIDENTS, 4, 8)
	assert(frames.size() == 32)
	for frame: Texture2D in frames:
		assert(float(frame.get_width()) / frame.get_height() > 0.4 and float(frame.get_width()) / frame.get_height() < 0.8)
		assert(frame.get_height() > 180)
	assert(art.resident("citizen-07", false, true, 0) != art.resident("citizen-07", false, true, 1))
	assert(art.resident("citizen-07", false, false, 0) != art.resident("citizen-07", true, false, 0))
	var identities := {}
	for i in 8: identities[art.resident_index("citizen-%02d" % (i + 1))] = true
	assert(identities.size() == 8)
	assert(art.frames(art.VEHICLES, 3, 1).size() == 3)
	assert(art.frames(art.ANIMALS, 4, 4).size() == 16)
	assert(Scale.CAR_LENGTH > Scale.ADULT_HEIGHT and Scale.CAR_LENGTH < Scale.VAN_LENGTH and Scale.VAN_LENGTH < Scale.BUS_SIZE.x)
	assert(absf(Scale.TOWN_DOG / Scale.TOWN_ACTOR - Scale.INTERIOR_DOG / Scale.INTERIOR_ACTOR) < 0.001)
	var gallery := Gallery.new()
	gallery.size = Vector2(1536,720)
	root.content_scale_size = Vector2i(1536,720)
	root.size = Vector2i(1536,720)
	root.add_child(gallery)
	await process_frame
	gallery.set_process(false)
	gallery.state = {"clock":{"minute":720,"paused":true}, "traffic":[
		{"kind":"car","position":10}, {"kind":"delivery_van","position":46}, {"kind":"bicycle","position":85}], "bus":{}}
	gallery._bus["phase"] = "dwell"
	gallery.queue_redraw()
	await process_frame
	RenderingServer.force_draw()
	var output := OS.get_environment("LESSVILLE_SCALE_OUT")
	assert(not output.is_empty())
	assert(root.get_texture().get_image().save_png(output) == OK)
	print("[scale] ratios, dog close-up consistency, and native atlas: PASS")
	gallery.queue_free()
	await process_frame
	quit()
