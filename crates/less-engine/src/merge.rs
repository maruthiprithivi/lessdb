//! Part merging: the Firefly compaction algorithm.
//!
//! A merge reads a selected set of parts, sorts by the sort key, collapses
//! uniqueness duplicates (keeping the last row per unique key — replacing
//! semantics), and writes a single new part at level `max(levels) + 1`.
//!
//! **Size-tiered policy**: instead of merge-everything,
//! parts are grouped by size tier (`log2(rows)`); the merge selects the
//! parts of the smallest tier holding at least two parts — and when no
//! tier does, the two smallest parts overall. Similar-sized parts merge
//! together, so a merged part is at most ~2x its inputs and small parts
//! don't get dragged into giant rewrites. (Streaming/external merges —
//! merging without holding all rows in memory — remain on the roadmap.)
//!
//! **Multi-writer FireflyCloud**: before merging shared parts, a node
//! claims exclusive ownership of the input part set in the shared CAS
//! metastore (`merge-claims/<table>/<part>`). A part claimed by another
//! node is left alone — two writers can flush and merge concurrently
//! without double-merging the same inputs into duplicate rows.

use std::collections::BTreeMap;

use arrow::record_batch::RecordBatch;

use less_catalog::{EngineKind, TableDef};
use less_common::{LessError, Result};
use less_storage::{DATA_FILE, META_FILE, PartMeta, sort_batch, write_part_sorted};

use crate::engine::{LessEngine, write_part_shared_sorted};
use crate::part::{DataPart, PartLocation};

/// How long a merge claim stays valid before other nodes may take it over
/// (guards against crashed owners leaving parts un-mergeable forever).
const CLAIM_TTL_SECS: u64 = 600;

/// Size tier of a part: `log2(rows)` (1-row parts land in tier 0).
fn tier_of(rows: u64) -> u32 {
    rows.max(1).ilog2()
}

/// Size-tiered selection: the parts of the smallest tier holding at least
/// two parts, else the two smallest parts overall. Selection is bounded by
/// `max_rows`: it takes the smallest parts of the chosen tier until the
/// cumulative row count reaches the cap, and refuses to merge when even the
/// two smallest eligible parts exceed it (those parts are too large for a
/// non-streaming merge — streaming input merges are on the roadmap). Returns
/// fewer than two parts when there is nothing safely mergeable.
fn select_merge_candidates(parts: &[DataPart], max_rows: u64) -> Vec<DataPart> {
    let mut by_tier: BTreeMap<u32, Vec<&DataPart>> = BTreeMap::new();
    for p in parts {
        by_tier
            .entry(tier_of(p.meta.row_count))
            .or_default()
            .push(p);
    }
    // Smallest tier with at least two parts wins; take the smallest parts of
    // that tier within the row budget.
    let mut group: Option<Vec<&DataPart>> = None;
    for (_, v) in by_tier.iter() {
        if v.len() >= 2 {
            group = Some(v.clone());
            break;
        }
    }
    let candidates: Vec<&DataPart> = match group {
        Some(v) => v,
        None => {
            // No tier holds two parts: fall back to the two smallest overall.
            let mut v: Vec<&DataPart> = parts.iter().collect();
            v.sort_by_key(|p| p.meta.row_count);
            v.truncate(2);
            v
        }
    };
    let mut picked: Vec<DataPart> = Vec::new();
    let mut rows = 0u64;
    for p in candidates {
        if rows + p.meta.row_count > max_rows {
            break;
        }
        picked.push(p.clone());
        rows += p.meta.row_count;
    }
    if picked.len() < 2 {
        // Even the two smallest eligible parts exceed the budget: skip the
        // merge rather than blow memory (parts this large need a streaming
        // merge).
        return Vec::new();
    }
    picked
}

/// Merge `table`'s parts to convergence with bounded passes: each pass
/// merges one size-tiered selection capped at
/// [`less_common::EngineConfig::max_merge_rows`] input rows, so peak merge
/// memory stays bounded no matter how large the table is (e.g. a 100M-row
/// table flushed in 256k-row parts converges over repeated small passes
/// instead of one giant in-memory merge). Returns the last merged part's
/// metadata or `None` when nothing was merged.
pub fn merge_all(engine: &LessEngine, table: &str) -> Result<Option<PartMeta>> {
    let mut last: Option<PartMeta> = None;
    loop {
        let def = engine.table(table)?;
        let parts = engine.parts(table)?;
        if parts.len() < 2 {
            return Ok(last);
        }
        let selected = select_merge_candidates(&parts, engine.config.max_merge_rows as u64);
        if selected.len() < 2 {
            return Ok(last);
        }
        let merged = if def.engine == EngineKind::FireflyCloud {
            merge_shared(engine, &def, table, &selected)?
        } else {
            merge_local(engine, &def, table, &selected)?
        };
        match merged {
            Some(m) => last = Some(m),
            // Shared merge with nothing exclusively claimable: stop.
            None => return Ok(last),
        }
    }
}

