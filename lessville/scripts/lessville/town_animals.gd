extends RefCounted
## Clock-driven ambient animals, not autonomous need/relationship agents.
const HOMES := [
	{"id":"miso", "name":"Miso", "row":0, "a":Vector2(155, 370), "b":Vector2(205, 392), "offset":0.0},
	{"id":"pepper", "name":"Pepper", "row":1, "a":Vector2(1245, 375), "b":Vector2(1285, 390), "offset":24.0},
	{"id":"clover", "name":"Clover", "row":2, "a":Vector2(110, 388), "b":Vector2(132, 390), "offset":47.0},
]

static func poses(minute: float) -> Array:
	var md := fposmod(minute, 1440.0)
	var result := []
	if md < 420.0 or md >= 1140.0: return result
	for animal: Dictionary in HOMES:
		var phase := fposmod(md + float(animal["offset"]), 90.0)
		var forward := phase < 8.0
		var backward := phase >= 48.0 and phase < 56.0
		var moving := forward or backward
		var fraction := phase / 8.0 if forward else (1.0 - (phase - 48.0) / 8.0 if backward else (1.0 if phase < 48.0 else 0.0))
		var a: Vector2 = animal["a"]
		var b: Vector2 = animal["b"]
		var frame := 1 + int(floor(a.distance_to(b) * fraction / 7.0)) % 2 if moving else (3 if phase > 16.0 and phase < 40.0 else 0)
		result.append({"animal":true, "id":animal["id"], "name":animal["name"], "row":animal["row"], "frame":frame,
			"pose":{"pos":a.lerp(b, fraction), "flip":backward}, "activity":"walking" if moving else ("sleeping" if frame == 3 else "resting")})
	for i in 2:
		var a := Vector2(730, 480) if i == 0 else Vector2(1130, 210)
		var b := Vector2(980, 455) if i == 0 else Vector2(650, 110)
		var phase := fposmod(md + i * 31.0, 100.0)
		var flight := phase < 6.0 or (phase >= 50.0 and phase < 56.0)
		var returning := phase >= 50.0
		var fraction := clampf((phase - (50.0 if returning else 0.0)) / 6.0, 0, 1)
		var pos := b.lerp(a, fraction) if returning else a.lerp(b, fraction)
		pos.y -= sin(fraction * PI) * 35.0 if flight else 0.0
		result.append({"animal":true, "id":"sparrow-%d" % i, "name":"House sparrow", "row":3,
			"frame":2 + int(md * 3.0) % 2 if flight else 0, "pose":{"pos":pos,"flip":(b.x < a.x) != returning},
			"activity":"flying" if flight else "perching"})
	return result
