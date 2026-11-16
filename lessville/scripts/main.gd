extends Control
## Lessville: watch five agents live, talk and remember — every memory a LessDB row.

const Api = preload("res://scripts/lessville/api_client.gd")
const TownView = preload("res://scripts/lessville/town_view.gd")
const InteriorView = preload("res://scripts/lessville/interior_view.gd")
const K = preload("res://scripts/lessville/ui_kit.gd")
const OpenSourceLicenses = preload("res://scripts/manus/open_source_licenses.gd")

const VIEW := Vector2(1600, 900)
const TOWN_RECT := Rect2(16, 70, 1024, 683)
const OFFMAP := [["grocery", "FreshMart"], ["school", "University"], ["gym", "Gym"], ["trail", "Trail"], ["away", "Out of town"]]
const ROLES := {"maya": "Café owner", "sam": "Office worker", "lin": "Student", "ama": "Parent", "bo": "Barista"}
const NUDGES := [["rain", "Start a shower"], ["bus_delay", "Delay next bus"], ["oat_out", "Empty oat milk"],
	["eggs_out", "Use the last eggs"], ["revoke_consent", "Lin revokes consent"]]

var api: Node
var town: Control
var interior: Control
var state: Dictionary = {}
var selected := "lin"
var tab := "story"
var _feed_top := -1
var _sql_top := -1
var _agent_detail: Dictionary = {}
var _agent_context: Dictionary = {}
var _agent_fetch_timer := 0.0
var _tables_timer := 0.0
var _tables: Dictionary = {}
var _boundaries: Dictionary = {}

var title_layer: Control
var title_status: Label
var start_btn: Button
var game_layer: Control
var clock_lbl: Label
var day_lbl: Label
var weather_lbl: Label
var db_lbl: Label
var pause_btn: Button
var speed_btns := {}
var roster_btns := {}
var tab_btns := {}
var content_scroll: ScrollContainer
var content: VBoxContainer
var place_cards := {}
var interior_layer: Control
var toast_panel: PanelContainer
var toast_lbl: Label
var toast_timer := 0.0
var modal_layer: Control
var modal_title: Label
var modal_body: VBoxContainer
var offline_banner: PanelContainer
var bus_lbl: Label


func _ready() -> void:
	set_anchors_preset(Control.PRESET_FULL_RECT)
	var bg := ColorRect.new()
	bg.color = K.BG
	bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	add_child(bg)
	api = Api.new()
	add_child(api)
	api.state_received.connect(_on_state)
	api.connection_changed.connect(_on_connection)
	api.reply.connect(_on_reply)
	_build_game()
	_build_title()
	api.call_api("boundaries", "/api/boundaries")
	api.start_polling()


# ---------------------------------------------------------------- title

const DISCLAIMER := "Fictional research simulation. The characters are not real people; any resemblance to real persons is coincidental. Lessville studies how agents evolve and grow over time, with their long-term memory powered by LessDB."
var public_mode := false
var control_nodes: Array[Control] = []
var nudge_title: Label
var model_lbl: Label


func _build_title() -> void:
	title_layer = Control.new()
	title_layer.set_anchors_preset(Control.PRESET_FULL_RECT)
	add_child(title_layer)
	var art := TextureRect.new()
	art.texture = load("res://assets/game/locations/town.png")
	art.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	art.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_COVERED
	art.set_anchors_preset(Control.PRESET_FULL_RECT)
	title_layer.add_child(art)
	var shade := ColorRect.new()
	shade.color = Color(0.04, 0.05, 0.09, 0.6)
	shade.set_anchors_preset(Control.PRESET_FULL_RECT)
	title_layer.add_child(shade)
	var card := K.panel(Color(0.07, 0.09, 0.13, 0.94), K.ACCENT, 34)
	card.position = Vector2(370, 130)
	card.custom_minimum_size = Vector2(860, 0)
	title_layer.add_child(card)
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 14)
	card.add_child(v)
	var t := K.label("LESSVILLE", 66, K.ACCENT)
	t.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	var bold := load("res://assets/template/fonts/ui_bold.tres") as Font
	if bold:
		t.add_theme_font_override("font", bold)
	v.add_child(t)
	var sub := K.label("A LessDB agent world  ·  v2.1.0", 22, K.TEXT)
	sub.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	v.add_child(sub)
	var notice := K.panel(Color("2a2414"), K.ACCENT, 10)
	notice.add_child(K.label(DISCLAIMER, 14, Color("f6e7c1"), true))
	v.add_child(notice)
	v.add_child(K.label("A neighbourhood of 17: five main characters and 12 supporting neighbours, with 83 more residents in the surrounding districts. Follow meals, work, rest, conversations and memories across continuous days. All 100 lives continue in the saved world; only people outdoors in this neighbourhood appear on its streets.", 16, K.MUTED, true))
	var how := VBoxContainer.new()
	how.add_theme_constant_override("separation", 4)
	for line: String in [
		"•  Click a person to read their mind; click a building to look inside.",
		"•  Story shows conversations and memories; LessDB live shows the exact SQL each agent ran.",
		"•  Watch plans change as agents experience events and retrieve the right LessDB context.",
		"•  Truth separates what LessDB guarantees from rules this app adds on top.",
		"•  Keys: Esc pause/close · 1–4 switch panels.",
	]:
		how.add_child(K.label(line, 15, K.TEXT, true))
	v.add_child(how)
	var row := HBoxContainer.new()
	row.alignment = BoxContainer.ALIGNMENT_CENTER
	row.add_theme_constant_override("separation", 12)
	start_btn = K.button("ENTER LESSVILLE", true, 18)
	start_btn.custom_minimum_size = Vector2(260, 50)
	start_btn.pressed.connect(_enter_game)
	row.add_child(start_btn)
	v.add_child(row)
	title_status = K.label("Connecting to the LessDB agent service…", 14, K.MUTED, true)
	title_status.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	v.add_child(title_status)
	var lic := K.button("Open Source Licenses", false, 12)
	lic.name = "OpenSourceLicensesButton"
	lic.position = Vector2(16, VIEW.y - 46)
	lic.pressed.connect(OpenSourceLicenses.open.bind(self))
	title_layer.add_child(lic)
	start_btn.call_deferred("grab_focus")


func _enter_game() -> void:
	title_layer.visible = false
	game_layer.visible = true


# ---------------------------------------------------------------- layout

func _build_game() -> void:
	game_layer = Control.new()
	game_layer.set_anchors_preset(Control.PRESET_FULL_RECT)
	game_layer.visible = false
	add_child(game_layer)
	_build_top_bar()
	var frame := K.panel(K.PANEL, K.LINE, 0)
	frame.position = TOWN_RECT.position - Vector2(3, 3)
	frame.size = TOWN_RECT.size + Vector2(6, 6)
	frame.mouse_filter = Control.MOUSE_FILTER_IGNORE
	game_layer.add_child(frame)
	town = TownView.new()
	town.position = TOWN_RECT.position
	town.size = TOWN_RECT.size
	town.selected = selected
	town.agent_clicked.connect(_select_agent)
	town.place_clicked.connect(_open_place)
	game_layer.add_child(town)
	_build_interior()
	_build_bottom()
	_build_right()
	offline_banner = K.panel(Color("4a1f22"), K.OPEN, 12)
	offline_banner.position = Vector2(TOWN_RECT.position.x + 220, TOWN_RECT.position.y + 14)
	offline_banner.custom_minimum_size = Vector2(584, 0)
	offline_banner.add_child(K.label("Waiting for the LessDB agent service… the town resumes from its stored state when it reconnects.", 14, K.TEXT, true))
	offline_banner.visible = false
	offline_banner.mouse_filter = Control.MOUSE_FILTER_IGNORE
	game_layer.add_child(offline_banner)
	toast_panel = K.panel(Color(0.08, 0.1, 0.14, 0.95), K.ACCENT, 12)
	toast_panel.position = Vector2(TOWN_RECT.position.x + 262, TOWN_RECT.end.y - 70)
	toast_panel.custom_minimum_size = Vector2(500, 0)
	toast_lbl = K.label("", 14, K.TEXT, true)
	toast_panel.add_child(toast_lbl)
	toast_panel.visible = false
	toast_panel.mouse_filter = Control.MOUSE_FILTER_IGNORE
	game_layer.add_child(toast_panel)
	_build_modal()


