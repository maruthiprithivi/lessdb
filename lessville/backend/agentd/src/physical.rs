//! Bounded physical-simulator ingestion above the embedded LessDB engine.
//!
//! This module is intentionally asynchronous and sampled. It must not be put
//! in a simulator's physics, servo, policy-inference or Gymnasium step loop.

use serde_json::{Value, json};

use crate::store::{Store, q};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct IngestSummary {
    pub episode_created: bool,
    pub observation_inserted: bool,
    pub proposal_inserted: bool,
}

fn required<'a>(value: &'a Value, path: &str) -> Result<&'a Value, String> {
    let mut current = value;
    for part in path.split('.') {
        current = current
            .get(part)
            .ok_or_else(|| format!("missing physical envelope field: {path}"))?;
    }
    if current.is_null() {
        return Err(format!("null physical envelope field: {path}"));
    }
    Ok(current)
}

fn string(value: &Value, path: &str) -> Result<String, String> {
    required(value, path)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("physical envelope field is not a string: {path}"))
}

fn optional_string(value: &Value, path: &str) -> String {
    value
        .get(path)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn integer(value: &Value, path: &str) -> Result<i64, String> {
    required(value, path)?
        .as_i64()
        .ok_or_else(|| format!("physical envelope field is not an integer: {path}"))
}

fn json_text(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".into())
}

/// Ingest one adapter envelope. Repeated envelope IDs are treated as replayed
/// observations and do not create duplicate LessDB rows.
pub async fn ingest_envelope(
    store: &Store,
    envelope: &Value,
    sim_minute: i64,
    actor: &str,
) -> Result<IngestSummary, String> {
    if string(envelope, "schema")? != "lessdb-physical-envelope/v1" {
        return Err("unsupported physical envelope schema".into());
    }
    let episode_id = string(envelope, "episode_id")?;
    let profile = string(envelope, "profile")?;
    let simulator = string(envelope, "simulator")?;
    let control_interface = string(envelope, "control_interface")?;
    let observed_at = string(envelope, "observation.observed_at")?;
    let observation_id = string(envelope, "observation.observation_id")?;
    let sequence = integer(envelope, "observation.sequence")?;
    let sensor_id = string(envelope, "observation.sensor_id")?;
    let kind = string(envelope, "observation.kind")?;
    let evidence_hash = optional_string(
        envelope.get("observation").unwrap_or(&Value::Null),
        "evidence_hash",
    );
    let payload_ref = optional_string(
        envelope.get("observation").unwrap_or(&Value::Null),
        "payload_ref",
    );
    let detail = json_text(envelope);

    let mut summary = IngestSummary::default();
    let existing_run = store
        .run(
            sim_minute,
            actor,
            "check physical simulation episode",
            &format!(
                "SELECT episode_id FROM lv_physical_runs WHERE episode_id = {}",
                q(&episode_id)
            ),
        )
        .await?;
    if existing_run.is_empty() {
        store
            .run(
                sim_minute,
                actor,
                "persist physical simulation episode",
                &format!(
                    "INSERT INTO lv_physical_runs VALUES ({}, {}, {}, {}, {}, {})",
                    q(&episode_id),
                    q(&profile),
                    q(&simulator),
                    q(&control_interface),
                    q(&observed_at),
                    q(&json_text(envelope.get("provenance").unwrap_or(&json!({})),)),
                ),
            )
            .await?;
        summary.episode_created = true;
    }

    let existing_observation = store
        .run(
            sim_minute,
            actor,
            "check physical observation replay",
            &format!(
                "SELECT observation_id FROM lv_physical_observations WHERE observation_id = {}",
                q(&observation_id)
            ),
        )
        .await?;
    if existing_observation.is_empty() {
        store
            .run(
                sim_minute,
                actor,
                "persist physical observation",
                &format!(
                    "INSERT INTO lv_physical_observations VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {})",
                    q(&observation_id),
                    q(&episode_id),
                    sequence,
                    q(&sensor_id),
                    q(&kind),
                    q(&observed_at),
                    q(&evidence_hash),
                    q(&payload_ref),
                    q(&detail),
                ),
            )
            .await?;
        summary.observation_inserted = true;
    }

    if let Some(proposal) = envelope.get("proposal").filter(|v| !v.is_null()) {
        let proposal_id = string(proposal, "proposal_id")?;
        let action_kind = string(proposal, "action_kind")?;
        let state = string(proposal, "state")?;
        let idempotency_key = string(proposal, "idempotency_key")?;
        let source_observation_id = proposal
            .get("source_observation_ids")
            .and_then(Value::as_array)
            .and_then(|ids| ids.first())
            .and_then(Value::as_str)
            .unwrap_or(&observation_id);
        let existing_proposal = store
            .run(
                sim_minute,
                actor,
                "check physical proposal replay",
                &format!(
                    "SELECT proposal_id FROM lv_physical_proposals WHERE proposal_id = {}",
                    q(&proposal_id)
                ),
            )
            .await?;
        if existing_proposal.is_empty() {
            store
                .run(
                    sim_minute,
                    actor,
                    "persist physical proposal",
                    &format!(
                        "INSERT INTO lv_physical_proposals VALUES ({}, {}, {}, {}, {}, {}, {})",
                        q(&proposal_id),
                        q(&episode_id),
                        q(&action_kind),
                        q(&state),
                        q(&idempotency_key),
                        q(source_observation_id),
                        q(&json_text(proposal)),
                    ),
                )
                .await?;
            summary.proposal_inserted = true;
        }
    }

    Ok(summary)
}

/// Append an executor result after a simulator/controller attempt.
pub async fn ingest_reconciliation(
    store: &Store,
    reconciliation: &Value,
    sim_minute: i64,
    actor: &str,
) -> Result<bool, String> {
    let attempt_id = string(reconciliation, "attempt_id")?;
    let proposal_id = string(reconciliation, "proposal_id")?;
    let episode_id = string(reconciliation, "episode_id")?;
    let state = string(reconciliation, "state")?;
    let observed_at = string(reconciliation, "observed_at")?;
    let error_code = optional_string(reconciliation, "error_code");
    let proposal_exists = store
        .run(
            sim_minute,
            actor,
            "check physical reconciliation proposal",
            &format!(
                "SELECT proposal_id FROM lv_physical_proposals WHERE proposal_id = {}",
                q(&proposal_id)
            ),
        )
        .await?;
    if proposal_exists.is_empty() {
        return Err(format!("unknown physical proposal: {proposal_id}"));
    }
    let exists = store
        .run(
            sim_minute,
            actor,
            "check physical reconciliation replay",
            &format!(
                "SELECT attempt_id FROM lv_physical_reconciliations WHERE attempt_id = {}",
                q(&attempt_id)
            ),
        )
        .await?;
    if !exists.is_empty() {
        return Ok(false);
    }
    store
        .run(
            sim_minute,
            actor,
            "persist physical reconciliation",
            &format!(
                "INSERT INTO lv_physical_reconciliations VALUES ({}, {}, {}, {}, {}, {}, {})",
                q(&attempt_id),
                q(&proposal_id),
                q(&episode_id),
                q(&state),
                q(&observed_at),
                q(&error_code),
                q(&json_text(reconciliation)),
            ),
        )
        .await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::TABLES;
    use less_common::config::EngineConfig;
    use less_engine::LessEngine;
    use less_query::LessSession;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn envelope_ingest_is_duplicate_safe_and_reconciles_after_proposal() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("lessville-physical-replay-{suffix}"));
        std::fs::create_dir_all(&dir).expect("data dir");
        let engine = LessEngine::open(EngineConfig::with_data_dir(dir.clone())).expect("engine");
        let session = LessSession::new_async(engine.clone())
            .await
            .expect("session");
        let store = Store::new(session, engine);
        for (name, ddl) in TABLES {
            store
                .run(10, "test", &format!("create {name}"), ddl)
                .await
                .expect("table");
        }
        let envelope = json!({
            "schema": "lessdb-physical-envelope/v1",
            "profile": "reachy-mini",
            "simulator": "reachy-mini-mujoco",
            "control_interface": "reachy-mini-sdk-localhost",
            "episode_id": "episode:test",
            "observation": {
                "observation_id": "obs:test:1",
                "sensor_id": "sensor:camera",
                "kind": "scene_observed",
                "observed_at": "2026-01-15T08:20:00Z",
                "evidence_hash": "sha256:test",
                "payload_ref": "evidence://test/frame-1",
                "sequence": 1
            },
            "provenance": {"source": "test"},
            "proposal": {
                "proposal_id": "proposal:test:1",
                "action_kind": "look_at",
                "state": "draft",
                "idempotency_key": "idem:test:1",
                "source_observation_ids": ["obs:test:1"]
            }
        });
        let first = ingest_envelope(&store, &envelope, 10, "test")
            .await
            .expect("first ingest");
        assert_eq!(
            first,
            IngestSummary {
                episode_created: true,
                observation_inserted: true,
                proposal_inserted: true
            }
        );
        let replay = ingest_envelope(&store, &envelope, 10, "test")
            .await
            .expect("replay");
        assert_eq!(replay, IngestSummary::default());
        let reconciliation = json!({
            "attempt_id": "attempt:test:1",
            "proposal_id": "proposal:test:1",
            "episode_id": "episode:test",
            "state": "succeeded",
            "observed_at": "2026-01-15T08:20:03Z",
            "error_code": null
        });
        let mut unknown = reconciliation.clone();
        unknown["proposal_id"] = json!("proposal:missing");
        assert!(
            ingest_reconciliation(&store, &unknown, 10, "test")
                .await
                .expect_err("unknown proposal must be rejected")
                .contains("unknown physical proposal")
        );
        assert!(
            ingest_reconciliation(&store, &reconciliation, 10, "test")
                .await
                .expect("reconcile")
        );
        assert!(
            !ingest_reconciliation(&store, &reconciliation, 10, "test")
                .await
                .expect("replay reconcile")
        );
        let counts = store
            .run(
                10,
                "test",
                "count physical replay",
                "SELECT count(*) AS n FROM lv_physical_observations",
            )
            .await
            .expect("count");
        assert_eq!(counts[0].get("n").and_then(Value::as_i64), Some(1));
        drop(store);
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[tokio::test]
    #[ignore = "requires a simulator-generated JSONL file"]
    async fn external_envelope_file_replays_into_lessdb() {
        let path = std::env::var("LESSVILLE_PHYSICAL_ENVELOPE_FILE")
            .expect("LESSVILLE_PHYSICAL_ENVELOPE_FILE");
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("lessville-physical-external-{suffix}"));
        std::fs::create_dir_all(&dir).expect("data dir");
        let engine = LessEngine::open(EngineConfig::with_data_dir(dir.clone())).expect("engine");
        let session = LessSession::new_async(engine.clone())
            .await
            .expect("session");
        let store = Store::new(session, engine);
        for (name, ddl) in TABLES {
            store
                .run(10, "external-test", &format!("create {name}"), ddl)
                .await
                .expect("table");
        }
        let mut inserted = 0;
        for line in std::fs::read_to_string(path)
            .expect("envelope file")
            .lines()
        {
            if line.trim().is_empty() {
                continue;
            }
            let envelope: Value = serde_json::from_str(line).expect("envelope JSON");
            let result = ingest_envelope(&store, &envelope, 10, "external-test")
                .await
                .expect("external ingest");
            inserted += usize::from(result.observation_inserted);
        }
        let rows = store
            .run(
                10,
                "external-test",
                "count external observations",
                "SELECT count(*) AS n FROM lv_physical_observations",
            )
            .await
            .expect("count observations");
        let count = rows[0]
            .get("n")
            .and_then(Value::as_i64)
            .expect("observation count");
        assert_eq!(inserted as i64, count);
        assert!(
            count > 0,
            "the external simulator must produce observations"
        );
        println!(
            "external_physical_replay={{\"observations\":{count},\"lessdb_data_dir\":\"temporary\"}}"
        );
        drop(store);
        std::fs::remove_dir_all(dir).expect("cleanup");
    }
}