/// Merge one locally-stored candidate set into a single part.
fn merge_local(
    engine: &LessEngine,
    def: &TableDef,
    table: &str,
    parts: &[DataPart],
) -> Result<Option<PartMeta>> {
    let schema = def.arrow_schema();
    // Prepare per-part sorted runs (each bounded by its part), then merge
    // them with the chunked k-way writer.
    let mut runs: Vec<RecordBatch> = Vec::new();
    for part in parts {
        for batch in engine.read_data_part(part)? {
            runs.push(sort_batch(&schema, &batch, &def.sort_key)?);
        }
    }

    let max_level = parts.iter().map(|p| p.level()).max().unwrap_or(0);
    let mut opts = engine.write_options(def)?;
    opts.level = max_level + 1;
    opts.wal_lsn_max = parts.iter().filter_map(|p| p.meta.wal_lsn_max).max();

    let merged = write_part_sorted(
        &engine.catalog().parts_dir(table),
        table,
        &schema,
        runs,
        &opts,
    )?;

    less_telemetry::global().parts_merged.inc();

    // Delete the input parts only after the new part is durable.
    for part in parts {
        if let PartLocation::Local(dir) = &part.location {
            std::fs::remove_dir_all(dir)?;
        }
    }
    Ok(Some(merged))
}

/// FireflyCloud merge with CAS ownership claims on the input parts.
/// `parts` is the pre-selected, budget-bounded candidate set from
/// [`merge_all`]; only parts we exclusively claim are merged.
fn merge_shared(
    engine: &LessEngine,
    def: &TableDef,
    table: &str,
    parts: &[DataPart],
) -> Result<Option<PartMeta>> {
    let ms = engine.metastore().clone();
    let owner = engine.node_id().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let value = serde_json::json!({ "owner": &owner, "expires_at": now + CLAIM_TTL_SECS })
        .to_string()
        .into_bytes();

    // Claim what we can; leave other nodes' parts to their owners.
    let mut claimed: Vec<DataPart> = Vec::new();
    let mut claim_keys: Vec<String> = Vec::new();
    for part in parts {
        let key = format!("merge-claims/{table}/{}", part.meta.name);
        let (ms2, key2, value2) = (ms.clone(), key.clone(), value.clone());
        let mut ok =
            engine.block_on_owned(async move { ms2.put_if_absent(&key2, value2).await })?;
        if !ok && ms.supports_update_cas() {
            // Take over expired claims (the owner crashed mid-merge).
            let (ms2, key2) = (ms.clone(), key.clone());
            let existing = engine.block_on_owned(async move { ms2.get(&key2).await })?;
            if let Some(existing) = existing {
                let expired = serde_json::from_slice::<serde_json::Value>(&existing)
                    .ok()
                    .and_then(|v| v.get("expires_at").and_then(|e| e.as_u64()))
                    .is_some_and(|t| t < now);
                if expired {
                    let (ms2, key2, value2) = (ms.clone(), key.clone(), value.clone());
                    ok = engine.block_on_owned(async move {
                        ms2.compare_and_swap(&key2, Some(&existing), value2).await
                    })?;
                }
            }
        }
        if ok {
            claimed.push(part.clone());
            claim_keys.push(key);
        }
    }

    if claimed.len() < 2 {
        // Nothing worth merging that we exclusively own: release and move on.
        for key in &claim_keys {
            let (ms2, key2) = (ms.clone(), key.clone());
            let _ = engine.block_on_owned(async move { ms2.delete(&key2).await });
        }
        return Ok(None);
    }

    let schema = def.arrow_schema();
    let mut runs: Vec<RecordBatch> = Vec::new();
    for part in &claimed {
        for batch in engine.read_data_part(part)? {
            runs.push(sort_batch(&schema, &batch, &def.sort_key)?);
        }
    }

    let max_level = claimed.iter().map(|p| p.level()).max().unwrap_or(0);
    let mut opts = engine.write_options(def)?;
    opts.level = max_level + 1;
    opts.wal_lsn_max = claimed.iter().filter_map(|p| p.meta.wal_lsn_max).max();

    let shared = engine
        .shared()
        .ok_or_else(|| LessError::Engine("shared store unavailable".into()))?
        .clone();
    let table_owned = table.to_string();
    let merged = engine.block_on_owned({
        let shared2 = shared.clone();
        async move { write_part_shared_sorted(&shared2, &table_owned, &schema, runs, &opts).await }
    })?;

    less_telemetry::global().parts_merged.inc();

    // Delete the claimed inputs only after the merged part is durable,
    // then release the claims.
    for part in &claimed {
        if let PartLocation::Object(prefix) = &part.location {
            let (shared2, prefix2) = (shared.clone(), prefix.clone());
            engine.block_on_owned(async move {
                let _ = shared2.delete(&format!("{prefix2}/{DATA_FILE}")).await;
                let _ = shared2.delete(&format!("{prefix2}/{META_FILE}")).await;
            });
        }
    }
    for key in &claim_keys {
        let (ms2, key2) = (ms.clone(), key.clone());
        let _ = engine.block_on_owned(async move { ms2.delete(&key2).await });
    }
    Ok(Some(merged))
}