func _build_top_bar() -> void:
	var bar := K.panel(K.PANEL, K.LINE, 10)
	bar.position = Vector2(12, 10)
	bar.size = Vector2(VIEW.x - 24, 50)
	game_layer.add_child(bar)
	var h := HBoxContainer.new()
	h.add_theme_constant_override("separation", 10)
	bar.add_child(h)
	h.add_child(K.label("LESSVILLE", 20, K.ACCENT))
	h.add_child(_vsep())
	day_lbl = K.label("Day –", 15, K.MUTED)
	h.add_child(day_lbl)
	clock_lbl = K.label("--:--", 24, K.TEXT)
	clock_lbl.custom_minimum_size = Vector2(72, 0)
	h.add_child(clock_lbl)
	h.add_child(K.label("Asia/Singapore", 12, K.MUTED))
	weather_lbl = K.label("", 14, K.TEXT)
	weather_lbl.custom_minimum_size = Vector2(260, 0)
	h.add_child(weather_lbl)
	h.add_child(_vsep())
	pause_btn = K.button("Pause", false, 14)
	pause_btn.custom_minimum_size = Vector2(84, 0)
	pause_btn.pressed.connect(_toggle_pause)
	h.add_child(pause_btn)
	control_nodes.append(pause_btn)
	var step := K.button("+5 min", false, 14)
	control_nodes.append(step)
	step.tooltip_text = "Advance the shared clock five minutes"
	step.pressed.connect(func() -> void: api.control("step"))
	h.add_child(step)
	for sp: int in [1, 2, 4, 8]:
		var b := K.button("×%d" % sp, false, 14)
		b.toggle_mode = true
		b.tooltip_text = "%d sim-minutes per real second" % (sp * 2)
		b.pressed.connect(func() -> void: api.control("speed", sp))
		speed_btns[sp] = b
		control_nodes.append(b)
		h.add_child(b)
	h.add_child(_vsep())
	bus_lbl = K.label("", 13, K.MUTED)
	bus_lbl.custom_minimum_size = Vector2(190, 0)
	h.add_child(bus_lbl)
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	h.add_child(spacer)
	var fic := K.label("Fictional research sim", 12, Color("f6e7c1"))
	fic.tooltip_text = DISCLAIMER
	fic.mouse_filter = Control.MOUSE_FILTER_PASS
	h.add_child(fic)
	model_lbl = K.label("Model · –", 12, K.MUTED)
	model_lbl.tooltip_text = "Optional model voice: one short task per call, bounded LessDB context, hard daily budget. Decisions are always rule-based."
	model_lbl.mouse_filter = Control.MOUSE_FILTER_PASS
	h.add_child(model_lbl)
	db_lbl = K.label("LessDB · connecting", 13, K.NATIVE)
	h.add_child(db_lbl)
	var reset := K.button("New Day 1", false, 13)
	reset.tooltip_text = "Wipe the Lessville tables in LessDB and start again"
	reset.pressed.connect(_confirm_reset)
	h.add_child(reset)
	control_nodes.append(reset)


func _vsep() -> Control:
	var s := ColorRect.new()
	s.color = K.LINE
	s.custom_minimum_size = Vector2(1, 28)
	return s


func _build_interior() -> void:
	interior_layer = Control.new()
	interior_layer.position = TOWN_RECT.position
	interior_layer.size = TOWN_RECT.size
	interior_layer.visible = false
	game_layer.add_child(interior_layer)
	interior = InteriorView.new()
	interior.size = TOWN_RECT.size
	interior.town = town
	interior_layer.add_child(interior)
	var back := K.button("Back to town", true, 14)
	back.position = Vector2(TOWN_RECT.size.x - 172, 5)
	back.pressed.connect(func() -> void: interior_layer.visible = false)
	interior_layer.add_child(back)


func _open_place(loc: String) -> void:
	if loc == "park" or loc == "bus_stop":
		_toast("Fountain Park and the bus stop are outdoors — the people there are drawn on the map.")
		return
	interior.state = state
	interior.open(loc)
	interior_layer.visible = true


func _build_bottom() -> void:
	var y := TOWN_RECT.end.y + 12
	var wrap := K.panel(K.PANEL, K.LINE, 10)
	wrap.position = Vector2(12, y)
	wrap.size = Vector2(TOWN_RECT.size.x + 8, VIEW.y - y - 10)
	game_layer.add_child(wrap)
	var h := HBoxContainer.new()
	h.add_theme_constant_override("separation", 14)
	wrap.add_child(h)
	var left := VBoxContainer.new()
	left.add_theme_constant_override("separation", 4)
	h.add_child(left)
	left.add_child(K.label("ELSEWHERE", 12, K.MUTED))
	var places := HBoxContainer.new()
	places.add_theme_constant_override("separation", 6)
	left.add_child(places)
	for p: Array in OFFMAP:
		var loc: String = p[0]
		var b := Button.new()
		b.custom_minimum_size = Vector2(104, 98)
		b.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
		b.add_theme_stylebox_override("normal", K.box(K.PANEL_2, K.LINE, 7, 4))
		b.add_theme_stylebox_override("hover", K.box(K.PANEL_2.lightened(0.1), K.ACCENT, 7, 4))
		b.add_theme_stylebox_override("pressed", K.box(K.PANEL_2, K.ACCENT, 7, 4))
		b.add_theme_stylebox_override("focus", K.box(Color(0, 0, 0, 0), K.ACCENT, 7, 4, 2))
		b.pressed.connect(func() -> void:
			if loc == "away":
				_toast("Out of town is off the map — nothing is simulated there.")
			else:
				_open_place(loc))
		var v := VBoxContainer.new()
		v.mouse_filter = Control.MOUSE_FILTER_IGNORE
		v.position = Vector2(4, 4)
		v.size = Vector2(96, 90)
		v.add_theme_constant_override("separation", 1)
		b.add_child(v)
		var thumb := TextureRect.new()
		thumb.mouse_filter = Control.MOUSE_FILTER_IGNORE
		if loc != "away":
			thumb.texture = load("res://assets/game/locations/%s.png" % loc)
		thumb.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		thumb.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_COVERED
		thumb.custom_minimum_size = Vector2(96, 44)
		v.add_child(thumb)
		var nm := K.label(p[1], 12, K.TEXT)
		nm.mouse_filter = Control.MOUSE_FILTER_IGNORE
		v.add_child(nm)
		var who := K.label("—", 11, K.MUTED)
		who.mouse_filter = Control.MOUSE_FILTER_IGNORE
		who.clip_text = true
		who.custom_minimum_size = Vector2(96, 0)
		v.add_child(who)
		place_cards[loc] = who
		places.add_child(b)
	h.add_child(_vsep())
	var right := VBoxContainer.new()
	right.add_theme_constant_override("separation", 6)
	right.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	h.add_child(right)
	nudge_title = K.label("NUDGE THE WORLD  ·  agents only learn by experiencing it or being told", 12, K.MUTED)
	right.add_child(nudge_title)
	var grid := GridContainer.new()
	grid.columns = 3
	grid.add_theme_constant_override("h_separation", 6)
	grid.add_theme_constant_override("v_separation", 6)
	right.add_child(grid)
	for n: Array in NUDGES:
		var b := K.button(n[1], false, 12)
		b.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		b.custom_minimum_size = Vector2(0, 34)
		var kind: String = n[0]
		b.pressed.connect(func() -> void: api.nudge(kind))
		grid.add_child(b)
		control_nodes.append(b)


