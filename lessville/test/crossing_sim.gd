extends RefCounted
## BL-0148 step 2b / BL-0167 job 1: seeded dense-crossing simulation.
##
## Drives the real TownView walker code (_update_walkers / _update_background)
## with named agents and background pedestrians whose destinations are biased
## to the far side of a zebra (trail S <-> everywhere north), with seeded
## mid-route destination flips.  Every tick it measures:
##   * crossing overlap: two visible walkers whose footprint circles intersect
##     while either foot is inside a crossing polygon or within ENTRY px of one
##     (footprint radius: named 20 px, background 20 px, map px; see
##     ROOT-CAUSE-step2b.md for the 40 px named-pair floor);
##   * facing error: angle between drawn heading and the actual tick step
##     (and the path tangent when a path exists), moving walkers only;
##   * off-nav: feet outside every sidewalk/crossing polygon (2 px eps);
##   * wait: continuous time a visible walker has a path but does not move.
## After the flip phase a settle phase (no flips) checks every walker reaches
## its last goal.  Pure measurement: it never moves a walker itself.
## BL-0167 job 1 sub-gates (one per sub-job, each fails on loop/base 2213eab):
##   * 1a arc-length follower: every visible walker with a path carries
##     "poly" (Array of Vector2, >= 2 points, its path polyline) and "s"
##     (arc length along it, float); feet == P(s) within 0.5 px; s never
##     decreases while the polyline is unchanged.
##   * 1b follow gap: two moving walkers headed to the same next waypoint in
##     the same direction (heading dot > 0.7), both outside every crossing
##     zone, never overlap footprints (shared-segment gap keeping).

const R_NAMED := 20.0
const R_BG := 20.0
const ENTRY := 24.0
const PLACES := ["park", "bus_stop", "cafe", "home", "office", "grocery", "trail", "transit"]
const NORTH := ["park", "bus_stop", "cafe", "home", "office", "grocery", "transit"]


