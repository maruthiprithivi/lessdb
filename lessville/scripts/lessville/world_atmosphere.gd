extends RefCounted
## Clock-driven sky, ambient light and wind. The sim clock is the only input;
## weather strength comes from a deterministic schedule seeded by date (the same
## roll as the LessDB backend), unless the snapshot already carries wind_kmh.
const DEFAULT_SEED := 0x4C45535356494C4C  ## "LESSVILL"
const SKY_BAND := Rect2(0, 0, 1536, 56)
const FLAG_BASE := Vector2(758, 560)  ## park, west of the fountain
const FLAG_ROI := Rect2(748, 488, 72, 72)


static func minute_of_day(clock: Dictionary, sim_t: float = -1.0) -> float:
	if sim_t >= 0.0:
		return fposmod(sim_t, 1440.0)
	if clock.has("minute_of_day"):
		return fposmod(float(clock.get("minute_of_day")), 1440.0)
	return fposmod(float(clock.get("minute", 0)), 1440.0)


static func day_index(clock: Dictionary) -> int:
	if clock.has("day"):
		return int(clock.get("day"))
	return int(float(clock.get("minute", 0)) / 1440.0) + 1


static func world_seed(clock: Dictionary) -> int:
	if clock.has("world_seed"):
		return int(clock.get("world_seed"))
	return DEFAULT_SEED


## 32-bit portable mix seeded by world_seed, day and 3-hour bucket.
## clock.wind_kmh, when present on a live snapshot, is used as-is so LessDB stays authoritative.
static func seeded_roll(seed: int, day: int, bucket: int) -> int:
	var x: int = (seed + day * 374761393 + bucket * 668265263) & 0x7FFFFFFF
	x = (x ^ (x >> 13)) & 0x7FFFFFFF
	x = (x * 1274126177) & 0x7FFFFFFF
	x = (x ^ (x >> 16)) & 0x7FFFFFFF
	return x


static func scheduled_weather(day: int, md: float, seed: int = DEFAULT_SEED) -> Dictionary:
	var bucket := int(md) / 180
	var roll := seeded_roll(seed, day, bucket) % 100
	if day == 1 and int(md) == 810:
		return {"kind": "rain", "temperature_c": 27, "wind_kmh": 18, "roll": roll}
	if roll < 18:
		return {"kind": "rain", "temperature_c": 27, "wind_kmh": 20, "roll": roll}
	if roll < 43:
		return {"kind": "cloudy", "temperature_c": 28, "wind_kmh": 12, "roll": roll}
	if roll > 90:
		return {"kind": "windy", "temperature_c": 30, "wind_kmh": 28, "roll": roll}
	return {"kind": "clear", "temperature_c": 30, "wind_kmh": 8, "roll": roll}


static func wind_kmh(clock: Dictionary, sim_t: float = -1.0) -> float:
	if clock.has("wind_kmh"):
		return float(clock.get("wind_kmh"))
	var wx := scheduled_weather(day_index(clock), minute_of_day(clock, sim_t), world_seed(clock))
	return float(wx["wind_kmh"])


static func phase_name(md: float) -> String:
	if md < 330.0 or md >= 1170.0:
		return "night"
	if md <= 450.0:
		return "dawn"
	if md < 1020.0:
		return "day"
	return "dusk"


static func _lerp_color(a: Color, b: Color, t: float) -> Color:
	return a.lerp(b, clampf(t, 0.0, 1.0))


## Smooth 0-1 between lo and hi.
static func _u(md: float, lo: float, hi: float) -> float:
	if hi <= lo:
		return 1.0
	return clampf((md - lo) / (hi - lo), 0.0, 1.0)


