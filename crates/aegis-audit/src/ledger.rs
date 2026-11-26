use std::collections::{HashMap, HashSet};

use aegis_types::{Decision, RecordedSnapshot, RiskLevel};

use crate::error::AuditError;
use crate::logger::{AuditEntry, AuditLogger, DecisionEntry};

mod commands;
pub use commands::{CommandSnapshot, RecordedCommand};

const PRUNE_COMMAND_PREFIX: &str = "aegis prune ";
const ROLLBACK_COMMAND_PREFIX: &str = "aegis rollback ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotLookupError {
    Pruned,
    NotFound,
}

#[derive(Debug, Clone, Default)]
pub struct SnapshotLedger {
    live: Vec<RecordedSnapshot>,
    pruned_ids: HashSet<String>,
    pruned_count: usize,
    commands: Vec<RecordedCommand>,
}

impl SnapshotLedger {
    pub fn load(logger: &AuditLogger) -> Result<Self, AuditError> {
        Ok(Self::from_entries(&logger.read_all()?))
    }

    pub fn from_entries(entries: &[AuditEntry]) -> Self {
        let mut recorded: Vec<Option<RecordedSnapshot>> = Vec::new();
        let mut slot_of: HashMap<(&str, &str), usize> = HashMap::new();
        let mut pruned_pairs: HashSet<(&str, &str)> = HashSet::new();
        let mut legacy_pruned_ids: HashSet<&str> = HashSet::new();

        for entry in entries {
            let base = entry.as_base();
            if base.decision == Decision::Pruned {
                if base.snapshots.is_empty()
                    && let Some(id) = snapshot_id_from_prune_command(&base.command)
                {
                    legacy_pruned_ids.insert(id);
                }
                for snapshot in &base.snapshots {
                    pruned_pairs.insert((&snapshot.plugin, &snapshot.snapshot_id));
                }
                continue;
            }
            if is_rollback_entry(base) {
                continue;
            }
            for snapshot in &base.snapshots {
                let slot = recorded.len();
                if let Some(old) = slot_of.insert((&snapshot.plugin, &snapshot.snapshot_id), slot) {
                    recorded[old] = None;
                }
                recorded.push(Some(RecordedSnapshot {
                    plugin: snapshot.plugin.clone(),
                    snapshot_id: snapshot.snapshot_id.clone(),
                    recorded_at: base.timestamp.into(),
                }));
            }
        }

        let mut recorded: Vec<RecordedSnapshot> = recorded.into_iter().flatten().collect();
        let is_pruned = |record: &RecordedSnapshot| {
            pruned_pairs.contains(&(record.plugin.as_str(), record.snapshot_id.as_str()))
                || legacy_pruned_ids.contains(record.snapshot_id.as_str())
        };
        let pruned_count = recorded.iter().filter(|record| is_pruned(record)).count();
        recorded.retain(|record| !is_pruned(record));

        let commands = commands::build(entries, |plugin, id| {
            !pruned_pairs.contains(&(plugin, id)) && !legacy_pruned_ids.contains(id)
        });

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
            commands,
        }
    }

    pub fn recent_commands(&self) -> &[RecordedCommand] {
        &self.commands
    }

    pub fn live_records(&self) -> &[RecordedSnapshot] {
        &self.live
    }

    pub fn pruned_count(&self) -> usize {
        self.pruned_count
    }

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

fn is_rollback_entry(base: &DecisionEntry) -> bool {
    let [snapshot] = base.snapshots.as_slice() else {
        return false;
    };
    base.decision == Decision::Approved
        && base.risk == RiskLevel::Safe
        && base
            .command
            .strip_prefix(ROLLBACK_COMMAND_PREFIX)
            .is_some_and(|id| id == snapshot.snapshot_id)
}

