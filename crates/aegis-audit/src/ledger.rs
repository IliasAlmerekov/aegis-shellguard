//! The Snapshot ledger: the single reading of the Audit log that answers which
//! Snapshots exist, which were pruned, and which record an id resolves to.
//!
//! Rules (ADR-049):
//! - A Snapshot is keyed by the pair (plugin, snapshot_id). A prune hides only
//!   the pair it names; a legacy prune entry without a `snapshots` entry hides
//!   every pair with the id from its `aegis prune <id>` command.
//! - A prune is final: a later record of a pruned pair does not revive it.
//! - Only creation entries record a Snapshot. The `Approved` entry that
//!   `aegis rollback <id>` writes neither creates nor refreshes a record.
//! - "Latest" means latest in log order (segments oldest to newest, then line
//!   order), never latest by timestamp.

use std::collections::HashSet;

use aegis_types::{Decision, RecordedSnapshot};

use crate::error::AuditError;
use crate::logger::{AuditEntry, AuditLogger};

const PRUNE_COMMAND_PREFIX: &str = "aegis prune ";
const ROLLBACK_COMMAND_PREFIX: &str = "aegis rollback ";

/// Why [`SnapshotLedger::resolve`] found no live record for an id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotLookupError {
    /// Every record with this id was pruned.
    Pruned,
    /// No record in the Audit log has this id.
    NotFound,
}

/// The Snapshots the Audit log records, split into live and pruned.
#[derive(Debug, Clone, Default)]
pub struct SnapshotLedger {
    /// Live records, oldest first in log order.
    live: Vec<RecordedSnapshot>,
    /// Ids of every pair hidden by a prune, plus legacy prune ids.
    pruned_ids: HashSet<String>,
    /// Number of recorded pairs hidden by a prune.
    pruned_count: usize,
}

impl SnapshotLedger {
    /// Read every segment of `logger`'s Audit log and build the ledger.
    ///
    /// # Errors
    ///
    /// Returns the [`AuditError`] of the first segment or line that cannot be
    /// read; an unreadable line is never skipped.
    pub fn load(logger: &AuditLogger) -> Result<Self, AuditError> {
        Ok(Self::from_entries(&logger.read_all()?))
    }

    /// Build the ledger from Audit log entries in log order.
    pub fn from_entries(entries: &[AuditEntry]) -> Self {
        let mut recorded: Vec<RecordedSnapshot> = Vec::new();
        let mut pruned_pairs: HashSet<(&str, &str)> = HashSet::new();
        let mut legacy_pruned_ids: HashSet<&str> = HashSet::new();

        for entry in entries {
            let base = entry.as_base();
            if base.decision == Decision::Pruned {
                if base.snapshots.is_empty() {
                    if let Some(id) = snapshot_id_from_prune_command(&base.command) {
                        legacy_pruned_ids.insert(id);
                    }
                }
                for snapshot in &base.snapshots {
                    pruned_pairs.insert((&snapshot.plugin, &snapshot.snapshot_id));
                }
                continue;
            }
            if base.decision == Decision::Approved
                && base.command.starts_with(ROLLBACK_COMMAND_PREFIX)
            {
                continue;
            }
            for snapshot in &base.snapshots {
                recorded.retain(|record| {
                    record.plugin != snapshot.plugin || record.snapshot_id != snapshot.snapshot_id
                });
                recorded.push(RecordedSnapshot {
                    plugin: snapshot.plugin.clone(),
                    snapshot_id: snapshot.snapshot_id.clone(),
                    recorded_at: base.timestamp.into(),
                });
            }
        }

        let is_pruned = |record: &RecordedSnapshot| {
            pruned_pairs.contains(&(record.plugin.as_str(), record.snapshot_id.as_str()))
                || legacy_pruned_ids.contains(record.snapshot_id.as_str())
        };
        let pruned_count = recorded.iter().filter(|record| is_pruned(record)).count();
        recorded.retain(|record| !is_pruned(record));

        let pruned_ids = pruned_pairs
            .iter()
            .map(|(_, id)| *id)
            .chain(legacy_pruned_ids)
            .map(str::to_owned)
            .collect();

        Self {
            live: recorded,
            pruned_ids,
            pruned_count,
        }
    }

    /// Live records, oldest first in log order.
    pub fn live_records(&self) -> &[RecordedSnapshot] {
        &self.live
    }

    /// Number of recorded Snapshots hidden because they were pruned.
    pub fn pruned_count(&self) -> usize {
        self.pruned_count
    }

