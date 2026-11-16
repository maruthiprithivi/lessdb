extends RefCounted

# The title menu owns the entry; the shared Web exporter owns the versioned page.
static func open(parent: Node) -> void:
	if OS.has_feature("web"):
		JavaScriptBridge.eval("window.open(new URL('open-source-licenses.html', window.location.href).href, '_blank', 'noopener');", true)
		parent.get_viewport().set_input_as_handled()
		return
	var dialog := AcceptDialog.new()
	dialog.name = "OpenSourceLicensesDialog"
	dialog.title = "Open Source Licenses"
	var text := TextEdit.new()
	text.editable = false
	text.wrap_mode = TextEdit.LINE_WRAPPING_BOUNDARY
	text.custom_minimum_size = Vector2(560, 320)
	text.text = "Godot Engine " + Engine.get_version_info().string + "\n\n" + Engine.get_license_text()
	text.text += "\n\n" + JSON.stringify(Engine.get_copyright_info(), "\t")
	for license_name: String in Engine.get_license_info():
		text.text += "\n\n" + license_name + "\n" + str(Engine.get_license_info()[license_name])
	dialog.add_child(text)
	parent.add_child(dialog)
	dialog.confirmed.connect(dialog.queue_free)
	dialog.canceled.connect(dialog.queue_free)
	dialog.popup_centered()
	parent.get_viewport().set_input_as_handled()
