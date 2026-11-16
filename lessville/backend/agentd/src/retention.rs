//! Bounded hot-table retention (added after the 2026-11-14 outage).
//!
//! LessVille's working tables are append-mostly. Without a bound, per-tick
//! recall and checkpoint size grow with the age of the world until the single
//! lite instance runs out of memory. Each night, every policy here removes rows
//! older than its logical-time window from the *hot* tables. Older evidence is
//! not rewritten: it remains in the hourly R2 checkpoint history (168 h) and in
//! archived prior worlds. Permanent memories are capped per owner rather than
//! aged out, so long-term identity survives.
//!
//! This is application-layer compaction over embedded LessDB `DELETE`; it is
//! not a native TTL, tiered storage or legal-retention guarantee.

use less_agent::{RETENTION_POLICY_SCHEMA_V1, RetentionArchive, RetentionPolicy};

use crate::store::{Store, i, q};

pub const RETENTION_POLICY_VERSION: &str = "lessville-retention/v1";
/// Permanent memories kept per owner (newest first).
pub const PERMANENT_MEMORIES_PER_OWNER: usize = 160;
const DAY: i64 = 1440;

/// (table, logical-time column, keep_days, max_rows)
const WINDOWS: &[(&str, &str, i64, usize)] = &[
    ("lv_events", "sim_minute", 3, 20_000),
    ("lv_presence_events", "sim_minute", 3, 8_000),
    ("lv_messages", "sim_minute", 3, 6_000),
    ("lv_decisions", "sim_minute", 3, 15_000),
    ("lv_access", "sim_minute", 3, 4_000),
    ("lv_llm_calls", "sim_minute", 3, 4_000),
    ("lv_orders", "sim_minute", 7, 6_000),
    ("lv_object_transitions", "sim_minute", 3, 4_000),
    ("lv_novelty_events", "sim_minute", 3, 8_000),
    ("lv_frontier_signals", "sim_minute", 3, 4_000),
    ("lv_evolution_proposals", "sim_minute", 3, 8_000),
    ("lv_counterfactuals", "sim_minute", 7, 2_000),
    ("lv_interactions", "sim_minute", 7, 6_000),
    ("lv_competency_events", "sim_minute", 7, 6_000),
    ("lv_memory_lifecycle", "changed_at", 3, 15_000),
];
/// Non-permanent memories age out; expired temporary memories are already
/// invisible to recall.
const MEMORY_KEEP_DAYS: i64 = 3;

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct RetentionReport {
    pub tables: Vec<(String, i64)>,
    pub removed: i64,
    pub failed: Vec<String>,
}

pub fn policies() -> Vec<RetentionPolicy> {
    let mut out: Vec<RetentionPolicy> = WINDOWS
        .iter()
        .map(|(table, _, days, max_rows)| RetentionPolicy {
            schema: RETENTION_POLICY_SCHEMA_V1.into(),
            table: (*table).into(),
            keep_minutes: days * DAY,
            max_rows: *max_rows,
            pinned_predicate: None,
            archive: RetentionArchive::CheckpointHistory,
            policy_version: RETENTION_POLICY_VERSION.into(),
        })
        .collect();
    out.push(RetentionPolicy {
        schema: RETENTION_POLICY_SCHEMA_V1.into(),
        table: "lv_memories".into(),
        keep_minutes: MEMORY_KEEP_DAYS * DAY,
        max_rows: PERMANENT_MEMORIES_PER_OWNER,
        pinned_predicate: Some("kind = 'permanent' (capped per owner, never aged out)".into()),
        archive: RetentionArchive::CheckpointHistory,
        policy_version: RETENTION_POLICY_VERSION.into(),
    });
    out
}

async fn count(store: &Store, now: i64, sql: &str) -> Result<i64, String> {
    let rows = store.run(now, "world", "retention: count", sql).await?;
    Ok(rows.first().map(|r| i(r, "n")).unwrap_or(0))
}

