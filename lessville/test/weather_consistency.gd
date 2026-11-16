extends SceneTree

const Atmosphere = preload("res://scripts/lessville/world_atmosphere.gd")

var checks := 0
var failures := 0

func _check(condition: bool, message: String) -> void:
	checks += 1
	if not condition:
		failures += 1
		push_error(message)

func _initialize() -> void:
	# Day one's 13:30 fallback is always rain, so a dry authoritative clock
	# must actively suppress the client-side schedule.
	for kind in ["clear", "foggy", "wet_after_rain"]:
		var clock := {"day": 1, "minute_of_day": 810, "weather": kind, "raining": false, "temperature_c": 24, "wind_kmh": 3}
		var atmo := Atmosphere.snapshot(clock)
		_check(not bool(atmo["raining"]), "Authoritative %s must suppress seeded rain" % kind)
		_check(atmo["weather_kind"] == kind, "Weather label follows the authoritative snapshot")
		_check(atmo["temperature_c"] == 24 and atmo["wind_kmh"] == 3, "Live temperature and wind are preserved")
	for kind in ["rain", "storm"]:
		_check(bool(Atmosphere.snapshot({"day": 1, "minute_of_day": 499, "weather": kind})["raining"]), "Legacy %s label renders rain without a flag" % kind)
	_check(not bool(Atmosphere.snapshot({"day": 1, "minute_of_day": 810, "weather": "foggy"})["raining"]), "Weather-only dry snapshot suppresses seeded rain")
	_check(not bool(Atmosphere.snapshot({"day": 1, "minute_of_day": 810, "raining": false})["raining"]), "Explicit dry flag suppresses seeded rain without a weather label")
	_check(bool(Atmosphere.snapshot({"day": 1, "minute_of_day": 499, "raining": true})["raining"]), "Explicit wet flag survives a dry seeded bucket")
	_check(not bool(Atmosphere.snapshot({"day": 1, "minute_of_day": 810, "weather": "rain", "raining": false})["raining"]), "Explicit rain flag is authoritative even over a legacy label")
	_check(bool(Atmosphere.snapshot({"day": 1, "minute_of_day": 810})["raining"]), "Missing authoritative weather retains the seeded first-day shower")
	print("[weather-consistency] %s: %d checks" % ["PASS" if failures == 0 else "FAIL", checks])
	quit(0 if failures == 0 else 1)