    /// Resolve an id to the latest live record with that id, across plugins.
    ///
    /// # Errors
    ///
    /// [`SnapshotLookupError::Pruned`] when every record with this id was
    /// pruned, [`SnapshotLookupError::NotFound`] when none has this id.
    pub fn resolve(&self, snapshot_id: &str) -> Result<&RecordedSnapshot, SnapshotLookupError> {
        if let Some(record) = self
            .live
            .iter()
            .rev()
            .find(|record| record.snapshot_id == snapshot_id)
        {
            return Ok(record);
        }
        if self.pruned_ids.contains(snapshot_id) {
            Err(SnapshotLookupError::Pruned)
        } else {
            Err(SnapshotLookupError::NotFound)
        }
    }
}

/// Extract the snapshot id from a prune entry's `aegis prune <id>` command.
///
/// Legacy prune entries carry the id only there, with an empty `snapshots`.
fn snapshot_id_from_prune_command(command: &str) -> Option<&str> {
    command
        .strip_prefix(PRUNE_COMMAND_PREFIX)
        .filter(|id| !id.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logger::AuditSnapshot;
    use aegis_types::RiskLevel;

    fn entry(command: &str, decision: Decision, pairs: &[(&str, &str)]) -> AuditEntry {
        AuditEntry::new(
            command,
            RiskLevel::Danger,
            Vec::new(),
            decision,
            pairs
                .iter()
                .map(|(plugin, id)| AuditSnapshot {
                    plugin: (*plugin).to_string(),
                    snapshot_id: (*id).to_string(),
                })
                .collect(),
            None,
            None,
        )
    }

    fn live_pairs(ledger: &SnapshotLedger) -> Vec<(&str, &str)> {
        ledger
            .live_records()
            .iter()
            .map(|record| (record.plugin.as_str(), record.snapshot_id.as_str()))
            .collect()
    }

    #[test]
    fn ledger_prune_hides_only_the_named_pair() {
        let ledger = SnapshotLedger::from_entries(&[
            entry("rm a", Decision::Approved, &[("git", "X")]),
            entry("rm b", Decision::Approved, &[("docker", "X")]),
            entry("aegis prune X", Decision::Pruned, &[("git", "X")]),
        ]);

        assert_eq!(live_pairs(&ledger), vec![("docker", "X")]);
        assert_eq!(ledger.pruned_count(), 1);
        assert_eq!(ledger.resolve("X").map(|r| r.plugin.as_str()), Ok("docker"));
    }

    #[test]
    fn ledger_legacy_prune_hides_every_pair_with_the_command_id() {
        let ledger = SnapshotLedger::from_entries(&[
            entry("rm a", Decision::Approved, &[("git", "X")]),
            entry("rm b", Decision::Approved, &[("docker", "X")]),
            entry("aegis prune X", Decision::Pruned, &[]),
        ]);

        assert!(live_pairs(&ledger).is_empty());
        assert_eq!(ledger.resolve("X"), Err(SnapshotLookupError::Pruned));
    }

    #[test]
    fn ledger_prune_is_final_against_a_later_record() {
        let ledger = SnapshotLedger::from_entries(&[
            entry("rm a", Decision::Approved, &[("git", "X")]),
            entry("aegis prune X", Decision::Pruned, &[("git", "X")]),
            entry("rm a again", Decision::Approved, &[("git", "X")]),
        ]);

        assert_eq!(ledger.resolve("X"), Err(SnapshotLookupError::Pruned));
    }

    #[test]
    fn ledger_rollback_entry_does_not_refresh_or_reorder() {
        let ledger = SnapshotLedger::from_entries(&[
            entry("rm a", Decision::Approved, &[("git", "A")]),
            entry("rm b", Decision::Approved, &[("git", "B")]),
            entry("aegis rollback A", Decision::Approved, &[("git", "A")]),
        ]);

        assert_eq!(live_pairs(&ledger), vec![("git", "A"), ("git", "B")]);
    }

    #[test]
    fn ledger_resolve_unknown_id_is_not_found() {
        let ledger =
            SnapshotLedger::from_entries(&[entry("rm a", Decision::Approved, &[("git", "A")])]);

        assert_eq!(
            ledger.resolve("missing"),
            Err(SnapshotLookupError::NotFound)
        );
    }

    #[test]
    fn ledger_load_fails_on_an_unreadable_line() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("audit.jsonl");
        let logger = AuditLogger::new(&path);
        logger
            .append(entry("rm a", Decision::Approved, &[("git", "A")]))
            .unwrap();
        let mut contents = std::fs::read_to_string(&path).unwrap();
        contents.push_str("not json\n");
        std::fs::write(&path, contents).unwrap();

        assert!(SnapshotLedger::load(&logger).is_err());
    }
}
