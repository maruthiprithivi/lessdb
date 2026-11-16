//! # less-agent — incubating agent-native contracts
//!
//! Typed, versioned values that describe *how an agent relates to its data*:
//! situation-shaped context recipes, shared-context grants, spatial frames,
//! evolution proposals, counterfactual replays, security assessments,
//! developmental evidence and situated presence.
//!
//! **Boundary.** Every type here is an application-layer contract. Validation
//! fails closed on malformed values, but nothing in this crate is native row
//! ACL, tenant isolation, authorization, formal verification, a safety
//! interlock or an actuator permission. Proposals never self-activate:
//! `auto_apply` must be `false` for every proposal type.
//!
//! Reconstructed on 2026-11-14 from the LessVille call sites and the Phase 3
//! design record after the original local-only incubation commit was lost.

use serde::{Deserialize, Serialize};

pub const CONTEXT_RECIPE_SCHEMA_V1: &str = "lessdb-context-recipe/v1";
pub const EVOLUTION_PROPOSAL_SCHEMA_V1: &str = "lessdb-evolution-proposal/v1";
pub const COUNTERFACTUAL_SCHEMA_V1: &str = "lessdb-counterfactual/v1";
pub const SOCIAL_INTERACTION_SCHEMA_V1: &str = "lessdb-social-interaction/v1";
pub const COMPETENCY_SCHEMA_V1: &str = "lessdb-competency/v1";
pub const COMPETENCY_STATE_SCHEMA_V1: &str = "lessdb-competency-state/v1";
pub const IDENTITY_THREAD_SCHEMA_V1: &str = "lessdb-identity-thread/v1";
pub const SECURITY_ASSESSMENT_SCHEMA_V1: &str = "lessdb-security-assessment/v1";
pub const CONTEXT_ACCESS_SCHEMA_V1: &str = "lessdb-context-access/v1";
pub const POLICY_PROPOSAL_SCHEMA_V1: &str = "lessdb-policy-proposal/v1";
pub const SPATIAL_FRAME_SCHEMA_V1: &str = "lessdb-spatial-frame/v1";
pub const FRAME_TRANSFORM_SCHEMA_V1: &str = "lessdb-frame-transform/v1";
pub const SPATIAL_OBSERVATION_SCHEMA_V1: &str = "lessdb-spatial-observation/v1";
pub const SHARED_CONTEXT_GRANT_SCHEMA_V1: &str = "lessdb-shared-context-grant/v1";
pub const RETENTION_POLICY_SCHEMA_V1: &str = "lessdb-retention-policy/v1";
pub const SITUATED_PRESENCE_SCHEMA_V1: &str = "lessdb-situated-presence/v1";

/// Upper bounds shared by the contracts. Small on purpose: these values are
/// carried into bounded prompts and observer panels.
pub const MAX_ID_BYTES: usize = 160;
pub const MAX_TEXT_BYTES: usize = 2_000;
pub const MAX_LIST: usize = 32;
pub const MAX_CONTEXT_ITEMS: usize = 64;
pub const MAX_CONTEXT_BYTES: usize = 64 * 1024;

pub type ContractResult = Result<(), &'static str>;

fn id(v: &str, err: &'static str) -> ContractResult {
    if v.trim().is_empty() || v.len() > MAX_ID_BYTES || v.chars().any(char::is_control) {
        Err(err)
    } else {
        Ok(())
    }
}
fn text(v: &str, err: &'static str) -> ContractResult {
    if v.trim().is_empty() || v.len() > MAX_TEXT_BYTES {
        Err(err)
    } else {
        Ok(())
    }
}
fn schema(v: &str, want: &str) -> ContractResult {
    if v == want {
        Ok(())
    } else {
        Err("unknown or mismatched contract schema")
    }
}
fn list<T>(v: &[T], err: &'static str) -> ContractResult {
    if v.len() > MAX_LIST { Err(err) } else { Ok(()) }
}