async fn delete(store: &Store, now: i64, table: &str, filter: &str, report: &mut RetentionReport) {
    let before = match count(
        store,
        now,
        &format!("SELECT COUNT(*) AS n FROM {table} WHERE {filter}"),
    )
    .await
    {
        Ok(n) => n,
        Err(e) => {
            report.failed.push(format!("{table}: {e}"));
            return;
        }
    };
    if before == 0 {
        return;
    }
    match store
        .run(
            now,
            "world",
            "retention: compact hot table",
            &format!("DELETE FROM {table} WHERE {filter}"),
        )
        .await
    {
        Ok(_) => {
            report.removed += before;
            if let Some(entry) = report.tables.iter_mut().find(|(t, _)| t == table) {
                entry.1 += before;
            } else {
                report.tables.push((table.to_string(), before));
            }
        }
        Err(e) => report.failed.push(format!("{table}: {e}")),
    }
}

/// Run every policy once. Deterministic for a given world state and minute.
pub async fn compact(store: &Store, now: i64, owners: &[&str]) -> RetentionReport {
    let mut report = RetentionReport::default();
    for p in policies() {
        if p.validate().is_err() {
            report.failed.push(format!("{}: invalid policy", p.table));
        }
    }
    for (table, col, days, max_rows) in WINDOWS {
        let cutoff = now - days * DAY;
        delete(store, now, table, &format!("{col} < {cutoff}"), &mut report).await;
        // Row cap: if a single busy window still exceeds the bound, keep newest.
        if let Ok(rows) = store
            .run(
                now,
                "world",
                "retention: row-cap boundary",
                &format!(
                    "SELECT {col} AS m FROM {table} ORDER BY {col} DESC LIMIT 1 OFFSET {}",
                    max_rows
                ),
            )
            .await
            && let Some(r) = rows.first()
        {
            let boundary = i(r, "m");
            delete(
                store,
                now,
                table,
                &format!("{col} < {boundary}"),
                &mut report,
            )
            .await;
        }
    }
    let cutoff = now - MEMORY_KEEP_DAYS * DAY;
    delete(
        store,
        now,
        "lv_memories",
        &format!("kind <> 'permanent' AND sim_minute < {cutoff}"),
        &mut report,
    )
    .await;
    for owner in owners {
        if let Ok(rows) = store
            .run(
                now,
                "world",
                "retention: permanent cap boundary",
                &format!(
                    "SELECT id FROM lv_memories WHERE owner = {} AND kind = 'permanent' ORDER BY id DESC LIMIT 1 OFFSET {}",
                    q(owner),
                    PERMANENT_MEMORIES_PER_OWNER
                ),
            )
            .await
            && let Some(r) = rows.first()
        {
            let boundary = i(r, "id");
            delete(
                store,
                now,
                "lv_memories",
                &format!("owner = {} AND kind = 'permanent' AND id <= {boundary}", q(owner)),
                &mut report,
            )
            .await;
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_policy_is_a_valid_bounded_contract() {
        let ps = policies();
        assert!(ps.len() >= 14);
        for p in &ps {
            p.validate().expect("valid retention policy");
            assert!(p.keep_minutes <= 7 * DAY, "{} keeps too long", p.table);
        }
    }

    #[test]
    fn retention_covers_every_append_mostly_table() {
        let covered: Vec<&str> = WINDOWS.iter().map(|w| w.0).collect();
        for (name, ddl) in crate::sim::TABLES {
            let keyed_state = ddl.contains("UNIQUE (k)")
                || ddl.contains("UNIQUE (agent)")
                || ddl.contains("UNIQUE (member_key)")
                || ddl.contains("UNIQUE (lesson_id)")
                || ddl.contains("UNIQUE (edge_id)")
                || ddl.contains("UNIQUE (competency_id)")
                || ddl.contains("UNIQUE (record_id)")
                || ddl.contains("UNIQUE (challenge_id)")
                || ddl.contains("UNIQUE (grant_id)")
                || ddl.contains("UNIQUE (episode_id)")
                // Private physical-replay tables are bounded by replay episodes
                // and carry string timestamps; they are never written by the
                // public town loop.
                || name.starts_with("lv_physical");
            if keyed_state || *name == "lv_memories" {
                continue;
            }
            assert!(
                covered.contains(name),
                "{name} grows without a retention policy"
            );
        }
    }
}