func _build_right() -> void:
	var x := TOWN_RECT.end.x + 18
	var wrap := K.panel(K.PANEL, K.LINE, 10)
	wrap.position = Vector2(x, 70)
	wrap.size = Vector2(VIEW.x - x - 12, VIEW.y - 80)
	game_layer.add_child(wrap)
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 8)
	wrap.add_child(v)
	var roster := HBoxContainer.new()
	roster.add_theme_constant_override("separation", 6)
	v.add_child(roster)
	for id: String in ["maya", "bo", "lin", "sam", "ama"]:
		var col: Color = K.AGENT_COLORS[id]
		var b := Button.new()
		b.custom_minimum_size = Vector2(98, 70)
		b.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
		b.toggle_mode = true
		b.add_theme_stylebox_override("normal", K.box(K.PANEL_2, K.LINE, 8, 4))
		b.add_theme_stylebox_override("hover", K.box(K.PANEL_2.lightened(0.08), col, 8, 4))
		b.add_theme_stylebox_override("pressed", K.box(Color(col, 0.25), col, 8, 4, 2))
		b.add_theme_stylebox_override("focus", K.box(Color(0, 0, 0, 0), col, 8, 4, 2))
		b.pressed.connect(func() -> void: _select_agent(id))
		var face := TextureRect.new()
		face.texture = load("res://assets/game/characters/%s_0.png" % id)
		face.mouse_filter = Control.MOUSE_FILTER_IGNORE
		face.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		face.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		face.position = Vector2(2, 3)
		face.size = Vector2(40, 64)
		b.add_child(face)
		var nm := K.label(id.capitalize(), 14, K.TEXT)
		nm.mouse_filter = Control.MOUSE_FILTER_IGNORE
		nm.position = Vector2(42, 10)
		b.add_child(nm)
		var rl := K.label(String(ROLES[id]), 10, K.MUTED)
		rl.mouse_filter = Control.MOUSE_FILTER_IGNORE
		rl.position = Vector2(42, 32)
		rl.size = Vector2(54, 30)
		rl.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		b.add_child(rl)
		roster_btns[id] = b
		roster.add_child(b)
	var tabs := HBoxContainer.new()
	tabs.add_theme_constant_override("separation", 6)
	v.add_child(tabs)
	for tdef: Array in [["story", "Story"], ["mind", "Mind"], ["db", "LessDB live"], ["truth", "Truth"]]:
		var key: String = tdef[0]
		var b := K.button(tdef[1], false, 14)
		b.toggle_mode = true
		b.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		b.pressed.connect(func() -> void: _set_tab(key))
		tab_btns[key] = b
		tabs.add_child(b)
	content_scroll = ScrollContainer.new()
	content_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	content_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	v.add_child(content_scroll)
	content = VBoxContainer.new()
	content.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	content.add_theme_constant_override("separation", 6)
	content_scroll.add_child(content)
	_set_tab("story")
	_refresh_roster()


func _build_modal() -> void:
	modal_layer = Control.new()
	modal_layer.set_anchors_preset(Control.PRESET_FULL_RECT)
	modal_layer.visible = false
	game_layer.add_child(modal_layer)
	var shade := ColorRect.new()
	shade.color = Color(0, 0, 0, 0.6)
	shade.set_anchors_preset(Control.PRESET_FULL_RECT)
	shade.gui_input.connect(func(e: InputEvent) -> void:
		if e is InputEventMouseButton and e.pressed:
			modal_layer.visible = false)
	modal_layer.add_child(shade)
	var card := K.panel(K.PANEL, K.ACCENT, 18)
	card.position = Vector2(300, 80)
	card.size = Vector2(1000, 740)
	modal_layer.add_child(card)
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 10)
	card.add_child(v)
	var head := HBoxContainer.new()
	v.add_child(head)
	modal_title = K.label("Memory provenance", 20, K.ACCENT)
	modal_title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	head.add_child(modal_title)
	var close := K.button("Close", false, 14)
	close.pressed.connect(func() -> void: modal_layer.visible = false)
	head.add_child(close)
	var sc := ScrollContainer.new()
	sc.size_flags_vertical = Control.SIZE_EXPAND_FILL
	sc.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	v.add_child(sc)
	modal_body = VBoxContainer.new()
	modal_body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	modal_body.add_theme_constant_override("separation", 8)
	sc.add_child(modal_body)


# ---------------------------------------------------------------- state

func _on_connection(online: bool, message: String) -> void:
	title_status.text = message if online else "%s — %s. The town appears as soon as the service answers." % [message, api.base_url]
	title_status.add_theme_color_override("font_color", K.NATIVE if online else K.OPEN)
	offline_banner.visible = not online
	db_lbl.text = "LessDB · connected" if online else "LessDB · offline"
	db_lbl.add_theme_color_override("font_color", K.NATIVE if online else K.OPEN)


func _on_state(s: Dictionary) -> void:
	state = s
	town.set_state(s)
	interior.state = s
	var c: Dictionary = s.get("clock", {})
	clock_lbl.text = String(c.get("label", "--:--"))
	day_lbl.text = "Day %d" % int(c.get("day", 1))
	var paused := bool(c.get("paused", false))
	pause_btn.text = "Resume" if paused else "Pause"
	for sp: int in speed_btns:
		(speed_btns[sp] as Button).set_pressed_no_signal(int(c.get("speed", 1)) == sp)
	var md := int(c.get("minute_of_day", 0))
	var phase := "Night" if (md >= 1170 or md < 400) else ("Evening" if md >= 1080 else ("Morning" if md < 720 else "Afternoon"))
	var weather := String(c.get("weather", "rain" if bool(c.get("raining", false)) else "clear")).replace("_", " ").capitalize()
	var readings := "%d°C · %dkm/h" % [int(c.get("temperature_c", 0)), int(c.get("wind_kmh", 0))]
	var source := "live Open-Meteo" if String(c.get("weather_source", "")) == "open-meteo-free" else "seeded fallback"
	weather_lbl.text = "%s · %s · %s · %s%s" % [weather, readings, phase, source, (" · paused" if paused else "")]
	weather_lbl.tooltip_text = "Weather source: %s. Open-Meteo attribution: https://open-meteo.com/en/licence" % source
	db_lbl.text = "%d local · %d nearby" % [int(s.get("population", {}).get("town_center", 17)), int(s.get("population", {}).get("surrounding", 83))]
	var pub := bool(s.get("public", false))
	if pub != public_mode:
		public_mode = pub
		for n: Control in control_nodes:
			n.visible = not pub
		nudge_title.text = ("OBSERVER MODE  ·  this is one shared, continuously running world. Watch, click people and places, and inspect LessDB — controls are off so no visitor can disrupt it." if pub
			else "NUDGE THE WORLD  ·  agents only learn by experiencing it or being told")
		nudge_title.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		nudge_title.custom_minimum_size = Vector2(560, 0)
	var lm: Variant = s.get("llm")
	if lm is Dictionary and bool(lm.get("enabled", false)):
		var cap := float(lm.get("budget_cap", 0))
		model_lbl.text = "Model · %d voiced%s" % [int(lm.get("ok", 0)), (" · %d/%d neurons" % [int(lm.get("budget_used", 0)), int(cap)]) if cap > 0 else ""]
		if bool(lm.get("budget_exhausted", false)):
			model_lbl.text = "Model · daily budget reached, rules only"
	else:
		model_lbl.text = "Model · off (rules only)"
	db_lbl.add_theme_color_override("font_color", K.NATIVE)
	var nb: Variant = s.get("bus", {}).get("next")
	if nb is Dictionary:
		var late := int(nb.get("delay", 0))
		bus_lbl.text = "Next bus %s%s" % [K.clock_label(int(nb.get("scheduled", 0))), ("  (+%d min late)" % late) if late > 0 else ""]
	else:
		bus_lbl.text = "Buses 07:00–21:00 every 20 min"
	_refresh_places()
	_refresh_roster()
	var feed: Array = s.get("feed", [])
	var top := int(feed[0].get("seq", -1)) if not feed.is_empty() else -1
	var sql: Array = s.get("sql", [])
	var stop := int(sql[0].get("id", -1)) if not sql.is_empty() else -1
	if tab == "story" and top != _feed_top:
		_feed_top = top
		_render_story()
	elif tab == "db" and stop != _sql_top:
		_sql_top = stop
		_render_db()