/// A stable reference to evidence that supports or produced a value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    pub kind: String,
    pub id: String,
    pub relation: String,
}
impl SourceRef {
    pub fn validate(&self) -> ContractResult {
        id(&self.kind, "source ref kind is required")?;
        id(&self.id, "source ref id is required")?;
        id(&self.relation, "source ref relation is required")
    }
}
fn refs(v: &[SourceRef], required: bool) -> ContractResult {
    if required && v.is_empty() {
        return Err("at least one source reference is required");
    }
    list(v, "too many source references")?;
    v.iter().try_for_each(SourceRef::validate)
}

// ---------------------------------------------------------------- context

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextSourceKind {
    MemoryLens,
    View,
    ObservationStream,
    Relationship,
    Lesson,
}

/// A bounded, situation-shaped composition of context sources for one task.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContextRecipe {
    pub schema: String,
    pub recipe_id: String,
    pub owner_agent: String,
    pub purpose: String,
    pub task_id: String,
    pub source_kinds: Vec<ContextSourceKind>,
    pub scope_ids: Vec<String>,
    pub grant_ids: Vec<String>,
    pub temporal_from: Option<i64>,
    pub temporal_until: Option<i64>,
    pub frame_ids: Vec<String>,
    pub max_items: usize,
    pub max_bytes: usize,
    pub policy_version: String,
    pub explain: bool,
}
impl ContextRecipe {
    pub fn validate(&self) -> ContractResult {
        schema(&self.schema, CONTEXT_RECIPE_SCHEMA_V1)?;
        id(&self.recipe_id, "recipe id is required")?;
        id(&self.owner_agent, "recipe owner is required")?;
        text(&self.purpose, "recipe purpose is required")?;
        id(&self.task_id, "recipe task is required")?;
        id(&self.policy_version, "recipe policy version is required")?;
        if self.source_kinds.is_empty() || self.source_kinds.len() > 5 {
            return Err("recipe needs between one and five source kinds");
        }
        list(&self.scope_ids, "too many scopes")?;
        list(&self.grant_ids, "too many grants")?;
        list(&self.frame_ids, "too many frames")?;
        if self.max_items == 0 || self.max_items > MAX_CONTEXT_ITEMS {
            return Err("recipe item budget out of range");
        }
        if self.max_bytes < 64 || self.max_bytes > MAX_CONTEXT_BYTES {
            return Err("recipe byte budget out of range");
        }
        if let (Some(a), Some(b)) = (self.temporal_from, self.temporal_until)
            && a > b
        {
            return Err("recipe temporal window is inverted");
        }
        Ok(())
    }
}

/// One replayable visibility decision for one candidate item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextAccessDecision {
    pub schema: String,
    pub task_id: String,
    pub recipient: String,
    pub grant_id: Option<String>,
    pub scope_id: String,
    pub item: SourceRef,
    pub included: bool,
    pub reason: String,
    pub policy_version: String,
}
impl ContextAccessDecision {
    pub fn validate(&self) -> ContractResult {
        schema(&self.schema, CONTEXT_ACCESS_SCHEMA_V1)?;
        id(&self.task_id, "access task is required")?;
        id(&self.recipient, "access recipient is required")?;
        id(&self.scope_id, "access scope is required")?;
        self.item.validate()?;
        id(&self.reason, "access reason is required")?;
        id(&self.policy_version, "access policy version is required")
    }
}

