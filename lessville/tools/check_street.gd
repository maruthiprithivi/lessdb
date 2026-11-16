extends SceneTree
const Road = preload("res://scripts/lessville/road_traffic.gd")
const Town = preload("res://scripts/lessville/town_view.gd")

func _initialize() -> void:
	var road := Road.new()
	var pair_checks := 0
	var bus_crossings := 0
	var bus_was_west := true
	for step in 18000:
		var progress := fmod(step * 0.03, 200.0)
		var returning := progress > 100.0
		var position := 200.0 - progress if returning else progress
		var raw := [
			{"id":"van", "kind":"delivery_van", "direction":"west" if returning else "east", "position":position},
			{"id":"car", "kind":"car", "direction":"west" if returning else "east", "position":position},
			{"id":"cycle", "kind":"bicycle", "direction":"west", "position":100.0 - fmod(step * 0.018, 100.0)}]
		var trip := int(step / 1200)
		var bus := {"trip":str(trip), "x":470.0 if step % 1200 < 600 else 1860.0}
		road.update(raw, bus, 1.0 / 30.0, false)
		var bus_x := float(road.poses["bus"]["x"])
		if bus_x < 0.0: bus_was_west = true
		if bus_x > 1536.0 and bus_was_west:
			bus_crossings += 1
			bus_was_west = false
		var all: Array = road.poses.values()
		for i in all.size():
			for j in range(i + 1, all.size()):
				if all[i]["direction"] != all[j]["direction"]: continue
				var gap := absf(float(all[i]["x"]) - float(all[j]["x"])) - (Road.length_for(all[i]["kind"]) + Road.length_for(all[j]["kind"])) / 2.0
				if gap < Road.GAP - 0.001:
					push_error("Traffic overlap at step %d: %s" % [step, all])
					quit(1)
					return
				pair_checks += 1
		var before: Dictionary = road.poses.duplicate(true)
		road.update(raw, bus, 1.0, true)
		if before != road.poses:
			push_error("Paused traffic changed")
			quit(1)
			return
	assert(bus_crossings >= 2, "The bus must traverse the town repeatedly, not merely avoid overlap by stopping")
	var town := Town.new()
	town._nav = town.Nav.build(town.NODES)
	for node: String in ["L", "N"]:
		var points := []
		for i in 5:
			var point: Vector2 = town._safe_point(node, town._outdoor_offset(node, i))
			assert(town.nav_contains(point))
			for other: Vector2 in points: assert(point.distance_to(other) >= 44.0)
			points.append(point)
	town.free()
	var animals = preload("res://scripts/lessville/town_animals.gd")
	assert(animals.poses(60).is_empty() and animals.poses(1300).is_empty())
	for minute in range(420, 1140):
		var living: Array = animals.poses(minute)
		assert(living.size() == 5)
		var flights := 0
		for animal: Dictionary in living:
			assert((animal["pose"]["pos"] as Vector2).y < 500.0)
			if animal["activity"] == "flying": flights += 1
		assert(flights <= 1)
	print("[street] %d bus/vehicle pair checks without overlap; %d bus crossings; pause and standing clearance: PASS" % [pair_checks, bus_crossings])
	quit()