func _refresh_places() -> void:
	var by_loc := {}
	for a: Dictionary in state.get("agents", []):
		if a.get("travel") is Dictionary:
			continue
		var loc := String(a.get("location"))
		by_loc[loc] = by_loc.get(loc, []) + [String(a.get("name"))]
	for loc: String in place_cards:
		var names: Array = by_loc.get(loc, [])
		(place_cards[loc] as Label).text = ", ".join(names) if not names.is_empty() else "—"


func _agent(id: String) -> Dictionary:
	for a: Dictionary in state.get("agents", []):
		if String(a.get("id")) == id:
			return a
	return {}


func _refresh_roster() -> void:
	for id: String in roster_btns:
		var b: Button = roster_btns[id]
		b.set_pressed_no_signal(id == selected)
		var a := _agent(id)
		if not a.is_empty():
			b.tooltip_text = "%s — %s\n%s" % [a.get("name"), a.get("place"), a.get("activity")]


func _select_agent(id: String) -> void:
	selected = id
	town.selected = id
	_agent_detail = {}
	_agent_context = {}
	_refresh_roster()
	_set_tab("mind")


func _set_tab(key: String) -> void:
	tab = key
	for k: String in tab_btns:
		(tab_btns[k] as Button).set_pressed_no_signal(k == key)
	match key:
		"story":
			_render_story()
		"mind":
			_render_mind()
			_agent_fetch_timer = 0.0
		"db":
			_render_db()
			_tables_timer = 0.0
		"truth":
			_render_truth()
	content_scroll.scroll_vertical = 0


func _toggle_pause() -> void:
	if state.is_empty():
		return
	api.control("resume" if bool(state.get("clock", {}).get("paused", false)) else "pause")


func _process(delta: float) -> void:
	if toast_timer > 0.0:
		toast_timer -= delta
		if toast_timer <= 0.0:
			toast_panel.visible = false
	if not game_layer.visible:
		return
	if tab == "mind":
		_agent_fetch_timer -= delta
		if _agent_fetch_timer <= 0.0:
			_agent_fetch_timer = 2.5
			api.call_api("agent", "/api/agent/" + selected)
			api.call_api("agent_context", "/api/agent/" + selected + "/context")
	elif tab == "db":
		_tables_timer -= delta
		if _tables_timer <= 0.0:
			_tables_timer = 3.0
			api.call_api("tables", "/api/tables")


func _on_reply(kind: String, ok: bool, data: Dictionary) -> void:
	match kind:
		"boundaries":
			if ok:
				_boundaries = data
				if tab == "truth":
					_render_truth()
		"agent":
			if ok and String(data.get("agent")) == selected:
				_agent_detail = data
				if tab == "mind":
					var keep := content_scroll.scroll_vertical
					_render_mind()
					content_scroll.set_deferred("scroll_vertical", keep)
		"agent_context":
			if ok and String(data.get("agent")) == selected:
				_agent_context = data
				if tab == "mind":
					var keep_context := content_scroll.scroll_vertical
					_render_mind()
					content_scroll.set_deferred("scroll_vertical", keep_context)
		"tables":
			if ok:
				_tables = data
				if tab == "db":
					var keep2 := content_scroll.scroll_vertical
					_render_db()
					content_scroll.set_deferred("scroll_vertical", keep2)
		"memory":
			_render_memory_modal(ok, data)
		"nudge":
			_toast(String(data.get("result", "Done")) if ok else String(data.get("error", "Not possible right now")))
		"control":
			if not ok:
				_toast(String(data.get("error", "Clock control failed")))
		"reset":
			_toast("A fresh Day 1 has started — every Lessville table was truncated in LessDB." if ok else String(data.get("error", "Reset failed")))


func _toast(text: String) -> void:
	toast_lbl.text = text
	toast_panel.visible = true
	toast_timer = 4.5


func _confirm_reset() -> void:
	K.clear(modal_body)
	modal_title.text = "Start a new Day 1?"
	modal_layer.visible = true
	modal_body.add_child(K.label("This runs TRUNCATE TABLE on every Lessville table in LessDB, then re-seeds the neighbourhood. Memories from this run will be gone for good.", 15, K.MUTED, true))
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 10)
	var yes := K.button("Wipe and restart", true, 15)
	yes.pressed.connect(func() -> void:
		modal_layer.visible = false
		api.call_api("reset", "/api/reset", {}))
	row.add_child(yes)
	var no := K.button("Keep this world", false, 15)
	no.pressed.connect(func() -> void: modal_layer.visible = false)
	row.add_child(no)
	modal_body.add_child(row)


# ---------------------------------------------------------------- Story

func _render_story() -> void:
	K.clear(content)
	content.add_child(K.label("Read back from LessDB rows the agents wrote. Click a memory chip to trace where it came from and what it changed.", 13, K.MUTED, true))
	if state.get("resumed") is String:
		content.add_child(_note(String(state["resumed"]), K.NATIVE))
	for f: Dictionary in state.get("feed", []):
		content.add_child(_feed_item(f))


func _note(text: String, color: Color) -> PanelContainer:
	var p := K.panel(Color(color, 0.12), Color(color, 0.6), 10)
	p.add_child(K.label(text, 13, K.TEXT, true))
	return p


func _feed_item(f: Dictionary) -> Control:
	var kind := String(f.get("kind"))
	var col: Color = K.KIND_COLORS.get(kind, K.MUTED)
	var p := K.panel(K.PANEL_2, Color(col, 0.35), 9)
	var h := HBoxContainer.new()
	h.add_theme_constant_override("separation", 8)
	p.add_child(h)
	var bar := ColorRect.new()
	bar.color = K.AGENT_COLORS.get(String(f.get("actor")), col)
	bar.custom_minimum_size = Vector2(4, 0)
	h.add_child(bar)
	var v := VBoxContainer.new()
	v.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	v.add_theme_constant_override("separation", 3)
	h.add_child(v)
	var top := HBoxContainer.new()
	top.add_theme_constant_override("separation", 6)
	v.add_child(top)
	top.add_child(K.label(K.clock_label(int(f.get("minute", 0))), 12, K.MUTED))
	top.add_child(K.chip(kind.to_upper(), col, 10))
	top.add_child(K.label(String(f.get("title")), 14, K.TEXT, true))
	var detail := String(f.get("detail", ""))
	if detail != "":
		v.add_child(K.label(detail, 13, Color("c9d2e0"), true))
	var refs: Array = f.get("refs", [])
	if not refs.is_empty():
		var rr := HFlowContainer.new()
		rr.add_theme_constant_override("h_separation", 4)
		v.add_child(rr)
		for r: String in refs:
			rr.add_child(_ref_chip(r))
	return p


func _ref_chip(r: String) -> Control:
	var parts := r.split(":")
	var kind := parts[0]
	var labels := {"mem": "memory #", "msg": "message #", "dec": "decision #", "ctx": "context ", "acc": "access log #", "lesson": "lesson "}
	var text := String(labels.get(kind, kind + " ")) + (parts[1] if parts.size() > 1 else "")
	if kind == "mem" and parts.size() > 1:
		var b := K.button(text + "  ›", false, 11)
		b.tooltip_text = "Trace this memory in LessDB"
		var mid := int(parts[1])
		b.pressed.connect(func() -> void: _open_memory(mid))
		return b
	return K.chip(text, K.MUTED, 11)


# ---------------------------------------------------------------- Mind

