//! Lessville world: deterministic, rule-based agents whose memories,
//! messages, decisions and observations are LessDB rows. An optional model
//! (see llm.rs) only re-voices chosen lines and writes nightly reflections;
//! it never makes decisions.
//!
//! Policy rules are explicit and versioned (`policy` column). Recall is a
//! real SQL SELECT whose scope filter is an application-layer rule
//! expressed as a subquery over `lv_scope_members`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::store::{Row, Store, i, q, s};

mod daily_life;
use daily_life::Life;

pub const DAY: i64 = 1440;
pub const DAY_START: i64 = 390; // 06:30
pub const DAY_END: i64 = 1320; // 22:00
const FEED_CAP: usize = 120;
pub const SUPPORTING_POPULATION: usize = 95;
pub const TOWN_SUPPORTING_POPULATION: usize = 12;

/// Newest-first bounded evidence list for upserted rows. The evidence count
/// still records every observation; only the reference list is bounded, so a
/// long-lived competency row cannot grow without limit (2026-11-14 outage).
pub const MAX_ROW_SOURCE_REFS: usize = 24;
pub fn bounded_refs(prior: &str, new: &[String]) -> String {
    let mut out: Vec<&str> = Vec::new();
    for r in prior.split('|').chain(new.iter().map(String::as_str)) {
        let r = r.trim();
        if r.is_empty() {
            continue;
        }
        out.retain(|x| *x != r);
        out.push(r);
    }
    let skip = out.len().saturating_sub(MAX_ROW_SOURCE_REFS);
    out[skip..].join("|")
}

pub const TABLES: &[(&str, &str)] = &[
    (
        "lv_kv",
        "CREATE TABLE lv_kv (k String, v String, sim_minute Int64) ENGINE=Firefly ORDER BY k UNIQUE (k)",
    ),
    (
        "lv_agent_state",
        "CREATE TABLE lv_agent_state (agent String, location String, activity String, state String, sim_minute Int64) ENGINE=Firefly ORDER BY agent UNIQUE (agent)",
    ),
    (
        "lv_scope_members",
        "CREATE TABLE lv_scope_members (member_key String, scope String, agent String, granted_by String, sim_minute Int64) ENGINE=Firefly ORDER BY member_key UNIQUE (member_key)",
    ),
    (
        "lv_events",
        "CREATE TABLE lv_events (id Int64, sim_minute Int64, agent String, kind String, location String, x Float64, y Float64, detail String) ENGINE=Firefly ORDER BY id",
    ),
    (
        "lv_messages",
        "CREATE TABLE lv_messages (id Int64, sim_minute Int64, sender String, channel String, recipient String, topic String, context_id String, body String) ENGINE=Firefly ORDER BY id",
    ),
    (
        "lv_memories",
        "CREATE TABLE lv_memories (id Int64, sim_minute Int64, owner String, kind String, scope String, location String, tag String, context_id String, expires_at Int64, source_kind String, source_id Int64, content String) ENGINE=Firefly ORDER BY id",
    ),
    (
        "lv_memory_lifecycle",
        "CREATE TABLE lv_memory_lifecycle (memory_id Int64, state String, changed_at Int64, reason String, source String) ENGINE=Firefly ORDER BY (memory_id, changed_at)",
    ),
    (
        "lv_lessons",
        "CREATE TABLE lv_lessons (lesson_id String, agent String, sim_minute Int64, topic String, trigger String, statement String, state String, confidence Int64, source_refs String, policy_version String, applied_at Int64, auto_apply Int64) ENGINE=Firefly ORDER BY lesson_id UNIQUE (lesson_id)",
    ),
    (
        "lv_relationships",
        "CREATE TABLE lv_relationships (edge_id String, subject_agent String, object_agent String, relation String, consent String, reciprocity String, trust_context String, evidence_refs String, state String, confidence Int64, policy_version String, updated_at Int64, auto_apply Int64) ENGINE=Firefly ORDER BY edge_id UNIQUE (edge_id)",
    ),
    (
        "lv_competencies",
        "CREATE TABLE lv_competencies (competency_id String, agent String, domain String, skill String, level Int64, evidence_count Int64, status String, source_refs String, policy_version String, updated_at Int64, auto_apply Int64) ENGINE=Firefly ORDER BY competency_id UNIQUE (competency_id)",
    ),
    (
        "lv_interactions",
        "CREATE TABLE lv_interactions (interaction_id String, sim_minute Int64, initiator String, recipient String, kind String, context_id String, initiator_action String, recipient_response String, reciprocity String, evidence_refs String, outcome String, auto_apply Int64) ENGINE=Firefly ORDER BY interaction_id UNIQUE (interaction_id)",
    ),
    (
        "lv_identity_threads",
        "CREATE TABLE lv_identity_threads (record_id String, thread_id String, agent String, parent_thread String, operation String, reason String, continuity String, evidence_refs String, policy_version String, sim_minute Int64, auto_apply Int64) ENGINE=Firefly ORDER BY record_id UNIQUE (record_id)",
    ),
    (
        "lv_competency_events",
        "CREATE TABLE lv_competency_events (event_id String, competency_id String, agent String, domain String, skill String, kind String, prior_level Int64, next_level Int64, evidence_refs String, reason String, policy_version String, sim_minute Int64, auto_apply Int64) ENGINE=Firefly ORDER BY event_id UNIQUE (event_id)",
    ),
    (
        "lv_lesson_challenges",
        "CREATE TABLE lv_lesson_challenges (challenge_id String, lesson_id String, agent String, challenger String, kind String, claim String, evidence_refs String, resolution String, sim_minute Int64, auto_apply Int64) ENGINE=Firefly ORDER BY challenge_id UNIQUE (challenge_id)",
    ),
    (
        "lv_teaching_grants",
        "CREATE TABLE lv_teaching_grants (grant_id String, lesson_id String, teacher_agent String, recipient_agent String, scope_id String, purpose String, valid_from Int64, valid_until Int64, state String, evidence_refs String, policy_version String, sim_minute Int64, auto_apply Int64) ENGINE=Firefly ORDER BY grant_id UNIQUE (grant_id)",
    ),
    (
        "lv_counterfactuals",
        "CREATE TABLE lv_counterfactuals (replay_id String, agent String, sim_minute Int64, topic String, baseline_policy String, baseline_action String, learned_policy String, learned_action String, difference String, source_refs String, not_applied Int64) ENGINE=Firefly ORDER BY replay_id UNIQUE (replay_id)",
    ),
    (
        "lv_novelty_events",
        "CREATE TABLE lv_novelty_events (event_id String, sim_minute Int64, day Int64, agent String, trigger String, option String, outcome String, seed Int64, replay_key String, scope String) ENGINE=Firefly ORDER BY event_id UNIQUE (event_id)",
    ),
    (
        "lv_frontier_signals",
        "CREATE TABLE lv_frontier_signals (signal_id String, sim_minute Int64, agent String, capability String, situation String, evidence_refs String, severity Int64, recurrence Int64, state String, proposed_operation String, seed Int64, auto_apply Int64) ENGINE=Firefly ORDER BY signal_id UNIQUE (signal_id)",
    ),
    (
        "lv_evolution_proposals",
        "CREATE TABLE lv_evolution_proposals (proposal_id String, sim_minute Int64, agent String, owner_scope String, operation String, patch String, purpose String, evidence_refs String, affected_scope String, compatibility String, state String, migration_ref String, rollback_ref String, replay_key String, auto_apply Int64) ENGINE=Firefly ORDER BY proposal_id UNIQUE (proposal_id)",
    ),
    (
        "lv_decisions",
        "CREATE TABLE lv_decisions (id Int64, sim_minute Int64, agent String, policy String, recalled String, action String, explanation String) ENGINE=Firefly ORDER BY id",
    ),
    (
        "lv_access",
        "CREATE TABLE lv_access (id Int64, sim_minute Int64, reader String, subject String, visible Int64, hidden Int64, rule String) ENGINE=Firefly ORDER BY id",
    ),
    (
        "lv_llm_calls",
        "CREATE TABLE lv_llm_calls (id Int64, sim_minute Int64, agent String, task String, model String, context_ids String, prompt String, input_tokens Int64, output_tokens Int64, neurons Float64, status String, output String) ENGINE=Firefly ORDER BY id",
    ),
    (
        "lv_orders",
        "CREATE TABLE lv_orders (id Int64, context_id String, sim_minute Int64, customer String, staff String, requested String, ingredient String, alternative String, status String, detail String) ENGINE=Firefly ORDER BY id",
    ),
    (
        "lv_object_transitions",
        "CREATE TABLE lv_object_transitions (id Int64, sim_minute Int64, actor String, object String, from_state String, to_state String, location String, context_id String, detail String) ENGINE=Firefly ORDER BY id",
    ),
    (
        "lv_physical_runs",
        "CREATE TABLE lv_physical_runs (episode_id String, profile String, simulator String, control_interface String, first_observed_at String, config_json String) ENGINE=Firefly ORDER BY episode_id UNIQUE (episode_id)",
    ),
    (
        "lv_physical_observations",
        "CREATE TABLE lv_physical_observations (observation_id String, episode_id String, sequence Int64, sensor_id String, kind String, observed_at String, evidence_hash String, payload_ref String, detail String) ENGINE=Firefly ORDER BY observation_id UNIQUE (observation_id)",
    ),
    (
        "lv_physical_proposals",
        "CREATE TABLE lv_physical_proposals (proposal_id String, episode_id String, action_kind String, state String, idempotency_key String, source_observation_id String, detail String) ENGINE=Firefly ORDER BY proposal_id UNIQUE (proposal_id)",
    ),
    (
        "lv_presence_events",
        "CREATE TABLE lv_presence_events (id Int64, sim_minute Int64, agent String, attention String, hands String, companion String, setting String, reason String, detail String) ENGINE=Firefly ORDER BY id",
    ),
    (
        "lv_physical_reconciliations",
        "CREATE TABLE lv_physical_reconciliations (attempt_id String, proposal_id String, episode_id String, state String, observed_at String, error_code String, detail String) ENGINE=Firefly ORDER BY attempt_id UNIQUE (attempt_id)",
    ),
];

pub const SCOPES: &[(&str, &[&str], &str)] = &[
    (
        "family",
        &["ama", "sam"],
        "Household channel for Ama and Sam",
    ),
    (
        "cafe-staff",
        &["maya", "bo"],
        "Café operations: stock, tables, shifts",
    ),
    (
        "cafe-regulars",
        &["maya", "bo"],
        "Preferences customers chose to share with café staff",
    ),
    (
        "neighborhood",
        &["maya", "bo", "lin", "ama", "sam"],
        "Public notices: buses, weather, trail",
    ),
];

pub fn anchor(loc: &str) -> (f64, f64) {
    match loc {
        "home" => (258.0, 372.0),
        "office" => (820.0, 380.0),
        "cafe" => (1430.0, 392.0),
        "park" => (820.0, 610.0),
        "bus_stop" => (470.0, 728.0),
        "grocery" => (1530.0, 560.0),
        "gym" => (1530.0, 560.0),
        "school" => (1530.0, 760.0),
        "trail" => (300.0, 1010.0),
        _ => (0.0, 560.0),
    }
}

pub fn offmap(loc: &str) -> bool {
    matches!(loc, "away" | "grocery" | "school" | "gym" | "trail")
}

pub fn place_name(loc: &str) -> &'static str {
    match loc {
        "home" => "Home (Ama & Sam)",
        "office" => "Office",
        "cafe" => "Less Café",
        "park" => "Fountain Park",
        "bus_stop" => "Bus stop",
        "grocery" => "FreshMart grocery",
        "school" => "University",
        "gym" => "Gym",
        "trail" => "Reservoir trail",
        "transit" => "On the way",
        _ => "Out of town",
    }
}

pub fn clock(m: i64) -> String {
    let md = m.rem_euclid(DAY);
    format!("{:02}:{:02}", md / 60, md % 60)
}

