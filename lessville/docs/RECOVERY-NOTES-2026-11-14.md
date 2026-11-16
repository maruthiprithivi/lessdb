# Recovery notes — 2026-11-14 (sandbox reset)

## What was lost in the reset
- All uncommitted LessVille work for the situated-presence / Singapore climate / traffic.v2 slice
  (presence.rs, climate.rs, traffic.rs, sim.rs integration, town_view.gd/main.gd patches,
  tools/derive_handsfree_sprites.py, tools/derive_foliage_mask.py, shaders/foliage_wind.gdshader,
  docs/research-notes-climate-traffic.md). Design is preserved in TODO/plan text below (re-add).
- The local-only LessDB candidate (/home/ubuntu/lessdb-wal/repo, "b32e600 + WAL/lifecycle/IPC/
  ownership patches", local commit 1442d2d incl. crates/less-agent). Only docs/AGENT-NATIVE-*.md survived.
- Untracked deploy/cloudflare/public/ (rebuilt from live site) and the ignored container binary.
- R2 S3 keys are empty in env after reset; CLOUDFLARE_API_TOKEN works (Wrangler + REST).

## Source locations found (GitHub CLI authenticated)
- Public repo `lessdb` (owner masked): main = 629c46f clean source (2026-10-01) + site commits to 44d823b
  (2026-10-11). Branch `fix/wal-recovery-fail-closed` = 6c1cd6d "Reject malformed WAL framing before
  writable recovery". Branch `docs/experimental-agent-memory`. Crates: less-auth catalog cli common cypher
  difftest engine fanout gpu graph mcp memory query server sqllogictest storage telemetry vector.
- Private repo `lessdb-private-archive`: 136 commits, latest 84ae452 (2026-10-01) WAL/ownership work,
  many codex/* branches. Neither repo contains b32e600, 1442d2d or any less-agent crate.
- Project handover zip LessDB-Handover-01-Core-2026-10-07.zip: older snapshots 84ae452/0b9b1e4/8c7a156.
- Toolchain pinned by lessdb rust-toolchain.toml: 1.96 (installed via rustup).

## Production outage facts (from Workers observability, 2026-11-07..14)
- Lite 256 MiB instance OOM on 11-10 and 11-11; DO resets; median container request wall time
  ~105 s (11-07) -> ~800 s (11-14), ceiling 900 s.
- v1 checkpoint: 14.7 MB gz, 18.7 MiB; rows approx: lv_memories 150k, lv_events 186k, lv_decisions 144k,
  lv_novelty_events 103k, lv_evolution_proposals 103k, lv_frontier_signals 52k, lv_llm_calls 43k,
  lv_orders 34k, lv_messages 35k, lv_object_transitions 24k, lv_access 10k; many 1-row lv_competencies
  parts of ~0.4 MiB each (unbounded source-reference lists).
- No hot-table retention exists in backend (no DELETE except a scripted forget).
- Recovered by CKPT_PREFIX lessville/v1 -> v2 and TOWN rebind; v1 archive 169 objects ~2.28 GB intact.
  Commit 15856e6.

## less-agent reconstruction contract (from LessVille call sites)
Used: SourceRef{kind,id,relation}; ContextRecipe{schema,recipe_id,owner_agent,purpose,task_id,
source_kinds:Vec<ContextSourceKind>,scope_ids,grant_ids,temporal_from:Option<i64>,temporal_until:Option<i64>,
frame_ids,max_items:usize,max_bytes:usize,policy_version,explain:bool}.validate()->Result<(),&'static str>;
ContextSourceKind{MemoryLens,View,ObservationStream,Relationship,Lesson};
CounterfactualDecision{schema,replay_id,agent_id,topic,baseline_policy,baseline_action,learned_policy,
learned_action,difference,source_refs,simulated_at:String,not_applied:bool}.validate();
EvolutionProposal{schema,proposal_id,parent_manifest_id:Option<String>,proposing_agent,owner_scope,
operation:EvolutionOperation,patch,purpose,based_on,affected_scopes,compatibility,required_approvals:Vec<String>,
state:EvolutionState,migration_ref:Option<String>,rollback_ref:Option<String>,lease_until:Option<i64>,
policy_version,auto_apply:bool}.validate() (Deserialize from JSON; state serializes "candidate");
EvolutionOperation{View,MemoryLens,Connector,FieldExtension,Retention,Grant,Policy};
EvolutionCompatibility{Unknown,Additive,Substitutive,Breaking}; EvolutionState{Candidate,...};
SecurityAssessment{schema,assessment_id,task_id,agent_id,policy_version,observed_at,risk_factors:Vec<String>,
evidence:Vec<SourceRef>,verdict:GuardrailVerdict,proposed_action,reason}; GuardrailVerdict{Danger,Blocked,Held,Observed};
PolicyProposal{schema,proposal_id,policy_version,based_on,pattern,proposed_change,reason,state:PolicyProposalState,
requires_approval,auto_apply,created_at}.validate(); PolicyProposalState{Candidate,...}.
Schema consts: COMPETENCY_SCHEMA_V1, COMPETENCY_STATE_SCHEMA_V1, CONTEXT_RECIPE_SCHEMA_V1, COUNTERFACTUAL_SCHEMA_V1,
EVOLUTION_PROPOSAL_SCHEMA_V1, IDENTITY_THREAD_SCHEMA_V1, SOCIAL_INTERACTION_SCHEMA_V1, POLICY_PROPOSAL_SCHEMA_V1,
SECURITY_ASSESSMENT_SCHEMA_V1. Exact string values: take from LessVille strings/docs (e.g. lessdb-identity-thread/v1,
lessdb-competency-state/v1, lessdb-competency/v1, lessdb-social-interaction/v1).

## Reconstruction progress (same day)
- Candidate rebuilt on private-archive 84ae452 + reconstructed crates/less-agent; pushed to private
  branch candidate/agent-native-2026-11 at 0367d81. less-agent 6 tests; LessVille 33 pass/1 ignored.
- Retention fix: backend/agentd/src/retention.rs (nightly, 3-7 day windows, permanent cap 160/owner),
  bounded_refs (24) for competency source_refs; scenario retention_bounds_hot_tables_and_caps_permanent_memories.
- FINDING: the 84ae452 engine refuses the checkpoint written by the deployed b32e600-lineage binary:
  "legacy nonempty table lacks ACTIVE.json; explicit audited migration required". Fails closed.
  => new binary must run on a fresh checkpoint prefix (lessville/v3/); v1 and v2 stay read-only.
- Docker installed (docker.io 29.1.3) for wrangler container builds.
