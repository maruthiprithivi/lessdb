extends RefCounted
## Small style helpers so every panel shares one visual language.

const BG := Color("0f131b")
const PANEL := Color("161c28")
const PANEL_2 := Color("1d2534")
const LINE := Color("2c3750")
const TEXT := Color("eef2f8")
const MUTED := Color("9aa6bb")
const ACCENT := Color("f2b54a")
const NATIVE := Color("4fc28b")
const APP := Color("6aa7ff")
const OPEN := Color("e07a6a")
const KIND_COLORS := {
	"memory": Color("b18cff"), "lesson": Color("8dd6a8"), "message": Color("6aa7ff"), "decision": Color("f2b54a"),
	"say": Color("dfe6f1"), "world": Color("7fcbd1"), "access": Color("e07a6a"),
	"frontier": Color("c29cff"), "proposal": Color("71c7c5"), "novelty": Color("f0b6a8"),
}
const MEM_KIND_COLORS := {
	"permanent": Color("b18cff"), "temporary": Color("f29a4a"), "contextual": Color("6aa7ff"),
	"spatial": Color("4fc28b"), "temporal": Color("e5d36a"),
}
const AGENT_COLORS := {"maya": Color("e9a23b"), "sam": Color("5b8def"), "lin": Color("4fb38a"),
	"ama": Color("d0679a"), "bo": Color("b4805a"), "world": Color("7fcbd1"), "observer": Color("9aa6bb"),
	"inspector": Color("6b7891")}


static func box(bg: Color, border: Color = Color(0, 0, 0, 0), radius: int = 8, pad: int = 10, bw: int = 1) -> StyleBoxFlat:
	var s := StyleBoxFlat.new()
	s.bg_color = bg
	s.border_color = border
	s.set_border_width_all(bw if border.a > 0 else 0)
	s.set_corner_radius_all(radius)
	s.content_margin_left = pad
	s.content_margin_right = pad
	s.content_margin_top = pad * 0.7
	s.content_margin_bottom = pad * 0.7
	return s


static func panel(bg: Color = PANEL, border: Color = LINE, pad: int = 10) -> PanelContainer:
	var p := PanelContainer.new()
	p.add_theme_stylebox_override("panel", box(bg, border, 8, pad))
	return p


static func label(text: String, fsize: int = 15, color: Color = TEXT, wrap: bool = false) -> Label:
	var l := Label.new()
	l.text = text
	l.add_theme_font_size_override("font_size", fsize)
	l.add_theme_color_override("font_color", color)
	if wrap:
		l.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		l.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	return l


static func button(text: String, primary: bool = false, fsize: int = 14) -> Button:
	var b := Button.new()
	b.text = text
	b.focus_mode = Control.FOCUS_ALL
	b.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	b.add_theme_font_size_override("font_size", fsize)
	var base := ACCENT if primary else PANEL_2
	var fg := Color("1a1406") if primary else TEXT
	b.add_theme_stylebox_override("normal", box(base, LINE if not primary else base, 7, 10))
	b.add_theme_stylebox_override("hover", box(base.lightened(0.12), ACCENT, 7, 10))
	b.add_theme_stylebox_override("pressed", box(base.darkened(0.15), ACCENT, 7, 10))
	b.add_theme_stylebox_override("focus", box(Color(0, 0, 0, 0), ACCENT, 7, 10, 2))
	b.add_theme_stylebox_override("disabled", box(base.darkened(0.4), LINE, 7, 10))
	b.add_theme_color_override("font_color", fg)
	b.add_theme_color_override("font_hover_color", fg)
	b.add_theme_color_override("font_pressed_color", fg)
	b.add_theme_color_override("font_focus_color", fg)
	return b


static func chip(text: String, color: Color, fsize: int = 12) -> PanelContainer:
	var p := PanelContainer.new()
	var s := box(Color(color, 0.18), Color(color, 0.7), 10, 6)
	s.content_margin_top = 1
	s.content_margin_bottom = 2
	p.add_theme_stylebox_override("panel", s)
	p.add_child(label(text, fsize, color.lightened(0.25)))
	return p


static func clear(node: Node) -> void:
	for c in node.get_children():
		node.remove_child(c)
		c.queue_free()


static func clock_label(minute: int) -> String:
	var md := posmod(minute, 1440)
	return "%02d:%02d" % [md / 60, md % 60]