/// Explicit, purpose-bound and time-bounded sharing of context kinds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedContextGrant {
    pub schema: String,
    pub grant_id: String,
    pub owner: String,
    pub scope_id: String,
    pub recipients: Vec<String>,
    pub purpose: String,
    pub allowed_kinds: Vec<String>,
    pub valid_from: i64,
    pub valid_until: i64,
    pub max_items: usize,
    pub max_bytes: usize,
    pub redactions: Vec<String>,
    pub policy_version: String,
}
impl SharedContextGrant {
    pub fn validate(&self) -> ContractResult {
        schema(&self.schema, SHARED_CONTEXT_GRANT_SCHEMA_V1)?;
        id(&self.grant_id, "grant id is required")?;
        id(&self.owner, "grant owner is required")?;
        id(&self.scope_id, "grant scope is required")?;
        text(&self.purpose, "grant purpose is required")?;
        if self.recipients.is_empty() || self.allowed_kinds.is_empty() {
            return Err("grant needs explicit recipients and kinds");
        }
        if self.recipients.iter().any(|r| r == &self.owner) {
            return Err("grant owner cannot be its own recipient");
        }
        list(&self.recipients, "too many recipients")?;
        list(&self.allowed_kinds, "too many kinds")?;
        if self.valid_from >= self.valid_until {
            return Err("grant window must be non-empty");
        }
        if self.max_items == 0 || self.max_items > MAX_CONTEXT_ITEMS || self.max_bytes == 0 {
            return Err("grant budget out of range");
        }
        id(&self.policy_version, "grant policy version is required")
    }
    /// Fail-closed contract helper — **not native row ACL**.
    pub fn permits(&self, recipient: &str, kind: &str, at: i64) -> bool {
        self.validate().is_ok()
            && self.recipients.iter().any(|r| r == recipient)
            && self.allowed_kinds.iter().any(|k| k == kind)
            && at >= self.valid_from
            && at < self.valid_until
    }
}

