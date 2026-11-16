# Lessville — Real-World Flows, Views and Movement Design

Status: design for the next build. Everything listed under **Build now** is implemented in this pass;
items under **Later** are deliberately deferred so this pass stays shippable.

## 1. Principles

1. **Continuity over teleporting.** A character is always somewhere: inside a building, walking a
   sidewalk, waiting at the stop, or seated on the bus. Every change of place is a visible walk,
   a door transition or a bus ride. Nothing pops in mid-street.
2. **Characters face where they go.** Walking down the screen shows the front pose, walking up the
   screen shows the back pose, walking sideways shows the side pose facing the direction of travel.
   Nobody walks "backwards".
3. **No backtracking.** When a plan changes mid-walk, the character continues from where they are
   along the street they are on, then turns at the next corner. They never reverse to a junction
   behind them unless that is genuinely the shortest way.
4. **Real-time choreography, simulated decisions.** The LessDB service decides *what* happens and
   *when* in simulated time. The client animates *how* it looks at a human pace (walk speed, bus
   braking, door pauses), so speeding up the clock does not turn people into streaks.
5. **Every scene shows the database at work.** Each view has a "what LessDB is doing here" strip:
   the recall a character just ran, the memory written, or the message sent, linked to provenance.

## 2. Movement rules (town map)

| Rule | Behaviour |
|---|---|
| Facing | 4 directions. Up = back sprite, down = front sprite, left/right = side walk (mirrored). Last facing is kept when idle. |
| Speed | Human pace ≈ 1.4 m/s on screen; Ama slightly slower, Lin slightly faster. Speeds scale gently with clock speed (√speed, capped). |
| Start/stop | Short ease-in and ease-out; a 0.4 s pause at doors before fading in or out. |
| Re-planning | New path starts from the walker's current segment endpoint that lies toward the goal; never from a junction behind. |
| Sidewalk lanes | Each character keeps a small lane offset, so two people walking the same street pass side by side instead of merging. |
| Doors | Entering: walk to the door, pause, fade. Leaving: fade in at the door, pause, walk. |
| Waiting | At the bus stop, riders stand under the shelter facing the road; at the park they idle near the fountain. |

## 3. The bus (town map)

The bus is a vehicle with a timetable rather than a sprite that flashes across the screen.

1. **Approach** (≈6 s real time): enters from the left edge on the road and brakes smoothly into the stop.
2. **Dwell** (≈5–8 s): stops at the shelter. Riders getting off appear at the door and walk away.
   Riders waiting at the stop walk to the door one at a time and board. The bus waits until
   everyone has boarded (bounded).
3. **Depart** (≈6 s): accelerates to the right and leaves the map toward the University.
4. **Delays are physical.** A delayed bus (road works, observer nudge) really arrives later. The
   people at the stop keep waiting, and a "Bus +15 min" sign appears on the shelter.
5. **Clicking the bus** while someone is on board opens the **On the bus** view.

## 4. Daily flows

Times are Asia/Singapore simulated time. **DB** marks the LessDB operations involved.

### Sam — office worker (commute on foot)

| Time | Scene | What you see | DB |
|---|---|---|---|
| 07:45 | Home → café | Leaves the front door, walks the sidewalk to Less Café | travel row in world checkpoint |
| 07:55 | Café | Orders a flat white at the counter; staff recall his shared usual | recall `pref.usual`, decision row |
| 08:10 | Café → office | Walks across the plaza with his coffee | — |
| 08:30 | Office | Badge in at the door, walks to **his desk**, sits and types | event row |
| 09:30 | Office | **Stand-up** at the meeting table with coworkers | message to #team |
| 10:00–12:30 | Office | Focus work at the desk (typing animation) | memory: report progress |
| 12:30 | Office → café | Lunch at the table Maya held for him | message + temporary memory |
| 13:15 | Café → office | Back to the desk | — |
| 15:00 | Office | Coffee-machine chat with a coworker | say bubble |
| 18:00 | Office → grocery/home | Reads the family channel; buys eggs only if nobody else is | inbox recall, decision |
| 19:00 | Home | Dinner with Ama and Mochi | — |

### Lin — student, bus commuter