static func run(tv: Control, cfg: Dictionary) -> Dictionary:
	var seed := int(cfg.get("seed", 1))
	var sim_s_total := float(cfg.get("seconds", 1200.0))
	var settle_s := float(cfg.get("settle", 240.0))
	var dt := float(cfg.get("dt", 1.0 / 30.0))
	var n_bg := int(cfg.get("background", 10))
	var flip_min := float(cfg.get("flip_min", 6.0))
	var flip_max := float(cfg.get("flip_max", 30.0))
	var start_minute := float(cfg.get("minute", 480.0))
	var rng := RandomNumberGenerator.new()
	rng.seed = seed
	var crossings: Array = []
	for e: Dictionary in tv.nav_polygons():
		if String(e["kind"]) == "crossing":
			crossings.append(e)
	var ids := ["maya", "sam", "lin", "ama", "bo"]
	var agent_loc := {}
	var agents: Array = []
	for i in ids.size():
		var loc: String = "trail" if i % 2 == 0 else NORTH[rng.randi() % NORTH.size()]
		agent_loc[ids[i]] = loc
		agents.append({"id": ids[i], "name": ids[i], "location": loc, "station": "street", "travel": null})
	var people: Array = []
	for i in n_bg:
		var loc2: String = "trail" if i % 2 == 0 else NORTH[rng.randi() % NORTH.size()]
		people.append({"id": "bg%d" % i, "location": loc2, "target": _far(loc2, rng)})
	var st := {"clock": {"minute": start_minute, "paused": false, "speed": 1}, "agents": agents, "background": people}
	tv.set_state(st)
	var next_flip := {}
	for id: String in ids:
		next_flip[id] = rng.randf_range(0.5, 3.0)
	for p: Dictionary in people:
		next_flip[String(p["id"])] = rng.randf_range(flip_min, flip_max)
	var res := {
		"seed": seed, "seconds": sim_s_total, "settle": settle_s, "dt": dt, "walkers": ids.size() + n_bg,
		"ticks": 0, "overlap_ticks": 0, "overlap_pairs": 0, "min_sep_margin": INF, "min_sep_pair": "",
		"min_sep_at": "", "per_crossing": {}, "crossing_visits": 0, "max_facing_err_deg": 0.0,
		"facing_bad_ticks": 0, "max_tangent_err_deg": 0.0, "off_nav": 0, "first_off_nav": "",
		"max_wait_s": 0.0, "max_wait_id": "", "unreached": [], "flips": 0, "moving_ticks": 0, "busy": [],
		"arc_checked": 0, "arc_missing_ticks": 0, "arc_bad_ticks": 0, "arc_max_err_px": 0.0, "arc_s_back": 0,
		"first_arc_bad": "", "follow_pairs": 0, "follow_overlap_ticks": 0, "min_follow_margin": 9999.0, "follow_pair_at": "",
		"first_zebra_overlap": {},
	}
	for c: Dictionary in crossings:
		res["per_crossing"][String(c["name"])] = {"overlap_ticks": 0, "min_margin": INF, "visits": 0}
	var prev := {}
	var wait := {}
	var prev_n := {}
	var last_busy := {}
	var in_cross_prev := {}
	var prev_arc := {}
	var total_ticks := int((sim_s_total + settle_s) / dt)
	var flip_ticks := int(sim_s_total / dt)
	for f in total_ticks:
		var t := float(f) * dt
		var changed := false
		if f < flip_ticks:
			for i in ids.size():
				var id: String = ids[i]
				if t >= float(next_flip[id]):
					next_flip[id] = t + rng.randf_range(flip_min, flip_max)
					var frm: String = agent_loc[id]
					var to := _far(frm, rng)
					agent_loc[id] = to
					agents[i] = {"id": id, "name": id, "location": "transit", "station": "street",
						"travel": {"from": frm, "to": to, "depart": start_minute + t * 2.0,
							"arrive": start_minute + t * 2.0 + 10.0, "mode": "walk"}}
					changed = true
					res["flips"] += 1
			for j in people.size():
				var pid := String(people[j]["id"])
				if t >= float(next_flip[pid]):
					next_flip[pid] = t + rng.randf_range(flip_min, flip_max)
					var cur := String(people[j]["target"])
					people[j] = {"id": pid, "location": cur, "target": _far(cur, rng)}
					changed = true
					res["flips"] += 1
		if changed:
			st["agents"] = agents
			st["background"] = people
			st["clock"]["minute"] = start_minute + t * 2.0
			tv.set_state(st)
		tv._update_walkers(dt)
		tv._update_background(dt)
		res["ticks"] += 1
		# Collect visible walkers.
		var w: Array = []
		for id: String in tv._vis:
			var v: Dictionary = tv._vis[id]
			if float(v["alpha"]) < 0.5:
				continue
			w.append({"id": id, "pos": v["pos"], "dir": v["dir"], "path": v["path"], "moving": bool(v.get("moving", false)), "r": R_NAMED,
				"poly": v.get("poly", null), "s": v.get("s", null)})
		for id: String in tv._bg_vis:
			var b: Dictionary = tv._bg_vis[id]
			if float(b.get("alpha", 1.0)) < 0.5:
				continue
			w.append({"id": id, "pos": b["pos"], "dir": b["dir"], "path": b["path"], "moving": bool(b.get("moving", false)), "r": R_BG,
				"poly": b.get("poly", null), "s": b.get("s", null)})
		for e: Dictionary in w:
			var id := String(e["id"])
			var p: Vector2 = e["pos"]
			var step: Vector2 = p - Vector2(prev.get(id, p))
			prev[id] = p
			var pn_now := (e["path"] as Array).size()
			if not tv.nav_contains(p):
				res["off_nav"] += 1
				if String(res["first_off_nav"]) == "":
					res["first_off_nav"] = "%s t=%.2fs pos=%s" % [id, t, str(p)]
			if step.length() > 0.05:
				res["moving_ticks"] += 1
				var err := rad_to_deg(acos(clampf((e["dir"] as Vector2).normalized().dot(step.normalized()), -1.0, 1.0)))
				res["max_facing_err_deg"] = maxf(float(res["max_facing_err_deg"]), err)
				if err > 11.48:
					res["facing_bad_ticks"] += 1
				var path: Array = e["path"]
				# Tangent gate only when the walker stayed on one segment this
				# tick (same remaining waypoint count), so a vertex blend is not
				# scored against the next segment.
				var same_seg := int(prev_n.get(id, -1)) == path.size()
				if same_seg and not path.is_empty() and p.distance_to(path[0]) > 1.0:
					var tan: Vector2 = ((path[0] as Vector2) - p).normalized()
					var terr := rad_to_deg(acos(clampf((e["dir"] as Vector2).normalized().dot(tan), -1.0, 1.0)))
					res["max_tangent_err_deg"] = maxf(float(res["max_tangent_err_deg"]), terr)
				wait[id] = 0.0
			elif not (e["path"] as Array).is_empty() and not bool(tv.state["clock"].get("paused", false)):
				wait[id] = float(wait.get(id, 0.0)) + dt
				if float(wait[id]) > float(res["max_wait_s"]):
					res["max_wait_s"] = float(wait[id])
					res["max_wait_id"] = "%s t=%.1fs pos=%s" % [id, t, str(p)]
			else:
				wait[id] = 0.0
			prev_n[id] = pn_now
			_arc_measure(res, prev_arc, e, t)
			var cname := _crossing_of(crossings, p, 0.0)
			if cname != "" and String(in_cross_prev.get(id, "")) != cname:
				res["per_crossing"][cname]["visits"] += 1
				res["crossing_visits"] += 1
			in_cross_prev[id] = cname
		# Busy moments (for the G4 crop gallery): >= 3 walkers on or at a zebra.
		var busy_n := {}
		for e2: Dictionary in w:
			var cb := _crossing_of(crossings, e2["pos"], ENTRY)
			if cb != "" and cb != "bus_boarding":
				busy_n[cb] = int(busy_n.get(cb, 0)) + 1
		for cb: String in busy_n:
			if int(busy_n[cb]) >= 3 and t - float(last_busy.get(cb, -99.0)) >= 3.0:
				last_busy[cb] = t
				(res["busy"] as Array).append([cb, snappedf(t, dt), int(busy_n[cb])])
		var tick_overlap := false
		for a in w.size():
			for b2 in range(a + 1, w.size()):
				var pa: Vector2 = w[a]["pos"]
				var pb: Vector2 = w[b2]["pos"]
				var cn := _crossing_of(crossings, pa, ENTRY)
				if cn == "":
					cn = _crossing_of(crossings, pb, ENTRY)
				if cn == "":
					continue
				var margin := pa.distance_to(pb) - (float(w[a]["r"]) + float(w[b2]["r"]))
				var pc: Dictionary = res["per_crossing"][cn]
				pc["min_margin"] = minf(float(pc["min_margin"]), margin)
				if margin < float(res["min_sep_margin"]):
					res["min_sep_margin"] = margin
					res["min_sep_pair"] = "%s-%s" % [w[a]["id"], w[b2]["id"]]
					res["min_sep_at"] = "%s t=%.2fs %s/%s" % [cn, t, str(pa), str(pb)]
				if margin < 0.0:
					if cn != "bus_boarding" and (res["first_zebra_overlap"] as Dictionary).is_empty():
						res["first_zebra_overlap"] = {"t": t, "a": w[a].duplicate(true), "b": w[b2].duplicate(true)}
					res["overlap_pairs"] += 1
					pc["overlap_ticks"] += 1
					tick_overlap = true
		if tick_overlap:
			res["overlap_ticks"] += 1
		_follow_measure(res, crossings, w, t)
	# Settle: every walker must have reached its last goal.
	for id: String in tv._vis:
		var v: Dictionary = tv._vis[id]
		if not (v["path"] as Array).is_empty():
			(res["unreached"] as Array).append("%s at %s" % [id, str(v["pos"])])
	for id: String in tv._bg_vis:
		var b: Dictionary = tv._bg_vis[id]
		if not (b["path"] as Array).is_empty() or bool(b.get("pending_entry", false)):
			(res["unreached"] as Array).append("%s at %s" % [id, str(b["pos"])])
	if float(res["min_sep_margin"]) == INF:
		res["min_sep_margin"] = 9999.0
	for k: String in res["per_crossing"]:
		if float(res["per_crossing"][k]["min_margin"]) == INF:
			res["per_crossing"][k]["min_margin"] = 9999.0
	return res