func _render_mind() -> void:
	K.clear(content)
	var a := _agent(selected)
	if a.is_empty():
		content.add_child(K.label("Waiting for the world state…", 14, K.MUTED))
		return
	var head := K.panel(K.PANEL_2, K.AGENT_COLORS[selected], 12)
	var h := HBoxContainer.new()
	h.add_theme_constant_override("separation", 12)
	head.add_child(h)
	var face := TextureRect.new()
	face.texture = load("res://assets/game/characters/%s_0.png" % selected)
	face.custom_minimum_size = Vector2(66, 78)
	face.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	face.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	h.add_child(face)
	var v := VBoxContainer.new()
	v.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	h.add_child(v)
	v.add_child(K.label("%s — %s" % [a.get("name"), a.get("title")], 17, K.TEXT, true))
	v.add_child(K.label("%s · %s" % [a.get("place"), a.get("activity")], 13, K.MUTED, true))
	var life: Dictionary = a.get("life", {})
	if not life.is_empty():
		v.add_child(K.label("Hunger %d · Fatigue %d · Social need %d / 100" % [roundi(float(life.get("hunger", 0))), roundi(float(life.get("fatigue", 0))), roundi(float(life.get("social_need", 0)))], 12, K.TEXT, true))
		v.add_child(K.label("Meals completed: %d" % int(life.get("meals_completed", 0)), 11, K.MUTED, true))
	var pres: Dictionary = a.get("presence", {}) if a.get("presence") is Dictionary else {}
	if not pres.is_empty():
		var held := String(pres.get("hands", "free"))
		if held == "carrying" and pres.get("item") != null:
			held = "carrying %s" % String(pres.get("item")).replace("_", " ")
		var with := " · with Mochi" if pres.get("companion") == "mochi" else ""
		var around: Array = pres.get("surroundings", [])
		var near: Array = pres.get("nearby", [])
		v.add_child(K.label("Presence: attending to %s · hands %s%s · %s" % [pres.get("attention", "?"), held, with, String(pres.get("setting", "")).replace("_", " ")], 12, K.TEXT, true))
		v.add_child(K.label("Notices: %s · near %s · rule %s (%s)" % [", ".join(around) if not around.is_empty() else "nothing unusual", ", ".join(near) if not near.is_empty() else "no one", String(pres.get("reason", "")).replace("_", " "), pres.get("policy", "")], 11, K.MUTED, true))
	var scopes := HFlowContainer.new()
	scopes.add_theme_constant_override("h_separation", 4)
	scopes.add_child(K.chip("private", K.MUTED, 11))
	for sc: Dictionary in _agent_detail.get("scopes", []):
		scopes.add_child(K.chip(String(sc.get("scope")), K.APP, 11))
	v.add_child(scopes)
	content.add_child(head)
	content.add_child(_growth_panel(a))
	content.add_child(_competency_panel())
	content.add_child(_identity_thread_panel())
	content.add_child(_competency_state_panel())
	content.add_child(_interaction_panel())
	content.add_child(_relationship_panel())
	content.add_child(_challenge_panel())
	content.add_child(_teaching_panel())
	content.add_child(_counterfactual_panel())
	if not _agent_context.is_empty():
		content.add_child(_context_packet_panel())
	if _agent_detail.is_empty():
		content.add_child(K.label("Reading this agent's rows from LessDB…", 13, K.MUTED))
		return
	content.add_child(_section("What %s can recall" % a.get("name"), "Its own rows plus any scope it belongs to. Expired temporary rows are filtered by the sim clock."))
	var vis: Array = _agent_detail.get("visible", [])
	if vis.is_empty():
		content.add_child(K.label("Nothing yet.", 13, K.MUTED))
	for m: Dictionary in vis:
		content.add_child(_memory_row(m))
	content.add_child(_note("%d other rows in lv_memories are invisible to %s — filtered by this app's scope predicate, not by native LessDB access control." % [int(_agent_detail.get("hidden", 0)), a.get("name")], K.OPEN))
	content.add_child(_sql_block(String(_agent_detail.get("visible_sql", ""))))
	var expired: Array = _agent_detail.get("expired", [])
	if not expired.is_empty():
		content.add_child(_section("Expired temporary memories", "Still stored, no longer recalled: expires_at has passed."))
		for m: Dictionary in expired:
			var r := _memory_row(m)
			r.modulate = Color(1, 1, 1, 0.55)
			content.add_child(r)
	content.add_child(_section("Recent decisions", "Deterministic, versioned rules (no language model). ‘Recalled’ lists the rows read first."))
	var decs: Array = _agent_detail.get("decisions", [])
	if decs.is_empty():
		content.add_child(K.label("No decisions yet.", 13, K.MUTED))
	for d: Dictionary in decs:
		var p := K.panel(K.PANEL_2, Color(K.ACCENT, 0.3), 9)
		var dv := VBoxContainer.new()
		p.add_child(dv)
		dv.add_child(K.label("%s  ·  %s" % [K.clock_label(int(d.get("sim_minute", 0))), d.get("action")], 14, K.TEXT, true))
		dv.add_child(K.label(String(d.get("explanation", "")), 12, Color("c9d2e0"), true))
		var meta := HFlowContainer.new()
		meta.add_theme_constant_override("h_separation", 4)
		meta.add_child(K.chip(String(d.get("policy", "")), K.ACCENT, 10))
		for r: String in String(d.get("recalled", "")).split(",", false):
			meta.add_child(_ref_chip(r))
		dv.add_child(meta)
		content.add_child(p)
	content.add_child(_section("Conversation", "Messages sent, received, or readable on this agent's channels."))
	var conv: Array = _agent_detail.get("conversation", [])
	if conv.is_empty():
		content.add_child(K.label("No messages yet.", 13, K.MUTED))
		for m: Dictionary in conv:
			var to := String(m.get("recipient", ""))
			var target := ("#" + String(m.get("channel"))) if to == "" else to.capitalize()
			content.add_child(K.label("%s  %s → %s:  %s" % [K.clock_label(int(m.get("sim_minute", 0))), String(m.get("sender")).capitalize(), target, m.get("body")], 13, K.TEXT, true))


func _growth_panel(a: Dictionary) -> Control:
	var growth: Dictionary = _agent_detail.get("growth", {})
	var stage := String(growth.get("stage", "observing")).replace("_", " ").capitalize()
	var p := K.panel(Color("15271f"), Color("8dd6a8", 0.75), 10)
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 4)
	p.add_child(v)
	var head := HBoxContainer.new()
	head.add_child(K.label("GROWTH · %s" % stage, 12, Color("8dd6a8")))
	var counts := K.label("%d accepted · %d forming" % [int(growth.get("accepted_count", 0)), int(growth.get("candidate_count", 0))], 11, K.MUTED)
	counts.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	counts.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	head.add_child(counts)
	v.add_child(head)
	v.add_child(K.label("Evidence becomes a lesson only through bounded deterministic rules; this is application-owned provenance, not consciousness or automatic policy change.", 11, K.MUTED, true))
	var lessons: Array = _agent_detail.get("lessons", [])
	if lessons.is_empty():
		v.add_child(K.label("No structured lessons yet. The agent is observing.", 12, K.TEXT, true))
	else:
		for lesson: Dictionary in lessons:
			var state_name := String(lesson.get("state", "candidate"))
			var col := Color("8dd6a8") if state_name == "accepted" else K.ACCENT
			var row := K.panel(K.PANEL_2, Color(col, 0.35), 7)
			var rv := VBoxContainer.new()
			rv.add_theme_constant_override("separation", 2)
			row.add_child(rv)
			rv.add_child(K.label("%s · %s · confidence %d%%" % [state_name.to_upper(), String(lesson.get("topic", "lesson")), int(lesson.get("confidence", 0))], 11, col))
			rv.add_child(K.label(String(lesson.get("statement", "")), 12, K.TEXT, true))
			rv.add_child(K.label("trigger: %s · policy: %s" % [String(lesson.get("trigger", "")), String(lesson.get("policy_version", ""))], 10, K.MUTED, true))
			var refs := String(lesson.get("source_refs", ""))
			if refs != "":
				rv.add_child(K.label("evidence: %s" % refs.replace("|", " · "), 10, K.MUTED, true))
				v.add_child(row)
	return p