fn snapshot_id_from_prune_command(command: &str) -> Option<&str> {
    command
        .strip_prefix(PRUNE_COMMAND_PREFIX)
        .filter(|id| !id.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logger::AuditSnapshot;

    fn entry(command: &str, decision: Decision, pairs: &[(&str, &str)]) -> AuditEntry {
        entry_with_risk(command, RiskLevel::Danger, decision, pairs)
    }

    fn entry_with_risk(
        command: &str,
        risk: RiskLevel,
        decision: Decision,
        pairs: &[(&str, &str)],
    ) -> AuditEntry {
        AuditEntry::new(
            command,
            risk,
            Vec::new(),
            decision,
            pairs
                .iter()
                .map(|(plugin, id)| AuditSnapshot {
                    plugin: (*plugin).to_string(),
                    snapshot_id: (*id).to_string(),
                    git_target: None,
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

    fn located(command: &str, cwd: &str, pairs: &[(&str, &str)]) -> AuditEntry {
        entry(command, Decision::Approved, pairs).with_cwd(Some(cwd.to_string()))
    }

    fn command_names(ledger: &SnapshotLedger) -> Vec<&str> {
        ledger
            .recent_commands()
            .iter()
            .map(|c| c.command.as_str())
            .collect()
    }

    #[test]
    fn recent_commands_lists_commands_with_live_snapshots_latest_first() {
        let ledger = SnapshotLedger::from_entries(&[
            located("rm a", "/a", &[("git", "A")]),
            located("ls", "/a", &[]),
            located("rm b", "/b", &[("git", "B"), ("docker", "C")]),
        ]);

        assert_eq!(command_names(&ledger), vec!["rm b", "rm a"]);
        let latest = &ledger.recent_commands()[0];
        assert_eq!(latest.cwd.as_deref(), Some("/b"));
        assert_eq!(latest.snapshots.len(), 2);
    }

    #[test]
    fn recent_commands_drops_pruned_snapshots_and_commands_left_empty() {
        let ledger = SnapshotLedger::from_entries(&[
            located("rm a", "/a", &[("git", "A")]),
            located("rm b", "/b", &[("git", "B"), ("docker", "C")]),
            entry("aegis prune A", Decision::Pruned, &[("git", "A")]),
            entry("aegis prune C", Decision::Pruned, &[("docker", "C")]),
        ]);

        assert_eq!(command_names(&ledger), vec!["rm b"]);
        assert_eq!(ledger.recent_commands()[0].snapshots.len(), 1);
    }

    #[test]
    fn recent_commands_skips_rollback_entries_and_reports_the_restore_time() {
        let ledger = SnapshotLedger::from_entries(&[
            located("rm a", "/a", &[("git", "A")]),
            entry_with_risk(
                "aegis rollback A",
                RiskLevel::Safe,
                Decision::Approved,
                &[("git", "A")],
            ),
        ]);

        assert_eq!(command_names(&ledger), vec!["rm a"]);
        assert!(
            ledger.recent_commands()[0].snapshots[0]
                .restored_at
                .is_some()
        );
    }

    #[test]
    fn recent_commands_skips_commands_that_did_not_run() {
        let ledger = SnapshotLedger::from_entries(&[
            located("rm a", "/a", &[("git", "A")]),
            entry("rm b", Decision::Denied, &[("git", "B")]).with_cwd(Some("/b".to_string())),
        ]);

        assert_eq!(command_names(&ledger), vec!["rm a"]);
    }

    #[test]
    fn recent_commands_gives_a_re_recorded_pair_to_its_latest_command() {
        let ledger = SnapshotLedger::from_entries(&[
            located("first", "/a", &[("git", "A")]),
            located("second", "/a", &[("git", "A")]),
        ]);

        assert_eq!(command_names(&ledger), vec!["second"]);
    }

    #[test]
    fn recent_commands_forgets_a_restore_when_a_later_command_records_the_same_pair() {
        let ledger = SnapshotLedger::from_entries(&[
            located("first", "/a", &[("git", "clean")]),
            entry_with_risk(
                "aegis rollback clean",
                RiskLevel::Safe,
                Decision::Approved,
                &[("git", "clean")],
            ),
            located("second", "/a", &[("git", "clean")]),
        ]);

        assert_eq!(ledger.recent_commands()[0].snapshots[0].restored_at, None);
    }

    #[test]
    fn recent_commands_keeps_an_entry_without_cwd_with_no_location() {
        let ledger =
            SnapshotLedger::from_entries(&[entry("rm a", Decision::Approved, &[("git", "A")])]);

        assert_eq!(ledger.recent_commands()[0].cwd, None);
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
            entry_with_risk(
                "aegis rollback A",
                RiskLevel::Safe,
                Decision::Approved,
                &[("git", "A")],
            ),
        ]);

        assert_eq!(live_pairs(&ledger), vec![("git", "A"), ("git", "B")]);
    }

    #[test]
    fn ledger_records_a_guarded_command_that_starts_like_a_rollback() {
        let ledger = SnapshotLedger::from_entries(&[entry(
            "aegis rollback x; rm -rf src",
            Decision::Approved,
            &[("git", "S")],
        )]);

        assert_eq!(live_pairs(&ledger), vec![("git", "S")]);
    }

    #[test]
    fn ledger_re_recorded_pair_moves_to_the_latest_position() {
        let ledger = SnapshotLedger::from_entries(&[
            entry("rm a", Decision::Approved, &[("git", "A")]),
            entry("rm b", Decision::Approved, &[("git", "B")]),
            entry("rm a again", Decision::Approved, &[("git", "A")]),
        ]);

        assert_eq!(live_pairs(&ledger), vec![("git", "B"), ("git", "A")]);
    }

    #[test]
    fn ledger_builds_a_large_log_in_linear_time() {
        let ids: Vec<String> = (0..40_000).map(|n| format!("snap-{n}")).collect();
        let entries: Vec<AuditEntry> = ids
            .iter()
            .map(|id| entry("rm a", Decision::Approved, &[("git", id.as_str())]))
            .collect();

        let started = std::time::Instant::now();
        let ledger = SnapshotLedger::from_entries(&entries);

        assert_eq!(ledger.live_records().len(), 40_000);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "ledger construction took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn ledger_pruned_count_distinguishes_zero_and_multiple_pairs() {
        let entries = [
            entry("rm a", Decision::Approved, &[("git", "A")]),
            entry("rm b", Decision::Approved, &[("docker", "B")]),
            entry("aegis prune A", Decision::Pruned, &[("git", "A")]),
            entry("aegis prune B", Decision::Pruned, &[("docker", "B")]),
        ];

        assert_eq!(
            SnapshotLedger::from_entries(&entries[..2]).pruned_count(),
            0
        );
        assert_eq!(SnapshotLedger::from_entries(&entries).pruned_count(), 2);
    }

    #[test]
    fn ledger_keeps_rollback_shaped_entries_unless_every_field_matches() {
        let ledger = SnapshotLedger::from_entries(&[
            entry("aegis rollback A", Decision::Approved, &[("git", "A")]),
            entry_with_risk(
                "aegis rollback B",
                RiskLevel::Safe,
                Decision::AutoApproved,
                &[("git", "B")],
            ),
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
