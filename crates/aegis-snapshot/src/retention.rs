//! Snapshot retention policy and prunable-record resolution.

use std::collections::{HashMap, HashSet};

use time::OffsetDateTime;

use aegis_config::PruneConfig;
use aegis_types::RecordedSnapshot;

/// One snapshot record that may be eligible for pruning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrunableRecord {
    /// Name of the snapshot plugin that created the record.
    pub plugin: String,
    /// Opaque snapshot identifier.
    pub snapshot_id: String,
    /// Timestamp of the Audit log entry that created the snapshot.
    pub recorded_at: OffsetDateTime,
}

/// Retention policy used to decide which snapshots become prune candidates.
///
/// A snapshot is kept if it satisfies either the per-provider count rule or
/// the global age rule. Only snapshots that fail both rules are returned by
/// [`RetentionPolicy::apply`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RetentionPolicy {
    max_count_per_provider: Option<usize>,
    max_age_days: Option<u32>,
}

impl RetentionPolicy {
    /// Retention policy that only uses a maximum age rule.
    pub fn from_max_age_days(days: u32) -> Self {
        Self {
            max_age_days: Some(days),
            ..Self::default()
        }
    }

    /// Retention policy that only uses a per-provider count rule.
    pub fn from_max_count_per_provider(count: usize) -> Self {
        Self {
            max_count_per_provider: Some(count),
            ..Self::default()
        }
    }

    /// Build a policy from the effective runtime config.
    pub fn from_config(config: &PruneConfig) -> Self {
        Self {
            max_count_per_provider: config.max_count_per_provider,
            max_age_days: config.max_age_days,
        }
    }

    /// Apply this policy to a set of records and return the prune candidates.
    ///
    /// `records` must be newest first in Audit log order. The per-provider
    /// count rule keeps the first `count` records of each provider in that
    /// order and never consults timestamps, which feed only the age rule
    /// (ADR-049). Candidates preserve the original input order.
    pub fn apply(&self, records: &[PrunableRecord], now: OffsetDateTime) -> Vec<PrunableRecord> {
        if self.max_count_per_provider.is_none() && self.max_age_days.is_none() {
            return Vec::new();
        }

        let mut kept = HashSet::new();

        if let Some(days) = self.max_age_days {
            let max_age = time::Duration::days(i64::from(days));
            for record in records {
                if now - record.recorded_at <= max_age {
                    kept.insert((record.plugin.clone(), record.snapshot_id.clone()));
                }
            }
        }

        if let Some(count) = self.max_count_per_provider {
            let mut by_provider: HashMap<&str, Vec<&PrunableRecord>> = HashMap::new();
            for record in records {
                by_provider
                    .entry(record.plugin.as_str())
                    .or_default()
                    .push(record);
            }
            for group in by_provider.values() {
                for record in group.iter().take(count) {
                    kept.insert((record.plugin.clone(), record.snapshot_id.clone()));
                }
            }
        }

        records
            .iter()
            .filter(|record| !kept.contains(&(record.plugin.clone(), record.snapshot_id.clone())))
            .cloned()
            .collect()
    }
}

/// Turn the Snapshot ledger's live records into prune records, newest first
/// in log order.
///
/// The ledger lists records oldest first in log order; retention and the prune
/// preview want the newest first.
pub(crate) fn prunable_records_newest_first(records: &[RecordedSnapshot]) -> Vec<PrunableRecord> {
    records
        .iter()
        .rev()
        .map(|record| PrunableRecord {
            plugin: record.plugin.clone(),
            snapshot_id: record.snapshot_id.clone(),
            recorded_at: OffsetDateTime::from(record.recorded_at),
        })
        .collect()
}