func _competency_panel() -> Control:
	var competencies: Array = _agent_detail.get("competencies", [])
	var p := K.panel(Color("1d2b25"), Color("8dd6a8", 0.75), 10)
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 4)
	p.add_child(v)
	v.add_child(K.label("COMPETENCIES · evidence, not a score", 12, Color("8dd6a8")))
	v.add_child(K.label("Repeated successful experiences build bounded skills with source references; they do not grant permissions or prove consciousness.", 11, K.MUTED, true))
	if competencies.is_empty():
		v.add_child(K.label("No competency evidence recorded yet.", 12, K.TEXT, true))
	else:
		for competency: Dictionary in competencies:
			var status := String(competency.get("status", "emerging"))
			var col := Color("8dd6a8") if status == "reliable" else K.ACCENT
			v.add_child(K.label("%s · %s · level %d/5 · %d evidence" % [String(competency.get("skill", "skill")), status, int(competency.get("level", 0)), int(competency.get("evidence_count", 0))], 11, col, true))
			v.add_child(K.label("domain: %s · policy: %s" % [String(competency.get("domain", "")), String(competency.get("policy_version", ""))], 10, K.MUTED, true))
			var refs := String(competency.get("source_refs", ""))
			if refs != "":
				v.add_child(K.label("evidence: %s" % refs.replace("|", " · "), 10, K.MUTED, true))
	return p


func _identity_thread_panel() -> Control:
	var threads: Array = _agent_detail.get("identity_threads", [])
	var p := K.panel(Color("242b3c"), Color("91b7e8", 0.75), 10)
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 4)
	p.add_child(v)
	v.add_child(K.label("IDENTITY THREAD · continuity without erasure", 12, Color("91b7e8")))
	v.add_child(K.label("Roots, forks and resets are append-only simulation records. They preserve prior evidence and do not claim consciousness or a human-like self.", 11, K.MUTED, true))
	if threads.is_empty():
		v.add_child(K.label("No continuity record has been written yet.", 12, K.TEXT, true))
	else:
		for thread: Dictionary in threads:
			var op := String(thread.get("operation", "root"))
			var col := Color("91b7e8") if op == "root" else Color("f0b6a8")
			v.add_child(K.label("%s · %s · %s" % [op.capitalize(), String(thread.get("thread_id", "")), K.clock_label(int(thread.get("sim_minute", 0)))], 11, col, true))
			v.add_child(K.label("%s · evidence: %s" % [String(thread.get("continuity", "")), String(thread.get("evidence_refs", "")).replace("|", " · ")], 10, K.MUTED, true))
	return p


func _competency_state_panel() -> Control:
	var events: Array = _agent_detail.get("competency_events", [])
	var p := K.panel(Color("33252c"), Color("e4a0a8", 0.75), 10)
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 4)
	p.add_child(v)
	v.add_child(K.label("COMPETENCY STATES · support, contradiction, decay", 12, Color("f0b6a8")))
	v.add_child(K.label("A mixed result can challenge a prior pattern and a quiet period can reduce its bounded level. These are evidence transitions, not automatic policy or permission changes.", 11, K.MUTED, true))
	if events.is_empty():
		v.add_child(K.label("No competency-state transitions recorded yet.", 12, K.TEXT, true))
	else:
		for event: Dictionary in events:
			var kind := String(event.get("kind", "support"))
			var col := Color("8dd6a8") if kind == "support" else Color("f0b6a8")
			v.add_child(K.label("%s · %s → %s · %s" % [kind.capitalize(), int(event.get("prior_level", 0)), int(event.get("next_level", 0)), K.clock_label(int(event.get("sim_minute", 0)))], 11, col, true))
			v.add_child(K.label("%s · evidence: %s" % [String(event.get("reason", "")), String(event.get("evidence_refs", "")).replace("|", " · ")], 10, K.MUTED, true))
	return p


func _interaction_panel() -> Control:
	var interactions: Array = _agent_detail.get("interactions", [])
	var p := K.panel(Color("28233b"), Color("c29cff", 0.75), 10)
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 4)
	p.add_child(v)
	v.add_child(K.label("RECIPROCAL INTERACTIONS · action → response", 12, Color("c29cff")))
	v.add_child(K.label("These are bounded exchanges grounded in LessDB order/context evidence, not a global trust score or automatic social permission.", 11, K.MUTED, true))
	if interactions.is_empty():
		v.add_child(K.label("No completed reciprocal exchange recorded yet.", 12, K.TEXT, true))
	else:
		for interaction: Dictionary in interactions:
			v.add_child(K.label("%s → %s · %s · %s" % [String(interaction.get("initiator", "")).capitalize(), String(interaction.get("recipient", "")).capitalize(), String(interaction.get("kind", "")), String(interaction.get("outcome", ""))], 11, Color("c29cff"), true))
			v.add_child(K.label("%s → %s" % [String(interaction.get("initiator_action", "")), String(interaction.get("recipient_response", ""))], 10, K.TEXT, true))
			v.add_child(K.label("%s · context: %s · evidence: %s" % [String(interaction.get("reciprocity", "")), String(interaction.get("context_id", "")), String(interaction.get("evidence_refs", "")).replace("|", " · ")], 10, K.MUTED, true))
	return p


func _relationship_panel() -> Control:
	var edges: Array = _agent_detail.get("relationships", [])
	var p := K.panel(Color("202038"), Color("b49aff", 0.75), 10)
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 4)
	p.add_child(v)
	v.add_child(K.label("RELATIONSHIPS · consent and reciprocity", 12, Color("c5b2ff")))
	v.add_child(K.label("Edges are fictional application evidence, not a global trust score or native permission grant.", 11, K.MUTED, true))
	if edges.is_empty():
		v.add_child(K.label("No relationship edge has been recorded for this agent yet.", 12, K.TEXT, true))
	else:
		for edge: Dictionary in edges:
			var state_name := String(edge.get("state", "proposed"))
			var edge_col := Color("8dd6a8") if state_name == "active" else Color("f0b6a8")
			var subject := String(edge.get("subject_agent", "")).capitalize()
			var object := String(edge.get("object_agent", "")).capitalize()
			v.add_child(K.label("%s ↔ %s · %s · %s" % [subject, object, String(edge.get("consent", "unknown")), state_name], 11, edge_col, true))
			v.add_child(K.label("%s · %s · context: %s" % [String(edge.get("relation", "")), String(edge.get("reciprocity", "")), String(edge.get("trust_context", ""))], 10, K.MUTED, true))
	return p


func _challenge_panel() -> Control:
	var challenges: Array = _agent_detail.get("lesson_challenges", [])
	var p := K.panel(Color("30251c"), Color("e4b36f", 0.75), 10)
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 4)
	p.add_child(v)
	v.add_child(K.label("CONTESTATION · repair without erasure", 12, Color("f4c77e")))
	v.add_child(K.label("A challenge preserves the old lesson as evidence; only an explicit repaired version becomes eligible for later rules.", 11, K.MUTED, true))
	if challenges.is_empty():
		v.add_child(K.label("No lesson challenges recorded.", 12, K.TEXT, true))
	else:
		for challenge: Dictionary in challenges:
			v.add_child(K.label("%s · %s · %s" % [String(challenge.get("kind", "challenge")).capitalize(), String(challenge.get("resolution", "open")), String(challenge.get("lesson_id", ""))], 11, Color("f4c77e"), true))
			v.add_child(K.label(String(challenge.get("claim", "")), 10, K.MUTED, true))
	return p


func _teaching_panel() -> Control:
	var grants: Array = _agent_detail.get("teaching_grants", [])
	var p := K.panel(Color("182a31"), Color("71c7c5", 0.75), 10)
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 4)
	p.add_child(v)
	v.add_child(K.label("TEACHING GRANTS · explicit, scoped, expiring", 12, Color("71c7c5")))
	v.add_child(K.label("A grant lets one agent present one lesson to one recipient for a bounded purpose; it never copies rows or widens native access.", 11, K.MUTED, true))
	if grants.is_empty():
		v.add_child(K.label("No explicit teaching grants recorded.", 12, K.TEXT, true))
	else:
		for grant: Dictionary in grants:
			var state_name := String(grant.get("state", "proposed"))
			var col := Color("71c7c5") if state_name == "active" else Color("f0b6a8")
			v.add_child(K.label("%s → %s · %s · until %s" % [String(grant.get("teacher_agent", "")).capitalize(), String(grant.get("recipient_agent", "")).capitalize(), state_name, K.clock_label(int(grant.get("valid_until", -1)))], 11, col, true))
			v.add_child(K.label("%s · %s · lesson: %s" % [String(grant.get("purpose", "")), String(grant.get("scope_id", "")), String(grant.get("lesson_id", ""))], 10, K.MUTED, true))
	return p