pub fn day_of(m: i64) -> i64 {
    m.div_euclid(DAY) + 1
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Travel {
    pub from: String,
    pub to: String,
    pub depart: i64,
    pub arrive: i64,
    pub mode: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Bubble {
    pub text: String,
    pub kind: String,
    pub until: i64,
    #[serde(default)]
    pub by: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub role: String,
    pub title: String,
    #[serde(default)]
    pub occupation: String,
    #[serde(default)]
    pub behaviour: String,
    #[serde(default)]
    pub mood: String,
    pub location: String,
    pub activity: String,
    #[serde(default)]
    pub activity_kind: String,
    #[serde(default)]
    pub station: String,
    #[serde(default)]
    pub pose: String,
    pub travel: Option<Travel>,
    pub bubble: Option<Bubble>,
    pub carrying: Option<String>,
    /// Situated presence (deterministic:presence/v1); drives every visible prop.
    #[serde(default)]
    pub presence: crate::presence::Presence,
    pub flags: BTreeMap<String, i64>,
    pub notes: BTreeMap<String, String>,
    pub last_decision: Option<i64>,
    #[serde(default)]
    pub life: Life,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BackgroundCharacter {
    pub id: String,
    pub name: String,
    pub occupation: String,
    pub location: String,
    pub target: String,
    pub activity: String,
    pub mood: String,
    pub next_change: i64,
    pub seed: u64,
    #[serde(default)]
    pub life: Life,
    #[serde(default)]
    pub travel_until: i64,
    #[serde(default)]
    pub eating_until: i64,
    #[serde(default)]
    pub food_portions: u8,
    #[serde(default)]
    pub supplied_day: i64,
    #[serde(default)]
    pub memories: VecDeque<String>,
}

impl BackgroundCharacter {
    fn in_town_center(&self) -> bool {
        self.id
            .strip_prefix("citizen-")
            .and_then(|n| n.parse::<usize>().ok())
            .is_some_and(|n| (1..=TOWN_SUPPORTING_POPULATION).contains(&n))
    }

    fn visible_in_town(&self, now: i64) -> bool {
        self.in_town_center()
            && (self.travel_until > now || matches!(self.location.as_str(), "park" | "bus_stop"))
    }

    fn remember(&mut self, now: i64, observation: &str) {
        if self.memories.len() >= 12 {
            self.memories.pop_front();
        }
        self.memories
            .push_back(format!("Day {} {}: {observation}", day_of(now), clock(now)));
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TrafficActor {
    pub id: String,
    pub kind: String,
    pub route: String,
    pub lane: String,
    pub position: f32,
    pub direction: String,
    pub speed_kmh: i32,
    pub phase: String,
    pub seed: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TrafficLight {
    pub id: String,
    pub intersection: String,
    pub phase: String,
    pub elapsed: i64,
    pub cycle_minutes: i64,
    pub offset: i64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct OccupationChange {
    pub agent: String,
    pub from: String,
    pub to: String,
    pub at: i64,
    pub reason: String,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct CafeOrder {
    pub context_id: String,
    pub customer: String,
    pub staff: String,
    pub requested: String,
    pub ingredient: String,
    pub alternative: String,
    pub status: String,
    pub detail: String,
    pub started_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CafeState {
    pub lifecycle: String,
    pub lights: String,
    pub front_door: String,
    pub till: String,
    pub espresso: String,
    pub fridge: String,
    pub tables: BTreeMap<String, String>,
    pub active_order: Option<CafeOrder>,
    #[serde(default)]
    pub queue: VecDeque<CafeOrder>,
}

impl Default for CafeState {
    fn default() -> Self {
        let mut tables = BTreeMap::new();
        for table in ["table1", "table2", "table3"] {
            tables.insert(table.into(), "free".into());
        }
        Self {
            lifecycle: "closed".into(),
            lights: "off".into(),
            front_door: "locked".into(),
            till: "closed".into(),
            espresso: "off".into(),
            fridge: "closed".into(),
            tables,
            active_order: None,
            queue: VecDeque::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Plan {
    pub at: i64,
    pub agent: String,
    pub action: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BusTrip {
    pub id: String,
    pub route: String,
    pub direction: String,
    pub scheduled: i64,
    pub effective: i64,
    pub phase: String,
}

pub fn bus_trip(scheduled: i64, delay: i64, now: i64) -> BusTrip {
    let md = scheduled.rem_euclid(DAY);
    BusTrip {
        id: format!("day{}-{:04}-inbound", day_of(scheduled), md),
        route: "Lessville loop".into(),
        direction: "inbound".into(),
        scheduled,
        effective: scheduled + delay,
        phase: if now < scheduled + delay {
            "scheduled".into()
        } else if now == scheduled + delay {
            "dwell".into()
        } else {
            "departed".into()
        },
    }
}

pub fn next_bus_start(now: i64, horizon: i64) -> Option<i64> {
    (now..=now + horizon).find(|m| {
        let md = m.rem_euclid(DAY);
        (420..=1260).contains(&md) && (md - 420) % 20 == 0
    })
}

fn seeded_roll(seed: u64, day: i64, bucket: i64) -> u64 {
    let mut x = seed
        ^ (day as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (bucket as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x.wrapping_mul(0x94d0_49bb_1331_11eb)
}

pub fn novelty_choice(
    seed: u64,
    day: i64,
    agent_index: usize,
    bucket: i64,
) -> Option<&'static str> {
    let roll = seeded_roll(
        seed.wrapping_add((agent_index as u64 + 1) * 0x9e37_79b9),
        day,
        bucket,
    );
    if agent_index != 0 && roll % 100 >= 55 {
        return None;
    }
    Some(match roll % 4 {
        0 => "social_detour",
        1 => "weather_route",
        2 => "shared_context_request",
        _ => "skill_experiment",
    })
}

fn storage_seed(seed: u64) -> i64 {
    (seed & i64::MAX as u64) as i64
}

pub fn weather_kind_from_code(code: i32) -> &'static str {
    match code {
        0 => "clear",
        1..=3 => "cloudy",
        45 | 48 => "foggy",
        51..=67 | 80..=82 => "rain",
        71..=77 | 85..=86 => "snow",
        95 | 96 | 99 => "storm",
        _ => "cloudy",
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct FeedItem {
    pub seq: u64,
    pub minute: i64,
    pub kind: String,
    pub actor: String,
    pub title: String,
    pub detail: String,
    pub refs: Vec<String>,
    pub scope: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct NoveltyOpportunity {
    pub id: String,
    pub day: i64,
    pub agent: String,
    pub trigger: String,
    pub option: String,
    pub outcome: String,
    pub seed: u64,
    pub replay_key: String,
    pub scope: String,
    pub opened_at: i64,
    pub expires_at: i64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct FrontierSignal {
    pub id: String,
    pub agent: String,
    pub capability: String,
    pub situation: String,
    pub evidence_refs: Vec<String>,
    pub severity: i64,
    pub recurrence: i64,
    pub state: String,
    pub proposed_operation: String,
    pub seed: u64,
    pub first_seen: i64,
    pub last_seen: i64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct World {
    /// Last nightly compaction report (runtime only; not persisted).
    #[serde(skip)]
    pub last_retention: Option<crate::retention::RetentionReport>,
    /// Latest remote (not face-to-face) message minute per agent; opens a short phone window.
    #[serde(default)]
    pub remote_msg: BTreeMap<String, i64>,
    pub minute: i64,
    pub speed: u32,
    pub paused: bool,
    #[serde(default)]
    pub stop_at: Option<i64>,
    pub rain_until: i64,
    pub last_rain_end: i64,
    pub rain_reason: String,
    #[serde(default)]
    pub weather_kind: String,
    #[serde(default)]
    pub temperature_c: i32,
    #[serde(default)]
    pub wind_kmh: i32,
    #[serde(default)]
    pub weather_changed_at: i64,
    #[serde(default)]
    pub weather_source: String,
    #[serde(default)]
    pub weather_observed_at: i64,
    #[serde(default)]
    pub weather_code: i32,
    #[serde(default)]
    pub world_seed: u64,
    pub stock: BTreeMap<String, i64>,
    pub agents: Vec<Agent>,
    #[serde(default)]
    pub background: Vec<BackgroundCharacter>,
    #[serde(default)]
    pub traffic: Vec<TrafficActor>,
    #[serde(default)]
    pub traffic_lights: Vec<TrafficLight>,
    #[serde(default)]
    pub occupation_changes: Vec<OccupationChange>,
    pub next_id: BTreeMap<String, i64>,
    pub bus_delays: BTreeMap<i64, i64>,
    pub bus_arrived: Option<(i64, i64)>,
    pub errand: Option<String>,
    pub restock: Option<String>,
    pub done: BTreeSet<String>,
    pub plans: Vec<Plan>,
    pub feed: VecDeque<FeedItem>,
    pub feed_seq: u64,
    pub resumed: Option<String>,
    pub generation: i64,
    #[serde(default)]
    pub novelty: Vec<NoveltyOpportunity>,
    #[serde(default)]
    pub frontier: Vec<FrontierSignal>,
    #[serde(default)]
    pub evolution_proposals: Vec<String>,
    #[serde(default)]
    pub cafe: CafeState,
    #[serde(default)]
    pub bus_trip: Option<String>,
}

fn agent(id: &str, name: &str, role: &str, title: &str, loc: &str, act: &str) -> Agent {
    let (activity_kind, station, pose) = activity_fields(id, loc, act);
    Agent {
        id: id.into(),
        name: name.into(),
        role: role.into(),
        title: title.into(),
        occupation: role.into(),
        behaviour: "routine".into(),
        mood: "neutral".into(),
        location: loc.into(),
        activity: act.into(),
        activity_kind: activity_kind.into(),
        station: station.into(),
        pose: pose.into(),
        travel: None,
        bubble: None,
        carrying: None,
        presence: Default::default(),
        flags: BTreeMap::new(),
        notes: BTreeMap::new(),
        last_decision: None,
        life: Life::default(),
    }
}

fn background_characters(seed: u64) -> Vec<BackgroundCharacter> {
    let mut residents: Vec<BackgroundCharacter> = [
        (
            "citizen-01",
            "Nora",
            "Teacher",
            "home",
            "cafe",
            "Walking to coffee",
        ),
        (
            "citizen-02",
            "Ishaan",
            "Courier",
            "grocery",
            "bus_stop",
            "Delivering parcels",
        ),
        (
            "citizen-03",
            "Mei",
            "Nurse",
            "home",
            "park",
            "Taking a short walk",
        ),
        (
            "citizen-04",
            "Ravi",
            "Mechanic",
            "office",
            "grocery",
            "Buying lunch supplies",
        ),
        (
            "citizen-05",
            "Sofia",
            "Designer",
            "cafe",
            "office",
            "Heading to work",
        ),
        (
            "citizen-06",
            "Theo",
            "Student",
            "school",
            "cafe",
            "Meeting a classmate",
        ),
        (
            "citizen-07",
            "June",
            "Shopkeeper",
            "grocery",
            "home",
            "Returning from work",
        ),
        (
            "citizen-08",
            "Arun",
            "Gardener",
            "park",
            "home",
            "Checking the gardens",
        ),
    ]
    .into_iter()
    .enumerate()
    .map(
        |(i, (id, name, occupation, location, target, activity))| BackgroundCharacter {
            id: id.into(),
            name: name.into(),
            occupation: occupation.into(),
            location: location.into(),
            target: target.into(),
            activity: activity.into(),
            mood: if i % 3 == 0 { "busy" } else { "calm" }.into(),
            next_change: DAY_START + 10 + (i as i64 * 7),
            seed: seed.wrapping_add((i as u64 + 1) * 7919),
            life: Life::default(),
            travel_until: 0,
            eating_until: 0,
            food_portions: 3,
            supplied_day: 1,
            memories: VecDeque::new(),
        },
    )
    .collect();
    let names = [
        "Aisha", "Ben", "Chloe", "Dev", "Elena", "Farid", "Grace", "Hana", "Ivan", "Jia", "Kai",
        "Leah", "Mateo", "Nadia", "Omar", "Priya", "Quinn", "Rosa", "Siti", "Tariq",
    ];
    let surnames = ["Tan", "Lim", "Lee", "Chen", "Kumar"];
    let jobs = [
        "Teacher",
        "Courier",
        "Nurse",
        "Mechanic",
        "Designer",
        "Student",
        "Shopkeeper",
        "Gardener",
    ];
    for i in residents.len()..SUPPORTING_POPULATION {
        residents.push(BackgroundCharacter {
            id: format!("citizen-{:02}", i + 1),
            name: format!("{} {}", names[i % names.len()], surnames[i / names.len()]),
            occupation: jobs[i % jobs.len()].into(),
            location: "away".into(),
            target: "away".into(),
            activity: "At home in the surrounding neighbourhood".into(),
            mood: "calm".into(),
            next_change: DAY_START,
            seed: seed.wrapping_add((i as u64 + 1) * 7919),
            life: Life::default(),
            travel_until: 0,
            eating_until: 0,
            food_portions: 3,
            supplied_day: 1,
            memories: VecDeque::new(),
        });
    }
    for (i, resident) in residents.iter_mut().enumerate() {
        resident.location = "away".into();
        resident.target = "away".into();
        resident.life.hunger += (i % 12) as f64;
        resident.activity = "At home in the surrounding neighbourhood".into();
    }
    residents
}

fn traffic_actors(seed: u64) -> Vec<TrafficActor> {
    [
        ("car-01", "car", "westbound", 12.0, "east", 24),
        ("car-02", "car", "westbound", 54.0, "east", 18),
        ("car-03", "car", "eastbound", 82.0, "west", 22),
        ("bike-01", "bicycle", "cycle-lane", 28.0, "east", 14),
        ("bike-02", "bicycle", "cycle-lane", 70.0, "west", 12),
        ("van-01", "delivery_van", "loading-lane", 45.0, "east", 12),
    ]
    .into_iter()
    .enumerate()
    .map(
        |(i, (id, kind, lane, position, direction, speed_kmh))| TrafficActor {
            id: id.into(),
            kind: kind.into(),
            route: "main_street".into(),
            lane: lane.into(),
            position,
            direction: direction.into(),
            speed_kmh,
            phase: "cruising".into(),
            seed: seed.wrapping_add((i as u64 + 1) * 104729),
        },
    )
    .collect()
}

fn traffic_lights() -> Vec<TrafficLight> {
    vec![
        TrafficLight {
            id: "signal-main-east-west".into(),
            intersection: "main-street-junction".into(),
            phase: "green".into(),
            elapsed: 0,
            cycle_minutes: 12,
            offset: 6,
        },
        TrafficLight {
            id: "signal-market-crossing".into(),
            intersection: "market-crossing".into(),
            phase: "green".into(),
            elapsed: 0,
            cycle_minutes: 12,
            offset: 9,
        },
    ]
}

impl World {
    pub fn hydrate(&mut self) {
        if self.world_seed == 0 {
            self.world_seed = 0x4c455353_56494c4c_u64 ^ self.generation as u64;
        }
        if self.weather_kind.is_empty() {
            self.weather_kind = if self.rain_until > self.minute {
                "rain"
            } else {
                "clear"
            }
            .into();
        }
        if self.temperature_c == 0 {
            self.temperature_c = 29;
        }
        if self.weather_changed_at == 0 {
            self.weather_changed_at = self.minute;
        }
        if self.weather_source.is_empty() {
            self.weather_source = "seeded-fallback".into();
        }
        for a in &mut self.agents {
            if a.occupation.is_empty() {
                a.occupation = if a.role.is_empty() {
                    "Resident".into()
                } else {
                    a.role.clone()
                };
            }
            if a.behaviour.is_empty() {
                a.behaviour = "routine".into();
            }
            if a.mood.is_empty() {
                a.mood = "neutral".into();
            }
        }
        for person in background_characters(self.world_seed) {
            if !self.background.iter().any(|p| p.id == person.id) {
                self.background.push(person);
            }
        }
        for key in ["cafe_meals", "home_meals", "dorm_meals"] {
            self.stock.entry(key.into()).or_insert(12);
        }
        if self.traffic.is_empty() {
            self.traffic = traffic_actors(self.world_seed);
        }
        if self.traffic_lights.is_empty() {
            self.traffic_lights = traffic_lights();
        }
        if self.novelty.len() > 24 {
            self.novelty.sort_by_key(|item| item.opened_at);
            self.novelty.drain(..self.novelty.len() - 24);
        }
        if self.frontier.len() > 24 {
            self.frontier.sort_by_key(|signal| signal.last_seen);
            self.frontier.drain(..self.frontier.len() - 24);
        }
        if self.evolution_proposals.len() > 24 {
            self.evolution_proposals
                .drain(..self.evolution_proposals.len() - 24);
        }
    }

    pub fn fresh(generation: i64) -> World {
        let mut stock = BTreeMap::new();
        stock.insert("oat_milk".into(), 1);
        stock.insert("milk".into(), 8);
        stock.insert("beans".into(), 20);
        stock.insert("eggs".into(), 0);
        let mut bus_delays = BTreeMap::new();
        bus_delays.insert(480, 15); // Day 1 08:00 bus: scripted road works
        World {
            last_retention: None,
            remote_msg: BTreeMap::new(),
            minute: DAY_START,
            speed: 1,
            paused: false,
            stop_at: None,
            rain_until: 0,
            last_rain_end: -10_000,
            rain_reason: String::new(),
            weather_kind: "clear".into(),
            temperature_c: 29,
            wind_kmh: 8,
            weather_changed_at: DAY_START,
            weather_source: "seeded-fallback".into(),
            weather_observed_at: 0,
            weather_code: 0,
            world_seed: 0x4c455353_56494c4c_u64 ^ generation as u64,
            stock,
            agents: vec![
                agent(
                    "maya",
                    "Maya",
                    "Owner",
                    "Café owner · lives above Less Café",
                    "cafe",
                    "Opening the café",
                ),
                agent(
                    "sam",
                    "Sam",
                    "Sam",
                    "Office worker · Ama's son",
                    "home",
                    "Getting ready for work",
                ),
                agent(
                    "lin",
                    "Lin",
                    "Customer",
                    "Student · café regular",
                    "away",
                    "At the dorm",
                ),
                agent(
                    "ama",
                    "Ama",
                    "Family",
                    "Grandmother · walks Mochi",
                    "home",
                    "Making breakfast",
                ),
                agent(
                    "bo",
                    "Bo",
                    "Barista",
                    "Barista (background role)",
                    "away",
                    "Commuting in",
                ),
            ],
            background: background_characters(0x4c455353_56494c4c_u64 ^ generation as u64),
            traffic: traffic_actors(0x4c455353_56494c4c_u64 ^ generation as u64),
            traffic_lights: traffic_lights(),
            occupation_changes: Vec::new(),
            next_id: BTreeMap::new(),
            bus_delays,
            bus_arrived: None,
            errand: None,
            restock: None,
            done: BTreeSet::new(),
            plans: Vec::new(),
            feed: VecDeque::new(),
            feed_seq: 0,
            resumed: None,
            generation,
            novelty: Vec::new(),
            frontier: Vec::new(),
            evolution_proposals: Vec::new(),
            cafe: CafeState::default(),
            bus_trip: None,
        }
    }
}

pub struct Sim {
    pub store: Arc<Store>,
    pub w: World,
    pub llm: Option<mpsc::UnboundedSender<crate::llm::Job>>,
}

pub fn acl(agent: &str) -> String {
    format!(
        "(owner = {a} OR scope IN (SELECT scope FROM lv_scope_members WHERE agent = {a}))",
        a = q(agent)
    )
}

const MEM_COLS: &str = "id, sim_minute, owner, kind, scope, location, tag, context_id, expires_at, source_kind, source_id, content";

impl Sim {
    fn now(&self) -> i64 {
        self.w.minute
    }
    fn md(&self) -> i64 {
        self.w.minute.rem_euclid(DAY)
    }
    fn ai(&self, id: &str) -> usize {
        self.w
            .agents
            .iter()
            .position(|a| a.id == id)
            .expect("known agent")
    }
    fn at(&self, id: &str) -> String {
        self.w.agents[self.ai(id)].location.clone()
    }
    fn name(&self, id: &str) -> String {
        self.w
            .agents
            .iter()
            .find(|a| a.id == id)
            .map(|a| a.name.clone())
            .unwrap_or_else(|| id.to_string())
    }
    fn next(&mut self, table: &str) -> i64 {
        let e = self.w.next_id.entry(table.to_string()).or_insert(0);
        *e += 1;
        *e
    }
    fn set_activity(&mut self, id: &str, act: &str) {
        let k = self.ai(id);
        let loc = self.w.agents[k].location.clone();
        let (kind, station, pose) = activity_fields(id, &loc, act);
        let a = &mut self.w.agents[k];
        a.activity = act.to_string();
        a.activity_kind = kind.into();
        a.station = station.into();
        a.pose = pose.into();
        if loc == "cafe" && pose == "seat" && station.starts_with("table") {
            let assigned = self
                .w
                .cafe
                .tables
                .iter()
                .find(|(_, who)| who.as_str() == id)
                .map(|(t, _)| t.clone())
                .or_else(|| {
                    self.w
                        .cafe
                        .tables
                        .get(station)
                        .filter(|who| who.as_str() == "free")
                        .map(|_| station.to_string())
                })
                .or_else(|| {
                    self.w
                        .cafe
                        .tables
                        .iter()
                        .find(|(_, who)| who.as_str() == "free")
                        .map(|(t, _)| t.clone())
                });
            if let Some(table) = assigned {
                self.w.cafe.tables.insert(table.clone(), id.into());
                self.w.agents[k].station = table;
            } else {
                self.w.agents[k].activity = "Waiting for a free table".into();
                self.w.agents[k].activity_kind = "waiting".into();
                self.w.agents[k].station = "queue".into();
                self.w.agents[k].pose = "idle".into();
            }
        } else if self.w.agents[k].life.meal.is_none() {
            for occupant in self.w.cafe.tables.values_mut() {
                if occupant == id {
                    *occupant = "free".into();
                }
            }
        }
    }
    fn bubble(&mut self, id: &str, kind: &str, text: &str) {
        let k = self.ai(id);
        let until = self.now() + 18;
        self.w.agents[k].bubble = Some(Bubble {
            text: text.into(),
            kind: kind.into(),
            until,
            by: String::new(),
        });
    }
    fn flag(&self, id: &str, f: &str) -> i64 {
        self.w.agents[self.ai(id)]
            .flags
            .get(f)
            .copied()
            .unwrap_or(0)
    }
    fn set_flag(&mut self, id: &str, f: &str, v: i64) {
        let k = self.ai(id);
        self.w.agents[k].flags.insert(f.into(), v);
    }

    pub fn feed(
        &mut self,
        kind: &str,
        actor: &str,
        title: &str,
        detail: &str,
        refs: Vec<String>,
        scope: &str,
    ) {
        self.w.feed_seq += 1;
        let item = FeedItem {
            seq: self.w.feed_seq,
            minute: self.now(),
            kind: kind.into(),
            actor: actor.into(),
            title: title.into(),
            detail: detail.into(),
            refs,
            scope: scope.into(),
        };
        if self.w.feed.len() >= FEED_CAP {
            self.w.feed.pop_front();
        }
        self.w.feed.push_back(item);
    }

    async fn sql(&self, actor: &str, purpose: &str, sql: &str) -> Vec<Row> {
        self.store
            .run(self.now(), actor, purpose, sql)
            .await
            .unwrap_or_default()
    }
    pub(crate) async fn sql_pub(&self, actor: &str, purpose: &str, sql: &str) -> Vec<Row> {
        self.sql(actor, purpose, sql).await
    }
    pub(crate) fn next_id(&mut self, table: &str) -> i64 {
        self.next(table)
    }

    /// Replace a rule-chosen line with its model-voiced version if it is still showing.
    pub fn voice(&mut self, agent: &str, original: &str, voiced: &str, call_id: i64) {
        let k = self.ai(agent);
        if let Some(b) = self.w.agents[k].bubble.as_mut() {
            if b.text == original {
                b.text = voiced.to_string();
                b.by = format!("model call #{call_id}");
            }
        }
        if let Some(f) = self
            .w
            .feed
            .iter_mut()
            .rev()
            .find(|f| f.kind == "say" && f.actor == agent && f.detail == original)
        {
            f.detail =
                format!("{voiced}  (voiced by model call #{call_id}; rule line: \"{original}\")");
            f.refs.push(format!("llm:{call_id}"));
        }
    }

    // ---------- primitive writes (each one LessDB INSERT) ----------

    pub async fn event(&mut self, agent: &str, kind: &str, loc: &str, detail: &str) -> i64 {
        let id = self.next("lv_events");
        let (x, y) = anchor(loc);
        let sql = format!(
            "INSERT INTO lv_events VALUES ({id}, {}, {}, {}, {}, {x:.1}, {y:.1}, {})",
            self.now(),
            q(agent),
            q(kind),
            q(loc),
            q(detail)
        );
        self.sql(agent, "record observation", &sql).await;
        id
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn object_transition(
        &mut self,
        actor: &str,
        object: &str,
        from_state: &str,
        to_state: &str,
        location: &str,
        context_id: &str,
        detail: &str,
    ) -> i64 {
        let id = self.next("lv_object_transitions");
        let sql = format!(
            "INSERT INTO lv_object_transitions VALUES ({id}, {}, {}, {}, {}, {}, {}, {}, {})",
            self.now(),
            q(actor),
            q(object),
            q(from_state),
            q(to_state),
            q(location),
            q(context_id),
            q(detail)
        );
        self.sql(actor, "record object transition", &sql).await;
        self.feed(
            "object",
            actor,
            &format!("{object}: {from_state} → {to_state}"),
            detail,
            vec![format!("obj:{id}"), format!("ctx:{context_id}")],
            location,
        );
        id
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn memory(
        &mut self,
        owner: &str,
        kind: &str,
        scope: &str,
        loc: &str,
        tag: &str,
        ctx: &str,
        expires: i64,
        source: (&str, i64),
        content: &str,
    ) -> i64 {
        let id = self.next("lv_memories");
        let sql = format!(
            "INSERT INTO lv_memories VALUES ({id}, {}, {}, {}, {}, {}, {}, {}, {expires}, {}, {}, {})",
            self.now(),
            q(owner),
            q(kind),
            q(scope),
            q(loc),
            q(tag),
            q(ctx),
            q(source.0),
            source.1,
            q(content)
        );
        self.sql(owner, &format!("write {kind} memory"), &sql).await;
        self.memory_lifecycle(id, "active", "memory created", source.0)
            .await;
        let who = self.name(owner);
        let vis = if scope == "private" {
            "private".to_string()
        } else {
            format!("shared → {scope}")
        };
        self.feed(
            "memory",
            owner,
            &format!("{who} remembers ({kind}, {vis})"),
            content,
            vec![format!("mem:{id}")],
            scope,
        );
        self.bubble(owner, "memory", &format!("(noted) {content}"));
        id
    }

    /// Application-owned lifecycle provenance. This is deliberately separate
    /// from LessDB table TTL and from physical deletion.
    pub async fn memory_lifecycle(
        &mut self,
        memory_id: i64,
        state: &str,
        reason: &str,
        source: &str,
    ) {
        let sql = format!(
            "INSERT INTO lv_memory_lifecycle VALUES ({memory_id}, {}, {}, {}, {})",
            q(state),
            self.now(),
            q(reason),
            q(source)
        );
        self.sql("world", "record memory lifecycle", &sql).await;
    }

    /// Persist one bounded lesson candidate or accepted lesson. Lesson rows are
    /// application-owned evidence above LessDB; they never auto-apply a policy.
    #[allow(clippy::too_many_arguments)]
    pub async fn lesson(
        &mut self,
        lesson_id: &str,
        agent: &str,
        topic: &str,
        trigger: &str,
        statement: &str,
        state: &str,
        confidence: i64,
        source_refs: &[String],
        policy_version: &str,
    ) {
        let existing = self
            .sql(
                agent,
                "check idempotent learning lesson",
                &format!(
                    "SELECT lesson_id FROM lv_lessons WHERE lesson_id = {}",
                    q(lesson_id)
                ),
            )
            .await;
        if !existing.is_empty() {
            return;
        }
        let applied_at = if state == "accepted" { self.now() } else { -1 };
        let refs = source_refs.join("|");
        let sql = format!(
            "INSERT INTO lv_lessons VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, 0)",
            q(lesson_id),
            q(agent),
            self.now(),
            q(topic),
            q(trigger),
            q(statement),
            q(state),
            confidence.clamp(0, 100),
            q(&refs),
            q(policy_version),
            applied_at
        );
        self.sql(agent, "persist learning lesson", &sql).await;
        self.feed(
            "lesson",
            agent,
            &format!(
                "{} {} lesson",
                self.name(agent),
                if state == "accepted" {
                    "integrates a"
                } else {
                    "forms a"
                }
            ),
            statement,
            vec![format!("lesson:{lesson_id}"), format!("evidence:{refs}")],
            "",
        );
    }

    /// Promote a recurring candidate pattern once the deterministic evidence
    /// threshold is met. This is a rule-level transition, not model approval.
    pub async fn promote_lesson_if_repeated(
        &mut self,
        agent: &str,
        topic: &str,
        accepted_id: &str,
        trigger: &str,
        statement: &str,
        source_refs: &[String],
        policy_version: &str,
    ) -> bool {
        let candidates = self
            .sql(
                agent,
                "count repeated learning evidence",
                &format!(
                    "SELECT lesson_id FROM lv_lessons WHERE agent = {} AND topic = {} AND state = 'candidate' ORDER BY sim_minute DESC LIMIT 8",
                    q(agent),
                    q(topic)
                ),
            )
            .await;
        if candidates.len() < 2 {
            return false;
        }
        self.lesson(
            accepted_id,
            agent,
            topic,
            trigger,
            statement,
            "accepted",
            80,
            source_refs,
            policy_version,
        )
        .await;
        true
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn relationship(
        &mut self,
        edge_id: &str,
        subject: &str,
        object: &str,
        relation: &str,
        consent: &str,
        reciprocity: &str,
        trust_context: &str,
        evidence_refs: &[String],
        state: &str,
        confidence: i64,
        policy_version: &str,
    ) {
        let existing = self
            .sql(
                subject,
                "check idempotent relationship edge",
                &format!(
                    "SELECT edge_id FROM lv_relationships WHERE edge_id = {}",
                    q(edge_id)
                ),
            )
            .await;
        if !existing.is_empty() {
            return;
        }
        let refs = evidence_refs.join("|");
        let sql = format!(
            "INSERT INTO lv_relationships VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, 0)",
            q(edge_id),
            q(subject),
            q(object),
            q(relation),
            q(consent),
            q(reciprocity),
            q(trust_context),
            q(&refs),
            q(state),
            confidence.clamp(0, 100),
            q(policy_version),
            self.now()
        );
        self.sql(subject, "persist relationship edge", &sql).await;
        self.feed(
            "relationship",
            subject,
            &format!(
                "{} relationship with {}",
                self.name(subject),
                self.name(object)
            ),
            &format!("{relation}; consent={consent}; reciprocity={reciprocity}; state={state}"),
            vec![format!("rel:{edge_id}"), format!("evidence:{refs}")],
            trust_context,
        );
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn identity_thread(
        &mut self,
        record_id: &str,
        thread_id: &str,
        agent: &str,
        parent_thread: &str,
        operation: &str,
        reason: &str,
        continuity: &str,
        evidence_refs: &[String],
        policy_version: &str,
    ) {
        if evidence_refs.is_empty() {
            return;
        }
        let existing = self
            .sql(
                agent,
                "check idempotent identity thread record",
                &format!(
                    "SELECT record_id FROM lv_identity_threads WHERE record_id = {}",
                    q(record_id)
                ),
            )
            .await;
        if !existing.is_empty() {
            return;
        }
        if !matches!(operation, "root" | "fork" | "reset")
            || (operation != "root" && parent_thread.is_empty())
        {
            return;
        }
        let refs = evidence_refs.join("|");
        let sql = format!(
            "INSERT INTO lv_identity_threads VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, 0)",
            q(record_id),
            q(thread_id),
            q(agent),
            q(parent_thread),
            q(operation),
            q(reason),
            q(continuity),
            q(&refs),
            q(policy_version),
            self.now()
        );
        self.sql(agent, "persist identity thread record", &sql)
            .await;
        self.feed(
            "identity",
            agent,
            &format!(
                "{} records a {} identity thread",
                self.name(agent),
                operation
            ),
            continuity,
            vec![format!("thread:{thread_id}"), format!("evidence:{refs}")],
            "identity-thread",
        );
    }
    pub async fn ensure_identity_threads(&mut self) {
        for agent in ["maya", "sam", "lin", "ama", "bo"] {
            self.identity_thread(
                &format!("thread-record:{agent}:root:v1"),
                &format!("thread:{agent}:v1"),
                agent,
                "",
                "root",
                "initial fictional resident thread",
                "This thread preserves simulation continuity; it is not a claim about consciousness or a human-like self.",
                &["seed:world".into()],
                "identity-thread.v1",
            )
            .await;
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn interaction(
        &mut self,
        interaction_id: &str,
        initiator: &str,
        recipient: &str,
        kind: &str,
        context_id: &str,
        initiator_action: &str,
        recipient_response: &str,
        reciprocity: &str,
        evidence_refs: &[String],
        outcome: &str,
    ) {
        let existing = self
            .sql(
                initiator,
                "check idempotent social interaction",
                &format!(
                    "SELECT interaction_id FROM lv_interactions WHERE interaction_id = {}",
                    q(interaction_id)
                ),
            )
            .await;
        if !existing.is_empty() {
            return;
        }
        let refs = evidence_refs.join("|");
        let sql = format!(
            "INSERT INTO lv_interactions VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, 0)",
            q(interaction_id),
            self.now(),
            q(initiator),
            q(recipient),
            q(kind),
            q(context_id),
            q(initiator_action),
            q(recipient_response),
            q(reciprocity),
            q(&refs),
            q(outcome)
        );
        self.sql(initiator, "persist reciprocal social interaction", &sql)
            .await;
        self.feed(
            "interaction",
            initiator,
            &format!(
                "{} and {} complete a reciprocal exchange",
                self.name(initiator),
                self.name(recipient)
            ),
            &format!("{kind}; {initiator_action} → {recipient_response}; outcome={outcome}"),
            vec![
                format!("interaction:{interaction_id}"),
                format!("evidence:{refs}"),
            ],
            context_id,
        );
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn competency(
        &mut self,
        agent: &str,
        domain: &str,
        skill: &str,
        evidence_refs: &[String],
        policy_version: &str,
    ) {
        if evidence_refs.is_empty() {
            return;
        }
        let competency_id = format!("skill:{agent}:{domain}:{skill}");
        let current = self
            .sql(
                agent,
                "read prior competency evidence",
                &format!(
                    "SELECT evidence_count, source_refs FROM lv_competencies WHERE competency_id = {} ORDER BY updated_at DESC LIMIT 1",
                    q(&competency_id)
                ),
            )
            .await;
        let prior_count = current
            .first()
            .map(|row| i(row, "evidence_count"))
            .unwrap_or(0);
        let evidence_count = prior_count.saturating_add(1);
        let prior_level = if prior_count == 0 {
            0
        } else {
            ((prior_count + 1) / 2).clamp(1, 5)
        };
        let level = ((evidence_count + 1) / 2).clamp(1, 5);
        let status = match evidence_count {
            0 | 1 => "emerging",
            2 | 3 => "practising",
            _ => "reliable",
        };
        let prior_refs = current
            .first()
            .map(|row| s(row, "source_refs"))
            .unwrap_or_default();
        let refs = bounded_refs(&prior_refs, evidence_refs);
        let sql = format!(
            "INSERT INTO lv_competencies VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, 0)",
            q(&competency_id),
            q(agent),
            q(domain),
            q(skill),
            level,
            evidence_count,
            q(status),
            q(&refs),
            q(policy_version),
            self.now()
        );
        self.sql(agent, "persist competency evidence", &sql).await;
        self.competency_event(
            &format!("competency-event:{competency_id}:support:{evidence_count}"),
            &competency_id,
            agent,
            domain,
            skill,
            "support",
            prior_level,
            level,
            evidence_refs,
            "A bounded successful experience added evidence to this competency.",
            policy_version,
        )
        .await;
        self.feed(
            "competency",
            agent,
            &format!("{} develops {skill}", self.name(agent)),
            &format!("{status}; evidence {evidence_count}; level {level}/5"),
            vec![
                format!("competency:{competency_id}"),
                format!("evidence:{refs}"),
            ],
            domain,
        );
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn competency_event(
        &mut self,
        event_id: &str,
        competency_id: &str,
        agent: &str,
        domain: &str,
        skill: &str,
        kind: &str,
        prior_level: i64,
        next_level: i64,
        evidence_refs: &[String],
        reason: &str,
        policy_version: &str,
    ) {
        if evidence_refs.is_empty()
            || !matches!(kind, "support" | "contradiction" | "decay" | "transfer")
        {
            return;
        }
        let existing = self
            .sql(
                agent,
                "check idempotent competency state event",
                &format!(
                    "SELECT event_id FROM lv_competency_events WHERE event_id = {}",
                    q(event_id)
                ),
            )
            .await;
        if !existing.is_empty() {
            return;
        }
        let refs = evidence_refs.join("|");
        let sql = format!(
            "INSERT INTO lv_competency_events VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, 0)",
            q(event_id),
            q(competency_id),
            q(agent),
            q(domain),
            q(skill),
            q(kind),
            prior_level.clamp(0, 5),
            next_level.clamp(0, 5),
            q(&refs),
            q(reason),
            q(policy_version),
            self.now()
        );
        self.sql(agent, "persist competency state event", &sql)
            .await;
        self.feed(
            "competency_state",
            agent,
            &format!("{} competency {kind}", self.name(agent)),
            reason,
            vec![
                format!("competency:{competency_id}"),
                format!("evidence:{refs}"),
            ],
            domain,
        );
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn competency_signal(
        &mut self,
        event_id: &str,
        agent: &str,
        domain: &str,
        skill: &str,
        kind: &str,
        evidence_refs: &[String],
        reason: &str,
        policy_version: &str,
    ) {
        if evidence_refs.is_empty() || !matches!(kind, "contradiction" | "decay" | "transfer") {
            return;
        }
        let competency_id = format!("skill:{agent}:{domain}:{skill}");
        let current = self
            .sql(
                agent,
                "read competency before state transition",
                &format!(
                    "SELECT level, evidence_count, source_refs FROM lv_competencies WHERE competency_id = {} ORDER BY updated_at DESC LIMIT 1",
                    q(&competency_id)
                ),
            )
            .await;
        let prior_level = current.first().map(|row| i(row, "level")).unwrap_or(0);
        let next_level = match kind {
            "transfer" => prior_level.max(1),
            _ => prior_level.saturating_sub(1).max(0),
        };
        if let Some(row) = current.first() {
            let evidence_count = i(row, "evidence_count");
            let prior_refs = s(row, "source_refs");
            let refs = bounded_refs(&prior_refs, evidence_refs);
            let status = match kind {
                "decay" if next_level == 0 => "dormant",
                "decay" => "fading",
                "contradiction" if next_level == 0 => "challenged",
                "contradiction" => "challenged",
                "transfer" => "transferred",
                _ => "emerging",
            };
            let sql = format!(
                "INSERT INTO lv_competencies VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, 0)",
                q(&competency_id),
                q(agent),
                q(domain),
                q(skill),
                next_level,
                evidence_count,
                q(status),
                q(&refs),
                q(policy_version),
                self.now()
            );
            self.sql(agent, "persist competency state projection", &sql)
                .await;
        }
        self.competency_event(
            event_id,
            &competency_id,
            agent,
            domain,
            skill,
            kind,
            prior_level,
            next_level,
            evidence_refs,
            reason,
            policy_version,
        )
        .await;
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn lesson_challenge(
        &mut self,
        challenge_id: &str,
        lesson_id: &str,
        agent: &str,
        challenger: &str,
        kind: &str,
        claim: &str,
        evidence_refs: &[String],
        resolution: &str,
    ) {
        let existing = self
            .sql(
                agent,
                "check idempotent lesson challenge",
                &format!(
                    "SELECT challenge_id FROM lv_lesson_challenges WHERE challenge_id = {}",
                    q(challenge_id)
                ),
            )
            .await;
        if !existing.is_empty() {
            return;
        }
        let refs = evidence_refs.join("|");
        let sql = format!(
            "INSERT INTO lv_lesson_challenges VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, 0)",
            q(challenge_id),
            q(lesson_id),
            q(agent),
            q(challenger),
            q(kind),
            q(claim),
            q(&refs),
            q(resolution),
            self.now()
        );
        self.sql(agent, "persist lesson challenge", &sql).await;
        self.feed(
            "lesson_challenge",
            agent,
            &format!("{} challenges a lesson", self.name(agent)),
            claim,
            vec![
                format!("challenge:{challenge_id}"),
                format!("lesson:{lesson_id}"),
                format!("evidence:{refs}"),
            ],
            "",
        );
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn repair_lesson(
        &mut self,
        challenge_id: &str,
        lesson_id: &str,
        agent: &str,
        challenger: &str,
        kind: &str,
        claim: &str,
        source_refs: &[String],
        replacement_id: &str,
        topic: &str,
        trigger: &str,
        statement: &str,
        policy_version: &str,
    ) {
        let mut refs = source_refs.to_vec();
        refs.push(format!("challenge:{challenge_id}"));
        self.lesson_challenge(
            challenge_id,
            lesson_id,
            agent,
            challenger,
            kind,
            claim,
            &refs,
            "repaired",
        )
        .await;
        self.lesson(
            replacement_id,
            agent,
            topic,
            trigger,
            statement,
            "accepted",
            70,
            &refs,
            policy_version,
        )
        .await;
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn teaching_grant(
        &mut self,
        grant_id: &str,
        lesson_id: &str,
        teacher: &str,
        recipient: &str,
        scope_id: &str,
        purpose: &str,
        valid_from: i64,
        valid_until: i64,
        state: &str,
        evidence_refs: &[String],
        policy_version: &str,
    ) {
        let existing = self
            .sql(
                recipient,
                "check idempotent teaching grant",
                &format!(
                    "SELECT grant_id FROM lv_teaching_grants WHERE grant_id = {}",
                    q(grant_id)
                ),
            )
            .await;
        if !existing.is_empty() {
            return;
        }
        let refs = evidence_refs.join("|");
        let sql = format!(
            "INSERT INTO lv_teaching_grants VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, 0)",
            q(grant_id),
            q(lesson_id),
            q(teacher),
            q(recipient),
            q(scope_id),
            q(purpose),
            valid_from,
            valid_until,
            q(state),
            q(&refs),
            q(policy_version),
            self.now()
        );
        self.sql(recipient, "persist teaching grant", &sql).await;
        self.feed(
            "teaching",
            recipient,
            &format!(
                "{} shares a lesson with {}",
                self.name(teacher),
                self.name(recipient)
            ),
            &format!("{purpose}; scope={scope_id}; state={state}; valid_until={valid_until}"),
            vec![
                format!("grant:{grant_id}"),
                format!("lesson:{lesson_id}"),
                format!("evidence:{refs}"),
            ],
            scope_id,
        );
    }

    /// Recall a lesson only through an explicit active grant. This returns the
    /// teacher's row for explanation; it never copies the lesson to the
    /// recipient and never changes scope membership.
    pub async fn recall_taught_lesson(
        &mut self,
        recipient: &str,
        teacher: &str,
        lesson_id: &str,
        purpose: &str,
    ) -> Vec<Row> {
        let sql = format!(
            "SELECT lesson_id, agent, sim_minute, topic, trigger, statement, state, confidence, source_refs, policy_version, applied_at, auto_apply FROM lv_lessons WHERE lesson_id = {} AND agent = {} AND EXISTS (SELECT grant_id FROM lv_teaching_grants WHERE lesson_id = lv_lessons.lesson_id AND teacher_agent = {} AND recipient_agent = {} AND state = 'active' AND (valid_from < 0 OR valid_from <= {}) AND (valid_until < 0 OR valid_until >= {})) AND NOT EXISTS (SELECT grant_id FROM lv_teaching_grants WHERE lesson_id = lv_lessons.lesson_id AND teacher_agent = {} AND recipient_agent = {} AND state = 'revoked' AND sim_minute <= {})",
            q(lesson_id),
            q(teacher),
            q(teacher),
            q(recipient),
            self.now(),
            self.now(),
            q(teacher),
            q(recipient),
            self.now()
        );
        self.sql(recipient, purpose, &sql).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn counterfactual(
        &mut self,
        replay_id: &str,
        agent: &str,
        topic: &str,
        baseline_policy: &str,
        baseline_action: &str,
        learned_policy: &str,
        learned_action: &str,
        difference: &str,
        source_refs: &[String],
    ) {
        let existing = self
            .sql(
                agent,
                "check idempotent counterfactual replay",
                &format!(
                    "SELECT replay_id FROM lv_counterfactuals WHERE replay_id = {}",
                    q(replay_id)
                ),
            )
            .await;
        if !existing.is_empty() {
            return;
        }
        let refs = source_refs.join("|");
        let sql = format!(
            "INSERT INTO lv_counterfactuals VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, 1)",
            q(replay_id),
            q(agent),
            self.now(),
            q(topic),
            q(baseline_policy),
            q(baseline_action),
            q(learned_policy),
            q(learned_action),
            q(difference),
            q(&refs)
        );
        self.sql(agent, "persist non-executing counterfactual", &sql)
            .await;
        self.feed(
            "counterfactual",
            agent,
            &format!("{} compares two bounded choices", self.name(agent)),
            difference,
            vec![format!("replay:{replay_id}"), format!("evidence:{refs}")],
            "",
        );
    }

    #[allow(clippy::too_many_arguments)]
    async fn frontier_signal(
        &mut self,
        signal_id: &str,
        agent: &str,
        capability: &str,
        situation: &str,
        evidence_refs: &[String],
        severity: i64,
        proposed_operation: &str,
        seed: u64,
    ) {
        let existing = self
            .sql(
                agent,
                "check idempotent research frontier signal",
                &format!(
                    "SELECT signal_id FROM lv_frontier_signals WHERE signal_id = {}",
                    q(signal_id)
                ),
            )
            .await;
        if !existing.is_empty() {
            return;
        }
        let refs = evidence_refs.join("|");
        let recurrence = self
            .sql(
                agent,
                "count recurring frontier signal",
                &format!(
                    "SELECT signal_id FROM lv_frontier_signals WHERE agent = {} AND capability = {}",
                    q(agent),
                    q(capability)
                ),
            )
            .await
            .len() as i64
            + 1;
        let sql = format!(
            "INSERT INTO lv_frontier_signals VALUES ({}, {}, {}, {}, {}, {}, {}, {}, 'candidate', {}, {}, 0)",
            q(signal_id),
            self.now(),
            q(agent),
            q(capability),
            q(situation),
            q(&refs),
            severity.clamp(1, 5),
            recurrence,
            q(proposed_operation),
            storage_seed(seed)
        );
        self.sql(agent, "persist research frontier signal", &sql)
            .await;
        self.w.frontier.push(FrontierSignal {
            id: signal_id.into(),
            agent: agent.into(),
            capability: capability.into(),
            situation: situation.into(),
            evidence_refs: evidence_refs.to_vec(),
            severity: severity.clamp(1, 5),
            recurrence,
            state: "candidate".into(),
            proposed_operation: proposed_operation.into(),
            seed,
            first_seen: self.now(),
            last_seen: self.now(),
        });
        self.feed(
            "frontier",
            agent,
            &format!("{} found a LessDB interaction gap", self.name(agent)),
            &format!("{capability}: {situation}; candidate {proposed_operation}; proposal remains inactive"),
            vec![format!("frontier:{signal_id}"), format!("evidence:{refs}")],
            "research",
        );
    }

    async fn evolution_candidate(
        &mut self,
        proposal_id: &str,
        agent: &str,
        operation: &str,
        patch: &str,
        purpose: &str,
        evidence_refs: &[String],
        replay_key: &str,
    ) {
        let existing = self
            .sql(
                agent,
                "check idempotent evolution candidate",
                &format!(
                    "SELECT proposal_id FROM lv_evolution_proposals WHERE proposal_id = {}",
                    q(proposal_id)
                ),
            )
            .await;
        if !existing.is_empty() {
            return;
        }
        let refs = evidence_refs.join("|");
        let sql = format!(
            "INSERT INTO lv_evolution_proposals VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, 'additive', 'candidate', '', '', {}, 0)",
            q(proposal_id),
            self.now(),
            q(agent),
            q(&format!("agent:{agent}")),
            q(operation),
            q(patch),
            q(purpose),
            q(&refs),
            q(&format!("agent:{agent}")),
            q(replay_key)
        );
        self.sql(agent, "persist inactive evolution candidate", &sql)
            .await;
        if !self
            .w
            .evolution_proposals
            .iter()
            .any(|id| id == proposal_id)
        {
            self.w.evolution_proposals.push(proposal_id.into());
            if self.w.evolution_proposals.len() > 24 {
                self.w.evolution_proposals.drain(..1);
            }
        }
        self.feed(
            "proposal",
            agent,
            &format!("{} proposed a logical evolution", self.name(agent)),
            &format!("{operation}: {patch}; state=candidate; auto_apply=false"),
            vec![
                format!("proposal:{proposal_id}"),
                format!("evidence:{refs}"),
            ],
            "agent-owned",
        );
    }

    async fn explore_novelty(&mut self) {
        let md = self.md();
        if !(DAY_START..DAY_END).contains(&md) || md % 90 != 15 {
            return;
        }
        let day = day_of(self.now());
        let bucket = md / 90;
        let ids = ["maya", "sam", "lin", "ama", "bo"];
        for (agent_index, agent) in ids.iter().enumerate() {
            let Some(option) = novelty_choice(self.w.world_seed, day, agent_index, bucket) else {
                continue;
            };
            let event_id = format!("novelty:{day}:{agent}:{bucket}");
            if self.w.novelty.iter().any(|item| item.id == event_id) {
                continue;
            }
            let replay_key = format!(
                "seed:{}:day:{day}:bucket:{bucket}:agent:{agent}",
                self.w.world_seed
            );
            let (trigger, outcome, capability, operation, patch, purpose) = match option {
                "social_detour" => (
                    "routine opened a low-risk social possibility",
                    "recorded a bounded social-context request without granting access",
                    "memory-lens.social-consent",
                    "memory_lens",
                    "compose a consent-bound social lens with reciprocity evidence",
                    "represent a social opportunity without turning proximity into permission",
                ),
                "weather_route" => (
                    "weather and route evidence arrived together",
                    "recorded a temporal-spatial context request without changing safety rules",
                    "context-recipe.temporal-spatial",
                    "context_recipe",
                    "combine weather, trail and route observations with frame freshness",
                    "compile situation-shaped context instead of hard-coded route SQL",
                ),
                "shared_context_request" => (
                    "an agent needed one bounded fact from a peer",
                    "held a least-privilege teaching request pending consent and expiry",
                    "shared-context.expiring-grant",
                    "grant",
                    "request one typed lesson through an expiring recipient-specific grant",
                    "share a useful lesson without implicit copying or scope widening",
                ),
                _ => (
                    "an agent tried a small alternative to its routine",
                    "recorded a skill experiment while leaving occupation and policy unchanged",
                    "derived-view.skill-evidence",
                    "view",
                    "compose a derived skill-evidence view from repeated outcomes",
                    "track competence evidence without reducing the agent to one score",
                ),
            };
            let seed = seeded_roll(self.w.world_seed, day, bucket + agent_index as i64);
            let opportunity = NoveltyOpportunity {
                id: event_id.clone(),
                day,
                agent: (*agent).into(),
                trigger: trigger.into(),
                option: option.into(),
                outcome: outcome.into(),
                seed,
                replay_key: replay_key.clone(),
                scope: format!("agent:{agent}"),
                opened_at: self.now(),
                expires_at: self.now() + 45,
            };
            let event_sql = format!(
                "INSERT INTO lv_novelty_events VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {})",
                q(&event_id),
                self.now(),
                day,
                q(agent),
                q(trigger),
                q(option),
                q(outcome),
                storage_seed(seed),
                q(&replay_key),
                q(&opportunity.scope)
            );
            self.sql(agent, "persist bounded novelty event", &event_sql)
                .await;
            self.w.novelty.push(opportunity);
            self.set_activity(agent, &format!("Exploring: {option}"));
            let k = self.ai(agent);
            self.w.agents[k].behaviour = "exploratory, bounded".into();
            self.w.agents[k].mood = "curious".into();
            let location = self.w.agents[k].location.clone();
            let event_row = self
                .event(agent, &format!("novelty.{option}"), &location, outcome)
                .await;
            let evidence = vec![
                format!("novelty:{event_id}"),
                format!("replay:{replay_key}"),
            ];
            let memory_id = self
                .memory(
                    agent,
                    "temporary",
                    "private",
                    &location,
                    &format!("novelty.{option}"),
                    &replay_key,
                    self.now() + 45,
                    ("event", event_row),
                    outcome,
                )
                .await;
            let mut refs = evidence.clone();
            refs.push(format!("mem:{memory_id}"));
            self.decide(
                agent,
                "agent.explore v1 (bounded variation)",
                &refs,
                outcome,
                "Seeded novelty changed a local routine only; no scope, schema or actuator boundary changed.",
            )
            .await;
            let signal_id = format!("frontier:{day}:{agent}:{capability}");
            self.frontier_signal(
                &signal_id, agent, capability, trigger, &refs, 2, operation, seed,
            )
            .await;
            let proposal_id = format!("evolution:{event_id}");
            self.evolution_candidate(
                &proposal_id,
                agent,
                operation,
                patch,
                purpose,
                &refs,
                &replay_key,
            )
            .await;
        }
    }

    pub async fn message(
        &mut self,
        sender: &str,
        channel: &str,
        recipient: &str,
        topic: &str,
        ctx: &str,
        body: &str,
    ) -> i64 {
        let id = self.next("lv_messages");
        let sql = format!(
            "INSERT INTO lv_messages VALUES ({id}, {}, {}, {}, {}, {}, {}, {})",
            self.now(),
            q(sender),
            q(channel),
            q(recipient),
            q(topic),
            q(ctx),
            q(body)
        );
        self.sql(sender, "send message", &sql).await;
        self.note_remote_message(sender, channel, recipient);
        let who = self.name(sender);
        let to = if channel == "direct" {
            format!("→ {}", self.name(recipient))
        } else {
            format!("→ #{channel}")
        };
        self.feed(
            "message",
            sender,
            &format!("{who} {to}"),
            body,
            vec![format!("msg:{id}")],
            channel,
        );
        self.bubble(sender, "message", body);
        id
    }

    pub async fn decide(
        &mut self,
        agent: &str,
        policy: &str,
        recalled: &[String],
        action: &str,
        why: &str,
    ) -> i64 {
        let id = self.next("lv_decisions");
        let rec = if recalled.is_empty() {
            String::new()
        } else {
            format!(",{},", recalled.join(","))
        };
        let sql = format!(
            "INSERT INTO lv_decisions VALUES ({id}, {}, {}, {}, {}, {}, {})",
            self.now(),
            q(agent),
            q(policy),
            q(&rec),
            q(action),
            q(why)
        );
        self.sql(agent, "record decision", &sql).await;
        let k = self.ai(agent);
        self.w.agents[k].last_decision = Some(id);
        let who = self.name(agent);
        let mut refs = vec![format!("dec:{id}")];
        refs.extend(recalled.iter().cloned());
        self.feed(
            "decision",
            agent,
            &format!("{who} decides: {action}"),
            why,
            refs,
            "",
        );
        id
    }

    /// Remote messages (not face-to-face) open a short phone window for the
    /// sender and every recipient who is somewhere else.
    fn note_remote_message(&mut self, sender: &str, channel: &str, recipient: &str) {
        let now = self.now();
        let mut to: Vec<String> = if recipient.is_empty() {
            SCOPES
                .iter()
                .find(|(name, _, _)| *name == channel)
                .map(|(_, members, _)| members.iter().map(|m| m.to_string()).collect())
                .unwrap_or_default()
        } else {
            vec![recipient.to_string()]
        };
        to.retain(|r| r != sender);
        let place = |w: &World, id: &str| {
            w.agents
                .iter()
                .find(|a| a.id == id)
                .map(|a| (a.location.clone(), a.travel.is_some()))
        };
        let Some(from) = place(&self.w, sender) else {
            return;
        };
        let mut any_remote = false;
        for r in to {
            if let Some(there) = place(&self.w, &r) {
                let face_to_face = there.0 == from.0 && !there.1 && !from.1;
                if !face_to_face {
                    self.w.remote_msg.insert(r, now);
                    any_remote = true;
                }
            }
        }
        if any_remote {
            self.w.remote_msg.insert(sender.to_string(), now);
        }
    }

    /// Derive each foreground character's situated presence from authoritative
    /// state and append a LessDB row whenever the observable state changes.
    async fn update_presence(&mut self) {
        let now = self.now();
        let md = self.md();
        let kind = self.w.weather_kind.clone();
        // rain_until is authoritative (nudged and seeded showers set it);
        // weather_kind covers live-weather rain and storms.
        let raining = self.w.rain_until > self.w.minute || kind == "rain" || kind == "storm";
        let wet = kind == "wet_after_rain";
        // Singapore: sunrise ~07:00, sunset ~19:10 all year
        let dark = md < 7 * 60 || md >= 19 * 60 + 15;
        let bus_due = next_bus_start(now, 6).is_some();
        let mut changes = Vec::new();
        for k in 0..self.w.agents.len() {
            let id = self.w.agents[k].id.clone();
            let until = self.flag(&id, "drink_until");
            if until > 0 && now >= until && self.w.agents[k].carrying.as_deref() == Some("coffee") {
                self.w.agents[k].carrying = None;
                self.set_flag(&id, "drink_until", 0);
                self.w.agents[k].life.hunger = (self.w.agents[k].life.hunger - 5.0).max(0.0);
                let place = self.at(&id);
                self.event(
                    &id,
                    "drink.consumed",
                    &place,
                    "Finished the received drink after 30 minutes; hunger reduced by 5.",
                )
                .await;
            }
            let a = &self.w.agents[k];
            let nearby: Vec<String> = self
                .w
                .agents
                .iter()
                .filter(|b| b.id != a.id)
                .filter(|b| match (&a.travel, &b.travel) {
                    (None, None) => b.location == a.location,
                    (Some(x), Some(y)) => x.from == y.from && x.to == y.to && x.mode == y.mode,
                    _ => false,
                })
                .map(|b| b.id.clone())
                .collect();
            let crowd = if a.travel.is_none() {
                self.w
                    .background
                    .iter()
                    .filter(|b| b.location == a.location && b.target == b.location)
                    .count()
                    + nearby.len()
            } else {
                0
            };
            let talking = a.bubble.as_ref().map(|b| b.until >= now).unwrap_or(false);
            let situation = crate::presence::Situation {
                id: &a.id,
                location: &a.location,
                activity: &a.activity,
                activity_kind: &a.activity_kind,
                travel_from: a.travel.as_ref().map(|t| t.from.as_str()),
                travel_to: a.travel.as_ref().map(|t| t.to.as_str()),
                travel_mode: a.travel.as_ref().map(|t| t.mode.as_str()),
                carrying: a.carrying.as_deref(),
                talking,
                minute: now,
                last_remote_message: self.w.remote_msg.get(&a.id).copied(),
                raining,
                wet_ground: wet,
                dark,
                nearby,
                crowd,
                bus_due,
            };
            let next = crate::presence::derive(&situation, Some(&a.presence));
            let changed = !crate::presence::same_state(&a.presence, &next);
            // Finished or put-away items leave the authoritative carried state.
            if matches!(next.reason.as_str(), "finished_drink" | "set_down_at_home") {
                self.w.agents[k].carrying = None;
            }
            self.w.agents[k].presence = next;
            if changed {
                changes.push(k);
            }
        }
        for k in changes {
            let agent = self.w.agents[k].id.clone();
            let p = self.w.agents[k].presence.clone();
            let detail = format!(
                "item={} umbrella={} nearby={} surroundings={} policy={}",
                p.item.as_deref().unwrap_or("-"),
                p.umbrella,
                p.nearby.join("|"),
                p.surroundings.join("|"),
                p.policy
            );
            let id = self.next("lv_presence_events");
            let sql = format!(
                "INSERT INTO lv_presence_events VALUES ({id}, {now}, {}, {}, {}, {}, {}, {}, {})",
                q(&agent),
                q(&p.attention),
                q(&p.hands),
                q(p.companion.as_deref().unwrap_or("")),
                q(&p.setting),
                q(&p.reason),
                q(&detail)
            );
            self.sql(&agent, "record situated presence", &sql).await;
        }
    }

    pub async fn say(&mut self, agent: &str, text: &str) {
        self.bubble(agent, "say", text);
        let who = self.name(agent);
        self.feed("say", agent, &format!("{who} says"), text, vec![], "");
        if let Some(tx) = &self.llm {
            let _ = tx.send(crate::llm::Job::Phrase {
                agent: agent.into(),
                text: text.into(),
                minute: self.w.minute,
            });
        }
    }

    /// Scope-filtered recall. Returns visible rows plus the SQL predicate used.
    pub async fn recall(&mut self, reader: &str, extra: &str, purpose: &str) -> Vec<Row> {
        let sql = format!(
            "SELECT {MEM_COLS} FROM lv_memories WHERE {} AND (expires_at < 0 OR expires_at > {}) AND {extra} ORDER BY id DESC LIMIT 5",
            acl(reader),
            self.now()
        );
        self.sql(reader, purpose, &sql).await
    }

    /// Count memories about `subject` that `reader`'s scope rule hides; log the access.
    pub async fn audit_hidden(&mut self, reader: &str, subject: &str, visible: usize) {
        let sql = format!(
            "SELECT count(*) AS n FROM lv_memories WHERE owner = {} AND NOT {}",
            q(subject),
            acl(reader)
        );
        let rows = self
            .sql(reader, "count rows hidden by scope rule", &sql)
            .await;
        let hidden = rows.first().map(|r| i(r, "n")).unwrap_or(0);
        let id = self.next("lv_access");
        let ins = format!(
            "INSERT INTO lv_access VALUES ({id}, {}, {}, {}, {visible}, {hidden}, {})",
            self.now(),
            q(reader),
            q(subject),
            q("app-layer scope rule: owner OR scope IN reader's memberships")
        );
        self.sql(reader, "audit access", &ins).await;
        if hidden > 0 {
            let (r, sname) = (self.name(reader), self.name(subject));
            self.feed(
                "access",
                reader,
                &format!("Scope rule hid {hidden} of {sname}'s memories from {r}"),
                &format!("{visible} visible · {hidden} private rows filtered by the app-layer scope predicate (not native LessDB ACL)"),
                vec![format!("acc:{id}")],
                "private",
            );
        }
    }

    async fn inbox(&mut self, agent: &str) -> Vec<Row> {
        let last = self.flag(agent, "inbox_seen");
        let sql = format!(
            "SELECT id, sim_minute, sender, channel, recipient, topic, context_id, body FROM lv_messages WHERE id > {last} AND sender <> {a} AND (recipient = {a} OR channel IN (SELECT scope FROM lv_scope_members WHERE agent = {a})) ORDER BY id",
            a = q(agent)
        );
        let rows = self.sql(agent, "read inbox", &sql).await;
        if let Some(maxid) = rows.iter().map(|r| i(r, "id")).max() {
            self.set_flag(agent, "inbox_seen", maxid);
        }
        rows
    }

    // ---------- movement ----------

    fn travel(&mut self, id: &str, to: &str, minutes: i64, mode: &str) {
        let k = self.ai(id);
        let from = self.w.agents[k].location.clone();
        if from == to {
            return;
        }
        for occupant in self.w.cafe.tables.values_mut() {
            if occupant == id {
                *occupant = "free".into();
            }
        }
        let now = self.now();
        let a = &mut self.w.agents[k];
        a.travel = Some(Travel {
            from,
            to: to.into(),
            depart: now,
            arrive: now + minutes,
            mode: mode.into(),
        });
        a.location = "transit".into();
        a.activity = format!("Heading to {}", place_name(to));
        a.activity_kind = if mode == "bus" {
            "riding_bus".into()
        } else {
            "commuting".into()
        };
        a.station = "street".into();
        a.pose = if mode == "bus" {
            "seat".into()
        } else {
            "walk_side".into()
        };
    }

    fn go(&mut self, id: &str, to: &str) {
        let from = self.at(id);
        let (mins, mode) = if offmap(&from) || offmap(to) {
            (12, "exit")
        } else {
            (8, "walk")
        };
        self.travel(id, to, mins, mode);
    }

    fn plan(&mut self, at: i64, agent: &str, action: &str) {
        self.w.plans.push(Plan {
            at,
            agent: agent.into(),
            action: action.into(),
        });
    }

    // ---------- lifecycle ----------

    pub async fn seed(&mut self) {
        self.supply_meals().await;
        for (scope, members, _) in SCOPES {
            for m in *members {
                let sql = format!(
                    "INSERT INTO lv_scope_members VALUES ({}, {}, {}, 'world-seed', {})",
                    q(&format!("{scope}/{m}")),
                    q(scope),
                    q(m),
                    self.now()
                );
                self.sql("world", "grant scope membership", &sql).await;
            }
        }
        let seeds: &[(&str, &str, &str, &str)] = &[
            (
                "ama",
                "family",
                "family.fact",
                "Sam is my son; we share the house with Mochi the corgi.",
            ),
            (
                "ama",
                "private",
                "pet.routine",
                "Mochi needs a morning and an afternoon walk.",
            ),
            (
                "sam",
                "private",
                "work.deadline",
                "Quarterly report is due Friday.",
            ),
            (
                "lin",
                "private",
                "pref.private",
                "I prefer oat milk — dairy doesn't agree with me.",
            ),
            (
                "maya",
                "cafe-staff",
                "cafe.ops",
                "Supplier runs fit best after the lunch rush (from 14:00).",
            ),
            (
                "bo",
                "cafe-staff",
                "cafe.ops",
                "Oat milk runs out fastest — check the fridge after oat orders.",
            ),
        ];
        for (owner, scope, tag, content) in seeds {
            self.memory(
                owner,
                "permanent",
                scope,
                "",
                tag,
                "",
                -1,
                ("seed", 0),
                content,
            )
            .await;
        }
        self.ensure_identity_threads().await;
        for a in self.w.agents.iter_mut() {
            a.bubble = None;
        }
        self.feed(
            "world",
            "world",
            "A new Lessville day begins",
            "Day 1 · 06:30 SGT. Six seed memories and scope memberships written to LessDB.",
            vec![],
            "",
        );
        self.persist().await;
    }

    pub async fn persist(&mut self) {
        let revision = self.next("world_snapshot");
        // Persist only the newest feed items; the full history already lives in LessDB tables.
        let mut world = serde_json::to_value(&self.w).unwrap_or_default();
        if let Some(feed) = world.get_mut("feed").and_then(|f| f.as_array_mut()) {
            let keep = 20usize.min(feed.len());
            let cut = feed.len() - keep;
            feed.drain(..cut);
        }
        let json = world.to_string();
        // Distinct keys avoid replacement-order loss when the engine merges parts.
        let key = format!("world:{revision:020}");
        self.store
            .run(
                self.now(),
                "world",
                "versioned world checkpoint",
                &format!(
                    "INSERT INTO lv_kv VALUES ({}, {}, {})",
                    q(&key),
                    q(&json),
                    self.now()
                ),
            )
            .await
            .expect("persist versioned world checkpoint");
        if revision > 64 {
            let oldest = format!("world:{:020}", revision - 63);
            self.store
                .run(
                    self.now(),
                    "world",
                    "bound world checkpoints",
                    &format!(
                        "DELETE FROM lv_kv WHERE k LIKE 'world:%' AND k < {}",
                        q(&oldest)
                    ),
                )
                .await
                .expect("retain newest world checkpoints");
        }
        let sql = format!(
            "INSERT INTO lv_kv VALUES ('world', {}, {})",
            q(&json),
            self.now()
        );
        self.sql("world", "checkpoint world snapshot", &sql).await;
        let agents = self.w.agents.clone();
        let mut values = Vec::new();
        for a in &agents {
            let st =
                json!({"travel": a.travel, "flags": a.flags, "carrying": a.carrying, "activity_kind": a.activity_kind, "station": a.station, "pose": a.pose, "life": a.life}).to_string();
            values.push(format!(
                "({}, {}, {}, {}, {})",
                q(&a.id),
                q(&a.location),
                q(&a.activity),
                q(&st),
                self.now()
            ));
        }
        let sql = format!("INSERT INTO lv_agent_state VALUES {}", values.join(", "));
        self.sql("world", "upsert agent state (UNIQUE agent)", &sql)
            .await;
    }

    pub async fn advance(&mut self) {
        self.w.minute += 1;
        let md = self.md();
        self.update_cafe_state();
        self.arrivals().await;
        self.tick_life().await;
        if md == DAY_END {
            self.night().await;
        }
        if !(DAY_START..DAY_END).contains(&md) {
            self.tick_background();
            self.tick_traffic_lights();
            self.tick_traffic();
            self.update_presence().await;
            if self.now() % 10 == 0 {
                self.persist().await;
            }
            return;
        }
        if md == DAY_START {
            self.morning().await;
        }
        self.weather().await;
        self.explore_novelty().await;
        self.buses().await;
        self.run_plans().await;
        if !self.life_busy("ama") && md < 1290 {
            self.ama().await;
        }
        if !self.life_busy("sam") && md < 1290 {
            self.sam().await;
        }
        if !self.life_busy("lin") && md < 1290 {
            self.lin().await;
        }
        if !self.life_busy("maya") && md < 1290 {
            self.maya().await;
        }
        if !self.life_busy("bo") && md < 1290 {
            self.bo().await;
        }
        self.tick_orders().await;
        self.tick_background();
        self.tick_traffic_lights();
        self.tick_traffic();
        self.update_presence().await;
        let now = self.now();
        for a in self.w.agents.iter_mut() {
            if a.bubble.as_ref().map(|b| b.until < now).unwrap_or(false) {
                a.bubble = None;
            }
        }
        if self
            .w
            .bus_arrived
            .map(|(_, at)| now > at + 3)
            .unwrap_or(false)
        {
            self.w.bus_arrived = None;
        }
        if now % 10 == 0 {
            self.persist().await;
        }
    }

    /// Nightly bounded compaction of hot tables (see retention.rs).
    pub(crate) async fn compact_hot_tables(&mut self) {
        let now = self.now();
        let report =
            crate::retention::compact(&self.store, now, &["maya", "sam", "lin", "ama", "bo"]).await;
        let summary = report
            .tables
            .iter()
            .map(|(t, n)| format!("{t}={n}"))
            .collect::<Vec<_>>()
            .join(", ");
        let detail = format!(
            "policy={} removed={} [{}]{}",
            crate::retention::RETENTION_POLICY_VERSION,
            report.removed,
            summary,
            if report.failed.is_empty() {
                String::new()
            } else {
                format!(" failed={}", report.failed.len())
            }
        );
        self.event("world", "retention", "", &detail).await;
        if report.removed > 0 {
            self.feed(
                "memory",
                "world",
                "Night compaction kept the hot memory bounded",
                &format!(
                    "{} old row(s) left the hot tables; older evidence stays in checkpoint history. Permanent memories are capped, not aged out.",
                    report.removed
                ),
                vec![],
                "",
            );
        }
        self.w.last_retention = Some(report);
    }

    async fn night(&mut self) {
        let day = day_of(self.now());
        for a in &mut self.w.agents {
            a.bubble = None;
            a.flags.remove("await_bus");
        }
        self.w.plans.clear();
        self.feed(
            "world",
            "world",
            &format!("Night falls on Day {day}"),
            "The clock continues minute by minute; residents recover while asleep. Stored memories persist in LessDB.",
            vec![],
            "",
        );
        self.event("world", "night", "", &format!("Day {day} ended"))
            .await;
        self.compact_hot_tables().await;
        if let Some(tx) = &self.llm {
            for id in ["maya", "sam", "lin", "ama", "bo"] {
                let _ = tx.send(crate::llm::Job::Reflect {
                    agent: id.into(),
                    day,
                    minute: self.w.minute,
                });
            }
            self.feed("world", "world", "Nightly reflections requested", "Each agent may turn up to 6 of today's own memories into one permanent lesson (one small model call each, within the daily budget).", vec![], "");
        }
        self.persist().await;
    }

    async fn morning(&mut self) {
        self.supply_meals().await;
        let day = day_of(self.now());
        if day > 1 {
            // Breakfast uses eggs; restocked fridges last a couple of days.
            let eggs = self.w.stock.get("eggs").copied().unwrap_or(0);
            self.w.stock.insert("eggs".into(), (eggs - 4).max(0));
            let oat = self.w.stock.get("oat_milk").copied().unwrap_or(0);
            self.w.stock.insert("oat_milk".into(), (oat - 3).max(0));
        }
        for (id, act) in [
            ("maya", "Opening the café"),
            ("sam", "Getting ready for work"),
            ("ama", "Making breakfast"),
            ("lin", "At the dorm"),
            ("bo", "Commuting in"),
        ] {
            self.set_activity(id, act);
        }
        if day == 2 && !self.w.done.contains("occupation-sam-analyst") {
            self.w.done.insert("occupation-sam-analyst".into());
            let k = self.ai("sam");
            let from = self.w.agents[k].occupation.clone();
            self.w.agents[k].occupation = "Operations analyst".into();
            self.w.agents[k].title = "Operations analyst · Ama's son".into();
            self.w.occupation_changes.push(OccupationChange {
                agent: "sam".into(),
                from: from.clone(),
                to: "Operations analyst".into(),
                at: self.now(),
                reason: "Sam completed a role transition at work; future routines use the new occupation.".into(),
            });
            let eid = self
                .event(
                    "sam",
                    "occupation.changed",
                    "office",
                    "Sam starts a new Operations analyst role",
                )
                .await;
            self.memory(
                "sam",
                "permanent",
                "private",
                "office",
                "occupation.transition",
                "",
                -1,
                ("event", eid),
                &format!("Occupation changed from {from} to Operations analyst on Day {day}."),
            )
            .await;
            self.feed(
                "world",
                "sam",
                "Sam's work changes",
                "The occupation transition is stored as an event and permanent memory; later context can explain why his routine changed.",
                vec![format!("event:{eid}")],
                "private",
            );
        }
        self.feed(
            "world",
            "world",
            &format!("Day {day} · 06:30 SGT"),
            "Morning routines start. Agents will consult what LessDB already holds before acting.",
            vec![],
            "",
        );
    }

    async fn weather(&mut self) {
        let now = self.now();
        let md = self.md();
        if (md % 180 == 0 || (day_of(now) == 1 && md == 810)) && self.w.weather_changed_at != now {
            let bucket = md / 180;
            let roll = seeded_roll(self.w.world_seed, day_of(now), bucket) % 100;
            let (kind, temp, wind) = if day_of(now) == 1 && md == 810 {
                ("rain", 27, 18)
            } else if roll < 18 {
                ("rain", 27, 20)
            } else if roll < 43 {
                ("cloudy", 28, 12)
            } else if roll > 90 {
                ("windy", 30, 28)
            } else {
                ("clear", 30, 8)
            };
            self.w.weather_kind = kind.into();
            self.w.temperature_c = temp;
            self.w.wind_kmh = wind;
            self.w.weather_changed_at = now;
            let detail = format!(
                "{}°C · wind {} km/h · seeded world roll {}",
                temp, wind, roll
            );
            self.event(
                "world",
                "weather.changed",
                "",
                &format!("Weather: {kind} ({detail})"),
            )
            .await;
            self.feed(
                "world",
                "world",
                &format!("Weather changes: {kind}"),
                &detail,
                vec![],
                "",
            );
            if kind == "rain" {
                self.start_rain(45, &format!("{detail}; outdoor routes may change"))
                    .await;
            }
        }
        if self.w.rain_until > 0 && now >= self.w.rain_until {
            self.w.rain_until = 0;
            self.w.last_rain_end = now;
            self.w.weather_kind = "wet_after_rain".into();
            self.event("world", "weather.clear", "", "Rain stopped")
                .await;
            self.feed("world", "world", "The rain stops", "Outdoor paths stay wet for a few hours (agents only know if someone observes and records it).", vec![], "");
        }
        let wet = self.w.rain_until > now || self.w.last_rain_end >= now - 180;
        for a in &mut self.w.agents {
            if wet
                && matches!(
                    a.location.as_str(),
                    "park" | "trail" | "transit" | "bus_stop"
                )
            {
                a.behaviour = "weather-aware".into();
                a.mood = "cautious".into();
            } else if !wet && a.behaviour == "weather-aware" {
                a.behaviour = "routine".into();
                a.mood = "neutral".into();
            }
        }
    }

    fn tick_background(&mut self) {
        let now = self.now();
        let md = self.md();
        let day = day_of(now);
        let wet = self.w.rain_until > now;
        let weekend = (day - 1) % 7 >= 5;
        let workplaces = [
            "school", "grocery", "office", "office", "office", "school", "grocery", "park",
        ];
        for (index, person) in self.w.background.iter_mut().enumerate() {
            let asleep = person.location == "away" && (md < DAY_START || md >= DAY_END);
            person.life.minute(asleep);
            if day > person.supplied_day && md >= DAY_START {
                person.food_portions = 3;
                person.supplied_day = day;
                person.remember(
                    now,
                    "Received three daily packed portions from the external household supply",
                );
            }
            if person.travel_until > 0 {
                if now < person.travel_until {
                    continue;
                }
                person.location = person.target.clone();
                person.travel_until = 0;
                person.remember(now, &format!("Arrived at {}", person.location));
            }
            if person.eating_until > 0 {
                if now < person.eating_until {
                    continue;
                }
                person.life.hunger = (person.life.hunger - 55.0).max(0.0);
                person.life.meals_completed += 1;
                person.eating_until = 0;
                person.remember(now, "Finished a packed meal after 15 minutes");
            }
            if asleep {
                person.activity = "Asleep at home".into();
                continue;
            }
            if person.life.hunger >= 60.0 && person.food_portions > 0 {
                person.food_portions -= 1;
                person.eating_until = now + 15;
                person.activity = "Eating a packed meal".into();
                person.remember(now, "Opened one packed portion; eating started");
                continue;
            }
            let shift = (index % TOWN_SUPPORTING_POPULATION) as i64 * 3;
            let work = workplaces[index % workplaces.len()];
            let target = if md < 450 + shift || md >= 1200 + shift {
                "away"
            } else if md < 1020 + shift && !weekend {
                work
            } else if wet {
                "grocery"
            } else if md < 1140 + shift {
                ["park", "gym", "trail", "grocery"][index % 4]
            } else {
                "grocery"
            };
            if person.location != target {
                person.target = target.into();
                person.travel_until = now + 10 + (index % 6) as i64;
                person.next_change = person.travel_until;
                person.activity = format!("Walking to {}", place_name(target));
                person.mood = if wet { "sheltering" } else { "calm" }.into();
            } else {
                person.activity = if target == work && !weekend {
                    format!("Working as {}", person.occupation)
                } else if target == "park" {
                    "Relaxing in the park".into()
                } else if target == "gym" {
                    "Exercising at the gym".into()
                } else if target == "trail" {
                    "Walking the reservoir trail".into()
                } else if target == "grocery" {
                    "Shopping for household supplies".into()
                } else {
                    "Resting at home".into()
                };
                if target == work && !weekend {
                    person.life.practice_minutes += 1;
                    if person.life.practice_minutes >= 60 {
                        person.life.practice_minutes = 0;
                        person.life.practice_sessions =
                            person.life.practice_sessions.saturating_add(1);
                        person.remember(now, "Completed an hour of work or study practice");
                    }
                }
            }
        }
    }

    fn tick_traffic_lights(&mut self) {
        let now = self.now();
        for light in &mut self.w.traffic_lights {
            let cycle = light.cycle_minutes.max(3);
            let phase_minute = (now + light.offset).rem_euclid(cycle);
            light.phase = if phase_minute < cycle / 2 {
                "green"
            } else if phase_minute == cycle / 2 {
                "amber"
            } else {
                "red"
            }
            .into();
            light.elapsed = phase_minute;
        }
    }

    fn tick_traffic(&mut self) {
        let now = self.now();
        let wet = self.w.rain_until > now || self.w.last_rain_end >= now - 180;
        let prior = self.w.traffic.clone();
        let junction_phase = self
            .w
            .traffic_lights
            .iter()
            .find(|light| light.id == "signal-main-east-west")
            .map(|light| light.phase.as_str())
            .unwrap_or("green");
        for actor in &mut self.w.traffic {
            let base = actor.speed_kmh.max(4) as f32 / 60.0;
            let weather_factor = if wet {
                if actor.kind == "bicycle" { 0.55 } else { 0.72 }
            } else {
                1.0
            };
            let sign = if actor.direction == "west" { -1.0 } else { 1.0 };
            let mut next = actor.position + sign * base * weather_factor;
            actor.phase = "cruising".into();
            if !(0.0..=100.0).contains(&next) {
                next = next.clamp(0.0, 100.0);
                actor.direction = if actor.direction == "west" {
                    "east".into()
                } else {
                    "west".into()
                };
                actor.phase = "turning_at_junction".into();
            }
            let blocked = prior.iter().any(|other| {
                other.id != actor.id
                    && other.lane == actor.lane
                    && other.direction == actor.direction
                    && (other.position - actor.position) * sign > 0.0
                    && (other.position - next).abs() < 7.0
            });
            if blocked {
                next = actor.position;
                actor.phase = "yielding_safe_gap".into();
            }
            let crosses_junction =
                (actor.position < 50.0 && next >= 50.0) || (actor.position > 50.0 && next <= 50.0);
            if crosses_junction && matches!(junction_phase, "red" | "amber") {
                next = if actor.direction == "west" {
                    51.0
                } else {
                    49.0
                };
                actor.phase = "stopped_at_red_light".into();
            }
            actor.position = next;
        }
    }

    pub async fn apply_external_weather(
        &mut self,
        code: i32,
        temperature_c: i32,
        wind_kmh: i32,
        observed_at: i64,
        source: &str,
    ) {
        let kind = weather_kind_from_code(code);
        let changed = self.w.weather_kind != kind
            || self.w.weather_code != code
            || self.w.temperature_c != temperature_c
            || self.w.wind_kmh != wind_kmh;
        self.w.weather_kind = kind.into();
        self.w.weather_code = code;
        self.w.temperature_c = temperature_c;
        self.w.wind_kmh = wind_kmh;
        self.w.weather_source = source.into();
        self.w.weather_observed_at = observed_at;
        self.w.weather_changed_at = self.now();
        if matches!(kind, "rain" | "showers" | "snow") {
            self.w.rain_until = self.now() + 45;
            self.w.rain_reason = format!("Live weather observation from {source}");
        }
        if changed {
            let detail = format!(
                "{kind} · {temperature_c}°C · wind {wind_kmh} km/h · WMO {code} · source {source}"
            );
            self.event("world", "weather.external", "", &detail).await;
            self.feed(
                "world",
                "world",
                "Live weather updates Lessville",
                &detail,
                vec![],
                "",
            );
        }
    }

    pub async fn start_rain(&mut self, minutes: i64, reason: &str) {
        self.w.rain_until = self.now() + minutes;
        self.w.rain_reason = reason.into();
        // keep the visible weather label consistent with the rain that is falling
        if self.w.weather_kind != "storm" {
            self.w.weather_kind = "rain".into();
        }
        self.event("world", "weather.rain", "", reason).await;
        self.feed("world", "world", "Rain starts", reason, vec![], "");
    }

    async fn arrivals(&mut self) {
        let now = self.now();
        let due: Vec<(String, String)> = self
            .w
            .agents
            .iter()
            .filter_map(|a| {
                a.travel
                    .as_ref()
                    .filter(|t| t.arrive <= now)
                    .map(|t| (a.id.clone(), t.to.clone()))
            })
            .collect();
        for (id, to) in due {
            let k = self.ai(&id);
            self.w.agents[k].travel = None;
            self.w.agents[k].location = to.clone();
            self.set_activity(&id, &format!("At {}", place_name(&to)));
            self.on_arrive(&id, &to).await;
        }
    }

    async fn buses(&mut self) {
        let now = self.now();
        for back in 0..=60 {
            let sched = now - back;
            let smd = sched.rem_euclid(DAY);
            if !(420..=1260).contains(&smd) || (smd - 420) % 20 != 0 {
                continue;
            }
            let delay = self.w.bus_delays.get(&sched).copied().unwrap_or(0);
            if sched + delay == now {
                self.bus_arrives(sched, delay).await;
            }
        }
    }

    async fn bus_arrives(&mut self, sched: i64, delay: i64) {
        let now = self.now();
        self.w.bus_arrived = Some((sched, now));
        let trip = bus_trip(sched, delay, now);
        self.w.bus_trip = Some(trip.id.clone());
        if delay > 0 {
            self.event(
                "world",
                "bus.arrive",
                "bus_stop",
                &format!("{} bus arrived {delay} min late", clock(sched)),
            )
            .await;
        }
        // Inbound riders
        let riders: Vec<Plan> = self
            .w
            .plans
            .iter()
            .filter(|p| p.at == sched && p.action.starts_with("ride"))
            .cloned()
            .collect();
        self.w
            .plans
            .retain(|p| !(p.at == sched && p.action.starts_with("ride")));
        for p in riders {
            let k = self.ai(&p.agent);
            self.w.agents[k].location = "bus_stop".into();
            self.w.agents[k].travel = None;
            self.set_activity(&p.agent, "Stepping off the bus");
            self.event(
                &p.agent,
                "bus.ride.complete",
                "bus_stop",
                &format!("Completed named trip {}", trip.id),
            )
            .await;
            self.on_bus_alight(&p.agent, sched, delay).await;
        }
        // Outbound riders waiting at the stop
        let waiting: Vec<(String, String)> = self
            .w
            .agents
            .iter()
            .filter(|a| {
                a.location == "bus_stop" && a.flags.get("await_bus").copied().unwrap_or(0) == 1
            })
            .map(|a| {
                (
                    a.id.clone(),
                    a.notes
                        .get("bus_to")
                        .cloned()
                        .unwrap_or_else(|| "away".into()),
                )
            })
            .collect();
        for (id, to) in waiting {
            self.set_flag(&id, "await_bus", 0);
            self.travel(&id, &to, 15, "bus");
            self.event(
                &id,
                "bus.ride.board",
                "bus_stop",
                &format!("Boarded named trip {} toward {to}", trip.id),
            )
            .await;
        }
    }

    async fn run_plans(&mut self) {
        let now = self.now();
        let busy: BTreeSet<String> = self
            .w
            .agents
            .iter()
            .filter(|a| a.life.meal.is_some() || a.life.conversation.is_some())
            .map(|a| a.id.clone())
            .collect();
        let due: Vec<Plan> = self
            .w
            .plans
            .iter()
            .filter(|p| p.at <= now && !p.action.starts_with("ride") && !busy.contains(&p.agent))
            .cloned()
            .collect();
        self.w.plans.retain(|p| {
            !(p.at <= now && !p.action.starts_with("ride") && !busy.contains(&p.agent))
        });
        for p in due {
            if let Some(dest) = p.action.strip_prefix("go:") {
                self.go(&p.agent, dest);
            } else if p.action == "maya:restock" {
                self.decide(
                    "maya",
                    "maya.supplier_run v1 (deterministic)",
                    &[self
                        .w
                        .restock
                        .clone()
                        .map(|c| format!("ctx:{c}"))
                        .unwrap_or_default()],
                    "Leave for the supplier run",
                    "Planned earlier from Bo's restock request; the lunch rush is over.",
                )
                .await;
                self.go("maya", "grocery");
            }
        }
    }

    // ---------- shared interactions ----------

    fn staff_on_duty(&self) -> Option<&'static str> {
        if self.at("bo") == "cafe"
            && !self.life_busy("bo")
            && self.w.agents[self.ai("bo")].activity_kind != "sleeping"
        {
            Some("bo")
        } else if self.at("maya") == "cafe"
            && !self.life_busy("maya")
            && self.w.agents[self.ai("maya")].activity_kind != "sleeping"
        {
            Some("maya")
        } else {
            None
        }
    }

    fn update_cafe_state(&mut self) {
        let md = self.md();
        let (lifecycle, lights, door, till, espresso) = if md < 405 {
            ("closed", "off", "locked", "closed", "off")
        } else if md < 420 {
            ("opening", "on", "locked", "ready", "heating")
        } else if md < 1200 {
            ("open", "on", "open", "ready", "ready")
        } else if md < 1220 {
            ("closing", "on", "locked", "cashout", "off")
        } else {
            ("closed", "off", "locked", "closed", "off")
        };
        self.w.cafe.lifecycle = lifecycle.into();
        self.w.cafe.lights = lights.into();
        self.w.cafe.front_door = door.into();
        self.w.cafe.till = till.into();
        self.w.cafe.espresso = espresso.into();
        self.w.cafe.fridge = "closed".into();
    }

    async fn order_transition(&mut self, order: &mut CafeOrder, status: &str, detail: &str) -> i64 {
        order.status = status.into();
        order.detail = detail.into();
        order.completed_at = if matches!(status, "handed_off" | "declined" | "cancelled") {
            Some(self.now())
        } else {
            None
        };
        let id = self.next("lv_orders");
        let sql = format!(
            "INSERT INTO lv_orders VALUES ({id}, {}, {}, {}, {}, {}, {}, {}, {}, {})",
            q(&order.context_id),
            self.now(),
            q(&order.customer),
            q(&order.staff),
            q(&order.requested),
            q(&order.ingredient),
            q(&order.alternative),
            q(status),
            q(detail)
        );
        self.sql(&order.staff, "record café order transition", &sql)
            .await;
        self.feed(
            "order",
            &order.customer,
            &format!("Café order {}", status.replace('_', " ")),
            detail,
            vec![format!("order:{id}"), format!("ctx:{}", order.context_id)],
            "cafe-staff",
        );
        if status == "handed_off" {
            let interaction_id = format!("interaction:{}", order.context_id);
            let evidence = vec![format!("order:{id}"), format!("ctx:{}", order.context_id)];
            self.interaction(
                &interaction_id,
                &order.customer,
                &order.staff,
                "cafe-service",
                &order.context_id,
                &format!("requests {}", order.requested),
                detail,
                "customer preference exchanged for a staff service response",
                &evidence,
                "handed_off",
            )
            .await;
            self.competency(
                &order.staff,
                "social-service",
                "context-aware service",
                &evidence,
                "cafe.service.v1",
            )
            .await;
            self.competency(
                &order.customer,
                "self-advocacy",
                "clear preference communication",
                &evidence,
                "cafe.preference.v1",
            )
            .await;
        }
        id
    }

    async fn counter_order(&mut self, cust: &str, drink: &str, ingredient: &str) {
        self.update_cafe_state();
        if self.at(cust) != "cafe"
            || self.w.agents[self.ai(cust)].travel.is_some()
            || self.life_busy(cust)
            || self.w.cafe.lifecycle != "open"
            || self.w.cafe.queue.iter().any(|o| o.customer == cust)
            || self
                .w
                .cafe
                .active_order
                .as_ref()
                .is_some_and(|o| o.customer == cust && o.status == "preparing")
        {
            return;
        }
        let Some(staff) = self.staff_on_duty() else {
            return;
        };
        let cname = self.name(cust);
        let rows = self
            .recall(
                staff,
                &format!("owner = {} AND tag = 'pref.usual'", q(cust)),
                &format!("recall {cname}'s shared usual"),
            )
            .await;
        self.audit_hidden(staff, cust, rows.len()).await;
        let (drink, ingredient) = if let Some(r) = rows.first() {
            let mid = i(r, "id");
            self.decide(
                staff,
                "staff.greet_regular v1 (deterministic)",
                &[format!("mem:{mid}")],
                &format!("Prepare {cname}'s remembered usual"),
                &s(r, "content"),
            )
            .await;
            ("oat latte (less sweet)", "oat_milk")
        } else {
            (drink, ingredient)
        };
        let mut order = CafeOrder {
            context_id: format!("order-d{}-{}-{}", day_of(self.now()), cust, self.now()),
            customer: cust.into(),
            staff: staff.into(),
            requested: drink.into(),
            ingredient: ingredient.into(),
            status: "queued".into(),
            started_at: self.now(),
            ..Default::default()
        };
        self.order_transition(
            &mut order,
            "queued",
            "Customer joined the counter queue; no ingredient consumed yet.",
        )
        .await;
        self.w.cafe.queue.push_back(order);
        self.set_activity(cust, "Waiting for a café order");
        if cust == "lin"
            && rows.is_empty()
            && self.flag("lin", "consented") == 0
            && self.at("maya") == "cafe"
            && self.flag("lin", "asked") == 0
        {
            self.set_flag("lin", "asked", 1);
            let ask = self
                .message(
                    "maya",
                    "direct",
                    "lin",
                    "consent.ask",
                    "",
                    "Want us to remember your usual for next time? Only café staff would see it.",
                )
                .await;
            let pref = self
                .recall(
                    "lin",
                    "owner = 'lin' AND tag = 'pref.private'",
                    "check own private preference",
                )
                .await;
            let mut rec = vec![format!("msg:{ask}")];
            if let Some(p) = pref.first() {
                rec.push(format!("mem:{}", i(p, "id")));
            }
            self.decide("lin", "lin.share_preference v1 (deterministic)", &rec, "Share my usual with café staff", "Maya asked first; sharing only the order (not why) to the cafe-regulars scope is low-risk.").await;
            let shared_mid = self
                .memory(
                    "lin",
                    "permanent",
                    "cafe-regulars",
                    "cafe",
                    "pref.usual",
                    "",
                    -1,
                    ("message", ask),
                    "Usual order: oat latte, less sweet.",
                )
                .await;
            self.set_flag("lin", "consented", 1);
            let grant = self
                .message(
                    "lin",
                    "direct",
                    "maya",
                    "consent.grant",
                    "",
                    "Sure — oat latte, less sweet. Thanks!",
                )
                .await;
            self.relationship(
                "rel:lin:maya:cafe:v1",
                "lin",
                "maya",
                "cafe-regular",
                "granted",
                "preference shared for service; café responds with the requested usual",
                "cafe-regulars",
                &[
                    format!("msg:{ask}"),
                    format!("mem:{shared_mid}"),
                    format!("msg:{grant}"),
                ],
                "active",
                90,
                "relationship.v1",
            )
            .await;
        }
    }

    async fn tick_orders(&mut self) {
        let now = self.now();
        if let Some(mut order) = self.w.cafe.active_order.take() {
            if order.status == "preparing" {
                let present = self.at(&order.customer) == "cafe"
                    && self.at(&order.staff) == "cafe"
                    && !self.life_busy(&order.customer)
                    && !self.life_busy(&order.staff)
                    && self.w.cafe.lifecycle == "open";
                if !present {
                    self.order_transition(&mut order, "cancelled", "Service interrupted; the reserved ingredient is discarded, no drink handed over.").await;
                } else if now >= order.started_at + 3 {
                    let k = self.ai(&order.customer);
                    self.w.agents[k].carrying = Some("coffee".into());
                    self.set_flag(
                        &order.customer,
                        "drink_until",
                        now + crate::presence::DRINK_MINUTES,
                    );
                    self.order_transition(
                        &mut order,
                        "handed_off",
                        "Preparation finished; the customer received the drink.",
                    )
                    .await;
                    self.object_transition(
                        &order.customer,
                        &order.ingredient,
                        "cafe:in_preparation",
                        "customer:carrying",
                        "cafe",
                        &order.context_id,
                        "Prepared drink handed to the present customer.",
                    )
                    .await;
                    self.set_activity(&order.customer, &format!("Enjoying {}", order.requested));
                    let count = self.flag(&order.staff, "successful_orders") + 1;
                    self.set_flag(&order.staff, "successful_orders", count);
                    self.w.agents[k].life.social_need =
                        (self.w.agents[k].life.social_need - 3.0).max(0.0);
                }
            }
            self.w.cafe.active_order = Some(order);
        }
        if self
            .w
            .cafe
            .active_order
            .as_ref()
            .is_some_and(|o| o.status == "preparing")
        {
            return;
        }
        let Some(mut order) = self.w.cafe.queue.pop_front() else {
            return;
        };
        if self.at(&order.customer) != "cafe"
            || self.life_busy(&order.customer)
            || self.w.cafe.lifecycle != "open"
            || now - order.started_at > 20
        {
            self.order_transition(
                &mut order,
                "cancelled",
                "Customer left, became busy, or service closed/timed out; no ingredient consumed.",
            )
            .await;
            self.w.cafe.active_order = Some(order);
            return;
        }
        let Some(staff) = self.staff_on_duty() else {
            self.w.cafe.queue.push_front(order);
            return;
        };
        order.staff = staff.into();
        let mut have = self.w.stock.get(&order.ingredient).copied().unwrap_or(0);
        if have <= 0
            && order.customer != "lin"
            && self.w.stock.get("milk").copied().unwrap_or(0) > 0
        {
            order.alternative = "milk".into();
            self.order_transition(&mut order, "alternative_accepted", "Customer's policy accepts milk; Lin's dietary constraint never permits this substitution.").await;
            order.ingredient = "milk".into();
            have = self.w.stock["milk"];
        }
        if have <= 0 {
            self.order_transition(
                &mut order,
                "declined",
                "No suitable ingredient in stock; no drink and no stock decrement.",
            )
            .await;
            self.check_stock(staff).await;
        } else {
            self.w.stock.insert(order.ingredient.clone(), have - 1);
            order.started_at = now;
            self.order_transition(
                &mut order,
                "preparing",
                "Ingredient reserved once; preparation takes three simulated minutes.",
            )
            .await;
            self.object_transition(
                staff,
                &order.ingredient,
                "cafe:in_stock",
                "cafe:in_preparation",
                "cafe",
                &order.context_id,
                "One ingredient reserved for the active order.",
            )
            .await;
        }
        self.w.cafe.active_order = Some(order);
    }

    async fn check_stock(&mut self, staff: &str) {
        let oat = self.w.stock.get("oat_milk").copied().unwrap_or(0);
        if oat > 1 || self.w.restock.is_some() {
            return;
        }
        let ctx = format!(
            "restock-d{}-{}",
            day_of(self.now()),
            clock(self.now()).replace(':', "")
        );
        self.w.restock = Some(ctx.clone());
        let what = if oat == 0 { "out" } else { "almost out" };
        let eid = self
            .event(
                staff,
                "stock.low",
                "cafe",
                &format!("Oat milk {what} ({oat} left)"),
            )
            .await;
        self.memory(
            staff,
            "contextual",
            "cafe-staff",
            "cafe",
            "stock.oat_low",
            &ctx,
            -1,
            ("event", eid),
            &format!("Oat milk is {what} — restock needed."),
        )
        .await;
        if staff == "bo" {
            self.message(
                "bo",
                "cafe-staff",
                "",
                "restock.request",
                &ctx,
                &format!("Oat milk is {what}. Can we restock today?"),
            )
            .await;
        } else {
            self.plan_restock().await;
        }
    }

    async fn plan_restock(&mut self) {
        let now = self.now();
        let md = self.md();
        let at = if md < 840 { now - md + 840 } else { now + 10 };
        let ctx = self.w.restock.clone().unwrap_or_default();
        let ops = self
            .recall(
                "maya",
                "tag = 'cafe.ops' AND owner = 'maya'",
                "recall supplier-run habit",
            )
            .await;
        let mut rec = vec![format!("ctx:{ctx}")];
        if let Some(r) = ops.first() {
            rec.push(format!("mem:{}", i(r, "id")));
        }
        let when = clock(at);
        self.decide(
            "maya",
            "maya.schedule_restock v1 (deterministic)",
            &rec,
            &format!("Schedule a supplier run at {when}"),
            "Restock request received; stored habit says runs fit after the lunch rush.",
        )
        .await;
        self.memory(
            "maya",
            "contextual",
            "cafe-staff",
            "grocery",
            "stock.restock_plan",
            &ctx,
            -1,
            (
                "decision",
                self.w.agents[self.ai("maya")].last_decision.unwrap_or(0),
            ),
            &format!("Buy oat milk at FreshMart at {when}."),
        )
        .await;
        self.plan(at, "maya", "maya:restock");
    }

    async fn observe_trail(&mut self, who: &str) {
        let now = self.now();
        let wet = self.w.rain_until > 0 || self.w.last_rain_end >= now - 180;
        let tag = if wet { "trail.muddy" } else { "trail.clear" };
        let recent = self
            .recall(
                who,
                &format!(
                    "tag = {} AND location = 'trail' AND sim_minute >= {}",
                    q(tag),
                    now - 240
                ),
                "check if trail already reported",
            )
            .await;
        let eid = self
            .event(
                who,
                "observe",
                "trail",
                if wet {
                    "Trail muddy after rain"
                } else {
                    "Trail dry"
                },
            )
            .await;
        if !recent.is_empty() {
            return;
        }
        if wet {
            self.memory(
                who,
                "spatial",
                "neighborhood",
                "trail",
                tag,
                "",
                -1,
                ("event", eid),
                "The reservoir trail is muddy and slippery after the rain.",
            )
            .await;
            self.message(
                who,
                "neighborhood",
                "",
                "notice.trail",
                "",
                "Heads-up: the reservoir trail is muddy after the rain.",
            )
            .await;
        } else if who == "ama" {
            self.memory(
                who,
                "spatial",
                "neighborhood",
                "trail",
                tag,
                "",
                -1,
                ("event", eid),
                "The reservoir trail is dry and pleasant today.",
            )
            .await;
        }
    }

    async fn on_arrive(&mut self, id: &str, loc: &str) {
        if self.life_busy(id) || !(DAY_START..1290).contains(&self.md()) {
            return;
        }
        let md = self.md();
        let now = self.now();
        match (id, loc) {
            ("sam", "cafe") if md < 600 => self.counter_order("sam", "flat white", "milk").await,
            ("sam", "cafe") => self.lunch_seat().await,
            ("lin", "cafe") if md < 600 => self.counter_order("lin", "oat latte", "oat_milk").await,
            ("lin", "cafe") => {
                self.set_activity("lin", "Studying with headphones on");
                let until = now - md + 1020;
                self.memory(
                    "lin",
                    "temporary",
                    "cafe-regulars",
                    "cafe",
                    "status.dnd",
                    "",
                    until,
                    ("event", 0),
                    "Studying for my exam — please don't interrupt until 17:00.",
                )
                .await;
            }
            ("sam", "office") => {
                self.set_activity("sam", "Working on the quarterly report");
            }
            ("sam", "grocery") => {
                self.set_activity("sam", "Buying eggs at FreshMart");
                let ctx = self.w.errand.clone().unwrap_or_default();
                self.object_transition(
                    "sam",
                    "eggs",
                    "freshmart:shelf",
                    "sam:grocery_bag",
                    "grocery",
                    &ctx,
                    "Sam completed the egg purchase.",
                )
                .await;
                self.w.stock.insert("eggs".into(), 12);
                let ctx = self.w.errand.take().unwrap_or_default();
                self.message(
                    "sam",
                    "family",
                    "",
                    "errand.done",
                    &ctx,
                    "Got the eggs on my way home.",
                )
                .await;
                let k = self.ai("sam");
                self.w.agents[k].carrying = Some("grocery_bag".into());
                self.object_transition(
                    "sam",
                    "eggs",
                    "sam:grocery_bag",
                    "home:pantry",
                    "grocery",
                    &ctx,
                    "The purchased eggs are travelling home with Sam.",
                )
                .await;
                self.plan(now + 12, "sam", "go:home");
            }
            ("sam", "home") if md > 1000 => self.set_activity("sam", "Dinner with Ama and Mochi"),
            ("ama", "park") => self.set_activity("ama", "Walking Mochi around the fountain"),
            ("ama", "home") => {
                self.set_activity(
                    "ama",
                    if md > 1000 {
                        "Cooking dinner"
                    } else {
                        "Tidying up at home"
                    },
                );
                if self.w.stock.get("eggs").copied().unwrap_or(0) == 0 {
                    self.check_eggs().await;
                }
                let k = self.ai("ama");
                self.w.agents[k].carrying = None;
            }
            ("ama", "grocery") => {
                self.set_activity("ama", "Shopping at FreshMart");
                if self.flag("ama", "buy_eggs") == 1 {
                    self.set_flag("ama", "buy_eggs", 0);
                    let ctx = self.w.errand.clone().unwrap_or_default();
                    self.object_transition(
                        "ama",
                        "eggs",
                        "freshmart:shelf",
                        "ama:grocery_bag",
                        "grocery",
                        &ctx,
                        "Ama completed the egg purchase.",
                    )
                    .await;
                    self.w.stock.insert("eggs".into(), 12);
                    let ctx = self.w.errand.take().unwrap_or_default();
                    self.message(
                        "ama",
                        "family",
                        "",
                        "errand.done",
                        &ctx,
                        "I picked up the eggs myself.",
                    )
                    .await;
                }
                let k = self.ai("ama");
                self.w.agents[k].carrying = Some("grocery_bag".into());
                self.plan(now + 20, "ama", "go:home");
            }
            ("ama", "trail") => {
                self.set_activity("ama", "Hiking the reservoir trail with Mochi");
                self.observe_trail("ama").await;
                self.plan(now + 45, "ama", "go:home");
            }
            ("maya", "grocery") => {
                self.set_activity("maya", "Buying oat milk from the supplier aisle");
                let ctx = self.w.restock.clone().unwrap_or_default();
                self.object_transition(
                    "maya",
                    "oat_milk",
                    "freshmart:shelf",
                    "maya:carrying",
                    "grocery",
                    &ctx,
                    "Maya completed the oat-milk supplier purchase.",
                )
                .await;
                self.w.stock.insert("oat_milk".into(), 12);
                let ctx = self.w.restock.take().unwrap_or_default();
                self.message(
                    "maya",
                    "cafe-staff",
                    "",
                    "restock.done",
                    &ctx,
                    "Restocked: 12 cartons of oat milk.",
                )
                .await;
                let k = self.ai("maya");
                self.w.agents[k].carrying = Some("milk".into());
                self.object_transition(
                    "maya",
                    "oat_milk",
                    "maya:carrying",
                    "cafe:fridge",
                    "grocery",
                    &ctx,
                    "The oat milk is travelling back to the café fridge.",
                )
                .await;
                self.plan(now + 15, "maya", "go:cafe");
            }
            ("maya", "cafe") => {
                self.set_activity("maya", "Running the counter");
                let k = self.ai("maya");
                self.w.agents[k].carrying = None;
            }
            ("bo", "cafe") => self.set_activity("bo", "Pulling espresso shots"),
            ("bo", "trail") => {
                self.set_activity("bo", "Walking home via the reservoir trail");
                self.observe_trail("bo").await;
                self.plan(now + 8, "bo", "go:away");
            }
            ("lin", "school") => {
                self.set_activity("lin", "Lecture: Intro to Databases");
                if !self.w.done.contains(&format!("lin-note-d{}", day_of(now))) && day_of(now) == 1
                {
                    self.w.done.insert(format!("lin-note-d{}", day_of(now)));
                    self.memory(
                        "lin",
                        "permanent",
                        "private",
                        "school",
                        "private.note",
                        "",
                        -1,
                        ("event", 0),
                        "Exam on Thursday — I'm a bit stressed about it.",
                    )
                    .await;
                }
            }
            ("lin", "gym") => self.set_activity("lin", "Workout at the gym"),
            ("lin", "bus_stop") => {
                self.set_activity("lin", "Waiting for the bus");
            }
            (_, "away") => self.set_activity(id, "Out of town"),
            _ => {}
        }
    }

    async fn on_bus_alight(&mut self, id: &str, sched: i64, delay: i64) {
        if id != "lin" {
            return;
        }
        let smd = sched.rem_euclid(DAY);
        if delay >= 10 {
            let eid = self
                .event(
                    "lin",
                    "bus.late",
                    "bus_stop",
                    &format!("{} bus {delay} min late", clock(sched)),
                )
                .await;
            let tag = format!("bus.late.{:02}{:02}", smd / 60, smd % 60);
            let mid = self
                .memory(
                    "lin",
                    "temporal",
                    "private",
                    "bus_stop",
                    &tag,
                    "",
                    -1,
                    ("event", eid),
                    &format!(
                        "The {} bus ran {delay} min late. Take an earlier one next time.",
                        clock(sched)
                    ),
                )
                .await;
            let refs = vec![format!("event:{eid}"), format!("mem:{mid}")];
            self.lesson(
                &format!("lesson:lin:commute.bus:candidate:{eid}"),
                "lin",
                "commute.bus",
                "a named bus arrived at least 10 minutes late",
                "When the 08:00 bus is repeatedly late, leave on the earlier bus.",
                "candidate",
                60,
                &refs,
                "lin.bus_choice.v2",
            )
            .await;
            self.promote_lesson_if_repeated(
                "lin",
                "commute.bus",
                "lesson:lin:commute.bus:accepted:v1",
                "repeated late-bus observations",
                "When the 08:00 bus is repeatedly late, leave on the earlier bus.",
                &refs,
                "lin.bus_choice.v2",
            )
            .await;
            self.message(
                "lin",
                "neighborhood",
                "",
                "notice.bus",
                "",
                &format!(
                    "Heads-up: the {} bus is running ~{delay} min late.",
                    clock(sched)
                ),
            )
            .await;
        }
        self.go("lin", "cafe");
        let k = self.ai("lin");
        if let Some(Travel { arrive, .. }) = self.w.agents[k].travel.as_mut() {
            *arrive -= 4; // bus stop → café is a short walk
        }
    }

    async fn lunch_seat(&mut self) {
        if self.at("maya") != "cafe" {
            self.say("sam", "I'll grab any free seat.").await;
            return;
        }
        let rows = self
            .recall(
                "maya",
                "tag = 'table.hold' AND context_id = 'sam'",
                "recall table holds for Sam",
            )
            .await;
        if let Some(r) = rows.first() {
            let mid = i(r, "id");
            self.decide(
                "maya",
                "maya.seat_hold v1 (deterministic)",
                &[format!("mem:{mid}")],
                "Seat Sam at the held table",
                &format!(
                    "Temporary memory #{mid} is still valid (expires {}).",
                    clock(i(r, "expires_at"))
                ),
            )
            .await;
            self.say("maya", "Table 2's ready for you, Sam!").await;
            self.set_activity("sam", "Lunch at the held table");
        } else {
            self.decide(
                "maya",
                "maya.seat_hold v1 (deterministic)",
                &[],
                "No hold found — offer any free seat",
                "No unexpired table.hold memory for Sam.",
            )
            .await;
            self.say("maya", "Grab any free seat, Sam!").await;
            self.set_activity("sam", "Lunch at the counter");
        }
    }

    pub async fn check_eggs(&mut self) {
        if self.w.errand.is_some() || self.at("ama") != "home" {
            return;
        }
        let ctx = format!(
            "errand-eggs-d{}-{}",
            day_of(self.now()),
            clock(self.now()).replace(':', "")
        );
        self.w.errand = Some(ctx.clone());
        self.set_flag("ama", "buy_eggs", 0);
        let eid = self
            .event("ama", "observe", "home", "Fridge has no eggs")
            .await;
        self.memory(
            "ama",
            "contextual",
            "family",
            "home",
            "errand.eggs",
            &ctx,
            -1,
            ("event", eid),
            "We're out of eggs at home.",
        )
        .await;
        self.message(
            "ama",
            "family",
            "",
            "errand.request",
            &ctx,
            "We're out of eggs — could someone pick some up today?",
        )
        .await;
    }

    // ---------- routines (deterministic schedules + memory-driven rules) ----------

    async fn ama(&mut self) {
        let md = self.md();
        let now = self.now();
        match md {
            400 => self.check_eggs().await,
            420 if self.at("ama") == "home" => {
                self.go("ama", "park");
                self.set_activity("ama", "Morning walk with Mochi");
            }
            460 if self.at("ama") == "park" => self.go("ama", "home"),
            570 if self.at("ama") == "home" => {
                if let Some(ctx) = self.w.errand.clone() {
                    let sql = format!(
                        "SELECT id, sender, body FROM lv_messages WHERE channel = 'family' AND context_id = {} AND topic IN ('errand.accept', 'errand.done') ORDER BY id",
                        q(&ctx)
                    );
                    let rows = self
                        .sql("ama", "check whether errand was taken", &sql)
                        .await;
                    if let Some(r) = rows.first() {
                        let who = self.name(&s(r, "sender"));
                        self.decide("ama", "ama.grocery_list v1 (deterministic)", &[format!("msg:{}", i(r, "id")), format!("ctx:{ctx}")], "Shop without eggs", &format!("{who} already took the egg errand (message #{}). Buying eggs too would duplicate work.", i(r, "id"))).await;
                        self.set_flag("ama", "buy_eggs", 0);
                    } else {
                        self.decide(
                            "ama",
                            "ama.grocery_list v1 (deterministic)",
                            &[format!("ctx:{ctx}")],
                            "Add eggs to my list",
                            "Nobody accepted the egg errand in #family yet.",
                        )
                        .await;
                        self.set_flag("ama", "buy_eggs", 1);
                    }
                }
                self.go("ama", "grocery");
            }
            990 if self.at("ama") == "home" => {
                let rows = self
                    .recall(
                        "ama",
                        &format!(
                            "location = 'trail' AND kind = 'spatial' AND sim_minute >= {}",
                            now - 720
                        ),
                        "recall recent trail conditions",
                    )
                    .await;
                let muddy = rows.iter().find(|r| s(r, "tag") == "trail.muddy");
                if let Some(r) = muddy {
                    let (mid, owner) = (i(r, "id"), self.name(&s(r, "owner")));
                    self.decide("ama", "ama.walk_route v1 (deterministic)", &[format!("mem:{mid}")], "Walk Mochi in the park instead of hiking", &format!("{owner} recorded at {} that the trail is muddy (memory #{mid}, within the 12-hour relevance window).", clock(i(r, "sim_minute")))).await;
                    self.say("ama", "Trail's muddy — park it is, Mochi.").await;
                    self.go("ama", "park");
                    self.plan(now + 50, "ama", "go:home");
                } else {
                    let rec: Vec<String> =
                        rows.iter().map(|r| format!("mem:{}", i(r, "id"))).collect();
                    self.decide(
                        "ama",
                        "ama.walk_route v1 (deterministic)",
                        &rec,
                        "Hike the reservoir trail",
                        "No muddy-trail memory visible within the last 12 hours.",
                    )
                    .await;
                    self.go("ama", "trail");
                }
            }
            _ => {}
        }
    }

    async fn sam(&mut self) {
        let md = self.md();
        let now = self.now();
        match md {
            465 if self.at("sam") == "home" => self.go("sam", "cafe"),
            490 if self.at("sam") == "cafe" => {
                let k = self.ai("sam");
                self.w.agents[k].carrying = None;
                self.go("sam", "office");
            }
            510 | 600 | 690 | 900 | 1020 => {
                let msgs = self.inbox("sam").await;
                for r in msgs {
                    if s(&r, "topic") == "errand.request"
                        && self.w.errand.as_deref() == Some(s(&r, "context_id").as_str())
                        && self.flag("sam", "errand_taken") == 0
                    {
                        let ctx = s(&r, "context_id");
                        let mid = i(&r, "id");
                        self.decide(
                            "sam",
                            "sam.take_errand v1 (deterministic)",
                            &[format!("msg:{mid}")],
                            "Take the egg errand after work",
                            "FreshMart is on my way home and Mum walks Mochi in the afternoon.",
                        )
                        .await;
                        self.message(
                            "sam",
                            "family",
                            "",
                            "errand.accept",
                            &ctx,
                            "I'll grab eggs on my way home tonight.",
                        )
                        .await;
                        self.memory(
                            "sam",
                            "contextual",
                            "family",
                            "grocery",
                            "errand.eggs",
                            &ctx,
                            -1,
                            ("message", mid),
                            "Pick up eggs at FreshMart after work.",
                        )
                        .await;
                        self.set_flag("sam", "errand_taken", 1);
                    }
                }
                if md == 690 && self.at("sam") == "office" {
                    self.message(
                        "sam",
                        "direct",
                        "maya",
                        "table.request",
                        "sam",
                        "Could you hold a table for me at 12:30?",
                    )
                    .await;
                }
            }
            570 if self.at("sam") == "office" => {
                self.set_activity("sam", "Stand-up with the team");
                self.say("sam", "Report draft is on track for Friday.")
                    .await;
            }
            585 if self.at("sam") == "office" => {
                self.set_activity("sam", "Focus work: quarterly report")
            }
            905 if self.at("sam") == "office" => {
                self.set_activity("sam", "Coffee-machine chat with Priya");
                self.say("sam", "One more section and the report's done.")
                    .await;
            }
            920 if self.at("sam") == "office" => {
                self.set_activity("sam", "Focus work: quarterly report")
            }
            750 if self.at("sam") == "office" => self.go("sam", "cafe"),
            795 if self.at("sam") == "cafe" => self.go("sam", "office"),
            1080 if self.at("sam") == "office" => {
                let rows = self
                    .recall(
                        "sam",
                        "owner = 'sam' AND tag = 'errand.eggs' AND kind = 'contextual'",
                        "recall open errands",
                    )
                    .await;
                let open = self.w.errand.clone();
                let hit = rows.iter().find(|r| Some(s(r, "context_id")) == open);
                if let Some(r) = hit {
                    let mid = i(r, "id");
                    self.decide(
                        "sam",
                        "sam.commute_home v1 (deterministic)",
                        &[format!("mem:{mid}")],
                        "Detour to FreshMart for eggs",
                        &format!("Contextual memory #{mid} still open: “{}”", s(r, "content")),
                    )
                    .await;
                    self.go("sam", "grocery");
                } else {
                    self.go("sam", "home");
                }
                self.set_flag("sam", "errand_taken", 0);
            }
            _ => {}
        }
        let _ = now;
    }

    async fn lin(&mut self) {
        let md = self.md();
        let now = self.now();
        match md {
            420 if self.at("lin") == "away" => {
                let learned = self
                    .sql(
                        "lin",
                        "recall accepted commute lesson",
                        "SELECT lesson_id, statement FROM lv_lessons WHERE agent = 'lin' AND topic = 'commute.bus' AND state = 'accepted' AND NOT EXISTS (SELECT challenge_id FROM lv_lesson_challenges WHERE lesson_id = lv_lessons.lesson_id AND resolution IN ('repaired', 'superseded')) ORDER BY sim_minute DESC LIMIT 1",
                    )
                    .await;
                let rows = self
                    .recall(
                        "lin",
                        &format!(
                            "owner = 'lin' AND tag = 'bus.late.0800' AND sim_minute >= {}",
                            now - 7 * DAY
                        ),
                        "recall bus punctuality",
                    )
                    .await;
                let base = now - md;
                if let Some(lesson) = learned.first() {
                    let lesson_id = s(lesson, "lesson_id");
                    self.decide(
                        "lin",
                        "lin.bus_choice v2 (accepted lesson)",
                        &[format!("lesson:{lesson_id}")],
                        "Take the 07:40 bus instead of 08:00",
                        &format!("Accepted lesson {lesson_id}: {}", s(lesson, "statement")),
                    )
                    .await;
                    self.plan(base + 460, "lin", "ride:in");
                } else if let Some(r) = rows.first() {
                    let mid = i(r, "id");
                    self.decide(
                        "lin",
                        "lin.bus_choice v1 (deterministic)",
                        &[format!("mem:{mid}")],
                        "Take the 07:40 bus instead of 08:00",
                        &format!(
                            "Temporal memory #{mid} from Day {}: “{}”",
                            day_of(i(r, "sim_minute")),
                            s(r, "content")
                        ),
                    )
                    .await;
                    self.plan(base + 460, "lin", "ride:in");
                } else {
                    self.decide(
                        "lin",
                        "lin.bus_choice v1 (deterministic)",
                        &[],
                        "Take the usual 08:00 bus",
                        "No stored lateness for the 08:00 bus in the last 7 days.",
                    )
                    .await;
                    self.plan(base + 480, "lin", "ride:in");
                }
                self.set_activity("lin", "Walking to the dorm bus stop");
            }
            600 if self.at("lin") == "cafe" => {
                let k = self.ai("lin");
                self.w.agents[k].carrying = Some("backpack".into());
                self.w.agents[k]
                    .notes
                    .insert("bus_to".into(), "school".into());
                self.travel("lin", "bus_stop", 6, "walk");
                self.set_flag("lin", "await_bus", 1);
            }
            900 if self.at("lin") == "school" => {
                let base = now - md;
                self.plan(base + 920, "lin", "ride:back");
                self.set_activity("lin", "Riding the bus back");
            }
            1050 if self.at("lin") == "cafe" => self.go("lin", "gym"),
            1140 if self.at("lin") == "gym" => self.go("lin", "away"),
            _ => {}
        }
    }

    async fn maya(&mut self) {
        let md = self.md();
        if md % 15 == 0 && self.at("maya") == "cafe" {
            let msgs = self.inbox("maya").await;
            for r in msgs {
                match s(&r, "topic").as_str() {
                    "restock.request"
                        if self.w.restock.as_deref() == Some(s(&r, "context_id").as_str())
                            && self.flag("maya", "restock_planned") == 0 =>
                    {
                        self.set_flag("maya", "restock_planned", 1);
                        self.plan_restock().await;
                        let when = self
                            .w
                            .plans
                            .iter()
                            .find(|p| p.action == "maya:restock")
                            .map(|p| clock(p.at))
                            .unwrap_or_default();
                        self.message(
                            "maya",
                            "cafe-staff",
                            "",
                            "restock.ack",
                            &s(&r, "context_id"),
                            &format!(
                                "Thanks Bo — I'll get oat milk at {when}, after the lunch rush."
                            ),
                        )
                        .await;
                    }
                    "table.request" => {
                        let base = self.now() - md;
                        let mid = i(&r, "id");
                        self.memory(
                            "maya",
                            "temporary",
                            "cafe-staff",
                            "cafe",
                            "table.hold",
                            "sam",
                            base + 765,
                            ("message", mid),
                            "Table 2 is held for Sam until 12:45.",
                        )
                        .await;
                        self.message(
                            "maya",
                            "direct",
                            "sam",
                            "table.ack",
                            "sam",
                            "Table 2 is yours until 12:45.",
                        )
                        .await;
                    }
                    _ => {}
                }
            }
        }
        if self.w.restock.is_none() {
            self.set_flag("maya", "restock_planned", 0);
        }
        match md {
            420 if self.at("maya") == "cafe" => self.set_activity("maya", "Running the counter"),
            1200 if self.at("maya") == "cafe" => self.set_activity("maya", "Closing up the café"),
            _ => {}
        }
    }

    async fn bo(&mut self) {
        let md = self.md();
        match md {
            420 if self.at("bo") == "away" => self.go("bo", "cafe"),
            450 | 780 if self.at("bo") == "cafe" => {
                let oat = self.w.stock.get("oat_milk").copied().unwrap_or(0);
                if oat > 0 {
                    self.w.stock.insert("oat_milk".into(), oat - 1);
                    self.event(
                        "bo",
                        "serve",
                        "cafe",
                        "Served a walk-in regular an oat flat white",
                    )
                    .await;
                }
                self.check_stock("bo").await;
            }
            900 if self.at("bo") == "cafe" => self.go("bo", "trail"),
            _ => {}
        }
    }

    // ---------- external nudges (player-injected world events) ----------

    pub async fn nudge(&mut self, kind: &str) -> Result<String, String> {
        let msg = match kind {
            "rain" => {
                self.start_rain(45, "Observer nudge: a passing shower")
                    .await;
                "Rain for 45 sim-minutes".to_string()
            }
            "bus_delay" => {
                let now = self.now();
                let next = next_bus_start(now, 40).unwrap_or(now);
                *self.w.bus_delays.entry(next).or_insert(0) += 15;
                self.event(
                    "world",
                    "bus.delay",
                    "bus_stop",
                    &format!("Observer nudge: {} bus delayed 15 min", clock(next)),
                )
                .await;
                self.feed(
                    "world",
                    "world",
                    &format!("The {} bus will be 15 min late", clock(next)),
                    "Riders only learn this when they experience it or someone posts a notice.",
                    vec![],
                    "",
                );
                format!("{} bus delayed", clock(next))
            }
            "oat_out" => {
                self.w.stock.insert("oat_milk".into(), 0);
                self.feed(
                    "world",
                    "world",
                    "The café fridge runs out of oat milk",
                    "Observer nudge. Staff discover it on their next stock check.",
                    vec![],
                    "",
                );
                if let Some(staff) = self.staff_on_duty() {
                    self.check_stock(staff).await;
                }
                "Oat milk emptied".into()
            }
            "eggs_out" => {
                self.w.stock.insert("eggs".into(), 0);
                self.feed(
                    "world",
                    "world",
                    "Someone used the last eggs",
                    "Observer nudge. Ama notices when she is at home.",
                    vec![],
                    "",
                );
                self.check_eggs().await;
                "Eggs emptied".into()
            }
            "revoke_consent" => {
                let rows = self.sql("lin", "find my shared preference", "SELECT id FROM lv_memories WHERE owner = 'lin' AND scope = 'cafe-regulars'").await;
                if rows.is_empty() {
                    return Err("Lin has not shared anything with the café yet".into());
                }
                let ids: Vec<String> = rows.iter().map(|r| i(r, "id").to_string()).collect();
                self.decide("lin", "lin.revoke_preference v1 (observer-triggered)", &ids.iter().map(|x| format!("mem:{x}")).collect::<Vec<_>>(), "Ask the café to forget my usual", "Observer nudge: Lin withdraws consent. Rows are deleted from LessDB, not just hidden.").await;
                for id in &ids {
                    if let Ok(memory_id) = id.parse::<i64>() {
                        self.memory_lifecycle(memory_id, "deleted", "consent revoked", "lin")
                            .await;
                    }
                }
                self.sql(
                    "lin",
                    "DELETE shared preference (forget on request)",
                    "DELETE FROM lv_memories WHERE owner = 'lin' AND scope = 'cafe-regulars'",
                )
                .await;
                self.set_flag("lin", "consented", 0);
                self.set_flag("lin", "asked", 0);
                self.relationship(
                    "rel:lin:maya:cafe:v2",
                    "lin",
                    "maya",
                    "cafe-regular",
                    "revoked",
                    "shared preference withdrawn; café should not recall the deleted usual",
                    "cafe-regulars",
                    &ids.iter().map(|id| format!("mem:{id}")).collect::<Vec<_>>(),
                    "revoked",
                    100,
                    "relationship.v1",
                )
                .await;
                self.feed(
                    "memory",
                    "lin",
                    "Lin's shared café memories were deleted",
                    &format!(
                        "DELETE removed {} row(s): staff can no longer recall her usual.",
                        ids.len()
                    ),
                    ids.iter().map(|x| format!("mem:{x}")).collect(),
                    "cafe-regulars",
                );
                format!("Deleted {} shared row(s)", ids.len())
            }
            _ => return Err(format!("unknown nudge '{kind}'")),
        };
        self.persist().await;
        Ok(msg)
    }

    pub fn snapshot(&self, sql_log: Vec<crate::store::SqlLog>, statements: u64) -> Value {
        let now = self.now();
        let md = self.md();
        // Keep overdue services visible within the same lookback used by buses().
        let next_bus = (now - 60..=now + 60)
            .filter(|m| next_bus_start(*m, 0).is_some())
            .map(|m| bus_trip(m, self.w.bus_delays.get(&m).copied().unwrap_or(0), now))
            .filter(|trip| trip.effective >= now)
            .min_by_key(|trip| (trip.effective, trip.scheduled))
            .map(|trip| {
                json!({
                    "id": trip.id,
                    "route": trip.route,
                    "direction": trip.direction,
                    "scheduled": trip.scheduled,
                    "effective": trip.effective,
                    "delay": trip.effective - trip.scheduled,
                    "phase": trip.phase,
                })
            });
        let active_bus = self.w.bus_arrived.and_then(|(scheduled, at)| {
            let age = now - at;
            if (0..=4).contains(&age) {
                let delay = self.w.bus_delays.get(&scheduled).copied().unwrap_or(0);
                let trip = bus_trip(scheduled, delay, at);
                Some(json!({
                    "id": trip.id,
                    "route": trip.route,
                    "direction": trip.direction,
                    "scheduled": scheduled,
                    "effective": at,
                    "at": at,
                    "phase": if age <= 2 { "dwell" } else { "depart" },
                }))
            } else {
                None
            }
        });
        json!({
            "generation": self.w.generation,
            "population": {"total": self.w.agents.len() + self.w.background.len(), "main": self.w.agents.len(), "supporting": self.w.background.len(),
                "town_center": self.w.agents.len() + self.w.background.iter().filter(|b| b.in_town_center()).count(),
                "surrounding": self.w.background.iter().filter(|b| !b.in_town_center()).count()},
            "clock": {
                "minute": now, "day": day_of(now), "minute_of_day": md, "label": clock(now),
                "tz": "Asia/Singapore (UTC+8)", "paused": self.w.paused, "speed": self.w.speed,
                "run_until": self.w.stop_at,
                "raining": self.w.rain_until > now, "rain_reason": self.w.rain_reason,
                "weather": self.w.weather_kind, "temperature_c": self.w.temperature_c,
                "wind_kmh": self.w.wind_kmh, "weather_code": self.w.weather_code,
                "weather_source": self.w.weather_source, "weather_observed_at": self.w.weather_observed_at,
                "weather_changed_at": self.w.weather_changed_at,
                "day_start": DAY_START, "day_end": DAY_END,
            },
            "agents": self.w.agents.iter().map(|a| json!({
                "id": a.id, "name": a.name, "role": a.role, "title": a.title,
                "occupation": a.occupation, "behaviour": a.behaviour, "mood": a.mood,
                "location": a.location, "place": place_name(&a.location), "activity": a.activity,
                "activity_kind": a.activity_kind, "station": a.station, "pose": a.pose,
                "travel": a.travel, "bubble": a.bubble, "carrying": a.carrying,
                "presence": a.presence, "life": a.life,
                "last_decision": a.last_decision,
            })).collect::<Vec<_>>(),
            "background": self.w.background.iter().map(|b| json!({
                "id": b.id, "name": b.name, "occupation": b.occupation,
                "district": if b.in_town_center() {"town_center"} else {"surrounding_neighborhoods"},
                "location": b.location, "target": b.target, "activity": b.activity,
                "mood": b.mood, "next_change": b.next_change, "life": b.life,
                "travel_until": b.travel_until, "food_portions": b.food_portions, "memories": b.memories,
                "visible": b.visible_in_town(now),
            })).collect::<Vec<_>>(),
            "traffic": self.w.traffic.iter().map(|v| json!({
                "id": v.id, "kind": v.kind, "route": v.route, "lane": v.lane,
                "position": v.position, "direction": v.direction, "speed_kmh": v.speed_kmh,
                "phase": v.phase,
            })).collect::<Vec<_>>(),
            "traffic_lights": self.w.traffic_lights.iter().map(|light| json!({
                "id": light.id, "intersection": light.intersection, "phase": light.phase,
                "elapsed": light.elapsed, "cycle_minutes": light.cycle_minutes,
            })).collect::<Vec<_>>(),
            "occupation_changes": self.w.occupation_changes,
            "stock": self.w.stock,
            "cafe": self.w.cafe,
            "bus": {"trip_id": self.w.bus_trip, "arrived": self.w.bus_arrived.map(|(s, a)| json!({"id": bus_trip(s, self.w.bus_delays.get(&s).copied().unwrap_or(0), a).id, "scheduled": s, "at": a})), "active": active_bus, "next": next_bus},
            "open": {"errand": self.w.errand, "restock": self.w.restock},
            "plans": self.w.plans.iter().map(|p| json!({"at": p.at, "label": clock(p.at), "agent": p.agent, "action": p.action})).collect::<Vec<_>>(),
            "presence_policy": {
                "schema": crate::presence::SCHEMA,
                "policy": crate::presence::POLICY,
                "boundary": "Deterministic rules over clock, place, travel, messages, carried items, weather and co-location. Not model reasoning or awareness; props are drawn only from this state.",
            },
            "research_frontier": {
                "novelty": self.w.novelty.iter().rev().take(12).collect::<Vec<_>>(),
                "signals": self.w.frontier.iter().rev().take(12).collect::<Vec<_>>(),
                "proposals": self.w.evolution_proposals.iter().rev().take(12).collect::<Vec<_>>(),
                "boundary": "Seeded bounded variation. Frontier signals and evolution candidates are application-owned research evidence; proposals remain inactive and never widen scope or actuate anything.",
            },
            "feed": self.w.feed.iter().rev().take(60).collect::<Vec<_>>(),
            "sql": sql_log,
            "statements": statements,
            "resumed": self.w.resumed,
        })
    }
}

/// Bounded render vocabulary. Human-readable activity remains explanatory copy,
/// but the client consumes these stable fields instead of parsing prose.
fn activity_fields(id: &str, loc: &str, act: &str) -> (&'static str, &'static str, &'static str) {
    let a = act.to_lowercase();
    if a == "asleep" {
        return (
            "sleeping",
            if loc == "cafe" { "upstairs" } else { "bed" },
            "sleep",
        );
    }
    if a == "eating a meal" {
        return (
            "eating",
            if loc == "cafe" { "table3" } else { "dining" },
            "seat",
        );
    }
    if a.starts_with("waiting for") {
        return (
            "waiting",
            if loc == "cafe" { "queue" } else { "living" },
            "idle",
        );
    }
    match loc {
        "office" if a.contains("stand-up") => ("meeting", "meeting", "talk"),
        "office" if a.contains("coffee") => ("meeting", "coffee", "talk"),
        "office" => ("typing", "desk", "type"),
        "cafe" if a.contains("opening") => ("opening", "till", "idle"),
        "cafe" if a.contains("closing") => ("closing", "till", "idle"),
        "cafe" if id == "bo" && (a.contains("stock") || a.contains("fridge")) => {
            ("checking_stock", "fridge", "idle")
        }
        "cafe" if id == "bo" && a.contains("espresso") => ("serving", "espresso", "cook"),
        "cafe" if id == "maya" => ("serving", "till", "talk"),
        "cafe" if a.contains("held table") => ("meeting", "table2", "seat"),
        "cafe" if a.contains("studying") => ("studying", "table1", "seat"),
        "cafe" if a.contains("lunch") => ("cooking", "table3", "seat"),
        "cafe" => ("serving", "queue", "idle"),
        "home" if a.contains("cooking") || a.contains("breakfast") => {
            ("cooking", "kitchen", "cook")
        }
        "home" if a.contains("dinner") => ("cooking", "dining", "cook"),
        "home" => ("resting", "living", "idle"),
        "grocery" if a.contains("buying") || a.contains("shopping") => {
            ("shopping", "aisle", "carry")
        }
        "grocery" => ("shopping", "aisle", "idle"),
        "school" => ("studying", "lecture", "seat"),
        "gym" => ("workout", "workout", "workout"),
        "trail" => ("hiking", "path", "walk_side"),
        "park" => ("walking", "fountain", "walk_side"),
        "bus_stop" => ("commuting", "shelter", "idle"),
        "transit" => ("commuting", "street", "walk_side"),
        "away" => ("resting", "offmap", "idle"),
        _ => ("idle", "", "idle"),
    }
}

#[cfg(test)]
mod world_tests {
    use std::collections::BTreeSet;

    use super::{World, activity_fields, bus_trip, next_bus_start, novelty_choice, seeded_roll};

    #[test]
    fn timetable_has_named_delayed_trip() {
        let scheduled = next_bus_start(479, 60).expect("next trip");
        let trip = bus_trip(scheduled, 15, 480);
        assert_eq!(scheduled, 480);
        assert_eq!(trip.id, "day1-0480-inbound");
        assert_eq!(trip.effective, 495);
        assert_eq!(trip.phase, "scheduled");
    }

    #[tokio::test]
    async fn timetable_snapshot_keeps_pending_delayed_arrivals() {
        let (store, world, dir) = crate::scenarios::fixture().await;
        let mut sim = super::Sim {
            store,
            w: world,
            llm: None,
        };
        for (now, delayed_start, delay, scheduled, effective) in [
            (481, 480, 15, 480, 495),
            (495, 480, 15, 480, 495),
            (496, 480, 15, 500, 500),
            (481, 480, 35, 500, 500),
            (1261, 1260, 15, 1260, 1275),
        ] {
            sim.w.minute = now;
            sim.w.bus_delays.clear();
            sim.w.bus_delays.insert(delayed_start, delay);
            let snapshot = sim.snapshot(vec![], 0);
            let next = &snapshot["bus"]["next"];
            assert_eq!(next["scheduled"], scheduled, "next trip at minute {now}");
            assert_eq!(next["effective"], effective);
            assert_eq!(next["delay"], effective - scheduled);
            assert_eq!(
                next["id"],
                bus_trip(scheduled, effective - scheduled, now).id
            );
        }
        sim.w.minute = 1300;
        assert!(sim.snapshot(vec![], 0)["bus"]["next"].is_null());
        drop(sim);
        std::fs::remove_dir_all(dir).expect("remove timetable test data");
    }

    #[test]
    fn activity_state_is_bounded_and_not_render_derived() {
        assert_eq!(
            activity_fields("bo", "cafe", "Pulling espresso shots"),
            ("serving", "espresso", "cook")
        );
        assert_eq!(
            activity_fields("sam", "office", "Focus work: quarterly report"),
            ("typing", "desk", "type")
        );
        let world = World::fresh(1);
        assert_eq!(world.cafe.lifecycle, "closed");
        assert!(world.cafe.tables.values().all(|state| state == "free"));
    }

    #[test]
    fn seeded_world_has_replayable_weather_and_background_population() {
        let world = World::fresh(1);
        assert_eq!(
            seeded_roll(world.world_seed, 1, 4),
            seeded_roll(world.world_seed, 1, 4)
        );
        assert_ne!(
            seeded_roll(world.world_seed, 1, 4),
            seeded_roll(world.world_seed, 2, 4)
        );
        assert_eq!(world.background.len(), super::SUPPORTING_POPULATION);
        assert!(
            world
                .background
                .iter()
                .all(|person| !person.occupation.is_empty())
        );
        assert!(world.background.iter().all(|character| character.seed != 0));
        assert_eq!(
            world
                .background
                .iter()
                .filter(|b| b.in_town_center())
                .count(),
            12
        );
        let mut regional = world.background[12].clone();
        regional.location = "park".into();
        regional.travel_until = 900;
        assert!(
            !regional.visible_in_town(800),
            "regional journeys do not cross the town viewport"
        );
        let mut local = world.background[0].clone();
        local.location = "office".into();
        assert!(!local.visible_in_town(800), "indoor residents stay indoors");
        local.travel_until = 900;
        assert!(local.visible_in_town(800));
    }

    #[test]
    fn novelty_is_replayable_bounded_and_seed_sensitive() {
        let first = World::fresh(1);
        assert_eq!(
            novelty_choice(first.world_seed, 1, 0, 5),
            novelty_choice(first.world_seed, 1, 0, 5)
        );
        let branches: BTreeSet<_> = (1..=8)
            .filter_map(|generation| novelty_choice(World::fresh(generation).world_seed, 1, 0, 5))
            .collect();
        assert!(branches.len() > 1);
        assert!(first.novelty.is_empty());
        assert!(first.frontier.is_empty());
        assert!(first.evolution_proposals.is_empty());
    }

    #[test]
    fn hydrate_repairs_older_agent_and_world_defaults() {
        let mut world = World::fresh(2);
        world.weather_kind.clear();
        world.temperature_c = 0;
        world.background.clear();
        world.world_seed = 0;
        world.agents[0].occupation.clear();
        world.agents[0].behaviour.clear();
        world.agents[0].mood.clear();
        world.hydrate();
        assert_eq!(world.weather_kind, "clear");
        assert_eq!(world.temperature_c, 29);
        assert_eq!(world.background.len(), super::SUPPORTING_POPULATION);
        assert_eq!(world.agents[0].occupation, "Owner");
        assert_eq!(world.agents[0].behaviour, "routine");
        assert_eq!(world.agents[0].mood, "neutral");
    }

    #[test]
    fn traffic_model_has_explicit_signal_and_lane_invariants() {
        let world = World::fresh(7);
        assert_eq!(world.traffic_lights.len(), 2);
        let light_ids: BTreeSet<_> = world
            .traffic_lights
            .iter()
            .map(|light| light.id.as_str())
            .collect();
        assert_eq!(light_ids.len(), world.traffic_lights.len());
        for light in &world.traffic_lights {
            assert!(light.cycle_minutes >= 3);
            assert!(matches!(light.phase.as_str(), "green" | "amber" | "red"));
        }
        for actor in &world.traffic {
            assert!((0.0..=100.0).contains(&actor.position));
            assert!(matches!(actor.direction.as_str(), "east" | "west"));
        }
    }
}