static func _far(frm: String, rng: RandomNumberGenerator) -> String:
	# 70% of trips cross the west zebra (trail <-> north side).
	if frm == "trail":
		return NORTH[rng.randi() % NORTH.size()]
	if rng.randf() < 0.7:
		return "trail"
	var to: String = NORTH[rng.randi() % NORTH.size()]
	return to if to != frm else "trail"


static func _crossing_of(crossings: Array, p: Vector2, grow: float) -> String:
	for c: Dictionary in crossings:
		var poly: PackedVector2Array = c["poly"]
		if Geometry2D.is_point_in_polygon(p, poly):
			return String(c["name"])
		if grow > 0.0:
			for i in poly.size():
				var q := Geometry2D.get_closest_point_to_segment(p, poly[i], poly[(i + 1) % poly.size()])
				if p.distance_to(q) <= grow:
					return String(c["name"])
	return ""


## Job 1a: arc-length state.  Polyline point at arc length s (clamped).
static func poly_point(poly: Array, s: float) -> Vector2:
	if poly.is_empty():
		return Vector2.ZERO
	var left := maxf(0.0, s)
	for i in range(1, poly.size()):
		var a: Vector2 = poly[i - 1]
		var b: Vector2 = poly[i]
		var l := a.distance_to(b)
		if left <= l:
			return a + (b - a) * (left / maxf(l, 0.000001))
		left -= l
	return poly[-1]


