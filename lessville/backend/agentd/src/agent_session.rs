//! Proposal-only agent-session adapter above LessVille's application-owned LessDB projections.
//!
//! This module is deliberately small and transport-neutral at its core. It uses the
//! incubating `less-agent` contracts, then compiles them through LessVille's current
//! application-layer recall predicate. It does not claim native ACL, automatic schema
//! evolution, or physical authorization.

use std::collections::BTreeSet;

use less_agent::{
    COMPETENCY_SCHEMA_V1, COMPETENCY_STATE_SCHEMA_V1, CONTEXT_RECIPE_SCHEMA_V1,
    COUNTERFACTUAL_SCHEMA_V1, ContextRecipe, ContextSourceKind, CounterfactualDecision,
    EVOLUTION_PROPOSAL_SCHEMA_V1, EvolutionCompatibility, EvolutionOperation, EvolutionProposal,
    EvolutionState, IDENTITY_THREAD_SCHEMA_V1, SOCIAL_INTERACTION_SCHEMA_V1, SourceRef,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::context::{ContextPacket, RecallIntent, recall};
use crate::sim::{World, acl};
use crate::store::{Store, i, q, s};

pub const SESSION_POLICY_VERSION: &str = "agent-session/v1";
const MAX_SESSION_ITEMS: usize = 6;
const MAX_SESSION_BYTES: usize = 4_096;
const ALLOWED_AGENTS: &[&str] = &["maya", "sam", "lin", "ama", "bo"];

#[derive(Clone, Debug, Deserialize)]
pub struct ComposeRequest {
    pub agent: String,
    pub task: Option<String>,
    pub purpose: Option<String>,
    pub source_kinds: Option<Vec<String>>,
    pub scope_ids: Option<Vec<String>>,
    pub max_items: Option<usize>,
    pub max_bytes: Option<usize>,
    pub policy_version: Option<String>,
    pub recipe_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Omission {
    pub reason: String,
    pub count: i64,
    pub explanation: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ComposeResult {
    pub recipe: ContextRecipe,
    pub packet: ContextPacket,
    pub omissions: Vec<Omission>,
    pub included_count: usize,
    pub boundary: &'static str,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SimulateRequest {
    pub proposal_id: String,
    pub agent: String,
    pub task: Option<String>,
}

pub fn known_agent(agent: &str) -> bool {
    ALLOWED_AGENTS.contains(&agent)
}

pub fn catalog(public: bool) -> Value {
    json!({
        "schema": "lessdb-agent-session-catalog/v1",
        "policy_version": SESSION_POLICY_VERSION,
        "public": public,
        "operations": [
            {"name": "catalog", "mode": "read", "available": true, "description": "Discover typed capabilities, bounds and explicit non-guarantees."},
            {"name": "compose", "mode": "private", "available": true, "description": "Compile a bounded situation-shaped context packet with omission explanations."},
            {"name": "explain", "mode": "read", "available": true, "description": "Inspect frontier or evolution provenance by replayable identifier."},
            {"name": "propose", "mode": "private", "available": true, "description": "Persist a declarative candidate evolution proposal; never activates it."},
            {"name": "simulate", "mode": "private", "available": true, "description": "Replay a candidate against bounded evidence; no shared state is applied."},
            {"name": "activate", "mode": "unsupported", "available": false, "description": "Separate approval and release workflow; intentionally absent from this slice."}
        ],
        "contracts": {
            "context_recipe": CONTEXT_RECIPE_SCHEMA_V1,
            "evolution_proposal": EVOLUTION_PROPOSAL_SCHEMA_V1,
            "counterfactual": COUNTERFACTUAL_SCHEMA_V1,
            "social_interaction": SOCIAL_INTERACTION_SCHEMA_V1,
            "competency": COMPETENCY_SCHEMA_V1,
            "competency_state": COMPETENCY_STATE_SCHEMA_V1,
            "identity_thread": IDENTITY_THREAD_SCHEMA_V1,
            "frontier_state": ["candidate", "simulated", "accepted", "rejected"]
        },
        "bounds": {
            "max_items": MAX_SESSION_ITEMS,
            "max_bytes": MAX_SESSION_BYTES,
            "proposal_patch": "declarative JSON-like text only; executable SQL and DDL are rejected"
        },
        "non_guarantees": [
            "application-layer scope filtering is not native LessDB row ACL",
            "candidate proposals are not automatic schema migration or policy execution",
            "simulation is not formal verification or a physical safety authorization",
            "encryption-at-rest, distributed consensus and tenant isolation require separate evidence"
        ],
        "boundary": "Incubating agent-session adapter above embedded LessDB; proposal and simulation remain owner-reviewed application operations."
    })
}

pub async fn compose(
    store: &Store,
    world: &World,
    request: ComposeRequest,
    minute: i64,
) -> Result<ComposeResult, String> {
    if !known_agent(&request.agent) {
        return Err("unknown agent".into());
    }
    let task = bounded_text(request.task.as_deref().unwrap_or("observe"), "task")?;
    let purpose = bounded_text(
        request
            .purpose
            .as_deref()
            .unwrap_or("compose situation-shaped context"),
        "purpose",
    )?;
    let policy_version = bounded_text(
        request
            .policy_version
            .as_deref()
            .unwrap_or(SESSION_POLICY_VERSION),
        "policy_version",
    )?;
    let max_items = request
        .max_items
        .unwrap_or(MAX_SESSION_ITEMS)
        .clamp(1, MAX_SESSION_ITEMS);
    let max_bytes = request
        .max_bytes
        .unwrap_or(MAX_SESSION_BYTES)
        .clamp(256, MAX_SESSION_BYTES);
    let source_kinds = parse_source_kinds(request.source_kinds.as_deref())?;
    let memberships = visible_memberships(store, &request.agent, minute).await?;
    let requested_scopes = request.scope_ids.unwrap_or_else(|| memberships.clone());
    let allowed: BTreeSet<_> = memberships.iter().cloned().collect();
    if requested_scopes
        .iter()
        .any(|scope| !allowed.contains(scope))
    {
        return Err("compose cannot request a scope the agent is not a member of".into());
    }

    let scope_predicate = if requested_scopes.is_empty() {
        "1 = 1".to_string()
    } else {
        format!(
            "(owner = {} OR scope IN ({}))",
            q(&request.agent),
            requested_scopes
                .iter()
                .map(|scope| q(scope))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let intent = RecallIntent {
        task: task.clone(),
        purpose: purpose.clone(),
        predicate: scope_predicate.clone(),
        max_items,
    };
    let mut packet = recall(store, world, &request.agent, minute, intent)
        .await
        .map_err(|error| format!("compose recall failed: {error}"))?;
    let mut packet_bytes = 0usize;
    let mut budget_omitted = 0i64;
    for item in &mut packet.items {
        if !item.included {
            continue;
        }
        let item_bytes = item.content.len();
        if packet_bytes.saturating_add(item_bytes) > max_bytes {
            item.included = false;
            item.omission_reason = Some("budget_exceeded".into());
            budget_omitted += 1;
        } else {
            packet_bytes = packet_bytes.saturating_add(item_bytes);
        }
    }
    packet.content_chars = packet
        .items
        .iter()
        .filter(|item| item.included)
        .map(|item| item.content.chars().count())
        .sum();
    let visible_count = count_visible(store, &request.agent, &scope_predicate, minute).await?;
    let hidden_count = count_rows(
        store,
        minute,
        &format!(
            "SELECT count(*) AS n FROM lv_memories WHERE NOT ({})",
            acl(&request.agent)
        ),
        "count hidden context candidates",
    )
    .await?;
    let expired_count = count_rows(
        store,
        minute,
        &format!(
            "SELECT count(*) AS n FROM lv_memories WHERE ({}) AND expires_at >= 0 AND expires_at <= {minute}",
            acl(&request.agent)
        ),
        "count expired context candidates",
    )
    .await?;
    let mut omissions = Vec::new();
    if hidden_count > 0 {
        omissions.push(Omission {
            reason: "scope_not_visible".into(),
            count: hidden_count,
            explanation: "The application-layer owner/membership predicate omitted rows outside this agent's visible scopes; this is not native LessDB ACL.".into(),
        });
    }
    if expired_count > 0 {
        omissions.push(Omission {
            reason: "expired_or_stale".into(),
            count: expired_count,
            explanation: "Rows outside their logical validity window are omitted while retained as evidence for replay.".into(),
        });
    }
    if visible_count > max_items as i64 || budget_omitted > 0 {
        omissions.push(Omission {
            reason: "budget_exceeded".into(),
            count: (visible_count - max_items as i64).max(budget_omitted),
            explanation: "Visible candidates exceeded the bounded item or byte budget; the packet keeps the newest bounded rows.".into(),
        });
    }
    let recipe = ContextRecipe {
        schema: CONTEXT_RECIPE_SCHEMA_V1.into(),
        recipe_id: request
            .recipe_id
            .unwrap_or_else(|| format!("recipe:{}:{}:{}", request.agent, task, minute)),
        owner_agent: request.agent.clone(),
        purpose,
        task_id: task,
        source_kinds,
        scope_ids: requested_scopes,
        grant_ids: Vec::new(),
        temporal_from: None,
        temporal_until: Some(minute),
        frame_ids: Vec::new(),
        max_items,
        max_bytes,
        policy_version,
        explain: true,
    };
    recipe.validate().map_err(str::to_owned)?;
    Ok(ComposeResult {
        included_count: packet.items.iter().filter(|item| item.included).count(),
        recipe,
        packet,
        omissions,
        boundary: "Application-layer scoped recall over embedded LessDB; omitted rows remain evidence and native ACL is not implied.",
    })
}

pub async fn explain(store: &Store, id: &str, minute: i64) -> Result<Option<Value>, String> {
    if id.starts_with("context:") || id.starts_with("recipe:") {
        return Ok(Some(json!({
            "kind": if id.starts_with("context:") { "context_packet" } else { "context_recipe" },
            "record": {"id": id, "request_scoped": true},
            "explanation": "This packet or recipe is request-scoped. Re-run compose with the same bounded inputs to reproduce its included items and omission reasons; source rows are not copied into the adapter.",
            "source_refs": "Returned by compose from embedded LessDB memory rows",
            "boundary": "Application-layer scope, temporal and budget explanation; not native ACL or optimizer provenance."
        })));
    }
    if id.starts_with("frontier:") {
        let rows = store
            .run(
                minute,
                "agent-session",
                "explain frontier signal",
                &format!(
                    "SELECT * FROM lv_frontier_signals WHERE signal_id = {}",
                    q(id)
                ),
            )
            .await?;
        return Ok(rows.first().map(|row| {
            json!({
                "kind": "frontier_signal",
                "record": row,
                "explanation": "A recurring application-level representation gap; it is not a schema migration or policy instruction.",
                "state": "candidate",
                "auto_apply": false,
            })
        }));
    }
    if id.starts_with("evolution:") {
        let rows = store
            .run(
                minute,
                "agent-session",
                "explain evolution proposal",
                &format!(
                    "SELECT * FROM lv_evolution_proposals WHERE proposal_id = {}",
                    q(id)
                ),
            )
            .await?;
        return Ok(rows.first().map(|row| {
            json!({
                "kind": "evolution_proposal",
                "record": row,
                "explanation": "Declarative candidate only; activation, migration and rollback remain outside this adapter.",
                "state": "candidate",
                "auto_apply": false,
            })
        }));
    }
    Ok(None)
}

pub async fn propose(
    store: &Store,
    proposal: EvolutionProposal,
    minute: i64,
) -> Result<Value, String> {
    proposal.validate().map_err(str::to_owned)?;
    if proposal.state != EvolutionState::Candidate {
        return Err("agent-session accepts only candidate proposals".into());
    }
    if proposal.migration_ref.is_none() || proposal.rollback_ref.is_none() {
        return Err("candidate proposal requires migration and rollback references".into());
    }
    if proposal.affected_scopes.iter().any(|scope| {
        scope != &proposal.owner_scope && scope != &format!("agent:{}", proposal.proposing_agent)
    }) {
        return Err("proposal cannot widen beyond its owner or proposing-agent scope".into());
    }
    let lower = proposal.patch.to_ascii_lowercase();
    for forbidden in [
        "select ",
        "insert ",
        "update ",
        "delete ",
        "drop ",
        "alter ",
        "truncate ",
        ";",
    ] {
        if lower.contains(forbidden) {
            return Err(
                "proposal patch must be declarative and cannot contain executable SQL".into(),
            );
        }
    }
    let operation = operation_name(&proposal.operation);
    let compatibility = compatibility_name(&proposal.compatibility);
    let evidence_refs = proposal
        .based_on
        .iter()
        .map(|item| format!("{}:{}:{}", item.kind, item.id, item.relation))
        .collect::<Vec<_>>()
        .join("|");
    let affected_scope = proposal.affected_scopes.join("|");
    let replay_key = format!("proposal:{}:minute:{minute}", proposal.proposal_id);
    let sql = format!(
        "INSERT INTO lv_evolution_proposals VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, 'candidate', {}, {}, {}, 0)",
        q(&proposal.proposal_id),
        minute,
        q(&proposal.proposing_agent),
        q(&proposal.owner_scope),
        q(operation),
        q(&proposal.patch),
        q(&proposal.purpose),
        q(&evidence_refs),
        q(&affected_scope),
        q(compatibility),
        q(proposal.migration_ref.as_deref().unwrap_or_default()),
        q(proposal.rollback_ref.as_deref().unwrap_or_default()),
        q(&replay_key),
    );
    store
        .run(
            minute,
            &proposal.proposing_agent,
            "persist agent-session proposal",
            &sql,
        )
        .await?;
    Ok(json!({
        "ok": true,
        "proposal": proposal,
        "persisted": {"table": "lv_evolution_proposals", "replay_key": replay_key, "state": "candidate", "auto_apply": false},
        "boundary": "Candidate persisted for review; no schema, scope, policy or actuator change was applied."
    }))
}

pub async fn simulate(
    store: &Store,
    request: SimulateRequest,
    minute: i64,
) -> Result<Value, String> {
    if !known_agent(&request.agent) {
        return Err("unknown agent".into());
    }
    let rows = store
        .run(
            minute,
            &request.agent,
            "load proposal for bounded simulation",
            &format!(
                "SELECT proposal_id, agent, owner_scope, operation, purpose, state, compatibility, evidence_refs, affected_scope, replay_key FROM lv_evolution_proposals WHERE proposal_id = {}",
                q(&request.proposal_id)
            ),
        )
        .await?;
    let Some(proposal) = rows.first() else {
        return Err("proposal not found".into());
    };
    if s(proposal, "state") != "candidate" {
        return Err("only candidate proposals can be simulated".into());
    }
    let visible = count_rows(
        store,
        minute,
        &format!(
            "SELECT count(*) AS n FROM lv_memories WHERE {} AND (expires_at < 0 OR expires_at > {minute})",
            acl(&request.agent)
        ),
        "count baseline context",
    )
    .await?;
    let task = request.task.unwrap_or_else(|| "simulate".into());
    let replay_id = format!(
        "simulation:{}:{}:{}",
        request.proposal_id, request.agent, minute
    );
    let counterfactual = CounterfactualDecision {
        schema: COUNTERFACTUAL_SCHEMA_V1.into(),
        replay_id: replay_id.clone(),
        agent_id: request.agent,
        topic: task,
        baseline_policy: "current_application_contract".into(),
        baseline_action: format!("visible_context_candidates={visible}"),
        learned_policy: request.proposal_id.clone(),
        learned_action: "candidate projection inspected; not applied".into(),
        difference:
            "simulation reports a candidate-only branch; source rows and permissions are unchanged"
                .into(),
        source_refs: vec![SourceRef {
            kind: "evolution_proposal".into(),
            id: request.proposal_id,
            relation: "simulated_against".into(),
        }],
        simulated_at: minute.to_string(),
        not_applied: true,
    };
    counterfactual.validate().map_err(str::to_owned)?;
    Ok(json!({
        "ok": true,
        "counterfactual": counterfactual,
        "proposal": proposal,
        "baseline": {"visible_context_candidates": visible},
        "candidate": {"source_rows_changed": false, "scope_changed": false, "policy_changed": false, "actuator_called": false},
        "boundary": "Replay-only comparison; no shared state, permissions, policy or actuator was changed."
    }))
}

async fn visible_memberships(
    store: &Store,
    agent: &str,
    minute: i64,
) -> Result<Vec<String>, String> {
    let rows = store
        .run(
            minute,
            agent,
            "agent-session list visible scopes",
            &format!(
                "SELECT scope FROM lv_scope_members WHERE agent = {} ORDER BY scope",
                q(agent)
            ),
        )
        .await?;
    Ok(rows.iter().map(|row| s(row, "scope")).collect())
}

async fn count_visible(
    store: &Store,
    agent: &str,
    scope_predicate: &str,
    minute: i64,
) -> Result<i64, String> {
    count_rows(
        store,
        minute,
        &format!(
            "SELECT count(*) AS n FROM lv_memories WHERE {} AND ({}) AND (expires_at < 0 OR expires_at > {minute}) AND NOT EXISTS (SELECT 1 FROM lv_memory_lifecycle WHERE memory_id = lv_memories.id AND state IN ('archived', 'revoked', 'deleted'))",
            acl(agent), scope_predicate
        ),
        "count composed context candidates",
    )
    .await
}

async fn count_rows(store: &Store, minute: i64, sql: &str, purpose: &str) -> Result<i64, String> {
    let rows = store.run(minute, "agent-session", purpose, sql).await?;
    Ok(rows.first().map(|row| i(row, "n")).unwrap_or(0))
}

fn bounded_text(value: &str, field: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 160 {
        return Err(format!("{field} must be 1–160 characters"));
    }
    Ok(trimmed.to_string())
}

fn parse_source_kinds(values: Option<&[String]>) -> Result<Vec<ContextSourceKind>, String> {
    let values = values
        .map(|items| items.to_vec())
        .unwrap_or_else(|| vec!["memory_lens".into()]);
    if values.is_empty() || values.len() > 5 {
        return Err("source_kinds must contain between 1 and 5 entries".into());
    }
    values
        .into_iter()
        .map(
            |value| match value.to_ascii_lowercase().replace('-', "_").as_str() {
                "memory_lens" | "memory" => Ok(ContextSourceKind::MemoryLens),
                "view" => Ok(ContextSourceKind::View),
                "observation_stream" | "observation" => Ok(ContextSourceKind::ObservationStream),
                "relationship" => Ok(ContextSourceKind::Relationship),
                "lesson" => Ok(ContextSourceKind::Lesson),
                _ => Err(format!("unsupported context source kind: {value}")),
            },
        )
        .collect()
}

fn operation_name(operation: &EvolutionOperation) -> &'static str {
    match operation {
        EvolutionOperation::View => "view",
        EvolutionOperation::MemoryLens => "memory_lens",
        EvolutionOperation::Connector => "connector",
        EvolutionOperation::FieldExtension => "field_extension",
        EvolutionOperation::Retention => "retention",
        EvolutionOperation::Grant => "grant",
        EvolutionOperation::Policy => "policy",
    }
}

fn compatibility_name(compatibility: &EvolutionCompatibility) -> &'static str {
    match compatibility {
        EvolutionCompatibility::Unknown => "unknown",
        EvolutionCompatibility::Additive => "additive",
        EvolutionCompatibility::Substitutive => "substitutive",
        EvolutionCompatibility::Breaking => "breaking",
    }
}

#[cfg(test)]
mod tests {
    use super::{bounded_text, known_agent, parse_source_kinds};

    #[test]
    fn session_inputs_are_bounded_and_agents_are_known() {
        assert!(known_agent("lin"));
        assert!(!known_agent("unknown"));
        assert!(bounded_text("observe", "task").is_ok());
        assert!(bounded_text(&"x".repeat(161), "task").is_err());
    }

    #[test]
    fn source_kind_names_are_explicit() {
        assert_eq!(
            parse_source_kinds(Some(&["memory-lens".into(), "lesson".into()]))
                .unwrap()
                .len(),
            2
        );
        assert!(parse_source_kinds(Some(&["arbitrary".into()])).is_err());
    }
}
