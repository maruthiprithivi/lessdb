//! Situated presence: what a character is attending to, what is in their hands,
//! who is with them and what they can perceive around them.
//!
//! This is a deterministic, rule-based policy (`deterministic:presence/v1`),
//! not model reasoning and not a claim of awareness. Every field is derived
//! from authoritative simulation state (clock, place, travel, schedule
//! activity, messages, carried items, weather, co-located agents), so the same
//! seed and history always yield the same presence. The renderer draws props
//! only from this state, never from text heuristics, so a phone, leash or bag
//! appears only while the situation justifies it.

use serde::{Deserialize, Serialize};

pub const POLICY: &str = "deterministic:presence/v1";
pub const SCHEMA: &str = "lessville-situated-presence/v1";
/// Minutes a remote message keeps a phone in hand (read / reply window).
pub const PHONE_WINDOW: i64 = 4;
/// Minutes a take-away drink lasts in hand before it is finished.
pub const DRINK_MINUTES: i64 = 30;
const CAFE_STAFF: [&str; 2] = ["maya", "bo"];
const INDOOR: [&str; 6] = ["home", "cafe", "office", "grocery", "school", "gym"];

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presence {
    /// conversation | task | navigation | waiting | environment | rest
    pub attention: String,
    /// free | phone | leash | carrying | serving
    pub hands: String,
    /// Item held when `hands == "carrying"` (coffee, milk, grocery_bag, backpack).
    #[serde(default)]
    pub item: Option<String>,
    /// Companion physically with the character (only "mochi" today).
    #[serde(default)]
    pub companion: Option<String>,
    #[serde(default)]
    pub umbrella: bool,
    /// indoor | outdoor | in_transit | away (off the map)
    pub setting: String,
    /// Other foreground characters sharing the place or the same street segment.
    #[serde(default)]
    pub nearby: Vec<String>,
    /// Perceivable conditions: rain, wet_ground, dark, crowded, bus_due, dog_on_leash.
    #[serde(default)]
    pub surroundings: Vec<String>,
    /// Rule that produced `hands` (traceable explanation, not free text).
    pub reason: String,
    pub since: i64,
    pub policy: String,
}

/// Everything the policy may look at. Built by the simulation each minute.
pub struct Situation<'a> {
    pub id: &'a str,
    pub location: &'a str,
    pub activity: &'a str,
    pub activity_kind: &'a str,
    pub travel_from: Option<&'a str>,
    pub travel_to: Option<&'a str>,
    pub travel_mode: Option<&'a str>,
    pub carrying: Option<&'a str>,
    pub talking: bool,
    pub minute: i64,
    /// Minute of the latest remote (not face-to-face) message sent or received.
    pub last_remote_message: Option<i64>,
    pub raining: bool,
    pub wet_ground: bool,
    pub dark: bool,
    pub nearby: Vec<String>,
    pub crowd: usize,
    pub bus_due: bool,
}

fn outdoor_place(loc: &str) -> bool {
    !INDOOR.contains(&loc)
}

