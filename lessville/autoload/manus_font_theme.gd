extends Node
## Bind the imported UI composites (Latin body face + Noto Sans SC) at runtime, after the editor's first import pass.
const GameFonts = preload("res://scripts/manus/game_fonts.gd")
var _regular: Font
var _theme: Theme

func _enter_tree() -> void:
	_regular = load("res://assets/template/fonts/ui_regular.tres") as Font
	assert(_regular != null, "Bundled UI font failed to import")
	_theme = GameFonts.create_theme(_regular, load("res://assets/template/fonts/ui_medium.tres") as Font, load("res://assets/template/fonts/ui_bold.tres") as Font)
	get_tree().node_added.connect(_bind_node)
	_bind_tree(get_tree().root)

func _bind_tree(node: Node) -> void:
	_bind_node(node)
	for child in node.get_children():
		_bind_tree(child)

func _bind_node(node: Node) -> void:
	if node is Control:
		if node.theme != null:
			if node.theme.default_font == null:
				node.theme = node.theme.duplicate()
				node.theme.default_font = _regular
		elif node.get_parent_control() == null:
			node.theme = _theme
	elif node is Label3D and node.font == null:
		GameFonts.apply_world_label(node, _regular)
