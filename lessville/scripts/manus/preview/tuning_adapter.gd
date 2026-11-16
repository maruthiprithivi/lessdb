extends "res://scripts/manus/preview/tuning_transport.gd"

func store() -> Variant:
	return get_node_or_null("/root/TuningStore")

func settings() -> Array:
	return store().get_settings()

func requested(setting: Dictionary) -> Variant:
	return store().get_requested_value(str(setting.id))

func active(setting: Dictionary) -> Variant:
	return store().get_active_value(str(setting.id))

func commit(patch: Dictionary) -> bool:
	return store().set_values(patch, false)

func control_for(setting: Dictionary) -> Dictionary:
	var localized := setting.duplicate(true)
	localized["category_key"] = "tuning.category." + str(setting.category).to_lower()
	return super.control_for(localized)