pub fn derive(s: &Situation, previous: Option<&Presence>) -> Presence {
    let act = s.activity.to_lowercase();
    let travelling = s.travel_to.is_some();
    let in_vehicle = travelling && matches!(s.travel_mode, Some("bus") | Some("car"));
    let setting = if s.location == "away" && !travelling {
        // off the map (out of town): nothing here is observable
        "away"
    } else if travelling {
        "in_transit"
    } else if outdoor_place(s.location) {
        "outdoor"
    } else {
        "indoor"
    };
    let outdoors = !matches!(setting, "indoor" | "away") && !in_vehicle;

    // Mochi is with Ama only on a dog walk: travelling to or from the park or
    // trail, or out at the park/trail itself. Never indoors, never on the bus.
    let walk_place = |p: Option<&str>| matches!(p, Some("park") | Some("trail"));
    let on_walk = s.id == "ama"
        && !in_vehicle
        && (act.contains("mochi")
            || walk_place(s.travel_to)
            || walk_place(s.travel_from)
            || (matches!(s.location, "park" | "trail") && !travelling));
    let companion = (on_walk && outdoors).then(|| "mochi".to_string());

    let serving = matches!(
        s.activity_kind,
        "serving" | "opening" | "closing" | "checking_stock"
    ) && s.location == "cafe"
        && CAFE_STAFF.contains(&s.id);
    let busy_task = matches!(
        s.activity_kind,
        "typing" | "cooking" | "workout" | "studying"
    );
    let phone_window = s
        .last_remote_message
        .map(|m| s.minute >= m && s.minute < m + PHONE_WINDOW)
        .unwrap_or(false);
    // Item lifecycles: shopping is put away once home and settled, a drink is
    // finished after DRINK_MINUTES in hand, a backpack is set down (kept, not
    // held) while settled indoors.
    let drink_done = s.carrying == Some("coffee")
        && previous
            .map(|p| p.item.as_deref() == Some("coffee") && s.minute - p.since >= DRINK_MINUTES)
            .unwrap_or(false);
    let set_down_reason = match s.carrying {
        Some("grocery_bag") | Some("milk") if s.location == "home" && !travelling => {
            Some("set_down_at_home")
        }
        Some("coffee") if drink_done => Some("finished_drink"),
        Some("backpack") if setting == "indoor" => Some("backpack_set_down"),
        _ => None,
    };
    let carry = s.carrying.filter(|_| set_down_reason.is_none());

    let (hands, item, reason) = if companion.is_some() {
        ("leash", None, "walking_mochi_outdoors")
    } else if serving {
        ("serving", None, "serving_at_counter")
    } else if let Some(item) = carry {
        ("carrying", Some(item.to_string()), "carrying_item")
    } else if let Some(r) = set_down_reason.filter(|_| !phone_window) {
        ("free", None, r)
    } else if phone_window && s.talking {
        ("free", None, "phone_deferred_in_conversation")
    } else if phone_window && busy_task {
        ("free", None, "phone_deferred_busy_task")
    } else if phone_window {
        ("phone", None, "reading_remote_message")
    } else {
        ("free", None, "no_prop_needed")
    };

    let attention = if s.talking {
        "conversation"
    } else if travelling {
        "navigation"
    } else if s.location == "bus_stop" {
        "waiting"
    } else if s.activity_kind == "sleeping" || act.contains("asleep") {
        "rest"
    } else if hands == "phone"
        || busy_task
        || serving
        || matches!(s.activity_kind, "shopping" | "serving")
    {
        "task"
    } else {
        "environment"
    };

    let mut surroundings = Vec::new();
    if setting == "away" {
        // no perception claims about places the simulation does not model
    } else if s.raining {
        surroundings.push("rain".to_string());
    } else if s.wet_ground && outdoors {
        surroundings.push("wet_ground".to_string());
    }
    if s.dark && setting != "away" {
        surroundings.push("dark".to_string());
    }
    if s.crowd >= 4 {
        surroundings.push("crowded".to_string());
    }
    if s.bus_due && (s.location == "bus_stop" || s.travel_mode == Some("bus")) {
        surroundings.push("bus_due".to_string());
    }
    if companion.is_some() {
        surroundings.push("dog_on_leash".to_string());
    }
    let umbrella = s.raining && outdoors && hands != "serving";

    let mut p = Presence {
        attention: attention.into(),
        hands: hands.into(),
        item,
        companion,
        umbrella,
        setting: setting.into(),
        nearby: s.nearby.clone(),
        surroundings,
        reason: reason.into(),
        since: s.minute,
        policy: POLICY.into(),
    };
    if let Some(prev) = previous {
        if same_state(prev, &p) {
            p.since = prev.since;
        }
    }
    p
}

