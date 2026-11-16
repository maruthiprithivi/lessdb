extends RefCounted
## Explicit walkable polygons for the Lessville town map (1536 x 1024, map px).
##
## The town is drawn in the existing top-down / three-quarter projection, so
## "where a pedestrian may stand" is a flat polygon set rather than a physics
## world.  The set is a union of:
##
##   * SIDEWALK corridors - one quad per edge of the street graph in
##     town_view.gd (NODES / EDGES).  The graph already traces the paved walks,
##     so a corridor of HALF px either side of an edge covers the pavement
##     without reaching the roadway, the grass beds or the building interiors.
##   * SIDEWALK plazas - paved areas wider than one edge.  Today that is the
##     covered waiting area under and in front of the bus shelter.
##   * CROSSINGS - the two painted zebra crossings plus the bus boarding apron
##     at the shelter door, where a rider steps off the curb into the bus.
##
## Both routing (town_view._path) and containment use this same set: every
## pedestrian sample can be tested with contains() and the routing polyline is
## built from corridor lines and the west crossing's directional lanes. The
## crossing corner plaza covers the bent lane joins; tests sample route containment.
##
## All coordinates are map pixels.  contains() allows a small epsilon so a point
## sitting exactly on a polygon edge still counts as inside.

const EPS := 2.0

## Street-graph edge + half width of its sidewalk corridor (map px).
const CORRIDORS := [
	["A", "B", 22.0],   # home front path
	["B", "C", 20.0],   # home corner -> office plaza
	["B", "W", 16.0],   # west exit
	["C", "D", 20.0],   # plaza -> office front
	["D", "F", 16.0],   # office frontage
	["F", "E", 18.0],   # office door
	["F", "G", 14.0],   # office -> east plaza
	["G", "H", 16.0],   # east plaza corner
	["H", "T", 14.0],   # terrace south edge (below the planters)
	["T", "I", 12.0],   # terrace east edge -> cafe door
	["I", "J", 22.0],   # cafe door
	["H", "X", 20.0],   # east exit
	["C", "K", 22.0],   # plaza -> shelter
	["K", "N", 22.0],   # shelter front -> bench
	["K", "L", 26.0],   # fountain promenade (west)
	["L", "M", 26.0],   # fountain promenade (east)
	["M", "H", 22.0],   # east promenade
	["K", "SW", 22.0],  # shelter frontage -> west crossing
	["SW", "S", 24.0],  # west crossing -> trail gate
]

## Paved sidewalk areas that are not a single graph edge.
const PLAZAS := [
	{
		"name": "west_crossing_corner",
		"poly": [Vector2(191, 616), Vector2(239, 616), Vector2(239, 664), Vector2(191, 664)],
	},
	{
		"name": "bus_shelter",
		# Covered waiting area: the shelter footprint plus the pavement in front
		# of it, stopping at the yellow curb (y = 734).
		"poly": [Vector2(350, 592), Vector2(565, 592), Vector2(565, 732), Vector2(350, 732)],
	},
]

## Marked crossings and the bus boarding apron.
const CROSSINGS := [
	{
		"name": "crossing_west",
		# Painted zebra across the road at x ~ 172..266, joined to both curbs.
		"poly": [Vector2(158, 726), Vector2(278, 726), Vector2(278, 912), Vector2(158, 912)],
	},
	{
		"name": "crossing_east",
		# Painted zebra by the cafe corner at x ~ 1249..1337.
		"poly": [Vector2(1238, 726), Vector2(1348, 726), Vector2(1348, 912), Vector2(1238, 912)],
	},
	{
		"name": "bus_boarding",
		# Curb -> bus door (BUS_DOOR 545,772).  Boarding is the one place a
		# pedestrian may stand on the roadway without a zebra.
		"poly": [Vector2(478, 726), Vector2(606, 726), Vector2(606, 806), Vector2(478, 806)],
	},
]


## Build the polygon list from the street-graph nodes.
## Returns [{kind: "sidewalk"|"crossing", name: String, poly: PackedVector2Array}].
static func build(nodes: Dictionary) -> Array:
	var out: Array = []
	for c: Array in CORRIDORS:
		var a: Vector2 = nodes[c[0]]
		var b: Vector2 = nodes[c[1]]
		out.append({
			"kind": "sidewalk",
			"name": "walk_%s_%s" % [c[0], c[1]],
			"poly": corridor(a, b, float(c[2])),
		})
	for p: Dictionary in PLAZAS:
		out.append({"kind": "sidewalk", "name": String(p["name"]), "poly": PackedVector2Array(p["poly"])})
	for p: Dictionary in CROSSINGS:
		out.append({"kind": "crossing", "name": String(p["name"]), "poly": PackedVector2Array(p["poly"])})
	return out


## A rectangle of `half` px either side of the segment a-b.
static func corridor(a: Vector2, b: Vector2, half: float) -> PackedVector2Array:
	var n := (b - a).normalized().orthogonal() * half
	return PackedVector2Array([a + n, b + n, b - n, a - n])


## True when p is inside `poly`, or within `eps` of one of its edges.
static func contains(poly: PackedVector2Array, p: Vector2, eps: float = EPS) -> bool:
	if Geometry2D.is_point_in_polygon(p, poly):
		return true
	var n := poly.size()
	for i in n:
		var a := poly[i]
		var b := poly[(i + 1) % n]
		if p.distance_to(Geometry2D.get_closest_point_to_segment(p, a, b)) <= eps:
			return true
	return false


## True when any polygon in `polys` (as returned by build) contains p.
static func contains_any(polys: Array, p: Vector2, eps: float = EPS) -> bool:
	for entry: Dictionary in polys:
		if contains(entry["poly"], p, eps):
			return true
	return false


## Nearest point inside the union of `polys`, used to keep cosmetic offsets on
## the pavement.  Returns p unchanged when it is already walkable.
static func clamp_inside(polys: Array, p: Vector2) -> Vector2:
	if contains_any(polys, p):
		return p
	var best := p
	var best_d := INF
	for entry: Dictionary in polys:
		var poly: PackedVector2Array = entry["poly"]
		var centre := Vector2.ZERO
		for v: Vector2 in poly:
			centre += v
		centre /= maxf(float(poly.size()), 1.0)
		var n := poly.size()
		for i in n:
			var a := poly[i]
			var b := poly[(i + 1) % n]
			var q := Geometry2D.get_closest_point_to_segment(p, a, b)
			var d := p.distance_to(q)
			if d < best_d:
				best_d = d
				# Nudge a little toward the polygon centre so the result is not
				# exactly on the boundary.
				best = q.lerp(centre, 0.02)
	return best