// ---------------------------------------------------------------- spatial

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpatialFrame {
    pub schema: String,
    pub frame_id: String,
    pub parent: Option<String>,
    pub version: u32,
    pub owner: String,
}
impl SpatialFrame {
    pub fn validate(&self) -> ContractResult {
        schema(&self.schema, SPATIAL_FRAME_SCHEMA_V1)?;
        id(&self.frame_id, "frame id is required")?;
        id(&self.owner, "frame owner is required")?;
        if self.version == 0 {
            return Err("frame version starts at 1");
        }
        if self.parent.as_deref() == Some(self.frame_id.as_str()) {
            return Err("frame cannot be its own parent");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrameTransform {
    pub schema: String,
    pub transform_id: String,
    pub from_frame: String,
    pub to_frame: String,
    pub translation: [f64; 3],
    pub yaw_rad: f64,
    pub uncertainty_m: f64,
    pub observed_at: i64,
    pub valid_until: i64,
    pub source_refs: Vec<SourceRef>,
}
impl FrameTransform {
    pub fn validate(&self) -> ContractResult {
        schema(&self.schema, FRAME_TRANSFORM_SCHEMA_V1)?;
        id(&self.transform_id, "transform id is required")?;
        id(&self.from_frame, "transform source frame is required")?;
        id(&self.to_frame, "transform target frame is required")?;
        if self.from_frame == self.to_frame {
            return Err("transform cannot map a frame to itself");
        }
        if !self
            .translation
            .iter()
            .chain([&self.yaw_rad])
            .all(|v| v.is_finite())
        {
            return Err("transform values must be finite");
        }
        if !self.uncertainty_m.is_finite() || self.uncertainty_m < 0.0 {
            return Err("transform uncertainty must be non-negative");
        }
        if self.valid_until <= self.observed_at {
            return Err("transform validity must follow observation");
        }
        refs(&self.source_refs, true)
    }
    pub fn is_fresh(&self, at: i64) -> bool {
        at >= self.observed_at && at < self.valid_until
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpatialObservation {
    pub schema: String,
    pub observation_id: String,
    pub device: String,
    pub sequence: u64,
    pub frame_id: String,
    pub kind: String,
    pub position: [f64; 3],
    pub uncertainty_m: f64,
    pub observed_at: i64,
    pub evidence: Vec<SourceRef>,
}
impl SpatialObservation {
    pub fn validate(&self) -> ContractResult {
        schema(&self.schema, SPATIAL_OBSERVATION_SCHEMA_V1)?;
        id(&self.observation_id, "observation id is required")?;
        id(&self.device, "observation device is required")?;
        id(&self.frame_id, "observation frame is required")?;
        id(&self.kind, "observation kind is required")?;
        if !self.position.iter().all(|v| v.is_finite())
            || !self.uncertainty_m.is_finite()
            || self.uncertainty_m < 0.0
        {
            return Err("observation pose must be finite with non-negative uncertainty");
        }
        refs(&self.evidence, false)
    }
}

// ---------------------------------------------------------------- evolution

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvolutionOperation {
    View,
    MemoryLens,
    Connector,
    FieldExtension,
    Retention,
    Grant,
    Policy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvolutionCompatibility {
    Unknown,
    Additive,
    Substitutive,
    Breaking,
}

/// Lifecycle states. Proposal, simulation and acceptance are separate states;
/// a syntactically valid proposal is never implicitly active.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvolutionState {
    Candidate,
    Simulated,
    Accepted,
    Rejected,
    RolledBack,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvolutionProposal {
    pub schema: String,
    pub proposal_id: String,
    pub parent_manifest_id: Option<String>,
    pub proposing_agent: String,
    pub owner_scope: String,
    pub operation: EvolutionOperation,
    pub patch: String,
    pub purpose: String,
    pub based_on: Vec<SourceRef>,
    pub affected_scopes: Vec<String>,
    pub compatibility: EvolutionCompatibility,
    pub required_approvals: Vec<String>,
    pub state: EvolutionState,
    pub migration_ref: Option<String>,
    pub rollback_ref: Option<String>,
    pub lease_until: Option<i64>,
    pub policy_version: String,
    pub auto_apply: bool,
}
impl EvolutionProposal {
    pub fn validate(&self) -> ContractResult {
        schema(&self.schema, EVOLUTION_PROPOSAL_SCHEMA_V1)?;
        id(&self.proposal_id, "proposal id is required")?;
        id(&self.proposing_agent, "proposing agent is required")?;
        id(&self.owner_scope, "proposal owner scope is required")?;
        text(&self.patch, "proposal patch is required")?;
        text(&self.purpose, "proposal purpose is required")?;
        refs(&self.based_on, true)?;
        if self.affected_scopes.is_empty() {
            return Err("proposal must declare affected scopes");
        }
        list(&self.affected_scopes, "too many affected scopes")?;
        list(&self.required_approvals, "too many approvals")?;
        id(&self.policy_version, "proposal policy version is required")?;
        if self.auto_apply {
            return Err("proposals never auto-apply");
        }
        if self.compatibility == EvolutionCompatibility::Breaking
            && self.required_approvals.is_empty()
        {
            return Err("breaking proposals require named approvals");
        }
        if matches!(self.state, EvolutionState::Accepted) && self.rollback_ref.is_none() {
            return Err("accepted proposals require a rollback reference");
        }
        Ok(())
    }
}

/// Replay-only comparison between a baseline and a learned/candidate policy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CounterfactualDecision {
    pub schema: String,
    pub replay_id: String,
    pub agent_id: String,
    pub topic: String,
    pub baseline_policy: String,
    pub baseline_action: String,
    pub learned_policy: String,
    pub learned_action: String,
    pub difference: String,
    pub source_refs: Vec<SourceRef>,
    pub simulated_at: String,
    pub not_applied: bool,
}
impl CounterfactualDecision {
    pub fn validate(&self) -> ContractResult {
        schema(&self.schema, COUNTERFACTUAL_SCHEMA_V1)?;
        id(&self.replay_id, "replay id is required")?;
        id(&self.agent_id, "replay agent is required")?;
        text(&self.topic, "replay topic is required")?;
        text(&self.baseline_action, "baseline action is required")?;
        text(&self.learned_action, "learned action is required")?;
        refs(&self.source_refs, true)?;
        if !self.not_applied {
            return Err("counterfactuals are replay-only and must not be applied");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- security

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardrailVerdict {
    #[default]
    Observed,
    Held,
    Blocked,
    Danger,
}

/// `Default` exists for backward-compatible deserialization of older rows that
/// predate the assessment field; an empty default never passes `validate()`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityAssessment {
    pub schema: String,
    pub assessment_id: String,
    pub task_id: String,
    pub agent_id: String,
    pub policy_version: String,
    pub observed_at: String,
    pub risk_factors: Vec<String>,
    pub evidence: Vec<SourceRef>,
    pub verdict: GuardrailVerdict,
    pub proposed_action: String,
    pub reason: String,
}
impl Default for SecurityAssessment {
    fn default() -> Self {
        Self {
            schema: SECURITY_ASSESSMENT_SCHEMA_V1.into(),
            assessment_id: String::new(),
            task_id: String::new(),
            agent_id: String::new(),
            policy_version: String::new(),
            observed_at: String::new(),
            risk_factors: Vec::new(),
            evidence: Vec::new(),
            verdict: GuardrailVerdict::Observed,
            proposed_action: String::new(),
            reason: String::new(),
        }
    }
}
impl SecurityAssessment {
    pub fn validate(&self) -> ContractResult {
        schema(&self.schema, SECURITY_ASSESSMENT_SCHEMA_V1)?;
        id(&self.assessment_id, "assessment id is required")?;
        id(&self.task_id, "assessment task is required")?;
        id(&self.agent_id, "assessment agent is required")?;
        id(
            &self.policy_version,
            "assessment policy version is required",
        )?;
        list(&self.risk_factors, "too many risk factors")?;
        refs(&self.evidence, false)?;
        text(&self.reason, "assessment reason is required")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyProposalState {
    Candidate,
    Accepted,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyProposal {
    pub schema: String,
    pub proposal_id: String,
    pub policy_version: String,
    pub based_on: Vec<SourceRef>,
    pub pattern: String,
    pub proposed_change: String,
    pub reason: String,
    pub state: PolicyProposalState,
    pub requires_approval: bool,
    pub auto_apply: bool,
    pub created_at: String,
}
impl PolicyProposal {
    pub fn validate(&self) -> ContractResult {
        schema(&self.schema, POLICY_PROPOSAL_SCHEMA_V1)?;
        id(&self.proposal_id, "policy proposal id is required")?;
        id(&self.policy_version, "policy version is required")?;
        refs(&self.based_on, true)?;
        text(&self.proposed_change, "proposed change is required")?;
        text(&self.reason, "policy proposal reason is required")?;
        if self.auto_apply || !self.requires_approval {
            return Err("policy proposals require approval and never auto-apply");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- retention

/// Bounded hot-set retention for one append-mostly table. Rows beyond the
/// bound are compacted out of the hot table; evidence that must survive is
/// summarized or archived by the application, never silently rewritten.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub schema: String,
    pub table: String,
    /// Keep rows whose logical minute is within this window of "now".
    pub keep_minutes: i64,
    /// Never keep more than this many rows, newest first.
    pub max_rows: usize,
    /// Rows the application must keep regardless (e.g. permanent memories).
    pub pinned_predicate: Option<String>,
    pub archive: RetentionArchive,
    pub policy_version: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionArchive {
    /// Rows leave the hot set; older checkpoints in object storage retain them.
    CheckpointHistory,
    /// Rows are folded into a bounded summary row before deletion.
    Summarized,
}
impl RetentionPolicy {
    pub fn validate(&self) -> ContractResult {
        schema(&self.schema, RETENTION_POLICY_SCHEMA_V1)?;
        id(&self.table, "retention table is required")?;
        if !self
            .table
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err("retention table must be a plain identifier");
        }
        if self.keep_minutes <= 0 || self.max_rows == 0 {
            return Err("retention bounds must be positive");
        }
        id(&self.policy_version, "retention policy version is required")
    }
}

// ---------------------------------------------------------------- situated presence

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionFocus {
    Task,
    Conversation,
    Navigation,
    Environment,
    Rest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandState {
    Free,
    Carrying,
    UsingPhone,
    Leash,
    Umbrella,
}

/// Time-bounded, evidence-backed embodied state for one agent. Rendered props
/// are a projection of this value; it never authorizes an action.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SituatedPresence {
    pub schema: String,
    pub agent_id: String,
    pub place: String,
    pub attention: AttentionFocus,
    pub hands: HandState,
    pub companion: Option<String>,
    pub nearby: Vec<String>,
    pub since_minute: i64,
    pub valid_until: i64,
    pub rule: String,
    pub source_refs: Vec<SourceRef>,
}
impl SituatedPresence {
    pub fn validate(&self) -> ContractResult {
        schema(&self.schema, SITUATED_PRESENCE_SCHEMA_V1)?;
        id(&self.agent_id, "presence agent is required")?;
        id(&self.place, "presence place is required")?;
        id(&self.rule, "presence rule is required")?;
        list(&self.nearby, "too many nearby entities")?;
        if self.valid_until <= self.since_minute {
            return Err("presence must have a bounded, non-empty window");
        }
        if self.hands == HandState::Leash && self.companion.is_none() {
            return Err("a leash requires an active companion");
        }
        refs(&self.source_refs, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(id: &str) -> SourceRef {
        SourceRef {
            kind: "event".into(),
            id: id.into(),
            relation: "supports".into(),
        }
    }

    fn proposal() -> EvolutionProposal {
        EvolutionProposal {
            schema: EVOLUTION_PROPOSAL_SCHEMA_V1.into(),
            proposal_id: "evolution:test:1".into(),
            parent_manifest_id: None,
            proposing_agent: "lin".into(),
            owner_scope: "agent:lin".into(),
            operation: EvolutionOperation::MemoryLens,
            patch: r#"{"add_lens":"route-weather"}"#.into(),
            purpose: "bounded route lens".into(),
            based_on: vec![r("ctx:1")],
            affected_scopes: vec!["agent:lin".into()],
            compatibility: EvolutionCompatibility::Additive,
            required_approvals: vec![],
            state: EvolutionState::Candidate,
            migration_ref: Some("m:1".into()),
            rollback_ref: Some("rb:1".into()),
            lease_until: Some(60),
            policy_version: "agent-session/test".into(),
            auto_apply: false,
        }
    }

    #[test]
    fn proposal_validates_and_never_auto_applies() {
        let mut p = proposal();
        assert!(p.validate().is_ok());
        p.auto_apply = true;
        assert!(p.validate().is_err());
        let mut p = proposal();
        p.compatibility = EvolutionCompatibility::Breaking;
        assert!(p.validate().is_err());
        let mut p = proposal();
        p.based_on.clear();
        assert!(p.validate().is_err());
        let mut p = proposal();
        p.schema = "lessdb-evolution-proposal/v9".into();
        assert!(p.validate().is_err());
    }

    #[test]
    fn enums_serialize_snake_case_and_round_trip() {
        let json = serde_json::to_value(proposal()).unwrap();
        assert_eq!(json["state"], "candidate");
        assert_eq!(json["operation"], "memory_lens");
        let back: EvolutionProposal = serde_json::from_value(json).unwrap();
        assert_eq!(back, proposal());
        assert_eq!(
            serde_json::to_value(GuardrailVerdict::Danger).unwrap(),
            "danger"
        );
    }

    #[test]
    fn grant_permits_fails_closed() {
        let g = SharedContextGrant {
            schema: SHARED_CONTEXT_GRANT_SCHEMA_V1.into(),
            grant_id: "grant:1".into(),
            owner: "coordinator".into(),
            scope_id: "zone:loading-bay".into(),
            recipients: vec!["verifier-b".into()],
            purpose: "inspect-nearby-obstacle".into(),
            allowed_kinds: vec!["neighbor_pose".into(), "obstacle_detected".into()],
            valid_from: 1200,
            valid_until: 1260,
            max_items: 12,
            max_bytes: 6000,
            redactions: vec![],
            policy_version: "grants/v1".into(),
        };
        assert!(g.permits("verifier-b", "neighbor_pose", 1200));
        assert!(
            !g.permits("verifier-b", "neighbor_pose", 1260),
            "window end is exclusive"
        );
        assert!(
            !g.permits("verifier-c", "neighbor_pose", 1230),
            "unlisted recipient"
        );
        assert!(
            !g.permits("verifier-b", "camera_frame", 1230),
            "unlisted kind"
        );
        let mut bad = g.clone();
        bad.recipients.push("coordinator".into());
        assert!(
            !bad.permits("verifier-b", "neighbor_pose", 1230),
            "invalid grant permits nothing"
        );
    }

    #[test]
    fn frames_and_transforms_reject_self_reference_and_staleness() {
        let f = SpatialFrame {
            schema: SPATIAL_FRAME_SCHEMA_V1.into(),
            frame_id: "duck-a/base".into(),
            parent: Some("duck-a/base".into()),
            version: 1,
            owner: "duck-a".into(),
        };
        assert!(f.validate().is_err());
        let t = FrameTransform {
            schema: FRAME_TRANSFORM_SCHEMA_V1.into(),
            transform_id: "tf:1".into(),
            from_frame: "duck-a/base".into(),
            to_frame: "warehouse/floor-1".into(),
            translation: [1.0, 2.0, 0.0],
            yaw_rad: 0.1,
            uncertainty_m: 0.05,
            observed_at: 100,
            valid_until: 160,
            source_refs: vec![r("calib:1")],
        };
        assert!(t.validate().is_ok());
        assert!(t.is_fresh(120) && !t.is_fresh(160));
        let mut nan = t.clone();
        nan.yaw_rad = f64::NAN;
        assert!(nan.validate().is_err());
    }

    #[test]
    fn counterfactual_and_policy_proposals_are_never_applied() {
        let c = CounterfactualDecision {
            schema: COUNTERFACTUAL_SCHEMA_V1.into(),
            replay_id: "replay:1".into(),
            agent_id: "lin".into(),
            topic: "commute".into(),
            baseline_policy: "baseline".into(),
            baseline_action: "08:00 bus".into(),
            learned_policy: "lesson".into(),
            learned_action: "check service notice".into(),
            difference: "earlier check".into(),
            source_refs: vec![r("lesson:1")],
            simulated_at: "540".into(),
            not_applied: false,
        };
        assert!(c.validate().is_err());
        let p = PolicyProposal {
            schema: POLICY_PROPOSAL_SCHEMA_V1.into(),
            proposal_id: "policy:1".into(),
            policy_version: "guardrails-v2-candidate".into(),
            based_on: vec![r("assessment:1")],
            pattern: "repeated".into(),
            proposed_change: "keep compound-path evaluation".into(),
            reason: "two blocked assessments".into(),
            state: PolicyProposalState::Candidate,
            requires_approval: true,
            auto_apply: false,
            created_at: "logical-minute:1".into(),
        };
        assert!(p.validate().is_ok());
    }

    #[test]
    fn retention_and_presence_are_bounded() {
        let mut rp = RetentionPolicy {
            schema: RETENTION_POLICY_SCHEMA_V1.into(),
            table: "lv_events".into(),
            keep_minutes: 7 * 1440,
            max_rows: 20_000,
            pinned_predicate: None,
            archive: RetentionArchive::CheckpointHistory,
            policy_version: "retention/v1".into(),
        };
        assert!(rp.validate().is_ok());
        rp.table = "lv_events; DROP TABLE x".into();
        assert!(rp.validate().is_err());
        let mut p = SituatedPresence {
            schema: SITUATED_PRESENCE_SCHEMA_V1.into(),
            agent_id: "ama".into(),
            place: "park".into(),
            attention: AttentionFocus::Navigation,
            hands: HandState::Leash,
            companion: Some("mochi".into()),
            nearby: vec![],
            since_minute: 400,
            valid_until: 430,
            rule: "dog-walk-outdoors".into(),
            source_refs: vec![],
        };
        assert!(p.validate().is_ok());
        p.companion = None;
        assert!(p.validate().is_err(), "leash without a companion");
        p.hands = HandState::Free;
        p.valid_until = 400;
        assert!(p.validate().is_err(), "unbounded window");
    }
}
