extends SceneTree
## Every agent at a Town seat (park bench, bus-stop bench, café terrace
## tables) must get a seated pose at the step-1 catalog anchor.
## Fails on df088d3 for cafe_table because agent_pose only seats park / bus_stop.

const TownView = preload("res://scripts/lessville/town_view.gd")
const SeatCatalog = preload("res://scripts/lessville/seat_catalog.gd")

const MAX_POS_ERR := 2.0


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	SeatCatalog.reload()
	var tv: Control = TownView.new()
	tv.size = Vector2(1536, 1024)
	root.add_child(tv)
	await process_frame

	var fails: PackedStringArray = PackedStringArray()
	var checks := 0

	# All catalog seats at location==town, grouped by station so slot indices match feet_for_station.
	var by_station := {}
	for s: Variant in SeatCatalog.seats():
		if not (s is Dictionary):
			continue
		var seat: Dictionary = s
		if String(seat.get("location")) != "town":
			continue
		var st := String(seat.get("station"))
		if not by_station.has(st):
			by_station[st] = []
		(by_station[st] as Array).append(seat)

	for st: String in by_station.keys():
		var seats: Array = by_station[st]
		var agents: Array = []
		for i in seats.size():
			var seat: Dictionary = seats[i]
			agents.append(_agent_for(seat, i))
		var clock := {"minute": 2190, "day": 2, "minute_of_day": 750, "label": "12:30", "paused": true, "speed": 1}
		tv.set_state({"clock": clock, "agents": agents, "background": [], "bus": {}})
		tv.queue_redraw()
		await process_frame
		await process_frame

		for i in seats.size():
			var seat: Dictionary = seats[i]
			var agent: Dictionary = agents[i]
			var placed: Dictionary = SeatCatalog.feet_for_station("town", st, i)
			checks += 1
			var pose: Dictionary = tv.agent_pose(agent, i, 2190.0)
			var sid := String(seat.get("id"))
			if pose.is_empty():
				fails.append("%s: agent_pose is empty (agent not drawn)" % sid)
				print("[terrace-visible] FAIL %s pose=empty expected feet=%s facing=%s" % [
					sid, placed.get("feet"), placed.get("facing")
				])
				continue
			if not bool(pose.get("seated", false)):
				fails.append("%s: pose is not seated" % sid)
			var pos: Vector2 = pose.get("pos", Vector2.INF)
			var feet: Vector2 = placed.get("feet", Vector2.ZERO)
			var dist := pos.distance_to(feet)
			if dist > MAX_POS_ERR:
				fails.append("%s: feet at %s, expected step-1 anchor %s (err=%.2f)" % [sid, pos, feet, dist])
			var facing := String(pose.get("facing", ""))
			var want_face := String(placed.get("facing", seat.get("facing", "")))
			if want_face != "" and facing != want_face:
				fails.append("%s: facing=%s expected %s" % [sid, facing, want_face])
			print("[terrace-visible] %s pose_ok=%s seated=%s pos=%s feet=%s dist=%.2f facing=%s want=%s" % [
				sid, not pose.is_empty(), bool(pose.get("seated", false)), pos, feet, dist, facing, want_face
			])

	# Explicit maya-terrace-1230 case from the attire model (one sitter at cafe_table → slot 0 / table_a).
	checks += 1
	var maya := {
		"id": "maya", "name": "Maya", "location": "town", "station": "cafe_table",
		"activity": "On the café terrace", "pose": "seat", "travel": null,
	}
	var clock2 := {"minute": 2190, "day": 2, "minute_of_day": 750, "label": "12:30", "paused": true, "speed": 1}
	tv.set_state({"clock": clock2, "agents": [maya], "background": [], "bus": {}})
	tv.queue_redraw()
	await process_frame
	await process_frame
	var placed_a: Dictionary = SeatCatalog.feet_for_station("town", "cafe_table", 0)
	var pose_m: Dictionary = tv.agent_pose(maya, 0, 2190.0)
	if pose_m.is_empty() or not bool(pose_m.get("seated", false)):
		fails.append("maya-terrace-1230: Maya is not drawn seated at town.cafe.table_a")
		print("[terrace-visible] FAIL maya-terrace-1230 pose=%s" % pose_m)
	else:
		var dist_m: float = (pose_m.get("pos", Vector2.INF) as Vector2).distance_to(placed_a.get("feet", Vector2.ZERO))
		var face_m := String(pose_m.get("facing", ""))
		if dist_m > MAX_POS_ERR or face_m != String(placed_a.get("facing", "e")):
			fails.append("maya-terrace-1230: pos err=%.2f facing=%s expected feet=%s facing=%s" % [
				dist_m, face_m, placed_a.get("feet"), placed_a.get("facing")
			])
		print("[terrace-visible] maya-terrace-1230 seated=%s pos=%s dist=%.2f facing=%s" % [
			bool(pose_m.get("seated", false)), pose_m.get("pos"), dist_m, face_m
		])

	tv.queue_free()
	if fails.is_empty():
		print("PASS: %d town-seat visibility checks (anchors, facing, seated pose)" % checks)
		quit(0)
	else:
		print("FAIL: %d of %d town-seat visibility checks" % [fails.size(), checks])
		for f in fails:
			print("  - " + f)
		quit(1)


func _agent_for(seat: Dictionary, index: int) -> Dictionary:
	var st := String(seat.get("station"))
	var loc := "town"
	if st == "park_bench":
		loc = "park"
	elif st == "bus_bench":
		loc = "bus_stop"
	return {
		"id": "maya" if index == 0 else "sam",
		"name": "Maya" if index == 0 else "Sam",
		"location": loc,
		"station": st,
		"activity": "Sitting",
		"pose": "seat",
		"travel": null,
	}