| Time | Scene | What you see | DB |
|---|---|---|---|
| 07:00 | Off-map dorm | Decides which bus to take using her memory of yesterday's delay | recall `bus.delay`, decision |
| 07:40/08:00 | **On the bus** (inbound) | Seated by the window reviewing notes; the next-stop sign counts down | memory if late |
| arrival | Bus stop → café | Steps off, walks to the café | — |
| 08:20 | Café | Orders an oat latte, studies with headphones | consent-scoped preference |
| 10:00 | Café → bus stop | Waits under the shelter | — |
| 10:00+ | **On the bus** (outbound) | Rides to the University | — |
| 10:30 | University | Lecture: Intro to Databases | — |
| 15:00 | **On the bus** back | Rides back | — |
| 17:30 | Café → gym → dorm | Walks off-map to the gym, later home | — |

### Ama — grandmother with Mochi

| Time | Scene | What you see | DB |
|---|---|---|---|
| 07:00 | Home → park | Morning walk with Mochi around the fountain | — |
| 07:40 | Park → home | Walks back | — |
| 09:30 | Home | Kitchen; checks the fridge; asks the family channel about eggs | message #family |
| 09:30+ | Home → grocery | Shops only if no one has claimed the errand | recall, decision |
| 16:30 | Home → trail or park | Chooses the park if a muddy-trail memory is recent | recall `trail.muddy` |
| 18:30 | Home | Cooking dinner | — |

### Maya — café owner / Bo — barista

| Time | Scene | What you see | DB |
|---|---|---|---|
| 06:30 | Café | Maya opens up: lights on, behind the counter | — |
| 07:00 | Café | Bo arrives from off-map, ties apron, starts the espresso machine | — |
| Rush | Café | Bo on espresso, Maya on the till; customers queue at the counter | recall usuals, decisions |
| After oat orders | Café | Bo checks the fridge; posts a restock request | message #cafe-staff |
| 14:00 | Café → grocery | Maya leaves for the supplier run | decision from Bo's request |
| 15:00 | Café → trail | Bo walks home via the trail; reports mud if it rained | shared memory `trail.muddy` |
| 20:00 | Café | Maya closes up | — |

## 5. Views

| View | Layout and stations | Live activity |
|---|---|---|
| **Town** | Street graph, buildings, bus road, shelter | Walkers, bus choreography, weather, day/night, speech bubbles |
| **Home** | Front door, kitchen counter, dining table, sofa, Mochi's bed | Ama cooks at the counter, Sam at the dining table in the evening, Mochi naps |
| **Office** | Door, Sam's desk, two coworker desks, meeting table, coffee machine | Sam types at his desk; coworkers type; stand-up at the meeting table; coffee chat |
| **Less Café** | Door, counter/till, espresso machine, fridge, tables 1–3 | Maya at the till, Bo at the espresso machine or fridge, customers seated or queuing |
| **On the bus** | Driver, two seat rows, windows with the city scrolling by, next-stop sign | Riders seated by the window; Lin reviews notes; ETA counts down from the travel record |
| **FreshMart** | Entrance, aisles, checkout | Shopper browses an aisle, then checks out |
| **University** | Lecture seats, lectern | Lin in a lecture |
| **Gym** | Treadmill, weights | Lin works out |
| **Trail** | Dirt path waypoints | Walkers follow the path; Mochi trots alongside |

Inside every view, characters enter at the door, walk to their station, perform the station's
activity (typing, pouring, cooking, seated, browsing), and walk back to the door when leaving.
Stations come from an explicit `station` field the service derives from location and activity, so
the view and the LessDB record agree on what the character is doing.

## 6. Build now (this pass)

1. Four-direction facing with new back-walk frames for Maya, Sam, Ama and Bo (Lin's already exist).
2. Walker re-planning without backtracking, sidewalk lanes, door pauses and ease-in/out.
3. Bus timetable choreography: approach, dwell with boarding and alighting, departure, delay sign.
4. **On the bus** interior view with seated riders, scrolling window scenery, next-stop sign and ETA.
5. Station-based interiors for home, office, café, grocery, school, gym and trail, with walking
   from the door to stations, plus office coworkers and the café espresso machine as ambient life.
6. Service `station` field and richer office routine (stand-up, focus work, coffee chat).

## 7. Later

- Pathfinding inside interiors around furniture (stations are connected by straight walkable lanes).
- More ambient townspeople on the sidewalks.
- Per-character idle animations beyond talking, typing and walking.