/// Whether two presences are the same observable state (ignores `since`).
pub fn same_state(a: &Presence, b: &Presence) -> bool {
    a.attention == b.attention
        && a.hands == b.hands
        && a.item == b.item
        && a.companion == b.companion
        && a.umbrella == b.umbrella
        && a.setting == b.setting
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base<'a>(id: &'a str, loc: &'a str) -> Situation<'a> {
        Situation {
            id,
            location: loc,
            activity: "",
            activity_kind: "",
            travel_from: None,
            travel_to: None,
            travel_mode: None,
            carrying: None,
            talking: false,
            minute: 600,
            last_remote_message: None,
            raining: false,
            wet_ground: false,
            dark: false,
            nearby: vec![],
            crowd: 0,
            bus_due: false,
        }
    }

    #[test]
    fn no_props_by_default() {
        let p = derive(&base("sam", "home"), None);
        assert_eq!(p.hands, "free");
        assert_eq!(p.companion, None);
        assert!(!p.umbrella);
        assert_eq!(p.policy, POLICY);
    }

    #[test]
    fn phone_only_inside_a_short_remote_message_window() {
        let mut s = base("sam", "bus_stop");
        s.last_remote_message = Some(598);
        assert_eq!(derive(&s, None).hands, "phone");
        s.minute = 598 + PHONE_WINDOW;
        assert_eq!(
            derive(&s, None).hands,
            "free",
            "phone is put away after the window"
        );
        s.minute = 599;
        s.talking = true;
        assert_eq!(derive(&s, None).reason, "phone_deferred_in_conversation");
        s.talking = false;
        s.activity_kind = "typing";
        s.location = "office";
        assert_eq!(
            derive(&s, None).hands,
            "free",
            "no phone while typing at a desk"
        );
    }

    #[test]
    fn mochi_and_leash_only_on_an_outdoor_walk() {
        let mut s = base("ama", "home");
        s.activity = "Feeding Mochi";
        assert_eq!(derive(&s, None).companion, None, "Mochi stays home indoors");
        s.travel_from = Some("home");
        s.travel_to = Some("park");
        s.travel_mode = Some("walk");
        let walk = derive(&s, None);
        assert_eq!(walk.companion.as_deref(), Some("mochi"));
        assert_eq!(walk.hands, "leash");
        s.travel_mode = Some("bus");
        assert_eq!(derive(&s, None).companion, None, "no dog on the bus");
        let mut other = base("sam", "park");
        other.activity = "walk";
        assert_eq!(derive(&other, None).companion, None, "only Ama walks Mochi");
        let mut back = base("ama", "home");
        back.travel_from = Some("park");
        back.travel_to = Some("home");
        back.travel_mode = Some("walk");
        assert_eq!(
            derive(&back, None).hands,
            "leash",
            "still leashed on the way home"
        );
    }

    #[test]
    fn shopping_is_set_down_at_home_and_serving_has_no_phone() {
        let mut s = base("ama", "home");
        s.carrying = Some("grocery_bag");
        let p = derive(&s, None);
        assert_eq!(
            (p.hands.as_str(), p.reason.as_str()),
            ("free", "set_down_at_home")
        );
        s.location = "grocery";
        assert_eq!(derive(&s, None).item.as_deref(), Some("grocery_bag"));
        let mut b = base("bo", "cafe");
        b.activity_kind = "serving";
        b.last_remote_message = Some(600);
        assert_eq!(derive(&b, None).hands, "serving");
    }

    #[test]
    fn customers_in_the_queue_are_not_staff_and_items_have_lifecycles() {
        let mut c = base("lin", "cafe");
        c.activity_kind = "serving";
        assert_eq!(
            derive(&c, None).hands,
            "free",
            "queueing customer is not serving"
        );
        let mut d = base("sam", "street");
        d.carrying = Some("coffee");
        let held = derive(&d, None);
        assert_eq!(held.item.as_deref(), Some("coffee"));
        d.minute = 600 + DRINK_MINUTES;
        assert_eq!(derive(&d, Some(&held)).reason, "finished_drink");
        let mut bag = base("lin", "school");
        bag.carrying = Some("backpack");
        assert_eq!(derive(&bag, None).reason, "backpack_set_down");
        bag.travel_from = Some("school");
        bag.travel_to = Some("cafe");
        bag.travel_mode = Some("walk");
        assert_eq!(derive(&bag, None).item.as_deref(), Some("backpack"));
    }

    #[test]
    fn umbrella_and_surroundings_follow_weather_and_setting() {
        let mut s = base("lin", "park");
        s.raining = true;
        s.dark = true;
        s.crowd = 5;
        let p = derive(&s, None);
        assert!(p.umbrella);
        assert_eq!(p.surroundings, vec!["rain", "dark", "crowded"]);
        s.location = "cafe";
        assert!(!derive(&s, None).umbrella, "no umbrella indoors");
        s.travel_to = Some("school");
        s.travel_from = Some("home");
        s.travel_mode = Some("bus");
        assert!(!derive(&s, None).umbrella, "no umbrella inside the bus");
        let mut away = base("lin", "away");
        away.raining = true;
        away.dark = true;
        let p = derive(&away, None);
        assert_eq!(p.setting, "away");
        assert!(
            !p.umbrella && p.surroundings.is_empty(),
            "no claims about off-map places"
        );
    }

    #[test]
    fn since_is_stable_while_state_holds_and_replay_is_deterministic() {
        let mut s = base("sam", "bus_stop");
        let first = derive(&s, None);
        s.minute = 610;
        let later = derive(&s, Some(&first));
        assert_eq!(later.since, 600);
        assert_eq!(later.attention, "waiting");
        s.last_remote_message = Some(610);
        let changed = derive(&s, Some(&later));
        assert_eq!(changed.since, 610);
        assert_eq!(derive(&s, Some(&later)), changed);
    }
}