static func snapshot(clock: Dictionary, sim_t: float = -1.0) -> Dictionary:
	var md := minute_of_day(clock, sim_t)
	var day := day_index(clock)
	var seed := world_seed(clock)
	var wx := scheduled_weather(day, md, seed)
	var raining := bool(clock.get("raining", false)) or String(wx["kind"]) == "rain"
	if clock.has("weather"):
		raining = raining or String(clock.get("weather")) == "rain"
	var wind := wind_kmh(clock, sim_t)

	# Key sky / ambient colours.
	var night_top := Color(0.05, 0.08, 0.18)
	var night_hz := Color(0.08, 0.12, 0.28)
	var dawn_top := Color(0.92, 0.48, 0.28)
	var dawn_hz := Color(1.0, 0.72, 0.42)
	var day_top := Color(0.55, 0.76, 0.95)
	var day_hz := Color(0.85, 0.92, 0.98)
	var dusk_top := Color(0.82, 0.28, 0.22)
	var dusk_hz := Color(0.95, 0.50, 0.22)
	var night_amb := Color(0.28, 0.34, 0.55)
	var day_amb := Color(1, 1, 1)

	var sky_top := night_top
	var sky_hz := night_hz
	var ambient := night_amb
	var darkness := 0.74
	var sky_alpha := 0.88
	var lamp := 1.0
	var window := 1.0
	var phase := phase_name(md)

	if phase == "dawn":
		var t := _u(md, 330.0, 450.0)
		sky_top = _lerp_color(night_top, dawn_top, t)
		sky_hz = _lerp_color(night_hz, dawn_hz, t)
		ambient = _lerp_color(night_amb, Color(1.0, 0.92, 0.82), t)
		darkness = lerpf(0.74, 0.12, t)
		sky_alpha = lerpf(0.88, 0.55, t)
		lamp = 1.0 - t
		window = 1.0 - t * 0.7
	elif phase == "day":
		var t := _u(md, 450.0, 540.0)  # last of the morning warmth
		sky_top = _lerp_color(dawn_top, day_top, t)
		sky_hz = _lerp_color(dawn_hz, day_hz, t)
		ambient = _lerp_color(Color(1.0, 0.92, 0.82), day_amb, t)
		darkness = lerpf(0.12, 0.0, t)
		sky_alpha = lerpf(0.55, 0.22, t)
		lamp = 0.0
		window = 0.0
		if md >= 960.0:  # 16:00, hint of late afternoon
			var u := _u(md, 960.0, 1020.0)
			sky_top = _lerp_color(day_top, dusk_top, u * 0.35)
			sky_hz = _lerp_color(day_hz, dusk_hz, u * 0.35)
			sky_alpha = lerpf(0.22, 0.35, u)
	elif phase == "dusk":
		var t := _u(md, 1020.0, 1170.0)
		var warm := Color(0.90, 0.38, 0.20)
		if t < 0.5:
			sky_top = _lerp_color(day_top, warm, t / 0.5)
		else:
			sky_top = _lerp_color(warm, night_top, (t - 0.5) / 0.5)
		sky_hz = _lerp_color(dusk_hz, night_hz, t)
		ambient = _lerp_color(day_amb, night_amb, t)
		darkness = lerpf(0.0, 0.74, t)
		sky_alpha = lerpf(0.35, 0.88, t)
		lamp = clampf((t - 0.25) / 0.5, 0.0, 1.0)
		window = lamp
	# else night: defaults above

	return {
		"md": md,
		"day": day,
		"phase": phase,
		"sky_top": sky_top,
		"sky_horizon": sky_hz,
		"sky_alpha": sky_alpha,
		"ambient": ambient,
		"darkness": darkness,
		"lamp": lamp,
		"window": window,
		"wind_kmh": wind,
		"raining": raining,
		"weather_kind": String(wx["kind"]),
		"temperature_c": int(wx["temperature_c"]),
	}


static func wind_offset(x: float, y: float, wind: float, phase: float) -> Vector2:
	if wind <= 0.0:
		return Vector2.ZERO
	var amp := clampf(wind / 20.0, 0.0, 2.2) * 2.8
	var dx := sin(phase * 2.3 + y * 0.11 + x * 0.04) * amp
	var dy := sin(phase * 1.7 + x * 0.09) * amp * 0.25
	return Vector2(dx, dy)
