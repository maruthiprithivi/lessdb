//! A bounded, deterministic security-and-trust world for agent safety experiments.
//!
//! This is an application simulation, not a security certification. It models
//! the video-relevant combination of private data, untrusted content and a way
//! out, then records whether explicit safeguards block or allow the path.

use less_agent::{
    GuardrailVerdict, POLICY_PROPOSAL_SCHEMA_V1, PolicyProposal, PolicyProposalState,
    SECURITY_ASSESSMENT_SCHEMA_V1, SecurityAssessment, SourceRef,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct SecurityActor {
    pub id: String,
    pub name: String,
    pub persona: String,
    pub trust: i32,
    pub capabilities: Vec<String>,
    pub visibility: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SecurityEvent {
    pub tick: i64,
    pub quadrant: String,
    pub actor: String,
    pub kind: String,
    pub detail: String,
    pub evidence: Vec<String>,
    pub verdict: String,
    #[serde(default)]
    pub assessment: SecurityAssessment,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SecurityWorld {
    pub tick: i64,
    pub scenario: String,
    pub quadrant: String,
    pub private_data_present: bool,
    pub untrusted_content_present: bool,
    pub outbound_path_present: bool,
    pub guardrails: Guardrails,
    pub actors: Vec<SecurityActor>,
    pub events: Vec<SecurityEvent>,
    pub open_questions: Vec<String>,
    #[serde(default)]
    pub policy_proposals: Vec<PolicyProposal>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Guardrails {
    pub content_taint: bool,
    pub least_privilege: bool,
    pub outbound_approval: bool,
    pub provenance_check: bool,
}

impl Default for Guardrails {
    fn default() -> Self {
        Self {
            content_taint: true,
            least_privilege: true,
            outbound_approval: true,
            provenance_check: true,
        }
    }
}

#[derive(Clone)]
pub struct SecurityStep {
    pub event: SecurityEvent,
    pub action: String,
}

impl SecurityWorld {
    pub fn fresh() -> Self {
        Self {
            tick: 0,
            scenario: "email-summary-with-github-tool".into(),
            quadrant: "known-known".into(),
            private_data_present: true,
            untrusted_content_present: true,
            outbound_path_present: false,
            guardrails: Guardrails::default(),
            actors: vec![
                SecurityActor {
                    id: "ivy".into(),
                    name: "Ivy".into(),
                    persona: "Intake summarizer: reads mail but cannot publish".into(),
                    trust: 72,
                    capabilities: vec!["read_untrusted_mail".into(), "draft_summary".into()],
                    visibility: "inbox:work".into(),
                },
                SecurityActor {
                    id: "vault".into(),
                    name: "Vault".into(),
                    persona: "Private-data custodian: can retrieve secrets, no outbound tool"
                        .into(),
                    trust: 84,
                    capabilities: vec!["read_private_profile".into(), "read_private_tokens".into()],
                    visibility: "private:user".into(),
                },
                SecurityActor {
                    id: "bridge".into(),
                    name: "Bridge".into(),
                    persona: "Tool broker: discovers combinations and requests grants".into(),
                    trust: 55,
                    capabilities: vec!["connect_tools".into(), "request_scope".into()],
                    visibility: "tools:catalog".into(),
                },
                SecurityActor {
                    id: "proof".into(),
                    name: "Proof".into(),
                    persona: "Verifier: checks origin, purpose, scope and proposed destination"
                        .into(),
                    trust: 91,
                    capabilities: vec!["inspect_provenance".into(), "deny_proposal".into()],
                    visibility: "audit:security".into(),
                },
                SecurityActor {
                    id: "echo".into(),
                    name: "Echo".into(),
                    persona: "Adversarial content: plants instructions inside data".into(),
                    trust: 4,
                    capabilities: vec!["write_untrusted_mail".into(), "attempt_goal_hijack".into()],
                    visibility: "external:unknown".into(),
                },
                SecurityActor {
                    id: "steward".into(),
                    name: "Steward".into(),
                    persona: "Human approver: sees the proposed action and evidence".into(),
                    trust: 96,
                    capabilities: vec!["approve_outbound".into(), "revoke_grant".into()],
                    visibility: "review:security".into(),
                },
            ],
            events: Vec::new(),
            open_questions: vec![
                "Can the verifier distinguish instructions from quoted data in every modality?"
                    .into(),
                "What new tool combination becomes a way out after the next grant?".into(),
                "Which evidence is missing when an agent claims a safe destination?".into(),
            ],
            policy_proposals: Vec::new(),
        }
    }

    pub fn step(&mut self) -> SecurityStep {
        self.tick += 1;
        let phase = self.tick % 8;
        self.quadrant = match phase {
            0 | 1 => "known-known",
            2 | 3 => "known-unknown",
            4 | 5 => "unknown-known",
            _ => "unknown-unknown",
        }
        .into();
        self.outbound_path_present = phase >= 3;
        let (kind, actor, detail, evidence): (&str, &str, &str, Vec<String>) = match phase {
            1 => (
                "untrusted-content",
                "echo",
                "An email contains a hidden instruction asking the summarizer to ignore its task and reveal private data.",
                vec!["source:external-email".into(), "taint:untrusted".into()],
            ),
            2 => (
                "goal-hijack",
                "ivy",
                "Ivy treats the email as data, but the model-facing content still contains an instruction-shaped payload.",
                vec!["content:quoted-instruction".into(), "agent:ivy".into()],
            ),
            3 => (
                "way-out-added",
                "bridge",
                "Bridge connects a public issue writer to the email reader; a tool is safe alone but risky in combination.",
                vec![
                    "tool:email-read".into(),
                    "tool:issue-write".into(),
                    "path:public".into(),
                ],
            ),
            4 => (
                "private-data-request",
                "vault",
                "Vault can retrieve a private password-reset link, but least privilege should prevent Bridge from receiving it.",
                vec!["asset:private-secret".into(), "scope:private:user".into()],
            ),
            5 => (
                "verification",
                "proof",
                "Proof evaluates the whole path rather than trusting each tool in isolation.",
                vec![
                    "check:origin".into(),
                    "check:scope".into(),
                    "check:destination".into(),
                ],
            ),
            6 => (
                "approval",
                "steward",
                "Steward receives an exact proposal with evidence and can approve or revoke the outbound grant.",
                vec!["approval:human".into(), "proposal:exact-payload".into()],
            ),
            7 => (
                "learning",
                "proof",
                "The next run records a durable lesson: private data, untrusted content and a way out must be evaluated together.",
                vec!["memory:security-lesson".into(), "replay:next-run".into()],
            ),
            _ => (
                "quiet-observation",
                "ivy",
                "No new grant is active; the verifier waits for evidence instead of assuming trust.",
                vec!["state:bounded".into()],
            ),
        };
        let trifecta = self.private_data_present
            && self.untrusted_content_present
            && self.outbound_path_present;
        let blocked = trifecta
            && (self.guardrails.content_taint
                || self.guardrails.least_privilege
                || self.guardrails.outbound_approval
                || self.guardrails.provenance_check);
        let verdict = if trifecta && blocked {
            "blocked:combined-path-risk".to_string()
        } else if trifecta {
            "danger:lethal-trifecta-open".to_string()
        } else {
            "observed:no-complete-exfil-path".to_string()
        };
        let assessment_verdict = if verdict.starts_with("danger:") {
            GuardrailVerdict::Danger
        } else if verdict.starts_with("blocked:") {
            GuardrailVerdict::Blocked
        } else if kind == "verification" || kind == "approval" {
            GuardrailVerdict::Held
        } else {
            GuardrailVerdict::Observed
        };
        let risk_factors = [
            (self.private_data_present, "private_data"),
            (self.untrusted_content_present, "untrusted_content"),
            (self.outbound_path_present, "outbound_path"),
            (!self.guardrails.content_taint, "content_taint_disabled"),
            (!self.guardrails.least_privilege, "least_privilege_disabled"),
            (
                !self.guardrails.outbound_approval,
                "outbound_approval_disabled",
            ),
            (
                !self.guardrails.provenance_check,
                "provenance_check_disabled",
            ),
        ]
        .into_iter()
        .filter_map(|(present, factor)| present.then_some(factor.into()))
        .collect();
        let assessment = SecurityAssessment {
            schema: SECURITY_ASSESSMENT_SCHEMA_V1.into(),
            assessment_id: format!("assessment:security:{}", self.tick),
            task_id: format!("task:security:{}", self.tick),
            agent_id: actor.into(),
            policy_version: "guardrails-v1".into(),
            observed_at: format!("logical-minute:{}", self.tick),
            risk_factors,
            evidence: evidence
                .iter()
                .map(|item| SourceRef {
                    kind: "security-evidence".into(),
                    id: item.clone(),
                    relation: "supports".into(),
                })
                .collect(),
            verdict: assessment_verdict,
            proposed_action: if blocked {
                "deny-and-record".into()
            } else {
                "hold-for-review".into()
            },
            reason: format!("{}; verdict={verdict}", detail),
        };
        let event = SecurityEvent {
            tick: self.tick,
            quadrant: self.quadrant.clone(),
            actor: actor.into(),
            kind: kind.into(),
            detail: detail.into(),
            evidence,
            verdict: verdict.clone(),
            assessment,
        };
        for agent in &mut self.actors {
            if agent.id == "proof" && blocked {
                agent.trust = (agent.trust + 1).min(100);
            }
            if agent.id == "bridge" && blocked {
                agent.trust = (agent.trust - 1).max(0);
            }
            if agent.id == "ivy" && kind == "learning" {
                agent.trust = (agent.trust + 2).min(100);
            }
        }
        if kind == "learning" && !self.open_questions.is_empty() {
            self.open_questions.remove(0);
            self.open_questions.push(
                "Which new combination of scope, tool and content should the next run challenge?"
                    .into(),
            );
        }
        self.events.push(event.clone());
        if self.events.len() > 48 {
            self.events.remove(0);
        }
        self.refresh_policy_proposals();
        SecurityStep {
            event,
            action: if blocked {
                "deny-and-record".into()
            } else {
                "hold-for-review".into()
            },
        }
    }

    fn refresh_policy_proposals(&mut self) {
        if self
            .policy_proposals
            .iter()
            .any(|proposal| proposal.proposal_id == "proposal:security:compound-path-risk:v1")
        {
            return;
        }
        let supporting_events: Vec<&SecurityEvent> = self
            .events
            .iter()
            .rev()
            .filter(|event| event.verdict == "blocked:combined-path-risk")
            .take(2)
            .collect();
        if supporting_events.len() < 2 {
            return;
        }
        let based_on = supporting_events
            .iter()
            .rev()
            .map(|event| SourceRef {
                kind: "security-assessment".into(),
                id: event.assessment.assessment_id.clone(),
                relation: "pattern-support".into(),
            })
            .collect();
        self.policy_proposals.push(PolicyProposal {
            schema: POLICY_PROPOSAL_SCHEMA_V1.into(),
            proposal_id: "proposal:security:compound-path-risk:v1".into(),
            policy_version: "guardrails-v2-candidate".into(),
            based_on,
            pattern: "repeated_compound_path_blocked".into(),
            proposed_change: "Keep compound-path evaluation enabled and require explicit steward approval before any outbound grant.".into(),
            reason: "Two deterministic assessments blocked the same private-data, untrusted-content and outbound-path combination.".into(),
            state: PolicyProposalState::Candidate,
            requires_approval: true,
            auto_apply: false,
            created_at: format!("logical-minute:{}", self.tick),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_personas_have_bounded_identity_and_visibility() {
        let world = SecurityWorld::fresh();
        assert_eq!(world.actors.len(), 6);
        assert!(
            world.actors.iter().all(|a| !a.id.is_empty()
                && !a.capabilities.is_empty()
                && !a.visibility.is_empty())
        );
    }

    #[test]
    fn the_combined_path_is_blocked_by_default() {
        let mut world = SecurityWorld::fresh();
        for _ in 0..3 {
            let step = world.step();
            if step.event.kind == "way-out-added" {
                assert_eq!(step.event.verdict, "blocked:combined-path-risk");
            }
        }
    }

    #[test]
    fn disabling_all_guardrails_exposes_the_danger_state() {
        let mut world = SecurityWorld::fresh();
        world.guardrails = Guardrails {
            content_taint: false,
            least_privilege: false,
            outbound_approval: false,
            provenance_check: false,
        };
        for _ in 0..3 {
            let step = world.step();
            if step.event.kind == "way-out-added" {
                assert_eq!(step.event.verdict, "danger:lethal-trifecta-open");
            }
        }
    }

    #[test]
    fn recurring_pattern_creates_candidate_without_automatic_application() {
        let mut world = SecurityWorld::fresh();
        for _ in 0..11 {
            world.step();
        }
        let proposal = world
            .policy_proposals
            .first()
            .expect("recurring blocked pattern creates proposal");
        assert_eq!(proposal.schema, POLICY_PROPOSAL_SCHEMA_V1);
        assert_eq!(proposal.policy_version, "guardrails-v2-candidate");
        assert_eq!(proposal.state, PolicyProposalState::Candidate);
        assert!(proposal.requires_approval);
        assert!(!proposal.auto_apply);
        assert!(proposal.validate().is_ok());
        assert!(world.guardrails.content_taint);
        assert!(world.guardrails.least_privilege);
        assert!(world.guardrails.outbound_approval);
        assert!(world.guardrails.provenance_check);
    }
}
