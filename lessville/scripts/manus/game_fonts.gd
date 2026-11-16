extends RefCounted
## Apply already imported project fonts at scene startup. No early
## project.godot font load, implicit system fallback, or repeated font acquisition.

static func create_theme(regular: Font, medium: Font = null, bold: Font = null) -> Theme:
	assert(regular != null, "A project font is required")
	var theme := Theme.new()
	theme.default_font = regular
	if medium != null:
		for control_type: String in ["Button", "CheckButton", "CheckBox", "OptionButton", "MenuButton"]:
			theme.set_font("font", control_type, medium)
	if bold != null:
		theme.set_font("normal_font", "RichTextLabel", regular)
		theme.set_font("bold_font", "RichTextLabel", bold)
	return theme

static func apply_world_label(label: Label3D, font: Font) -> void:
	assert(font != null, "A project font is required")
	# Label3D does not inherit a sibling Control's Theme.
	label.font = font

## Installed only in the verifier's existing finite boot. Observe resolved
## bindings on encountered text controls/world labels without restricting font
## families, glyph coverage, file sizes, system fallback or user-uploaded fonts.
## Only the base text theme is observed. Styled/inline RichTextLabel/push_font
## spans and custom draw calls are not inspected.
class BindingProbe extends Node:
	var _nodes: Dictionary = {}
	var _pending: Dictionary = {}
	var _fonts: Dictionary = {}
	var _font_resources: Dictionary = {}
	var _checked: Dictionary = {}
	var _world_fonts: Dictionary = {}
	var _active: Dictionary = {}

	func _ready() -> void:
		process_mode = Node.PROCESS_MODE_ALWAYS
		get_tree().node_added.connect(_observe)
		_visit(get_tree().root)

	func _process(_delta: float) -> void:
		# Several text setters and Label3D.font expose no change signal. Only
		# compare activity/binding identity; unchanged labels do not rerun proof.
		for identity: int in _nodes:
			var node: Node = _nodes[identity].get_ref() as Node
			if node == null:
				continue
			if _text_visible(node) != _active.get(identity, false):
				_schedule(node)
			if node is Label3D and node.font != _world_fonts[identity]:
				_world_fonts[identity] = node.font
				_schedule(node)

	func _visit(node: Node) -> void:
		_observe(node)
		for child: Node in node.get_children():
			_visit(child)

	func _observe(node: Node) -> void:
		if not (node is Label or node is Button or node is LineEdit or node is TextEdit or node is RichTextLabel or node is ItemList or node is Tree or node is PopupMenu or node is Label3D):
			return
		var identity: int = node.get_instance_id()
		if not _nodes.has(identity):
			_nodes[identity] = weakref(node)
			if node is Control:
				node.theme_changed.connect(_schedule.bind(node))
			elif node is Label3D:
				_world_fonts[identity] = node.font
		_schedule(node)

	func _text_visible(node: Node) -> bool:
		if not node.is_inside_tree():
			return false
		if node is CanvasItem and not node.is_visible_in_tree():
			return false
		if node is Label3D:
			return node.is_visible_in_tree() and not node.text.strip_edges().is_empty()
		if node is Label or node is Button:
			return not node.text.strip_edges().is_empty()
		if node is LineEdit or node is TextEdit:
			return not (node.text + node.placeholder_text).strip_edges().is_empty()
		if node is RichTextLabel:
			return not node.get_parsed_text().strip_edges().is_empty()
		if node is ItemList or node is PopupMenu:
			if node is PopupMenu and not node.visible:
				return false
			for index: int in range(node.item_count):
				if not node.get_item_text(index).strip_edges().is_empty():
					return true
		if node is Tree:
			var item: TreeItem = node.get_root()
			if item != null and node.hide_root:
				item = item.get_next_visible()
			while item != null:
				for column: int in range(node.columns):
					if not item.get_text(column).strip_edges().is_empty():
						return true
				item = item.get_next_visible()
		return false

	func _schedule(node: Node) -> void:
		if not is_instance_valid(node):
			return
		var identity: int = node.get_instance_id()
		if _pending.has(identity):
			return
		_pending[identity] = true
		_check.call_deferred(identity)

	func _font_changed() -> void:
		_font_resources.clear()
		for reference: WeakRef in _nodes.values():
			var node: Node = reference.get_ref() as Node
			if node != null:
				_schedule(node)

	func _record_font(font: Font, seen: Dictionary) -> void:
		if font == null or seen.has(font):
			return
		seen[font] = true
		if not _fonts.has(font):
			_fonts[font] = true
			font.changed.connect(_font_changed)
		if font is FontVariation:
			_record_font(font.base_font, seen)
		else:
			_font_resources[font] = true
		for fallback: Font in font.fallbacks:
			_record_font(fallback, seen)

	func _check(identity: int) -> void:
		_pending.erase(identity)
		var node: Node = _nodes[identity].get_ref() as Node
		if node == null or not node.is_inside_tree():
			return
		_active[identity] = _text_visible(node)
		if not _active[identity]:
			return
		_checked[identity] = true
		var bindings: Dictionary = {}
		if node is Label3D:
			bindings["font"] = node.font
		elif node is RichTextLabel:
			bindings["normal_font"] = node.get_theme_font("normal_font")
		else:
			bindings["font"] = node.get_theme_font("font")
		for slot: String in bindings:
			_record_font(bindings[slot], {})

	func _exit_tree() -> void:
		# Catch late property changes on Label3D (no Control.theme_changed signal).
		for identity: int in _nodes:
			_check(identity)
		print("[MANUS_FONT_BINDINGS]" + JSON.stringify({"scope": "boot-encountered-theme-and-Label3D-bindings", "nodes": _checked.size(), "fontResources": _font_resources.size(), "errors": 0}))
		for font: Font in _fonts:
			font.changed.disconnect(_font_changed)
		_fonts.clear()
		_font_resources.clear()
		_world_fonts.clear()
		_active.clear()