func _counterfactual_panel() -> Control:
	var replays: Array = _agent_detail.get("counterfactuals", [])
	var p := K.panel(Color("29233a"), Color("c29cff", 0.75), 10)
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 4)
	p.add_child(v)
	v.add_child(K.label("COUNTERFACTUAL REPLAY · explanation only", 12, Color("c29cff")))
	v.add_child(K.label("The baseline and lesson-aware branches are compared without executing either branch.", 11, K.MUTED, true))
	if replays.is_empty():
		v.add_child(K.label("No replay comparison recorded.", 12, K.TEXT, true))
	else:
		for replay: Dictionary in replays:
			v.add_child(K.label("%s · %s vs %s" % [String(replay.get("topic", "replay")), String(replay.get("baseline_policy", "")), String(replay.get("learned_policy", ""))], 11, Color("c29cff"), true))
			v.add_child(K.label("Before: %s\nAfter: %s\n%s" % [String(replay.get("baseline_action", "")), String(replay.get("learned_action", "")), String(replay.get("difference", ""))], 10, K.MUTED, true))
	return p


func _context_packet_panel() -> Control:
	var packet: Dictionary = _agent_context.get("packet", {})
	var situation: Dictionary = packet.get("situation", {})
	var items: Array = packet.get("items", [])
	var p := K.panel(Color("102536"), K.APP, 10)
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 4)
	p.add_child(v)
	v.add_child(K.label("CONTEXT PACKET · %s" % String(packet.get("task", "observe")).to_upper(), 12, K.APP))
	v.add_child(K.label("Situation: %s at %s · %s · %s" % [situation.get("activity", "idle"), situation.get("place", "unknown"), situation.get("logical_minute", 0), situation.get("time_zone", "")], 12, K.TEXT, true))
	v.add_child(K.label("Included %d/%d bounded memory items · context %s" % [items.size(), int(packet.get("max_items", 0)), packet.get("context_id", "")], 11, K.MUTED, true))
	for item: Dictionary in items:
		var line := K.label("• #%d [%s · %s] %s" % [int(item.get("id", 0)), item.get("kind", "memory"), item.get("scope", ""), item.get("content", "")], 11, K.TEXT, true)
		v.add_child(line)
	v.add_child(K.label("%s" % String(_agent_context.get("boundary", "Application-layer scope filter; not native LessDB ACL.")), 11, K.OPEN, true))
	return p


func _section(title: String, sub: String) -> Control:
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 1)
	var spacer := Control.new()
	spacer.custom_minimum_size = Vector2(0, 4)
	v.add_child(spacer)
	v.add_child(K.label(title.to_upper(), 13, K.ACCENT, true))
	if sub != "":
		v.add_child(K.label(sub, 12, K.MUTED, true))
	return v


func _memory_row(m: Dictionary) -> Control:
	var kind := String(m.get("kind", ""))
	var col: Color = K.MEM_KIND_COLORS.get(kind, K.MUTED)
	var mid := int(m.get("id", 0))
	var p := K.panel(K.PANEL_2, Color(col, 0.45), 9)
	p.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	p.tooltip_text = "Click to trace memory #%d" % mid
	p.gui_input.connect(func(e: InputEvent) -> void:
		if e is InputEventMouseButton and e.pressed and e.button_index == MOUSE_BUTTON_LEFT:
			_open_memory(mid))
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 3)
	v.mouse_filter = Control.MOUSE_FILTER_PASS
	p.add_child(v)
	var top := HFlowContainer.new()
	top.mouse_filter = Control.MOUSE_FILTER_PASS
	top.add_theme_constant_override("h_separation", 5)
	v.add_child(top)
	var scope := String(m.get("scope", ""))
	var items: Array[Control] = [K.label("#%d" % mid, 11, K.MUTED), K.chip(kind, col, 10),
		K.chip(scope, K.APP if scope != "private" else K.MUTED, 10),
		K.label("by %s · %s" % [String(m.get("owner", "")).capitalize(), K.clock_label(int(m.get("sim_minute", 0)))], 11, K.MUTED)]
	var exp := int(m.get("expires_at", -1))
	if exp >= 0:
		items.append(K.label("expires %s" % K.clock_label(exp), 11, K.MEM_KIND_COLORS["temporary"]))
	items.append(K.label("trace ›", 11, K.ACCENT))
	for c: Control in items:
		c.mouse_filter = Control.MOUSE_FILTER_PASS
		top.add_child(c)
	var body := K.label(String(m.get("content", "")), 13, K.TEXT, true)
	body.mouse_filter = Control.MOUSE_FILTER_PASS
	v.add_child(body)
	return p


func _sql_block(sql: String) -> Control:
	var p := K.panel(Color("0b0f16"), K.LINE, 8)
	p.add_child(K.label(sql, 11, Color("9fe0b8"), true))
	return p


# ---------------------------------------------------------------- LessDB live

func _render_db() -> void:
	K.clear(content)
	content.add_child(_note("Embedded LessDB inside the lessville-agentd service — %s. Every statement below really executed." % _tables.get("source", "local engine"), K.NATIVE))
	content.add_child(_research_frontier_panel())
	var counts: Dictionary = _tables.get("tables", {})
	if not counts.is_empty():
		var grid := GridContainer.new()
		grid.columns = 4
		grid.add_theme_constant_override("h_separation", 6)
		grid.add_theme_constant_override("v_separation", 6)
		for name: String in counts:
			var p := K.panel(K.PANEL_2, K.LINE, 8)
			p.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			var v := VBoxContainer.new()
			p.add_child(v)
			v.add_child(K.label(str(counts[name]), 18, K.TEXT))
			v.add_child(K.label(name, 11, K.MUTED))
			grid.add_child(p)
		content.add_child(grid)
		content.add_child(K.label("Data directory: %s" % _tables.get("data_dir", ""), 11, K.MUTED, true))
	content.add_child(_section("Live statements", "Newest first. Actor = the agent (or the world/inspector) whose code issued the SQL."))
	for row: Dictionary in state.get("sql", []):
		var actor := String(row.get("actor", ""))
		var col: Color = K.AGENT_COLORS.get(actor, K.MUTED)
		var p := K.panel(K.PANEL_2, Color(col, 0.35), 8)
		var v := VBoxContainer.new()
		v.add_theme_constant_override("separation", 3)
		p.add_child(v)
		var top := HBoxContainer.new()
		top.add_theme_constant_override("separation", 6)
		v.add_child(top)
		top.add_child(K.label(K.clock_label(int(row.get("sim_minute", 0))), 11, K.MUTED))
		top.add_child(K.chip(actor, col, 10))
		top.add_child(K.label(String(row.get("purpose", "")), 12, K.TEXT, true))
		var ok := bool(row.get("ok", true))
		top.add_child(K.label(("%d rows · %d µs" % [int(row.get("rows", 0)), int(row.get("micros", 0))]) if ok else "ERROR", 11, K.MUTED if ok else K.OPEN))
		v.add_child(K.label(String(row.get("sql", "")), 11, Color("9fe0b8"), true))
		if not ok:
			v.add_child(K.label(String(row.get("error", "")), 11, K.OPEN, true))
		content.add_child(p)


