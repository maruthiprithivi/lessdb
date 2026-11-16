//! Situation-aware context assembly for Lessville agents.
//!
//! This is an incubation layer for the future LessDB agent-context API. It
//! deliberately composes existing SQL/application scope rules rather than
//! claiming native LessDB row ACL, TTL or ranking guarantees.

use serde::Serialize;

use crate::sim::{Agent, World, acl, day_of, place_name};
use crate::store::{Row, Store, i, s};

const MAX_ITEMS: usize = 6;
const MAX_CONTENT_CHARS: usize = 160;

#[derive(Clone, Debug, Serialize)]
pub struct Situation {
    pub agent: String,
    pub name: String,
    pub title: String,
    pub occupation: String,
    pub behaviour: String,
    pub mood: String,
    pub logical_minute: i64,
    pub day: i64,
    pub time_zone: &'static str,
    pub location: String,
    pub place: String,
    pub activity: String,
    pub activity_kind: String,
    pub station: String,
    pub pose: String,
    pub nearby_agents: Vec<String>,
    pub active_commitments: Vec<String>,
    pub weather: String,
    pub world_conditions: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct RecallIntent {
    pub task: String,
    pub purpose: String,
    /// An internal, already-escaped SQL predicate. Public agent-facing APIs
    /// should eventually replace this with typed filters.
    pub predicate: String,
    pub max_items: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct ContextItem {
    pub id: i64,
    pub owner: String,
    pub kind: String,
    pub scope: String,
    pub location: String,
    pub tag: String,
    pub context_id: String,
    pub expires_at: i64,
    pub source_kind: String,
    pub source_id: i64,
    pub content: String,
    pub included: bool,
    pub omission_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ContextPacket {
    pub context_id: String,
    pub task: String,
    pub purpose: String,
    pub situation: Situation,
    pub items: Vec<ContextItem>,
    pub visibility_rule: &'static str,
    pub max_items: usize,
    pub content_chars: usize,
}

impl Situation {
    pub fn from_world(world: &World, agent_id: &str, minute: i64) -> Self {
        let current = world
            .agents
            .iter()
            .find(|a| a.id == agent_id)
            .cloned()
            .unwrap_or_else(|| Agent {
                id: agent_id.to_string(),
                name: agent_id.to_string(),
                role: String::new(),
                title: String::new(),
                occupation: String::new(),
                behaviour: "unknown".into(),
                mood: "unknown".into(),
                location: "unknown".into(),
                activity: String::new(),
                activity_kind: "idle".into(),
                station: String::new(),
                pose: "idle".into(),
                travel: None,
                bubble: None,
                carrying: None,
                presence: Default::default(),
                flags: Default::default(),
                notes: Default::default(),
                last_decision: None,
                life: Default::default(),
            });
        let nearby_agents = world
            .agents
            .iter()
            .filter(|a| a.id != agent_id && a.location == current.location)
            .map(|a| a.name.clone())
            .collect();
        let active_commitments = world
            .plans
            .iter()
            .filter(|p| p.agent == agent_id && p.at >= minute)
            .take(4)
            .map(|p| format!("{} at {}", p.action, crate::sim::clock(p.at)))
            .collect();
        let weather = if world.rain_until > minute {
            "rain"
        } else if world.last_rain_end >= minute - 180 {
            "wet_after_rain"
        } else if world.weather_kind.is_empty() {
            "clear"
        } else {
            world.weather_kind.as_str()
        };
        let mut world_conditions = vec![format!("cafe:{}", world.cafe.lifecycle)];
        world_conditions.push(format!("temperature_c:{}", world.temperature_c));
        world_conditions.push(format!("wind_kmh:{}", world.wind_kmh));
        if let Some(trip) = &world.bus_trip {
            world_conditions.push(format!("bus_trip:{trip}"));
        }
        if weather != "clear" {
            world_conditions.push(weather.into());
        }
        Situation {
            agent: current.id,
            name: current.name,
            title: current.title,
            occupation: current.occupation,
            behaviour: current.behaviour,
            mood: current.mood,
            logical_minute: minute,
            day: day_of(minute),
            time_zone: "Asia/Singapore",
            place: place_name(&current.location).into(),
            location: current.location,
            activity: current.activity,
            activity_kind: current.activity_kind,
            station: current.station,
            pose: current.pose,
            nearby_agents,
            active_commitments,
            weather: weather.into(),
            world_conditions,
        }
    }
}

impl ContextPacket {
    pub fn empty(situation: Situation, task: String, purpose: String, max_items: usize) -> Self {
        Self {
            context_id: format!("{}:{}:{}", situation.agent, task, situation.logical_minute),
            task,
            purpose,
            situation,
            items: Vec::new(),
            visibility_rule: "application-layer: owner OR membership in a visible scope; not native LessDB ACL",
            max_items,
            content_chars: 0,
        }
    }

    pub fn ids(&self) -> String {
        self.items
            .iter()
            .map(|item| format!("mem:{}", item.id))
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn bullets(&self) -> String {
        self.items
            .iter()
            .filter(|item| item.included)
            .map(|item| {
                if item.owner.is_empty() || item.owner == self.situation.agent {
                    format!("- {}", item.content)
                } else {
                    format!(
                        "- {}, {}: {}",
                        display_name(&item.owner),
                        item.scope,
                        item.content
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn empty_items_text(&self) -> String {
        if self.items.is_empty() {
            "- nothing relevant".into()
        } else {
            self.bullets()
        }
    }
}

/// Assemble a bounded packet from the existing LessDB memory table. The
/// application-layer ACL is explicit in the SQL and in the returned packet.
pub async fn recall(
    store: &Store,
    world: &World,
    agent: &str,
    minute: i64,
    intent: RecallIntent,
) -> Result<ContextPacket, String> {
    let max_items = intent.max_items.clamp(1, MAX_ITEMS);
    let sql = format!(
        "SELECT id, owner, kind, scope, location, tag, context_id, expires_at, source_kind, source_id, content FROM lv_memories WHERE {} AND (expires_at < 0 OR expires_at > {minute}) AND NOT EXISTS (SELECT 1 FROM lv_memory_lifecycle WHERE memory_id = lv_memories.id AND state IN ('archived', 'revoked', 'deleted')) AND ({}) ORDER BY id DESC LIMIT {max_items}",
        acl(agent),
        intent.predicate
    );
    let rows = store
        .run(
            minute,
            agent,
            &format!("context recall: {}", intent.purpose),
            &sql,
        )
        .await?;
    let items: Vec<_> = rows.into_iter().map(item).collect();
    let content_chars = items.iter().map(|item| item.content.chars().count()).sum();
    let situation = Situation::from_world(world, agent, minute);
    Ok(ContextPacket {
        context_id: format!("{}:{}:{}", agent, intent.task, minute),
        task: intent.task,
        purpose: intent.purpose,
        situation,
        items,
        visibility_rule: "application-layer: owner OR membership in a visible scope; not native LessDB ACL",
        max_items,
        content_chars,
    })
}

fn item(row: Row) -> ContextItem {
    ContextItem {
        id: i(&row, "id"),
        owner: s(&row, "owner"),
        kind: s(&row, "kind"),
        scope: s(&row, "scope"),
        location: s(&row, "location"),
        tag: s(&row, "tag"),
        context_id: s(&row, "context_id"),
        expires_at: i(&row, "expires_at"),
        source_kind: s(&row, "source_kind"),
        source_id: i(&row, "source_id"),
        content: truncate(&s(&row, "content"), MAX_CONTENT_CHARS),
        included: true,
        omission_reason: None,
    }
}

fn display_name(id: &str) -> String {
    let mut chars = id.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        format!("{}...", value.chars().take(max_chars).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::{Situation, truncate};
    use crate::sim::World;

    #[test]
    fn situation_contains_time_place_and_nearby_agents() {
        let world = World::fresh(1);
        let situation = Situation::from_world(&world, "maya", 390);
        assert_eq!(situation.time_zone, "Asia/Singapore");
        assert_eq!(situation.place, "Less Café");
        assert!(situation.nearby_agents.is_empty());
    }

    #[test]
    fn context_text_is_bounded() {
        assert_eq!(truncate("abcdef", 3), "abc...");
        assert_eq!(truncate("abc", 3), "abc");
    }
}