static func _arc_measure(res: Dictionary, prev_arc: Dictionary, e: Dictionary, t: float) -> void:
	if (e["path"] as Array).is_empty():
		prev_arc.erase(String(e["id"]))
		return
	res["arc_checked"] += 1
	var id := String(e["id"])
	var poly = e.get("poly")
	var s = e.get("s")
	if not (poly is Array) or (poly as Array).size() < 2 or not (s is float or s is int):
		res["arc_missing_ticks"] += 1
		if String(res["first_arc_bad"]) == "":
			res["first_arc_bad"] = "%s t=%.2fs: no poly/s state" % [id, t]
		return
	var p: Vector2 = e["pos"]
	var err := p.distance_to(poly_point(poly, float(s)))
	res["arc_max_err_px"] = maxf(float(res["arc_max_err_px"]), err)
	if err > 0.5:
		res["arc_bad_ticks"] += 1
		if String(res["first_arc_bad"]) == "":
			res["first_arc_bad"] = "%s t=%.2fs: feet %s != P(s=%.2f) (%.2f px)" % [id, t, str(p), float(s), err]
	var sig := "%d|%s|%s" % [(poly as Array).size(), str(poly[0]), str(poly[-1])]
	var pa: Array = prev_arc.get(id, [])
	if not pa.is_empty() and String(pa[0]) == sig and float(s) < float(pa[1]) - 0.001:
		res["arc_s_back"] += 1
		if String(res["first_arc_bad"]) == "":
			res["first_arc_bad"] = "%s t=%.2fs: s went back %.2f -> %.2f" % [id, t, float(pa[1]), float(s)]
	prev_arc[id] = [sig, float(s)]


## Job 1b: same-direction followers on a shared segment (outside crossing zones).
static func _follow_measure(res: Dictionary, crossings: Array, w: Array, t: float) -> void:
	var tick := false
	for a in w.size():
		var ea: Dictionary = w[a]
		if not bool(ea["moving"]) or (ea["path"] as Array).is_empty():
			continue
		for b in range(a + 1, w.size()):
			var eb: Dictionary = w[b]
			if not bool(eb["moving"]) or (eb["path"] as Array).is_empty():
				continue
			if (ea["path"][0] as Vector2).distance_to(eb["path"][0]) > 1.0:
				continue
			if (ea["dir"] as Vector2).normalized().dot((eb["dir"] as Vector2).normalized()) <= 0.7:
				continue
			if _crossing_of(crossings, ea["pos"], ENTRY) != "" or _crossing_of(crossings, eb["pos"], ENTRY) != "":
				continue
			res["follow_pairs"] += 1
			var margin := (ea["pos"] as Vector2).distance_to(eb["pos"]) - (float(ea["r"]) + float(eb["r"]))
			if margin < float(res["min_follow_margin"]):
				res["min_follow_margin"] = margin
				res["follow_pair_at"] = "%s-%s t=%.2fs %s/%s" % [ea["id"], eb["id"], t, str(ea["pos"]), str(eb["pos"])]
			if margin < 0.0:
				tick = true
	if tick:
		res["follow_overlap_ticks"] += 1