func _research_frontier_panel() -> Control:
	var frontier: Dictionary = state.get("research_frontier", {})
	var novelty: Array = frontier.get("novelty", [])
	var signals: Array = frontier.get("signals", [])
	var proposals: Array = frontier.get("proposals", [])
	var p := K.panel(Color("211d32"), Color("c29cff", 0.8), 10)
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 4)
	p.add_child(v)
	v.add_child(K.label("RESEARCH FRONTIER · bounded emergence", 13, Color("c29cff")))
	v.add_child(K.label(String(frontier.get("boundary", "")), 11, K.MUTED, true))
	v.add_child(K.label("%d novelty · %d gap signals · %d inactive proposals" % [novelty.size(), signals.size(), proposals.size()], 12, K.TEXT))
	if not novelty.is_empty():
		var recent: Dictionary = novelty[0]
		v.add_child(K.label("Latest: %s · %s · seed %s" % [String(recent.get("agent", "")).capitalize(), String(recent.get("option", "")), String(recent.get("replay_key", ""))], 11, Color("f0b6a8"), true))
	if not signals.is_empty():
		var gap_signal: Dictionary = signals[0]
		v.add_child(K.label("Gap: %s · %s · recurrence %s · state %s" % [String(gap_signal.get("capability", "")), String(gap_signal.get("situation", "")), str(gap_signal.get("recurrence", 0)), String(gap_signal.get("state", "candidate"))], 11, Color("c29cff"), true))
	if not proposals.is_empty():
		v.add_child(K.label("Proposal: %s · candidate only · no automatic schema or policy change" % String(proposals[0]), 11, Color("71c7c5"), true))
	return p


# ---------------------------------------------------------------- Truth

func _render_truth() -> void:
	K.clear(content)
	var notice := K.panel(Color("2a2414"), K.ACCENT, 10)
	notice.add_child(K.label(DISCLAIMER, 13, Color("f6e7c1"), true))
	content.add_child(notice)
	content.add_child(K.label("What is real here — and what is not (yet)", 17, K.TEXT, true))
	content.add_child(K.label("Lessville is a demonstration. This panel separates guarantees provided by LessDB itself from rules the app implements on top of it, and from capabilities that remain open work.", 13, K.MUTED, true))
	if _boundaries.is_empty():
		content.add_child(K.label("Loading…", 13, K.MUTED))
		return
	for group: Array in [["native", "Provided by LessDB (this build)", K.NATIVE], ["application", "Implemented by the Lessville app on top of LessDB", K.APP]]:
		content.add_child(_section(group[1], ""))
		for item: Array in _boundaries.get(group[0], []):
			var p := K.panel(K.PANEL_2, Color(group[2], 0.5), 10)
			var v := VBoxContainer.new()
			p.add_child(v)
			v.add_child(K.label(String(item[0]), 14, (group[2] as Color).lightened(0.2), true))
			v.add_child(K.label(String(item[1]), 13, K.TEXT, true))
			content.add_child(p)
	content.add_child(_section("Open work — not claimed", ""))
	for item: String in _boundaries.get("open", []):
		content.add_child(_note(item, K.OPEN))


# ---------------------------------------------------------------- provenance

func _open_memory(id: int) -> void:
	K.clear(modal_body)
	modal_title.text = "Memory #%d — provenance" % id
	modal_body.add_child(K.label("Reading memory #%d and its lineage from LessDB…" % id, 14, K.MUTED))
	modal_layer.visible = true
	api.call_api("memory", "/api/memory/%d" % id)


func _render_memory_modal(ok: bool, d: Dictionary) -> void:
	K.clear(modal_body)
	if not ok:
		modal_body.add_child(_note(String(d.get("error", "Memory not found.")), K.OPEN))
		return
	var m: Dictionary = d.get("memory", {})
	var kind := String(m.get("kind"))
	var col: Color = K.MEM_KIND_COLORS.get(kind, K.MUTED)
	var owner := String(m.get("owner"))
	var head := K.panel(K.PANEL_2, col, 14)
	var hv := VBoxContainer.new()
	hv.add_theme_constant_override("separation", 8)
	head.add_child(hv)
	hv.add_child(K.label("“%s”" % m.get("content"), 18, K.TEXT, true))
	var meta := HFlowContainer.new()
	meta.add_theme_constant_override("h_separation", 6)
	meta.add_theme_constant_override("v_separation", 4)
	meta.add_child(K.chip(kind, col, 12))
	meta.add_child(K.chip("scope: %s" % m.get("scope"), K.APP, 12))
	meta.add_child(K.chip("owner: %s" % owner.capitalize(), K.AGENT_COLORS.get(owner, K.MUTED), 12))
	var at := int(m.get("sim_minute", 0))
	meta.add_child(K.chip("Day %d, %s" % [at / 1440 + 1, K.clock_label(at)], K.MEM_KIND_COLORS["temporal"], 12))
	if String(m.get("location", "")) != "":
		meta.add_child(K.chip("where: %s" % m.get("location"), K.MEM_KIND_COLORS["spatial"], 12))
	if int(m.get("expires_at", -1)) >= 0:
		meta.add_child(K.chip("expires %s" % K.clock_label(int(m.get("expires_at"))), K.MEM_KIND_COLORS["temporary"], 12))
	if String(m.get("tag", "")) != "":
		meta.add_child(K.chip("tag: %s" % m.get("tag"), K.MUTED, 12))
	hv.add_child(meta)
	modal_body.add_child(head)
	modal_body.add_child(_sql_block(String(d.get("memory_sql", ""))))
	modal_body.add_child(_section("1 · Where it came from", "The row this memory was derived from (source_kind = %s)." % m.get("source_kind")))
	var src: Array = d.get("source", [])
	if src.is_empty():
		modal_body.add_child(K.label("Seeded at the start of the day, or directly observed — no upstream row.", 13, K.MUTED, true))
	else:
		modal_body.add_child(_kv_panel(src[0]))
		modal_body.add_child(_sql_block(String(d.get("source_sql", ""))))
	modal_body.add_child(_section("2 · Who may read it", "The owner plus members of ‘%s’. Enforced by the app's SQL predicate, not native LessDB ACLs." % m.get("scope")))
	var rf := HFlowContainer.new()
	rf.add_theme_constant_override("h_separation", 6)
	rf.add_child(K.chip(owner.capitalize() + " (owner)", K.AGENT_COLORS.get(owner, K.MUTED), 12))
	for r: Dictionary in d.get("readers", []):
		var who := String(r.get("agent"))
		if who != owner:
			rf.add_child(K.chip(who.capitalize(), K.AGENT_COLORS.get(who, K.MUTED), 12))
	modal_body.add_child(rf)
	modal_body.add_child(_section("3 · What it changed", "Decisions that recalled this row before acting."))
	var used: Array = d.get("used_by", [])
	if used.is_empty():
		modal_body.add_child(K.label("No decision has used this memory yet.", 13, K.MUTED))
	for u: Dictionary in used:
		var p := K.panel(K.PANEL_2, Color(K.ACCENT, 0.4), 10)
		var v := VBoxContainer.new()
		p.add_child(v)
		v.add_child(K.label("%s · %s decided: %s" % [K.clock_label(int(u.get("sim_minute", 0))), String(u.get("agent")).capitalize(), u.get("action")], 14, K.TEXT, true))
		v.add_child(K.label(String(u.get("explanation", "")), 12, Color("c9d2e0"), true))
		v.add_child(K.chip(String(u.get("policy", "")), K.ACCENT, 10))
		modal_body.add_child(p)
	modal_body.add_child(_sql_block(String(d.get("used_sql", ""))))


func _kv_panel(row: Dictionary) -> Control:
	var p := K.panel(K.PANEL_2, K.LINE, 10)
	var g := GridContainer.new()
	g.columns = 2
	g.add_theme_constant_override("h_separation", 12)
	p.add_child(g)
	for k: String in row:
		g.add_child(K.label(k, 12, K.MUTED))
		var raw: Variant = row[k]
		var shown := str(int(raw)) if (raw is float and float(raw) == floorf(float(raw))) else str(raw)
		var val := K.label(shown if shown != "<null>" else "—", 12, K.TEXT, true)
		val.custom_minimum_size = Vector2(780, 0)
		g.add_child(val)
	return p


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("pause"):
		if modal_layer.visible:
			modal_layer.visible = false
		elif interior_layer.visible:
			interior_layer.visible = false
		elif game_layer.visible:
			_toggle_pause()
		get_viewport().set_input_as_handled()
	elif event is InputEventKey and event.pressed and not event.echo and game_layer.visible and not modal_layer.visible:
		match (event as InputEventKey).keycode:
			KEY_1:
				_set_tab("story")
			KEY_2:
				_set_tab("mind")
			KEY_3:
				_set_tab("db")
			KEY_4:
				_set_tab("truth")
